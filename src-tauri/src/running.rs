//! "Running now": which installed tools are running, and Open (Switch to) / Close for them.
//!
//! Running state comes from the process list (matched on the installed program's path), so a
//! tool started from a desktop shortcut or the Start menu shows up too, not only ones the hub
//! launched. A running tool is just "Running"; nothing is hidden or kept in the background.
//! Close asks the tool to quit (its own quit command, then a polite quit signal) and only
//! force-quits when the person pressed Close and it still didn't exit. Nothing else ever
//! closes or force-quits a tool.
use crate::store::{self, InstalledTool};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RunInfo {
    pub slug: String,
    pub name: String,
    pub pid: u32,
    /// Unix seconds.
    pub started_at: u64,
}

#[derive(Default)]
pub struct RunState {
    pub tools: Mutex<BTreeMap<String, RunInfo>>,
    sys: Mutex<Option<System>>,
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

/// Scan the process list once and return what's running.
pub fn scan(app: &AppHandle) -> BTreeMap<String, RunInfo> {
    let lib = store::library(app);
    let state = app.state::<RunState>();
    let mut guard = state.sys.lock().unwrap();
    let sys = guard.get_or_insert_with(|| {
        System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)))
    });
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut out = BTreeMap::new();
    for t in &lib {
        let target = PathBuf::from(&t.exe);
        let mut best: Option<(u32, u64)> = None;
        for (pid, p) in sys.processes() {
            let Some(exe) = p.exe() else { continue };
            // macOS: the program lives inside the .app bundle we recorded.
            let hit = same_path(exe, &target) || (cfg!(target_os = "macos") && exe.starts_with(&target));
            if hit && best.is_none_or(|(_, s)| p.start_time() < s) {
                best = Some((pid.as_u32(), p.start_time()));
            }
        }
        if let Some((pid, started_at)) = best {
            out.insert(t.slug.clone(), RunInfo { slug: t.slug.clone(), name: t.name.clone(), pid, started_at });
        }
    }
    out
}

/// True if `exe` is `path` or lives inside it (a .app bundle or an install folder).
fn inside(exe: &Path, path: &Path) -> bool {
    if cfg!(windows) {
        let lower = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
        lower(exe).starts_with(lower(path))
    } else {
        exe.starts_with(path)
    }
}

/// True if any process at all runs a program at `path` or inside it, whatever copy or path the
/// library recorded. Checked before anything moves, replaces or deletes an installed app.
pub fn path_in_use(path: &Path) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    let mut paths = vec![path.to_path_buf()];
    if let Ok(real) = path.canonicalize() {
        if real != path {
            paths.push(real);
        }
    }
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    sys.processes().values().any(|p| p.exe().is_some_and(|exe| paths.iter().any(|d| inside(exe, d))))
}

/// The program to run for a control command. macOS: the binary inside the .app bundle
/// (Contents/MacOS/<CFBundleExecutable>); a bundle folder itself can't be started.
fn control_program(t: &InstalledTool) -> PathBuf {
    let exe = PathBuf::from(&t.exe);
    #[cfg(target_os = "macos")]
    {
        if exe.extension().is_some_and(|e| e == "app") && exe.is_dir() {
            if let Some(bin) = bundle_binary(&exe) {
                return bin;
            }
        }
    }
    exe
}

#[cfg(target_os = "macos")]
fn bundle_binary(app: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("/usr/bin/defaults")
        .arg("read")
        .arg(app.join("Contents").join("Info"))
        .arg("CFBundleExecutable")
        .output()
        .ok()?;
    let mut name = if out.status.success() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { String::new() };
    if name.is_empty() {
        name = app.file_stem()?.to_string_lossy().to_string();
    }
    let bin = app.join("Contents").join("MacOS").join(name);
    bin.is_file().then_some(bin)
}

/// Send a running tool one of its own control commands (single-instance forwards it).
fn send_control(t: &InstalledTool, args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    let mut cmd = std::process::Command::new(control_program(t));
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.spawn() {
        Ok(mut child) => {
            // Reap it once it has handed the command over and exited.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

fn installed(app: &AppHandle, slug: &str) -> Option<InstalledTool> {
    store::library(app).into_iter().find(|t| t.slug == slug)
}

pub fn is_running(app: &AppHandle, slug: &str) -> bool {
    app.state::<RunState>().tools.lock().unwrap().contains_key(slug)
}

fn refresh(app: &AppHandle) {
    let now = scan(app);
    let (changed, stopped) = {
        let state = app.state::<RunState>();
        let mut cur = state.tools.lock().unwrap();
        let changed = *cur != now;
        let stopped: Vec<String> = cur.keys().filter(|k| !now.contains_key(*k)).cloned().collect();
        *cur = now.clone();
        (changed, stopped)
    };
    if changed {
        app.emit("running-changed", now.values().cloned().collect::<Vec<_>>()).ok();
        // Anything that just closed gets its pending update now, once it has fully exited
        // (update_one checks again that no process runs from it before touching anything).
        for slug in stopped {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                crate::installer::update_one(&app, &slug).await
            });
        }
    }
}

/// Watch the process list every 1.5 s for the life of the app.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        refresh(&app);
        std::thread::sleep(Duration::from_millis(1500));
    });
}

/// Re-check soon after we start/stop something, so the UI doesn't wait for the next tick.
fn nudge(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        for ms in [300, 1200, 3000] {
            std::thread::sleep(Duration::from_millis(ms));
            refresh(&app);
        }
    });
}

#[tauri::command]
pub fn tool_running(app: AppHandle) -> Vec<RunInfo> {
    app.state::<RunState>().tools.lock().unwrap().values().cloned().collect()
}

/// Open: start it, or bring its window to the front if it's already running.
#[tauri::command]
pub fn tool_open(app: AppHandle, slug: String) -> Result<(), String> {
    let pid = app.state::<RunState>().tools.lock().unwrap().get(&slug).map(|r| r.pid);
    match pid {
        Some(pid) => {
            // Ask the tool to show itself, and bring its window forward (we have focus to give).
            if let Some(t) = installed(&app, &slug) {
                send_control(&t, &t.show_args);
            }
            native::show(pid);
            nudge(&app);
            Ok(())
        }
        _ => {
            crate::installer::tool_launch(app.clone(), slug)?;
            nudge(&app);
            Ok(())
        }
    }
}

/// Close (the person pressed Close): ask it to quit, and force it if it still hasn't.
#[tauri::command]
pub async fn tool_close(app: AppHandle, slug: String) -> Result<(), String> {
    close(&app, &slug, true).await;
    Ok(())
}

/// Ask a running tool to quit: its own quit command first (single-instance apps pass it to the
/// running copy), then a polite quit signal. Only when `force` (the person pressed Close) is it
/// force-quit after that. True once it has exited.
pub async fn close(app: &AppHandle, slug: &str, force: bool) -> bool {
    let Some(pid) = app.state::<RunState>().tools.lock().unwrap().get(slug).map(|r| r.pid) else { return true };
    let wait = |ms: u32| async move { tokio::task::spawn_blocking(move || native::wait_exit(pid, ms)).await.unwrap_or(false) };
    let asked = installed(app, slug).is_some_and(|t| send_control(&t, &t.quit_args));
    let mut exited = asked && wait(3000).await;
    if !exited {
        native::ask_to_quit(pid);
        exited = wait(3000).await;
    }
    if !exited && force {
        native::kill_tree(pid);
        exited = wait(2000).await;
    }
    refresh(app);
    exited
}

#[cfg(windows)]
mod native {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct Found {
        pid: u32,
        windows: Vec<(HWND, i64, bool)>,
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let found = &mut *(lparam as *mut Found);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == found.pid && GetWindow(hwnd, GW_OWNER).is_null() && GetWindowTextLengthW(hwnd) > 0 {
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                if ex & WS_EX_TOOLWINDOW == 0 {
                    // Size it by its normal (restored) rectangle: a minimized window's current
                    // rectangle is a tiny stub, which made a small overlay (e.g. Flow's bar) win.
                    let mut wp: WINDOWPLACEMENT = std::mem::zeroed();
                    wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
                    let mut r: RECT = std::mem::zeroed();
                    if GetWindowPlacement(hwnd, &mut wp) != 0 {
                        r = wp.rcNormalPosition;
                    } else {
                        GetWindowRect(hwnd, &mut r);
                    }
                    let area = ((r.right - r.left) as i64) * ((r.bottom - r.top) as i64);
                    found.windows.push((hwnd, area, IsWindowVisible(hwnd) != 0));
                }
            }
        }
        1
    }

    /// The tool's main window: its largest titled top-level window (shown or hidden).
    fn main_window(pid: u32) -> Option<(HWND, bool)> {
        let mut found = Found { pid, windows: vec![] };
        unsafe { EnumWindows(Some(collect), &mut found as *mut Found as LPARAM) };
        found.windows.sort_by_key(|w| -w.1);
        found.windows.first().map(|w| (w.0, w.2))
    }

    pub fn show(pid: u32) -> bool {
        let Some((hwnd, _)) = main_window(pid) else { return false };
        unsafe {
            ShowWindow(hwnd, if IsIconic(hwnd) != 0 { SW_RESTORE } else { SW_SHOW });
            // Windows only lets the foreground app hand over focus; a quick Alt tap unlocks it.
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_MENU};
            keybd_event(VK_MENU as u8, 0, 0, 0);
            keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
            SetForegroundWindow(hwnd);
        }
        true
    }

    /// Ask nicely: WM_CLOSE to every top-level window of the process.
    pub fn ask_to_quit(pid: u32) {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .creation_flags(0x0800_0000)
            .status();
    }

    /// Wait up to `ms` for the process to exit. True if it's gone.
    pub fn wait_exit(pid: u32, ms: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if h.is_null() {
                return true; // already gone
            }
            let r = WaitForSingleObject(h, ms);
            CloseHandle(h);
            r == WAIT_OBJECT_0
        }
    }

    pub fn kill_tree(pid: u32) {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .status();
    }

    use std::os::windows::process::CommandExt;
}

#[cfg(not(windows))]
mod native {
    //! macOS: bring the app forward through System Events, quit with SIGTERM (polite) and
    //! SIGKILL (only for an explicit Close that didn't work).
    fn osa(script: &str) -> bool {
        std::process::Command::new("osascript").args(["-e", script]).status().map(|s| s.success()).unwrap_or(false)
    }
    pub fn show(pid: u32) -> bool {
        osa(&format!("tell application \"System Events\" to set frontmost of (first process whose unix id is {pid}) to true"))
    }
    pub fn ask_to_quit(pid: u32) {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    }
    pub fn wait_exit(pid: u32, ms: u32) -> bool {
        let alive = || std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false);
        let end = std::time::Instant::now() + std::time::Duration::from_millis(ms as u64);
        while std::time::Instant::now() < end {
            if !alive() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        !alive()
    }
    pub fn kill_tree(pid: u32) {
        let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}
