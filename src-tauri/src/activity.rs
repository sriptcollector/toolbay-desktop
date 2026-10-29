//! Builder Social's "what I'm working on": the app in front, only for people who turned it on.
//!
//! Privacy: only builder apps (editors, terminals, design and AI tools, Toolbay's own) are ever
//! named; anything else reads as nothing. For code editors on Windows the project (folder) name
//! is taken from the window title, never the file name. Read only when the hub asks (about once
//! a minute while it's open); nothing is stored here.
use serde::Serialize;

#[derive(Serialize, Default)]
pub struct Foreground {
    /// What the hub may show, e.g. "Coding in VS Code". None for apps not on the list.
    pub label: Option<String>,
    /// The project open in a code editor (Windows), e.g. "westward".
    pub project: Option<String>,
    /// The app's own name, kept on this computer: the hub only uses it to spot Toolbay tools.
    pub app: Option<String>,
}

/// (matches the app name or exe, lowercase, contains) -> (label, is a code editor)
const APPS: &[(&str, &str, bool)] = &[
    ("code - insiders", "Coding in VS Code", true),
    ("visual studio code", "Coding in VS Code", true),
    ("code.exe", "Coding in VS Code", true),
    ("cursor", "Coding in Cursor", true),
    ("windsurf", "Coding in Windsurf", true),
    ("zed", "Coding in Zed", true),
    ("xcode", "Coding in Xcode", false),
    ("android studio", "Coding in Android Studio", false),
    ("studio64", "Coding in Android Studio", false),
    ("intellij", "Coding in IntelliJ", false),
    ("idea64", "Coding in IntelliJ", false),
    ("pycharm", "Coding in PyCharm", false),
    ("webstorm", "Coding in WebStorm", false),
    ("rustrover", "Coding in RustRover", false),
    ("goland", "Coding in GoLand", false),
    ("rider", "Coding in Rider", false),
    ("sublime", "Coding in Sublime Text", false),
    ("devenv", "Coding in Visual Studio", false),
    ("nvim", "Coding in Neovim", false),
    ("windowsterminal", "In the terminal", false),
    ("terminal", "In the terminal", false),
    ("iterm", "In the terminal", false),
    ("warp", "In the terminal", false),
    ("ghostty", "In the terminal", false),
    ("figma", "Designing in Figma", false),
    ("sketch", "Designing in Sketch", false),
    ("blender", "Modeling in Blender", false),
    ("unity", "Building in Unity", false),
    ("godot", "Building in Godot", false),
    ("unrealeditor", "Building in Unreal", false),
    ("claude", "Building with Claude", false),
    ("chatgpt", "Building with ChatGPT", false),
    ("github desktop", "In GitHub Desktop", false),
    ("postman", "Testing APIs in Postman", false),
    ("docker", "In Docker", false),
];

fn classify(app: &str, title: &str) -> Foreground {
    let mut key = app.to_lowercase();
    // VS Code's Mac name is just "Code" (and "code" is inside "xcode").
    if key == "code" {
        key = "visual studio code".into();
    }
    let hit = APPS.iter().find(|(m, _, _)| key.contains(m));
    let project = hit.filter(|(_, _, editor)| *editor).and_then(|_| project_from_title(title));
    Foreground { label: hit.map(|(_, l, _)| l.to_string()), project, app: Some(app.to_string()) }
}

/// VS Code-style titles: "● file.ts - project - Visual Studio Code" -> "project".
fn project_from_title(title: &str) -> Option<String> {
    let parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    let p = (parts.len() >= 3).then(|| parts[parts.len() - 2])?;
    let p = p.trim_start_matches(['●', '•', ' ']).trim();
    let p = p.split(" (").next().unwrap_or(p).trim();
    (!p.is_empty() && p.len() <= 60 && !p.contains(['/', '\\'])).then(|| p.to_string())
}

#[cfg(windows)]
fn read() -> Option<(String, String)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, path.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&path[..len as usize]);
        let exe = full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string();
        Some((exe, title))
    }
}

/// `lsappinfo` needs no permission prompt (unlike System Events); it gives the app's name only.
#[cfg(target_os = "macos")]
fn read() -> Option<(String, String)> {
    use std::process::Command;
    let front = Command::new("lsappinfo").arg("front").output().ok()?;
    let asn = String::from_utf8_lossy(&front.stdout).trim().to_string();
    if asn.is_empty() {
        return None;
    }
    let info = Command::new("lsappinfo").args(["info", "-only", "name", &asn]).output().ok()?;
    let s = String::from_utf8_lossy(&info.stdout);
    // "LSDisplayName"="Code"
    let name = s.split_once('=')?.1.trim().trim_matches('"').to_string();
    (!name.is_empty()).then(|| (name, String::new()))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn read() -> Option<(String, String)> {
    None
}

#[tauri::command]
pub async fn foreground_activity() -> Foreground {
    tauri::async_runtime::spawn_blocking(|| read().map(|(app, title)| classify(&app, &title)).unwrap_or_default())
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editors_give_the_project_not_the_file() {
        let f = classify("Code.exe", "● page.tsx - westward - Visual Studio Code");
        assert_eq!(f.label.as_deref(), Some("Coding in VS Code"));
        assert_eq!(f.project.as_deref(), Some("westward"));
        let f = classify("Cursor.exe", "main.rs - toolbay-desktop (Workspace) - Cursor");
        assert_eq!(f.project.as_deref(), Some("toolbay-desktop"));
    }

    #[test]
    fn other_apps_are_not_named() {
        let f = classify("chrome.exe", "Inbox (3) - someone@example.com - Gmail - Google Chrome");
        assert!(f.label.is_none() && f.project.is_none());
        let f = classify("Figma", "");
        assert_eq!(f.label.as_deref(), Some("Designing in Figma"));
        assert!(f.project.is_none());
    }
}
