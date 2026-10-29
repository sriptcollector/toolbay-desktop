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

/// App name (macOS) or exe stem (Windows), lowercase, matched exactly or as a prefix (so
/// "Cloudflare WARP" isn't the Warp terminal) -> (label, editor's title suffix for the project).
const APPS: &[(&str, &str, Option<&str>)] = &[
    ("code", "Coding in VS Code", Some("Visual Studio Code")),
    ("visual studio code", "Coding in VS Code", Some("Visual Studio Code")),
    ("code - insiders", "Coding in VS Code", Some("Visual Studio Code - Insiders")),
    ("cursor", "Coding in Cursor", Some("Cursor")),
    ("windsurf", "Coding in Windsurf", Some("Windsurf")),
    ("zed", "Coding in Zed", None),
    ("xcode", "Coding in Xcode", None),
    ("android studio", "Coding in Android Studio", None),
    ("studio64", "Coding in Android Studio", None),
    ("intellij idea", "Coding in IntelliJ", None),
    ("idea64", "Coding in IntelliJ", None),
    ("pycharm", "Coding in PyCharm", None),
    ("webstorm", "Coding in WebStorm", None),
    ("rustrover", "Coding in RustRover", None),
    ("goland", "Coding in GoLand", None),
    ("rider", "Coding in Rider", None),
    ("rider64", "Coding in Rider", None),
    ("sublime text", "Coding in Sublime Text", None),
    ("sublime_text", "Coding in Sublime Text", None),
    ("devenv", "Coding in Visual Studio", None),
    ("windowsterminal", "In the terminal", None),
    ("terminal", "In the terminal", None),
    ("iterm2", "In the terminal", None),
    ("iterm", "In the terminal", None),
    ("warp", "In the terminal", None),
    ("ghostty", "In the terminal", None),
    ("figma", "Designing in Figma", None),
    ("sketch", "Designing in Sketch", None),
    ("blender", "Modeling in Blender", None),
    ("unity", "Building in Unity", None),
    ("unity hub", "Building in Unity", None),
    ("godot", "Building in Godot", None),
    ("unrealeditor", "Building in Unreal", None),
    ("claude", "Building with Claude", None),
    ("chatgpt", "Building with ChatGPT", None),
    ("github desktop", "In GitHub Desktop", None),
    ("githubdesktop", "In GitHub Desktop", None),
    ("postman", "Testing APIs in Postman", None),
    ("docker desktop", "In Docker", None),
];

fn classify(app: &str, title: &str) -> Foreground {
    let lower = app.to_lowercase();
    let key = lower.strip_suffix(".exe").unwrap_or(&lower);
    // Exact name first; then a name followed by a version or edition ("pycharm64", "godot_v4.2").
    let hit = APPS.iter().find(|(m, _, _)| key == *m).or_else(|| {
        APPS.iter()
            .filter(|(m, _, _)| m.len() >= 4)
            .find(|(m, _, _)| key.strip_prefix(m).is_some_and(|rest| rest.starts_with(|c: char| !c.is_ascii_alphabetic())))
    });
    let project = hit.and_then(|(_, _, suffix)| suffix.and_then(|s| project_from_title(title, s)));
    Foreground { label: hit.map(|(_, l, _)| l.to_string()), project, app: Some(app.to_string()) }
}

/// VS Code-style titles, "● file.ts - project - Visual Studio Code" -> "project". Only when the
/// title really ends with the editor's name; never a remote host ("[SSH: host]") or a file name.
fn project_from_title(title: &str, editor: &str) -> Option<String> {
    let rest = title.strip_suffix(editor)?.trim_end().strip_suffix(" -")?.trim_end();
    let parts: Vec<&str> = rest.split(" - ").map(str::trim).collect();
    // "file - project": two parts. One part is a window with no folder (just a file): nothing.
    if parts.len() < 2 {
        return None;
    }
    let p = parts[parts.len() - 1].trim_start_matches(['●', '•', ' ']).trim();
    let p = p.split(" [").next().unwrap_or(p).split(" (").next().unwrap_or(p).trim();
    let looks_like_file = p.rsplit_once('.').is_some_and(|(_, ext)| (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()));
    (!p.is_empty() && p.len() <= 60 && !p.contains(['/', '\\', '@', ':']) && !looks_like_file).then(|| p.to_string())
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
    fn no_hosts_or_file_names() {
        let f = classify("Code.exe", "file.ts - proj [SSH: prod-db.corp.internal] - Visual Studio Code");
        assert_eq!(f.project.as_deref(), Some("proj"));
        let f = classify("Code.exe", "q3 - plan.md - Visual Studio Code");
        assert!(f.project.is_none());
        let f = classify("Code.exe", "notes.txt - Visual Studio Code");
        assert!(f.project.is_none());
        let f = classify("Cloudflare WARP", "");
        assert!(f.label.is_none());
        let f = classify("Xcode", "");
        assert_eq!(f.label.as_deref(), Some("Coding in Xcode"));
        let f = classify("Code", "");
        assert_eq!(f.label.as_deref(), Some("Coding in VS Code"));
        let f = classify("pycharm64.exe", "");
        assert_eq!(f.label.as_deref(), Some("Coding in PyCharm"));
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
