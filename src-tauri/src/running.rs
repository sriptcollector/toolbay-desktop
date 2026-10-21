//! "Running now": which installed tools are running, and Open / Hide / Close for them.
//!
//! Running state comes from the process list (matched on the installed program's path), so a
//! tool started from a desktop shortcut or the Start menu shows up too, not only ones the hub
//! launched. Windows: Open/Hide act on the tool's main window directly (the largest top-level
//! window of its process), Close asks it to quit and force-quits after 5 s.
//! A tool can also report its window state in <its app data dir>/toolbay-state.json
//! {"pid":123,"window":"shown"|"hidden"}; that wins over our own guess when its pid matches.
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
    /// "shown" | "hidden"
    pub window: String,
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
            let window = reported_window(app, t, pid).unwrap_or_else(|| native::window_state(pid));
            out.insert(
                t.slug.clone(),
                RunInfo { slug: t.slug.clone(), name: t.name.clone(), pid, started_at, window },
            );
        }
    }
    out
}

/// The tool's own report of its window, if it writes one and it's about this process.
/// Accepts {"window":"shown"|"hidden"} or {"visible":true|false}.
fn reported_window(app: &AppHandle, t: &InstalledTool, pid: u32) -> Option<String> {
    let id = t.identifier.as_deref()?;
    let name = t.state_file.as_deref().unwrap_or("toolbay-state.json");
    let file = app.path().data_dir().ok()?.join(id).join(name);
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    if v["pid"].as_u64()? != pid as u64 {
        return None;
    }
    if let Some(w) = v["window"].as_str() {
        return Some(w.to_string());
    }
    v["visible"].as_bool().map(|b| if b { "shown".into() } else { "hidden".into() })
}

/// Send a running tool one of its own control commands (single-instance forwards it).
fn send_control(t: &InstalledTool, args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    let mut cmd = std::process::Command::new(&t.exe);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().is_ok()
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
        crate::rebuild_tray(app);
        // Anything that just closed gets its pending update now.
        for slug in stopped {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::installer::update_one(&app, &slug).await });
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

/// Hide: put its window away; the tool keeps running (e.g. Flow keeps its hotkey).
#[tauri::command]
pub fn tool_hide(app: AppHandle, slug: String) -> Result<(), String> {
    let pid = app.state::<RunState>().tools.lock().unwrap().get(&slug).map(|r| r.pid);
    let Some(pid) = pid else { return Ok(()) };
    let asked = installed(&app, &slug).is_some_and(|t| send_control(&t, &t.hide_args));
    if !asked && !native::hide(pid) {
        return Err("Couldn't hide that window. Try again.".into());
    }
    nudge(&app);
    Ok(())
}

/// Close: ask it to quit, then force it after 5 seconds.
#[tauri::command]
pub async fn tool_close(app: AppHandle, slug: String) -> Result<(), String> {
    close(&app, &slug).await;
    Ok(())
}

pub async fn close(app: &AppHandle, slug: &str) {
    let Some(pid) = app.state::<RunState>().tools.lock().unwrap().get(slug).map(|r| r.pid) else { return };
    // The tool's own quit command if it has one (single-instance apps pass it to the running copy).
    let asked = installed(app, slug).is_some_and(|t| send_control(&t, &t.quit_args));
    if !asked {
        native::ask_to_quit(pid);
    }
    // Give it 3 seconds to quit by itself, then make it.
    let exited = tokio::task::spawn_blocking(move || native::wait_exit(pid, 3000)).await.unwrap_or(false);
    if !exited {
        native::kill_tree(pid);
        let _ = tokio::task::spawn_blocking(move || native::wait_exit(pid, 2000)).await;
    }
    refresh(app);
}

/// Tray menu action ids: "tool:<open|hide|close>:<slug>".
pub fn handle_tray(app: &AppHandle, id: &str) {
    let mut parts = id.splitn(3, ':');
    let (Some("tool"), Some(action), Some(slug)) = (parts.next(), parts.next(), parts.next()) else { return };
    let (app, slug) = (app.clone(), slug.to_string());
    let result = match action {
        "open" => tool_open(app.clone(), slug),
        "hide" => tool_hide(app.clone(), slug),
        "close" => {
            tauri::async_runtime::spawn(async move { close(&app, &slug).await });
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(e) = result {
        let _ = crate::installer::notify_error(&e);
    }
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

    pub fn window_state(pid: u32) -> String {
        match main_window(pid) {
            Some((hwnd, true)) if unsafe { IsIconic(hwnd) } == 0 => "shown".into(),
            _ => "hidden".into(),
        }
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

    pub fn hide(pid: u32) -> bool {
        let Some((hwnd, _)) = main_window(pid) else { return false };
        unsafe { ShowWindow(hwnd, SW_HIDE) };
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
    //! macOS: drive the app through System Events (hide/show) and a polite quit.
    fn osa(script: &str) -> bool {
        std::process::Command::new("osascript").args(["-e", script]).status().map(|s| s.success()).unwrap_or(false)
    }
    pub fn window_state(_pid: u32) -> String {
        "shown".into()
    }
    pub fn show(pid: u32) -> bool {
        osa(&format!("tell application \"System Events\" to set frontmost of (first process whose unix id is {pid}) to true"))
    }
    pub fn hide(pid: u32) -> bool {
        osa(&format!("tell application \"System Events\" to set visible of (first process whose unix id is {pid}) to false"))
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
