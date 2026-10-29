//! Mac: exactly one Toolbay. An older copy of Toolbay.app left somewhere else (Downloads, the
//! Desktop, a second /Applications) could be opened by a toolbay:// link and run next to this one:
//! two Toolbay icons in the Dock, and Quit seemed to need two presses (it quit one of them).
//! On start this:
//!   - closes any older Toolbay running from another place (or, if the other one is newer, opens
//!     that one and quits this one), and
//!   - moves older copies of Toolbay.app in the usual folders to the Trash, so nothing can open
//!     them again. Copies on a mounted disk image (/Volumes) are left alone.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// The .app bundle this process runs from (None in a dev build without a bundle).
fn own_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

fn bundle_version(bundle: &Path) -> Option<String> {
    let out = Command::new("defaults")
        .arg("read")
        .arg(bundle.join("Contents/Info"))
        .arg("CFBundleShortVersionString")
        .output()
        .ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !v.is_empty()).then_some(v)
}

fn bundle_id(bundle: &Path) -> Option<String> {
    let out = Command::new("defaults").arg("read").arg(bundle.join("Contents/Info")).arg("CFBundleIdentifier").output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Other running Toolbay processes: (pid, bundle).
fn other_running(own: &Path, exe_name: &str) -> Vec<(u32, PathBuf)> {
    let Ok(out) = Command::new("ps").args(["-axo", "pid=,command="]).output() else { return vec![] };
    let me = std::process::id();
    let marker = format!(".app/Contents/MacOS/{exe_name}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, cmd) = line.split_once(' ')?;
            let pid: u32 = pid.parse().ok()?;
            let cmd = cmd.trim_start();
            let end = cmd.find(&marker)? + ".app".len();
            let bundle = PathBuf::from(&cmd[..end]);
            let bundle = bundle.canonicalize().unwrap_or(bundle);
            (pid != me && bundle != own).then_some((pid, bundle))
        })
        .collect()
}

fn alive(pid: u32) -> bool {
    Command::new("kill").args(["-0", &pid.to_string()]).status().is_ok_and(|s| s.success())
}

/// Ask it to quit and wait up to 5 s. Never force-killed: a tool copy it started (rsync) is a
/// child process that finishes on its own, and a copy that won't quit is simply left running.
fn stop(pid: u32) {
    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    for _ in 0..50 {
        if !alive(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Running from a disk image or from macOS's translocation sandbox (an app opened straight from
/// Downloads or a DMG): this copy isn't an install, so it changes nothing on disk.
fn is_transient(own: &Path) -> bool {
    own.starts_with("/Volumes") || own.to_string_lossy().contains("/AppTranslocation/")
}

/// Toolbay.app copies in the folders people actually keep apps in.
fn copies_on_disk() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = vec![PathBuf::from("/Applications")];
    // ~/Applications is left alone: it's where people without admin rights install apps.
    if let Some(h) = &home {
        for d in ["Downloads", "Desktop", "Documents"] {
            dirs.push(h.join(d));
        }
    }
    let mut found = vec![];
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let is_app = p.extension().is_some_and(|x| x == "app");
            let name = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if is_app && name.starts_with("toolbay") && bundle_id(&p).as_deref() == Some("ai.toolbay.desktop") {
                found.push(p.canonicalize().unwrap_or(p));
            }
        }
    }
    found
}

fn to_trash(bundle: &Path) {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return };
    let name = bundle.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Toolbay.app".into());
    let trash = home.join(".Trash");
    let mut target = trash.join(&name);
    let mut n = 2;
    while target.exists() {
        target = trash.join(format!("{} {n}.app", name.trim_end_matches(".app")));
        n += 1;
    }
    if std::fs::rename(bundle, &target).is_err() {
        // Another disk or owned by root: ask Finder (it may ask for a password once).
        let script = format!("tell application \"Finder\" to delete POSIX file \"{}\"", bundle.display());
        let _ = Command::new("osascript").args(["-e", &script]).status();
    }
}

/// Returns false when this copy steps aside for a newer one that's already running (setup
/// must then stop: the app is exiting). `links` are toolbay:// links this launch was opened
/// with; they're passed on so a sign-in or install link still lands.
pub fn check(app: &AppHandle, links: &[String]) -> bool {
    let Some(own) = own_bundle() else { return true };
    let Some(exe_name) = std::env::current_exe().ok().and_then(|e| e.file_name().map(|n| n.to_string_lossy().to_string())) else {
        return true;
    };
    let mine = app.package_info().version.to_string();

    for (pid, bundle) in other_running(&own, &exe_name) {
        let theirs = bundle_version(&bundle).unwrap_or_default();
        if crate::manifest::newer(&theirs, &mine) {
            // That one is newer: bring it forward (with this launch's links) and step aside.
            let _ = Command::new("open").arg("-a").arg(&bundle).args(links).status();
            app.exit(0);
            return false;
        }
        if !is_transient(&own) {
            stop(pid);
        }
    }
    if is_transient(&own) {
        return true;
    }

    std::thread::spawn(move || {
        let own_in_apps = own.starts_with("/Applications");
        for bundle in copies_on_disk() {
            // Never trash the /Applications copy from a stray copy elsewhere: that one is the real install.
            if bundle == own || bundle.starts_with("/Volumes") || (bundle.starts_with("/Applications") && !own_in_apps) {
                continue;
            }
            // Only strictly older copies: a same-version copy might be the one macOS is about to run.
            let theirs = bundle_version(&bundle).unwrap_or_default();
            if theirs.is_empty() || !crate::manifest::newer(&mine, &theirs) {
                continue;
            }
            let running = other_running(&own, &exe_name).into_iter().any(|(_, b)| b == bundle);
            if !running {
                to_trash(&bundle);
            }
        }
    });
    true
}
