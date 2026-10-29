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

/// Toolbay updating itself: closing the window (or a shortcut launch timing out) waits for it,
/// like it waits for a tool install, so the app is never killed mid-replace.
#[tauri::command]
pub fn hub_busy(app: AppHandle, on: bool) {
    let busy = app.state::<Busy>();
    let mut b = busy.0.lock().unwrap();
    if on {
        b.insert("__hub__".into());
    } else {
        b.remove("__hub__");
    }
}

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
    let root = mac_root(app);
    #[cfg(not(target_os = "macos"))]
    let root = app.path().local_data_dir().expect("local data").join("Programs").join("Toolbay Tools");
    std::fs::create_dir_all(&root).ok();
    root
}

/// A folder Toolbay installed tools into. On a Mac that's either Toolbay folder: which one
/// mac_root picks can change (e.g. /Applications stopped being writable), and an uninstall
/// must still remove a tool from the other one.
fn in_tools_root(app: &AppHandle, dir: &Path) -> bool {
    #[cfg(target_os = "macos")]
    if let Ok(home) = app.path().home_dir() {
        if dir.starts_with("/Applications/Toolbay") || dir.starts_with(home.join("Applications").join("Toolbay")) {
            return true;
        }
    }
    dir.starts_with(tools_root(app))
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

/// Install from the hub's Install button (or the first-run prompt); counted on toolbay.ai.
#[tauri::command]
pub async fn tool_install(app: AppHandle, slug: String, source: String) -> Result<InstalledTool, String> {
    let entry = install_tool(app.clone(), slug.clone(), source).await?;
    // Funnel count for /admin/sources ("Tool installs"); no account needed, deduped by the site.
    let platform = if cfg!(target_os = "macos") { "mac" } else { "win" };
    let body = serde_json::json!({ "event": "tool_install", "toolSlug": slug, "platform": platform }).to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::api::send(&app, "POST", "/api/v1/desktop/events", Some(body)).await;
    });
    Ok(entry)
}

/// Install or update a tool (shared by the Install button and quiet auto-updates).
pub async fn install_tool(app: AppHandle, slug: String, source: String) -> Result<InstalledTool, String> {
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
    if in_use(app, slug) {
        return Err("Close the tool first, then try again.".into());
    }
    emit(app, slug, "preparing", 0, None, None);
    let r = manifest::resolve(source).await?;
    let kind = r.entry.kind.to_ascii_lowercase();
    let data_dir = tool_data_dir(app, &r);

    // Files still to fetch (skip ones already in place, e.g. a model kept from standalone use).
    let mut todo: Vec<&FileEntry> = vec![];
    // CI smoke test (TOOLBAY_CI_SKIP_FILES): install the program only, not its large models.
    let skip_files = std::env::var_os("TOOLBAY_CI_SKIP_FILES").is_some();
    for f in r.manifest.files.iter().filter(|_| !skip_files) {
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
    // Updates go where the tool already lives, so there's never a second copy.
    let previous_dir = store::library(app)
        .into_iter()
        .find(|t| t.slug == slug)
        .map(|t| PathBuf::from(t.dir))
        .filter(|d| d.is_dir() && cfg!(windows));
    let dir = previous_dir.unwrap_or_else(|| tools_root(app).join(sanitize(&r.manifest.id)));
    // It may have been opened while the download ran: never replace files a running app uses.
    if in_use(app, slug) || crate::running::path_in_use(&dir) {
        std::fs::remove_dir_all(&staging).ok();
        return Err("Close the tool first, then try again.".into());
    }
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
        "dmg" => {
            let previous_exe = store::library(app).into_iter().find(|t| t.slug == slug).map(|t| PathBuf::from(t.exe));
            let id = r.manifest.identifier.as_deref();
            (install_dmg(&package, &tools_root(app), r.entry.app.as_deref(), id, previous_exe).await?, None)
        }
        _ => return Err("This tool needs a newer version of Toolbay. Update Toolbay from Settings, then try again.".into()),
    };
    std::fs::remove_dir_all(&staging).ok();
    // Toolbay downloads with its own HTTP client, so nothing it installs is marked "downloaded from
    // the internet". Clear the flag anyway, so macOS never calls a tool damaged or asks to confirm.
    #[cfg(target_os = "macos")]
    {
        let target = if kind == "dmg" { exe.clone() } else { dir.clone() };
        let _ = tokio::process::Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&target).status().await;
    }

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
        quit_args: r.manifest.control_args("quit"),
        show_args: r.manifest.control_args("show"),
        hide_args: r.manifest.control_args("hide"),
        state_file: r.manifest.launch.state_file.clone().filter(|f| safe_rel(f)),
        uninstaller: uninstaller.map(|u| u.to_string_lossy().into()),
        files: r.manifest.files.iter().map(|f| data_dir.join(&f.path).to_string_lossy().into()).collect(),
        shortcut: previous.and_then(|p| p.shortcut),
        identifier: r.manifest.identifier.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    store::upsert(app, entry.clone())?;
    // A tool's own installer may have replaced our desktop shortcut (which opens it signed in
    // through Toolbay); put ours back if the user had one.
    if entry.shortcut.is_some() {
        let _ = tool_shortcut(app.clone(), slug.to_string());
    }
    app.emit("library-changed", ()).ok();
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
    let args = if r.entry.silent_args.is_empty() { vec!["/S".to_string()] } else { r.entry.silent_args.clone() };
    // NSIS wants /D= last and unquoted, even with spaces ("Toolbay Tools"); .arg() would quote it
    // and NSIS would silently ignore it. raw_arg passes it through as-is.
    let status = tokio::process::Command::new(package)
        .args(&args)
        .raw_arg(format!("/D={}", dir.display()))
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

/// The bundle identifier of an installed .app, read from its Info.plist.
#[cfg(target_os = "macos")]
fn bundle_id(app: &Path) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/defaults")
        .arg("read")
        .arg(app.join("Contents").join("Info"))
        .arg("CFBundleIdentifier")
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Every Mac tool lives in one folder, /Applications/Toolbay (or ~/Applications/Toolbay when
/// /Applications isn't writable), and nowhere else: two copies meant two Dock icons, and macOS
/// permissions granted to one didn't apply to the other.
#[cfg(target_os = "macos")]
fn mac_root(app: &AppHandle) -> PathBuf {
    let sys = PathBuf::from("/Applications/Toolbay");
    if std::fs::create_dir_all(&sys).is_ok() && dir_writable(&sys) {
        return sys;
    }
    app.path().home_dir().expect("home").join("Applications").join("Toolbay")
}

#[cfg(target_os = "macos")]
fn dir_writable(d: &Path) -> bool {
    let probe = d.join(".toolbay-write-test");
    let ok = std::fs::File::create(&probe).is_ok();
    std::fs::remove_file(&probe).ok();
    ok
}

/// Other copies of the same app (same bundle id) in the usual places, excluding `target`.
#[cfg(target_os = "macos")]
fn mac_copies(target: &Path, name: &std::ffi::OsStr, id: Option<&str>, previous: Option<PathBuf>) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut places = vec![
        PathBuf::from("/Applications").join(name),
        PathBuf::from("/Applications/Toolbay").join(name),
        home.join("Applications").join(name),
        home.join("Applications").join("Toolbay").join(name),
    ];
    places.extend(previous);
    let Some(id) = id else { return vec![] };
    let mut out: Vec<PathBuf> = Vec::new();
    for p in places {
        if p != target && p.is_dir() && !out.contains(&p) && bundle_id(&p).as_deref() == Some(id) {
            out.push(p);
        }
    }
    out
}

/// Where a Mac tool goes, plus the stray copies to clean up. A stray copy Toolbay can move is
/// moved into place first (the Dock and macOS keep treating it as the same app).
#[cfg(target_os = "macos")]
fn mac_target(root: &Path, name: &std::ffi::OsStr, id: Option<&str>, previous: Option<PathBuf>) -> (PathBuf, Vec<PathBuf>) {
    let target = root.join(name);
    let mut others = mac_copies(&target, name, id, previous);
    // Copies something is running from are left alone (moving or deleting an open app breaks it).
    others.retain(|p| !crate::running::path_in_use(p));
    if !target.exists() {
        if let Some(i) = others.iter().position(|p| std::fs::rename(p, &target).is_ok()) {
            others.remove(i);
        }
    }
    (target, others)
}

/// Remove stray copies. Ones owned by the system (an old .pkg install) need an admin password,
/// so macOS asks once, saying why. Only ever removes `.app` bundles in Applications folders.
#[cfg(target_os = "macos")]
fn remove_copies(app_name: &str, copies: Vec<PathBuf>) {
    let mut stubborn = vec![];
    for c in copies {
        // Never delete a copy that any process is running from.
        if crate::running::path_in_use(&c) {
            continue;
        }
        if std::fs::remove_dir_all(&c).is_err() && c.exists() {
            stubborn.push(c);
        }
    }
    let safe: Vec<String> = stubborn
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "app") && p.to_string_lossy().contains("/Applications/"))
        .map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "'\\''")))
        .collect();
    if safe.is_empty() {
        return;
    }
    let cmd = format!("/bin/rm -rf {}", safe.join(" "));
    let prompt = format!("Toolbay found an older copy of {app_name} it can't update. Remove it so you only have one?");
    let script = format!(
        "do shell script \"{}\" with administrator privileges with prompt \"{}\"",
        cmd.replace('\\', "\\\\").replace('"', "\\\""),
        prompt.replace('"', "'")
    );
    let _ = std::process::Command::new("/usr/bin/osascript").arg("-e").arg(script).status();
}

#[cfg(target_os = "macos")]
async fn install_dmg(
    file: &Path,
    root: &Path,
    app_name: Option<&str>,
    id: Option<&str>,
    previous: Option<PathBuf>,
) -> Result<PathBuf, String> {
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
    let src = app_in_volume(&mount, app_name);
    let result = match src {
        Some(src) if crate::running::path_in_use(&root.join(src.file_name().unwrap())) => {
            Err("Close the tool first, then try again.".to_string())
        }
        Some(src) => {
            let (dest, others) = mac_target(root, src.file_name().unwrap(), id, previous);
            // Update the bundle in place (same folder), so the Dock and macOS keep treating it as
            // the same app; a fresh copy only when there's none yet or the in-place sync fails.
            let mut copied = false;
            if dest.is_dir() {
                copied = tokio::process::Command::new("/usr/bin/rsync")
                    .arg("-a")
                    .arg("--delete")
                    .arg(format!("{}/", src.display()))
                    .arg(format!("{}/", dest.display()))
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false);
            }
            if !copied {
                if dest.exists() {
                    std::fs::remove_dir_all(&dest).ok();
                }
                copied = tokio::process::Command::new("ditto")
                    .arg(&src)
                    .arg(&dest)
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false);
            }
            if copied {
                let name = dest.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                remove_copies(&name, others);
                // Tell Launch Services this is the copy to open.
                let _ = tokio::process::Command::new(
                    "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
                )
                .arg("-f")
                .arg(&dest)
                .status()
                .await;
                Ok(dest)
            } else {
                Err(NO_SPACE.to_string())
            }
        }
        None => Err("Installed, but couldn't find the app in the download. Please reinstall.".into()),
    };
    let _ = tokio::process::Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status().await;
    std::fs::remove_dir_all(&mount).ok();
    result
}

/// The .app to copy out of a mounted DMG. Prefers the manifest's `app`, matched without case
/// (Mac volumes are case-insensitive, and a manifest saying "Bedrock.app" for a bundle named
/// "bedrock.app" must still install), then the only .app at the top, then the shallowest one
/// within two folders. Skips the "Applications" shortcut most DMGs carry.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn app_in_volume(mount: &Path, app_name: Option<&str>) -> Option<PathBuf> {
    let is_app = |p: &Path| {
        p.extension().is_some_and(|x| x.eq_ignore_ascii_case("app"))
            && !p.is_symlink()
            && p.is_dir()
    };
    let mut apps: Vec<PathBuf> = vec![];
    let mut stack = vec![(mount.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if is_app(&p) {
                apps.push(p);
            } else if depth < 2 && p.is_dir() && !p.is_symlink() {
                stack.push((p, depth + 1));
            }
        }
    }
    apps.sort_by_key(|p| p.components().count());
    if let Some(n) = app_name {
        let n = n.to_lowercase();
        if let Some(p) = apps.iter().find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().to_lowercase() == n)) {
            return Some(p.clone());
        }
    }
    let top = mount.components().count() + 1;
    let at_top: Vec<&PathBuf> = apps.iter().filter(|p| p.components().count() == top).collect();
    if at_top.len() == 1 {
        return Some(at_top[0].clone());
    }
    apps.into_iter().next()
}

#[cfg(not(target_os = "macos"))]
async fn install_dmg(
    _file: &Path,
    _root: &Path,
    _app: Option<&str>,
    _id: Option<&str>,
    _previous: Option<PathBuf>,
) -> Result<PathBuf, String> {
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

/// Running, judged by every process on the machine (not only the path the library recorded):
/// anything running from the tool's program or its folder counts.
fn in_use(app: &AppHandle, slug: &str) -> bool {
    if is_running(app, slug) {
        return true;
    }
    store::library(app)
        .into_iter()
        .find(|t| t.slug == slug)
        .is_some_and(|t| crate::running::path_in_use(Path::new(&t.exe)) || crate::running::path_in_use(Path::new(&t.dir)))
}

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
    // Count the open on toolbay.ai (safe to repeat; it's what credits a friend's invite).
    if crate::api::token().is_some() && valid_slug(&slug) && !slug.starts_with("p-") {
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::api::send(&app2, "POST", &format!("/api/v1/me/tools/{slug}/opened"), Some("{}".into())).await;
        });
    }
    Ok(())
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ---------------------------------------------------------------- uninstall

#[tauri::command]
pub async fn tool_uninstall(app: AppHandle, slug: String) -> Result<(), String> {
    let tool = find(&app, &slug)?;
    if is_running(&app, &slug) && !crate::running::close(&app, &slug, false).await {
        return Err(format!("{} didn't close. Close it, then try again.", tool.name));
    }
    if in_use(&app, &slug) {
        return Err(format!("Close {} first, then try again.", tool.name));
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
    if in_tools_root(&app, &dir) && dir.exists() {
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
    {
        let _guard = store::LIBRARY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut lib = store::library(&app);
        lib.retain(|t| t.slug != slug);
        store::save_library(&app, &lib)?;
    }
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

/// Tools being updated right now, so "Update now" and the update that runs when a tool
/// closes never install the same tool twice at once.
static UPDATING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn begin_update(slug: &str) -> bool {
    let mut u = UPDATING.lock().unwrap();
    if u.iter().any(|s| s == slug) {
        return false;
    }
    u.push(slug.to_string());
    true
}

fn end_update(slug: &str) {
    UPDATING.lock().unwrap().retain(|s| s != slug);
}

fn pending_key(slug: &str) -> String {
    format!("pendingUpdate:{slug}")
}

/// Look for a newer version and, with auto-update on (the default), install it right away if the
/// tool isn't running. A running tool is never closed for an update: the update waits and installs
/// once it has fully exited (or when the person clicks "Update"). With auto-update off, the hub
/// shows an "Update" button instead.
pub async fn update_one(app: &AppHandle, slug: &str) {
    let Some(t) = store::library(app).into_iter().find(|t| t.slug == slug) else { return };
    let source = source_for(app, &t);
    let Ok(r) = manifest::resolve(&source).await else { return };
    if !manifest::newer(&r.manifest.version, &t.version) {
        store::settings_set(app.clone(), pending_key(slug), String::new()).ok();
        return;
    }
    let auto = store::setting(app, "autoUpdate").as_deref() != Some("false");
    if auto && !in_use(app, slug) {
        let _ = update_now(app, slug).await;
        return;
    }
    let was = store::setting(app, &pending_key(slug)).unwrap_or_default();
    store::settings_set(app.clone(), pending_key(slug), r.manifest.version.clone()).ok();
    if was != r.manifest.version {
        app.emit("tool-update-ready", serde_json::json!({ "slug": slug, "name": t.name, "version": r.manifest.version }))
            .ok();
    }
}

/// Versions waiting to be installed, by slug (tools that were open when the update was found).
#[tauri::command]
pub fn tool_pending_updates(app: AppHandle) -> std::collections::HashMap<String, String> {
    store::library(&app)
        .into_iter()
        .filter_map(|t| {
            let v = store::setting(&app, &pending_key(&t.slug)).filter(|v| !v.is_empty())?;
            manifest::newer(&v, &t.version).then_some((t.slug, v))
        })
        .collect()
}

/// "Update now" (the person clicked Update): ask the tool to quit if it's open, install the new
/// version, and open it again. It's never force-quit; if it won't close, nothing is touched.
#[tauri::command]
pub async fn tool_update_now(app: AppHandle, slug: String) -> Result<InstalledTool, String> {
    update_now(&app, &slug).await
}

async fn update_now(app: &AppHandle, slug: &str) -> Result<InstalledTool, String> {
    let t = find(app, slug)?;
    if !begin_update(slug) {
        return Err(format!("{} is already updating.", t.name));
    }
    // The hub shows a big progress bar at the top until "tool-updated" or an error.
    app.emit("tool-update-started", serde_json::json!({ "slug": slug, "name": t.name })).ok();
    let was_running = is_running(app, slug);
    if was_running && !crate::running::close(app, slug, false).await {
        end_update(slug);
        app.emit("tool-update-failed", serde_json::json!({ "slug": slug })).ok();
        return Err(format!("{} didn't close. Close it, then click Update.", t.name));
    }
    let source = source_for(app, &t);
    let result = install_tool(app.clone(), slug.to_string(), source).await;
    end_update(slug);
    match &result {
        Ok(new) => {
            store::settings_set(app.clone(), pending_key(slug), String::new()).ok();
            app.emit("tool-updated", serde_json::json!({ "slug": slug, "name": new.name, "version": new.version })).ok();
        }
        // Every failure clears the top bar, including ones install_tool returns before it starts.
        Err(_) => {
            app.emit("tool-update-failed", serde_json::json!({ "slug": slug })).ok();
        }
    }
    if was_running {
        tool_launch(app.clone(), slug.to_string()).ok();
    }
    result
}

/// Mac: move every installed tool into the one Toolbay folder and clear out duplicates, so each
/// app exists once (one Dock icon, one set of permissions). Any copy something is running from
/// (whatever its path) is left alone until it's closed. macOS asks for a password at most once
/// per stray system-owned copy.
#[cfg(target_os = "macos")]
fn consolidate_mac(app: &AppHandle) {
    let root = tools_root(app);
    for mut t in store::library(app) {
        if t.kind != "dmg" || in_use(app, &t.slug) {
            continue;
        }
        let exe = PathBuf::from(&t.exe);
        let Some(name) = exe.file_name().map(|n| n.to_os_string()) else { continue };
        let target = root.join(&name);
        if exe != target && exe.is_dir() && !target.exists() && std::fs::rename(&exe, &target).is_ok() {
            t.exe = target.to_string_lossy().into();
            t.dir = t.exe.clone();
            store::upsert(app, t.clone()).ok();
            app.emit("library-changed", ()).ok();
        }
        let installed = PathBuf::from(&t.exe);
        let copies: Vec<PathBuf> = mac_copies(&installed, &name, t.identifier.as_deref(), None)
            .into_iter()
            .filter(|c| !crate::running::path_in_use(c))
            .filter(|c| {
                // Only ever ask for a password once per copy; after that just try quietly.
                let key = format!("dupAsked:{}", c.to_string_lossy());
                if store::setting(app, &key).is_some() {
                    std::fs::remove_dir_all(c).ok();
                    return false;
                }
                store::settings_set(app.clone(), key, "1".into()).ok();
                true
            })
            .collect();
        if !copies.is_empty() {
            remove_copies(&t.name, copies);
        }
    }
}

/// Check every installed tool shortly after start, then every 30 minutes.
pub fn start_auto_update(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            #[cfg(target_os = "macos")]
            {
                let a = app.clone();
                let _ = tokio::task::spawn_blocking(move || consolidate_mac(&a)).await;
            }
            for t in store::library(&app) {
                update_one(&app, &t.slug).await;
            }
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
}

/// CI smoke test: TOOLBAY_CI_INSTALL="<slug> <source>" installs that tool through the normal
/// install path, opens it, and writes "ok" or the problem to TOOLBAY_CI_RESULT, then quits.
/// On a Mac it also fails if the installed app carries com.apple.quarantine (Gatekeeper prompt).
pub fn ci_smoke_test(app: &AppHandle) {
    let Ok(spec) = std::env::var("TOOLBAY_CI_INSTALL") else { return };
    let out = std::env::var("TOOLBAY_CI_RESULT").unwrap_or_else(|_| "toolbay-ci-result.txt".into());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result: Result<(), String> = async {
            let (slug, source) = spec.split_once(' ').ok_or("TOOLBAY_CI_INSTALL must be \"<slug> <source>\"")?;
            let t = install_tool(app.clone(), slug.into(), source.into()).await?;
            #[cfg(target_os = "macos")]
            {
                let q = std::process::Command::new("xattr").args(["-p", "com.apple.quarantine"]).arg(&t.exe).output();
                if q.map(|o| o.status.success()).unwrap_or(false) {
                    return Err(format!("{} is quarantined", t.exe));
                }
            }
            tool_launch(app.clone(), slug.into())?;
            tokio::time::sleep(Duration::from_secs(10)).await;
            if !crate::running::is_running(&app, slug) {
                return Err(format!("{} didn't stay open", t.name));
            }
            let _ = crate::running::tool_close(app.clone(), slug.into()).await;
            Ok(())
        }
        .await;
        let _ = std::fs::write(&out, match &result {
            Ok(()) => "ok".to_string(),
            Err(e) => e.clone(),
        });
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}

#[cfg(test)]
mod tests {
    use super::app_in_volume;

    fn volume(apps: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("toolbay-vol-{}", uuid::Uuid::new_v4()));
        for a in apps {
            std::fs::create_dir_all(root.join(a).join("Contents")).unwrap();
        }
        std::fs::create_dir_all(root.join(".background")).unwrap();
        root
    }

    fn name(p: Option<std::path::PathBuf>) -> String {
        p.and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned())).unwrap_or_default()
    }

    #[test]
    fn manifest_name_matches_without_case() {
        let v = volume(&["bedrock.app"]);
        assert_eq!(name(app_in_volume(&v, Some("Bedrock.app"))), "bedrock.app");
    }

    #[test]
    fn wrong_manifest_name_falls_back_to_the_only_app() {
        let v = volume(&["Cereal.app"]);
        assert_eq!(name(app_in_volume(&v, Some("Toolbay Notes.app"))), "Cereal.app");
    }

    #[test]
    fn exact_name_wins_over_other_apps() {
        let v = volume(&["Helper.app", "Toolbay Flow.app"]);
        assert_eq!(name(app_in_volume(&v, Some("Toolbay Flow.app"))), "Toolbay Flow.app");
    }

    #[test]
    fn finds_an_app_one_folder_down() {
        let v = volume(&["Bedrock/Bedrock.app"]);
        assert_eq!(name(app_in_volume(&v, Some("Bedrock.app"))), "Bedrock.app");
    }
}
