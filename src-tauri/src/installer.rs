//! Install, launch, update and uninstall tools, driven by each tool's toolbay.json
//! (see manifest.rs). One click does everything a tool needs: the program and its
//! extra files (AI models), behind one progress bar, with no admin prompt.
//!
//! Where things go:
//! - Windows: %LOCALAPPDATA%\Programs\Toolbay Tools\<id>\   (NSIS installers run with /S /D=<that>);
//!   kept out of the hub's own folder so uninstalling the hub never touches tools.
//! - macOS:   ~/Applications/Toolbay/<App>.app
//! - Extra files: the tool's own app data dir (%APPDATA%\<identifier>, ~/Library/Application Support/<identifier>)
use crate::manifest::{self, FileEntry, Resolved};
use crate::store::{self, InstalledTool};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::io::AsyncWriteExt;

const NO_INTERNET: &str = "The download stopped. Check your internet connection and try again.";
const NO_SPACE: &str = "Couldn't save the download. Make sure your computer has free space, then try again.";
const DAMAGED: &str = "The download was damaged on the way. Please try again.";

/// Installs in flight, so a second click (or the auto-updater) doesn't start another.
#[derive(Default)]
pub struct Busy(pub Mutex<HashSet<String>>);

#[derive(Serialize, Clone)]
struct Progress<'a> {
    slug: &'a str,
    stage: &'a str,
    received: u64,
    total: Option<u64>,
    message: Option<String>,
}

fn emit(app: &AppHandle, slug: &str, stage: &str, received: u64, total: Option<u64>, message: Option<String>) {
    app.emit("install-progress", Progress { slug, stage, received, total, message }).ok();
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.len() < 80 && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn tools_root(app: &AppHandle) -> PathBuf {
    #[cfg(target_os = "macos")]
    let root = app.path().home_dir().expect("home").join("Applications").join("Toolbay");
    #[cfg(not(target_os = "macos"))]
    let root = app.path().local_data_dir().expect("local data").join("Programs").join("Toolbay Tools");
    std::fs::create_dir_all(&root).ok();
    root
}

/// The tool's own app data dir (Tauri's app_data_dir for its identifier), so files
/// the hub downloads are exactly where the tool looks when it runs standalone.
fn tool_data_dir(app: &AppHandle, r: &Resolved) -> PathBuf {
    match r.manifest.identifier.as_deref() {
        Some(id)
            if !id.is_empty()
                && !id.contains("..")
                && id.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)) =>
        {
            app.path().data_dir().expect("data dir").join(id)
        }
        _ => store::data_dir(app).join("tool-data").join(sanitize(&r.manifest.id)),
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || ".-_+ ".contains(c) { c } else { '_' })
        .collect()
}

fn safe_rel(p: &str) -> bool {
    !p.is_empty() && !p.contains("..") && !Path::new(p).is_absolute() && !p.starts_with(['/', '\\'])
}

fn local_path(url: &str) -> Option<PathBuf> {
    let p = url.strip_prefix("file://")?.trim_start_matches('/');
    Some(if cfg!(windows) { PathBuf::from(p) } else { PathBuf::from(format!("/{p}")) })
}

// ---------------------------------------------------------------- install

/// Look up what installing a tool would fetch (size, version) without installing.
#[tauri::command]
pub async fn tool_resolve(app: AppHandle, slug: String, source: String) -> Result<Resolved, String> {
    let source = store::setting(&app, &format!("source:{slug}")).unwrap_or(source);
    manifest::resolve(&source).await
}

#[tauri::command]
pub async fn tool_install(app: AppHandle, slug: String, source: String) -> Result<InstalledTool, String> {
    if !valid_slug(&slug) {
        return Err("Something went wrong. Please restart Toolbay and try again.".into());
    }
    // A dev/test override: settings "source:<slug>" = manifest URL or owner/repo.
    let source = store::setting(&app, &format!("source:{slug}")).unwrap_or(source);
    if !app.state::<Busy>().0.lock().unwrap().insert(slug.clone()) {
        return Err("Already installing. One moment…".into());
    }
    let result = install(&app, &slug, &source).await;
    app.state::<Busy>().0.lock().unwrap().remove(&slug);
    match &result {
        Ok(_) => emit(&app, &slug, "done", 0, None, None),
        Err(e) => emit(&app, &slug, "error", 0, None, Some(e.clone())),
    }
    result
}

async fn install(app: &AppHandle, slug: &str, source: &str) -> Result<InstalledTool, String> {
    if is_running(app, slug) {
        return Err("Close the tool first, then try again.".into());
    }
    emit(app, slug, "preparing", 0, None, None);
    let r = manifest::resolve(source).await?;
    let kind = r.entry.kind.to_ascii_lowercase();
    let data_dir = tool_data_dir(app, &r);

    // Files still to fetch (skip ones already in place, e.g. a model kept from standalone use).
    let mut todo: Vec<&FileEntry> = vec![];
    for f in &r.manifest.files {
        if !safe_rel(&f.path) {
            return Err("This tool's download list looks wrong, so nothing was installed.".into());
        }
        let have = std::fs::metadata(data_dir.join(&f.path)).map(|m| m.len()).ok();
        if have.is_none() || (f.size.is_some() && have != f.size) {
            todo.push(f);
        }
    }
    let total: Option<u64> = r.entry.size.and_then(|s| todo.iter().try_fold(s, |acc, f| f.size.map(|z| acc + z)));

    let staging = store::data_dir(app).join("downloads").join(sanitize(slug));
    std::fs::create_dir_all(&staging).map_err(|_| NO_SPACE.to_string())?;
    let file_name = r
        .entry
        .asset
        .clone()
        .or_else(|| r.download_url.rsplit(['/', '\\']).next().map(String::from))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "download.bin".into());
    let package = staging.join(sanitize(&file_name));

    let mut received = 0u64;
    download(app, slug, &r.download_url, &package, r.entry.sha256.as_deref(), &mut received, total).await?;
    for f in &todo {
        let dest = data_dir.join(&f.path);
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p).map_err(|_| NO_SPACE.to_string())?;
        }
        let part = dest.with_extension("part");
        download(app, slug, &f.url, &part, f.sha256.as_deref(), &mut received, total).await?;
        std::fs::rename(&part, &dest).map_err(|_| NO_SPACE.to_string())?;
    }

    emit(app, slug, "installing", received, total, None);
    let dir = tools_root(app).join(sanitize(&r.manifest.id));
    let (exe, uninstaller) = match kind.as_str() {
        "nsis" => install_nsis(&package, &dir, &r).await?,
        "zip" => {
            if dir.exists() {
                std::fs::remove_dir_all(&dir).ok();
            }
            let (p, d) = (package.clone(), dir.clone());
            tokio::task::spawn_blocking(move || unzip(&p, &d)).await.map_err(|e| e.to_string())??;
            (program_in(&dir, r.entry.exe.as_deref())?, None)
        }
        "exe" => {
            std::fs::create_dir_all(&dir).map_err(|_| NO_SPACE.to_string())?;
            let name = r.entry.exe.clone().filter(|e| safe_rel(e)).unwrap_or(file_name.clone());
            let target = dir.join(name);
            std::fs::copy(&package, &target).map_err(|_| NO_SPACE.to_string())?;
            (target, None)
        }
        "dmg" => (install_dmg(&package, &tools_root(app), r.entry.app.as_deref()).await?, None),
        _ => return Err("This tool needs a newer version of Toolbay. Update Toolbay from Settings, then try again.".into()),
    };
    std::fs::remove_dir_all(&staging).ok();

    let previous = store::library(app).into_iter().find(|t| t.slug == slug);
    let entry = InstalledTool {
        slug: slug.into(),
        name: r.manifest.name.clone(),
        version: r.manifest.version.clone(),
        repo: if r.manifest.repo.is_empty() { source.into() } else { r.manifest.repo.clone() },
        kind,
        // The folder it actually lives in (an installer may reuse an earlier install location).
        dir: if cfg!(target_os = "macos") {
            exe.to_string_lossy().into()
        } else if exe.starts_with(&dir) {
            dir.to_string_lossy().into()
        } else {
            exe.parent().unwrap_or(&dir).to_string_lossy().into()
        },
        exe: exe.to_string_lossy().into(),
        launch_args: r.manifest.launch.args.clone(),
        quit_args: r.manifest.control.quit.clone(),
        uninstaller: uninstaller.map(|u| u.to_string_lossy().into()),
        files: r.manifest.files.iter().map(|f| data_dir.join(&f.path).to_string_lossy().into()).collect(),
        shortcut: previous.and_then(|p| p.shortcut),
        identifier: r.manifest.identifier.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    store::upsert(app, entry.clone())?;
    app.emit("library-changed", ()).ok();
    crate::rebuild_tray(app);
    Ok(entry)
}

async fn download(
    app: &AppHandle,
    slug: &str,
    url: &str,
    to: &Path,
    sha256: Option<&str>,
    received: &mut u64,
    total: Option<u64>,
) -> Result<(), String> {
    let mut hasher = Sha256::new();
    if let Some(src) = local_path(url) {
        // Local packages, for development and tests.
        let bytes = std::fs::read(&src).map_err(|_| NO_INTERNET.to_string())?;
        hasher.update(&bytes);
        std::fs::write(to, &bytes).map_err(|_| NO_SPACE.to_string())?;
        *received += bytes.len() as u64;
        emit(app, slug, "downloading", *received, total, None);
    } else {
        if !url.starts_with("https://") {
            return Err("This tool's download isn't secure, so Toolbay didn't install it.".into());
        }
        let res = reqwest::Client::builder()
            .user_agent(concat!("ToolbayDesktop/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?
            .get(url)
            .send()
            .await
            .map_err(|_| NO_INTERNET.to_string())?;
        if !res.status().is_success() {
            return Err("The download isn't available right now. Please try again later.".into());
        }
        let mut out = tokio::fs::File::create(to).await.map_err(|_| NO_SPACE.to_string())?;
        let mut stream = res.bytes_stream();
        let mut last = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| NO_INTERNET.to_string())?;
            hasher.update(&chunk);
            out.write_all(&chunk).await.map_err(|_| NO_SPACE.to_string())?;
            *received += chunk.len() as u64;
            if *received - last > 512 * 1024 {
                last = *received;
                emit(app, slug, "downloading", *received, total, None);
            }
        }
        out.flush().await.map_err(|_| NO_SPACE.to_string())?;
        emit(app, slug, "downloading", *received, total, None);
    }
    if let Some(want) = sha256.filter(|s| !s.is_empty()) {
        if hex::encode(hasher.finalize()) != want.to_ascii_lowercase() {
            std::fs::remove_file(to).ok();
            return Err(DAMAGED.into());
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn install_nsis(package: &Path, dir: &Path, r: &Resolved) -> Result<(PathBuf, Option<PathBuf>), String> {
    let mut args = if r.entry.silent_args.is_empty() { vec!["/S".to_string()] } else { r.entry.silent_args.clone() };
    // NSIS wants /D= last and unquoted.
    args.push(format!("/D={}", dir.display()));
    let status = tokio::process::Command::new(package)
        .args(&args)
        .creation_flags(0x0800_0000)
        .status()
        .await
        .map_err(|_| "Windows blocked the installer. Try again; if it keeps happening, restart your computer.".to_string())?;
    if !status.success() {
        return Err("The installer didn't finish. Please try again.".into());
    }
    // Where did it actually go? Our folder, unless the tool was installed before and its
    // installer reused that folder: then its manifest default or the Windows uninstall entry.
    let mut candidates = vec![dir.to_path_buf()];
    if let Some(d) = r.entry.install_dir.as_deref() {
        candidates.push(PathBuf::from(expand_env(d)));
    }
    candidates.extend(registry_install_dirs(&r.manifest.name));
    for c in candidates {
        let Some(exe) = r.entry.exe.as_deref().filter(|e| safe_rel(e)).map(|e| c.join(e)).filter(|p| p.exists()) else {
            continue;
        };
        let named = r.entry.uninstaller.clone().filter(|u| safe_rel(u));
        let un = named
            .iter()
            .map(String::as_str)
            .chain(["uninstall.exe", "Uninstall.exe"])
            .map(|n| c.join(n))
            .find(|p| p.exists());
        return Ok((exe, un));
    }
    Ok((program_in(dir, r.entry.exe.as_deref())?, None))
}

/// "%LOCALAPPDATA%\Toolbay Flow" -> "C:\Users\me\AppData\Local\Toolbay Flow"
#[cfg(windows)]
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                out.push_str(&std::env::var(name).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Install folders Windows has on record for a program with this display name (per-user first).
#[cfg(windows)]
fn registry_install_dirs(name: &str) -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let mut out = vec![];
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let Ok(root) = RegKey::predef(hive).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall") else {
            continue;
        };
        for key in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&key) else { continue };
            let display: String = k.get_value("DisplayName").unwrap_or_default();
            if !display.eq_ignore_ascii_case(name) {
                continue;
            }
            if let Ok(loc) = k.get_value::<String, _>("InstallLocation") {
                if !loc.trim().is_empty() {
                    out.push(PathBuf::from(loc.trim().trim_matches('"')));
                }
            }
            if let Ok(un) = k.get_value::<String, _>("UninstallString") {
                let path = un.trim().trim_start_matches('"');
                let path = path.split('"').next().unwrap_or(path);
                if let Some(parent) = Path::new(path).parent() {
                    out.push(parent.to_path_buf());
                }
            }
        }
    }
    out
}

#[cfg(not(windows))]
async fn install_nsis(_package: &Path, _dir: &Path, _r: &Resolved) -> Result<(PathBuf, Option<PathBuf>), String> {
    Err("This installer only runs on Windows.".into())
}

/// The program to start: the manifest's `exe` if present, else the shallowest .exe/.app.
fn program_in(dir: &Path, exe: Option<&str>) -> Result<PathBuf, String> {
    if let Some(e) = exe.filter(|e| safe_rel(e)) {
        let p = dir.join(e);
        if p.exists() {
            return Ok(p);
        }
    }
    let mut found: Vec<PathBuf> = vec![];
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if (name.ends_with(".exe") || name.ends_with(".app")) && !name.contains("unins") {
                found.push(p);
            } else if p.is_dir() && depth < 3 {
                stack.push((p, depth + 1));
            }
        }
    }
    found.sort_by_key(|p| p.components().count());
    found
        .into_iter()
        .next()
        .ok_or_else(|| "Installed, but couldn't find the program to open. Please reinstall.".into())
}

pub(crate) fn unzip(file: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(file).map_err(|_| NO_SPACE.to_string())?;
    let mut zip = zip::ZipArchive::new(f).map_err(|_| DAMAGED.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| DAMAGED.to_string())?;
        // enclosed_name rejects absolute paths and ../ traversal.
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|_| NO_SPACE.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|_| NO_SPACE.to_string())?;
        }
        let mut w = std::fs::File::create(&out).map_err(|_| NO_SPACE.to_string())?;
        std::io::copy(&mut entry, &mut w).map_err(|_| NO_SPACE.to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode)).ok();
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn install_dmg(file: &Path, root: &Path, app_name: Option<&str>) -> Result<PathBuf, String> {
    let mount = std::env::temp_dir().join(format!("toolbay-mnt-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&mount).map_err(|_| NO_SPACE.to_string())?;
    let ok = tokio::process::Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(file)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return Err(DAMAGED.into());
    }
    let src = std::fs::read_dir(&mount).into_iter().flatten().flatten().map(|e| e.path()).find(|p| match app_name {
        Some(n) => p.file_name().is_some_and(|f| f == n),
        None => p.extension().is_some_and(|x| x == "app"),
    });
    let result = match src {
        Some(src) => {
            let dest = root.join(src.file_name().unwrap());
            if dest.exists() {
                std::fs::remove_dir_all(&dest).ok();
            }
            let copied = tokio::process::Command::new("ditto")
                .arg(&src)
                .arg(&dest)
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if copied { Ok(dest) } else { Err(NO_SPACE.to_string()) }
        }
        None => Err("Installed, but couldn't find the app in the download. Please reinstall.".into()),
    };
    let _ = tokio::process::Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status().await;
    std::fs::remove_dir_all(&mount).ok();
    result
}

#[cfg(not(target_os = "macos"))]
async fn install_dmg(_file: &Path, _root: &Path, _app: Option<&str>) -> Result<PathBuf, String> {
    Err("This installer only runs on a Mac.".into())
}

// ---------------------------------------------------------------- launch / stop

fn find(app: &AppHandle, slug: &str) -> Result<InstalledTool, String> {
    store::library(app)
        .into_iter()
        .find(|t| t.slug == slug)
        .ok_or_else(|| "This tool isn't installed. Click Install first.".to_string())
}

fn is_running(app: &AppHandle, slug: &str) -> bool {
    crate::running::is_running(app, slug)
}

/// Show a plain-language message in the hub (used from the tray, where there's no button to fail on).
pub fn notify_error(msg: &str) -> Result<(), String> {
    if let Some(app) = APP.get() {
        app.emit("auth-error", msg).ok();
    }
    Ok(())
}

pub static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

/// Start the tool as its own app, signed in as the hub's user.
#[tauri::command]
pub fn tool_launch(app: AppHandle, slug: String) -> Result<(), String> {
    let tool = find(&app, &slug)?;
    let exe = PathBuf::from(&tool.exe);
    if !exe.exists() {
        return Err(format!("{} seems to be missing. Click Reinstall on its page.", tool.name));
    }
    #[cfg(target_os = "macos")]
    let mut cmd = {
        // `open -W` waits until the app quits, so we know when it stops running.
        let mut c = std::process::Command::new("open");
        c.arg("-W").arg("-a").arg(&exe);
        if !tool.launch_args.is_empty() {
            c.arg("--args").args(&tool.launch_args);
        }
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut cmd = {
        let mut c = std::process::Command::new(&exe);
        c.args(&tool.launch_args).current_dir(exe.parent().unwrap_or(Path::new(".")));
        c
    };
    cmd.env("TOOLBAY_HUB", "1");
    match crate::api::token() {
        Some(t) => cmd.env("TOOLBAY_TOKEN", t),
        None => cmd.env_remove("TOOLBAY_TOKEN"),
    };
    cmd.spawn()
        .map_err(|_| format!("{} didn't start. Try again, or reinstall it from its page.", tool.name))?;
    Ok(())
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ---------------------------------------------------------------- uninstall

#[tauri::command]
pub async fn tool_uninstall(app: AppHandle, slug: String) -> Result<(), String> {
    let tool = find(&app, &slug)?;
    if is_running(&app, &slug) {
        crate::running::close(&app, &slug).await;
    }
    if let Some(un) = tool.uninstaller.as_ref().filter(|u| Path::new(u).exists()) {
        #[cfg(windows)]
        let _ = tokio::process::Command::new(un).arg("/S").creation_flags(0x0800_0000).status().await;
        #[cfg(not(windows))]
        let _ = un;
        // NSIS uninstallers hand off to a temp copy and return early; give it a moment.
        for _ in 0..40 {
            if !Path::new(&tool.exe).exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    let dir = PathBuf::from(&tool.dir);
    if dir.starts_with(tools_root(&app)) && dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|_| format!("Couldn't remove {}. Close it if it's open, then try again.", tool.name))?;
    }
    for f in &tool.files {
        std::fs::remove_file(f).ok();
        // Tidy up folders the files left empty (stops at the first non-empty one).
        let mut dir = Path::new(f).parent();
        while let Some(d) = dir {
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    if let Some(s) = &tool.shortcut {
        std::fs::remove_file(s).ok();
    }
    let mut lib = store::library(&app);
    lib.retain(|t| t.slug != slug);
    store::save_library(&app, &lib)?;
    app.emit("library-changed", ()).ok();
    Ok(())
}

// ---------------------------------------------------------------- desktop shortcut

/// Windows: a .lnk on the desktop that opens the tool through Toolbay
/// (toolbay://launch/<slug>), so it starts signed in. macOS: an alias to the app.
#[tauri::command]
pub fn tool_shortcut(app: AppHandle, slug: String) -> Result<String, String> {
    let mut tool = find(&app, &slug)?;
    let desktop = app.path().desktop_dir().map_err(|_| "Couldn't find your desktop folder.".to_string())?;
    #[cfg(windows)]
    let path = {
        let link = desktop.join(format!("{}.lnk", sanitize(&tool.name)));
        let hub = std::env::current_exe().map_err(|e| e.to_string())?;
        let q = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.Arguments='toolbay://launch/{}';$s.IconLocation='{},0';$s.Description='{}';$s.Save()",
            q(&link),
            q(&hub),
            slug,
            q(Path::new(&tool.exe)),
            tool.name.replace('\'', "''")
        );
        let ok = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok || !link.exists() {
            return Err("Couldn't add the shortcut. Please try again.".into());
        }
        link
    };
    #[cfg(unix)]
    let path = {
        let link = desktop.join(Path::new(&tool.exe).file_name().unwrap_or_default());
        std::fs::remove_file(&link).ok();
        std::os::unix::fs::symlink(&tool.exe, &link).map_err(|_| "Couldn't add the shortcut. Please try again.".to_string())?;
        link
    };
    tool.shortcut = Some(path.to_string_lossy().into());
    store::upsert(&app, tool)?;
    app.emit("library-changed", ()).ok();
    Ok(path.to_string_lossy().into())
}

#[tauri::command]
pub fn tool_reveal(app: AppHandle, slug: String) -> Result<(), String> {
    let tool = find(&app, &slug)?;
    app.opener().reveal_item_in_dir(tool.exe).map_err(|_| "Couldn't open the folder.".to_string())
}

// ---------------------------------------------------------------- silent updates

fn source_for(app: &AppHandle, t: &InstalledTool) -> String {
    store::setting(app, &format!("source:{}", t.slug)).unwrap_or_else(|| t.repo.clone())
}

pub async fn update_one(app: &AppHandle, slug: &str) {
    if store::setting(app, "autoUpdate").as_deref() == Some("false") || is_running(app, slug) {
        return;
    }
    let Some(t) = store::library(app).into_iter().find(|t| t.slug == slug) else { return };
    let source = source_for(app, &t);
    let Ok(r) = manifest::resolve(&source).await else { return };
    if !manifest::newer(&r.manifest.version, &t.version) {
        return;
    }
    if tool_install(app.clone(), slug.to_string(), source).await.is_ok() {
        app.emit("tool-updated", serde_json::json!({ "slug": slug, "name": t.name, "version": r.manifest.version }))
            .ok();
    }
}

/// Check every installed tool shortly after start, then every 6 hours. Tools that are
/// open are skipped and updated when they close.
pub fn start_auto_update(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            for t in store::library(&app) {
                update_one(&app, &t.slug).await;
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}
