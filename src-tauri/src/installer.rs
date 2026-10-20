//! Install, launch, update and uninstall tools.
//!
//! Every tool lives in its own folder under <app data>/tools/<slug>/<version>.
//! Portable zips (the common case for open-source Windows releases) are unpacked
//! there; a .dmg on macOS is mounted and its .app copied in; a setup .exe/.msi is
//! handed to the OS installer. Updating = installing the new version, then
//! removing the old folder.
use crate::store::{self, InstalledTool};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;
use tokio::io::AsyncWriteExt;

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

#[tauri::command]
pub async fn tool_install(
    app: AppHandle,
    slug: String,
    name: String,
    version: String,
    url: String,
    launch: Option<String>,
) -> Result<InstalledTool, String> {
    if !valid_slug(&slug) {
        return Err("bad slug".into());
    }
    let result = install(&app, &slug, &name, &version, &url, launch.as_deref()).await;
    match &result {
        Ok(_) => emit(&app, &slug, "done", 0, None, None),
        Err(e) => emit(&app, &slug, "error", 0, None, Some(e.clone())),
    }
    result
}

/// `launch` is an optional catalog hint: a path inside the unpacked folder plus
/// arguments, e.g. "espanso-portable/espansod.exe launcher".
async fn install(
    app: &AppHandle,
    slug: &str,
    name: &str,
    version: &str,
    url: &str,
    launch: Option<&str>,
) -> Result<InstalledTool, String> {
    let now = chrono::Utc::now().to_rfc3339();

    // Built-in tools ship inside this app; "installing" just adds them to the library.
    if url.starts_with("builtin:") {
        let entry = InstalledTool {
            slug: slug.into(),
            name: name.into(),
            version: version.into(),
            path: String::new(),
            launch_path: None,
            launch_args: vec![],
            installed_at: now,
        };
        record(app, entry.clone())?;
        return Ok(entry);
    }

    let parsed = url::Url::parse(url).map_err(|_| "This tool has no download for your computer yet.")?;
    if parsed.scheme() != "https" {
        return Err("Downloads must use https.".into());
    }
    let file_name = parsed
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or("download.bin")
        .to_string();

    let root = store::tools_dir(app).join(slug);
    let dest = root.join(sanitize(version));
    let staging = root.join("_download");
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let file = staging.join(sanitize(&file_name));

    download(app, slug, url, &file).await?;

    let lower = file_name.to_ascii_lowercase();
    if dest.exists() {
        std::fs::remove_dir_all(&dest).ok();
    }
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    let launch_path: Option<PathBuf> = if lower.ends_with(".zip") {
        emit(app, slug, "extracting", 0, None, None);
        let (f, d) = (file.clone(), dest.clone());
        tokio::task::spawn_blocking(move || unzip(&f, &d))
            .await
            .map_err(|e| e.to_string())??;
        find_launchable(&dest, slug, name)
    } else if lower.ends_with(".dmg") {
        emit(app, slug, "installing", 0, None, None);
        install_dmg(&file, &dest).await?;
        find_launchable(&dest, slug, name)
    } else if lower.ends_with(".msi") || ((lower.contains("setup") || lower.contains("installer")) && lower.ends_with(".exe")) {
        // A real installer: hand off to it. The tool then lives wherever it installs itself.
        emit(app, slug, "installing", 0, None, None);
        let target = dest.join(sanitize(&file_name));
        std::fs::rename(&file, &target).map_err(|e| e.to_string())?;
        let status = if lower.ends_with(".msi") {
            tokio::process::Command::new("msiexec").arg("/i").arg(&target).status().await
        } else {
            tokio::process::Command::new(&target).status().await
        }
        .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("The installer was closed or failed.".into());
        }
        None
    } else if lower.ends_with(".exe") || lower.ends_with(".appimage") {
        let target = dest.join(sanitize(&file_name));
        std::fs::rename(&file, &target).map_err(|e| e.to_string())?;
        Some(target)
    } else {
        return Err(format!("Don't know how to install {file_name}."));
    };
    std::fs::remove_dir_all(&staging).ok();

    // Drop older versions now that the new one is in place.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            if e.path() != dest {
                std::fs::remove_dir_all(e.path()).ok();
            }
        }
    }

    // A catalog launch hint wins over the guess, if it points at a real file inside the folder.
    let mut launch_args = vec![];
    let mut launch_path = launch_path;
    if let Some(hint) = launch.filter(|h| !h.trim().is_empty()) {
        let mut parts = hint.split_whitespace();
        let rel = parts.next().unwrap_or_default();
        let candidate = dest.join(rel);
        if !rel.contains("..") && candidate.exists() {
            launch_path = Some(candidate);
            launch_args = parts.map(String::from).collect();
        }
    }

    let entry = InstalledTool {
        slug: slug.into(),
        name: name.into(),
        version: version.into(),
        path: dest.to_string_lossy().into(),
        launch_path: launch_path.map(|p| p.to_string_lossy().into()),
        launch_args,
        installed_at: now,
    };
    record(app, entry.clone())?;
    Ok(entry)
}

fn record(app: &AppHandle, entry: InstalledTool) -> Result<(), String> {
    let mut lib = store::library(app);
    lib.retain(|t| t.slug != entry.slug);
    lib.push(entry);
    store::save_library(app, &lib)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || ".-_+".contains(c) { c } else { '_' })
        .collect()
}

pub(crate) async fn download(app: &AppHandle, slug: &str, url: &str, to: &Path) -> Result<(), String> {
    let res = reqwest::Client::builder()
        .user_agent(concat!("ToolbayDesktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Download failed (HTTP {}).", res.status().as_u16()));
    }
    let total = res.content_length();
    let mut out = tokio::fs::File::create(to).await.map_err(|e| e.to_string())?;
    let mut stream = res.bytes_stream();
    let mut received: u64 = 0;
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        if received - last_emit > 256 * 1024 {
            last_emit = received;
            emit(app, slug, "downloading", received, total, None);
        }
    }
    out.flush().await.map_err(|e| e.to_string())?;
    emit(app, slug, "downloading", received, total, None);
    Ok(())
}

pub(crate) fn unzip(file: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("Bad zip: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name rejects absolute paths and ../ traversal.
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode)).ok();
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn install_dmg(file: &Path, dest: &Path) -> Result<(), String> {
    let mount = dest.join("_mnt");
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let ok = tokio::process::Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(file)
        .status()
        .await
        .map_err(|e| e.to_string())?
        .success();
    if !ok {
        return Err("Could not open the disk image.".into());
    }
    let mut copied = false;
    if let Ok(entries) = std::fs::read_dir(&mount) {
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "app") {
                copied = tokio::process::Command::new("cp")
                    .arg("-R")
                    .arg(e.path())
                    .arg(dest)
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false);
            }
        }
    }
    let _ = tokio::process::Command::new("hdiutil").arg("detach").arg(&mount).status().await;
    std::fs::remove_dir_all(&mount).ok();
    if copied { Ok(()) } else { Err("No app found in the disk image.".into()) }
}

#[cfg(not(target_os = "macos"))]
async fn install_dmg(_file: &Path, _dest: &Path) -> Result<(), String> {
    Err("Disk images only install on macOS.".into())
}

/// Pick the program to start: the .app (mac) or the .exe whose name best matches the tool.
fn find_launchable(dir: &Path, slug: &str, name: &str) -> Option<PathBuf> {
    let want = |s: &str| s.to_ascii_lowercase().replace([' ', '-', '_'], "");
    let (slug_k, name_k) = (want(slug), want(name));
    let mut best: Option<(i32, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let fname = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            let lower = fname.to_ascii_lowercase();
            let is_app = cfg!(target_os = "macos") && lower.ends_with(".app");
            let is_exe = cfg!(windows) && lower.ends_with(".exe");
            if p.is_dir() && !is_app {
                if depth < 4 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            if !(is_app || is_exe) {
                continue;
            }
            if ["unins", "uninstall", "setup", "crash", "update", "helper", "cli"].iter().any(|b| lower.contains(b)) {
                continue;
            }
            let stem = want(lower.trim_end_matches(".exe").trim_end_matches(".app"));
            let mut score = 10 - depth * 2;
            if stem == slug_k || stem == name_k {
                score += 20;
            } else if stem.contains(&slug_k) || slug_k.contains(&stem) || stem.contains(&name_k) {
                score += 10;
            }
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

fn find(app: &AppHandle, slug: &str) -> Result<InstalledTool, String> {
    store::library(app)
        .into_iter()
        .find(|t| t.slug == slug)
        .ok_or_else(|| "Not installed.".to_string())
}

#[tauri::command]
pub fn tool_launch(app: AppHandle, slug: String) -> Result<(), String> {
    let tool = find(&app, &slug)?;
    let Some(path) = tool.launch_path.clone().filter(|p| Path::new(p).exists()) else {
        if !tool.path.is_empty() {
            return app.opener().open_path(tool.path, None::<&str>).map_err(|e| e.to_string());
        }
        return Err("Nothing to launch for this tool.".into());
    };
    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        cmd.arg(&path);
        if !tool.launch_args.is_empty() {
            cmd.arg("--args").args(&tool.launch_args);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let p = Path::new(&path);
        std::process::Command::new(p)
            .args(&tool.launch_args)
            .current_dir(p.parent().unwrap_or(Path::new(".")))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn tool_reveal(app: AppHandle, slug: String) -> Result<(), String> {
    let tool = find(&app, &slug)?;
    let target = tool.launch_path.unwrap_or(tool.path);
    if target.is_empty() {
        return Err("This tool is built into Toolbay.".into());
    }
    app.opener().reveal_item_in_dir(target).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tool_uninstall(app: AppHandle, slug: String) -> Result<(), String> {
    if !valid_slug(&slug) {
        return Err("bad slug".into());
    }
    let dir = store::tools_dir(&app).join(&slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not remove files (is the tool still open?): {e}"))?;
    }
    let mut lib = store::library(&app);
    lib.retain(|t| t.slug != slug);
    store::save_library(&app, &lib)
}

/// Skills install through the Toolbay CLI: `npx toolbay add <slug>`.
#[tauri::command]
pub async fn skill_install(slug: String) -> Result<String, String> {
    if !valid_slug(&slug) {
        return Err("bad slug".into());
    }
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let out = tokio::process::Command::new(npx)
        .args(["--yes", "toolbay", "add", &slug])
        .output()
        .await
        .map_err(|_| "Node.js is needed to install skills (npx not found).".to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() { Ok(text) } else { Err(text) }
}
