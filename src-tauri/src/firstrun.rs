//! "Install the hub, then the tool": which tool someone wanted when they downloaded Toolbay,
//! and screens the webview should open once it's ready.
//!
//! toolbay.ai serves the hub installer from a tool page as `Toolbay-Setup--<slug>.exe|.pkg`.
//! The Windows installer writes its own file name to `<install dir>/installed-from.txt`; the Mac
//! .pkg postinstall writes it to `~/Library/Application Support/ai.toolbay.desktop/installed-from.txt`.
//! On launch we read it once, delete it, and offer "Install <tool>?".
use serde_json::Value;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

static PENDING: Mutex<Vec<Value>> = Mutex::new(Vec::new());

/// Queue a screen for the webview (it may not be listening yet on a cold start) and ping it.
pub fn push_route(app: &AppHandle, route: Value) {
    PENDING.lock().unwrap().push(route);
    app.emit("route-pending", ()).ok();
}

/// The webview takes queued screens when it starts and whenever "route-pending" fires.
#[tauri::command]
pub fn route_take() -> Vec<Value> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

/// The tool slug in an installer file name: "Toolbay-Setup--toolbay-flow (1).exe" -> "toolbay-flow".
pub fn slug_from_installer_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let (_, rest) = base.split_once("--")?;
    let slug: String = rest
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    let slug = slug.trim_matches('-').to_string();
    (!slug.is_empty() && slug.len() <= 64).then_some(slug)
}

fn marker_paths(app: &AppHandle) -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        v.push(dir.join("installed-from.txt"));
    }
    if let Ok(dir) = app.path().app_data_dir() {
        v.push(dir.join("installed-from.txt"));
    }
    v
}

/// Read (and remove) the installer's note, and queue "Install <tool>?" if it names one.
/// On the very first launch, also tell toolbay.ai the app was installed (GET /api/v1/desktop/handoff,
/// counted once); its toolSlug is the fallback when the file name didn't say which tool.
pub fn check(app: &AppHandle) {
    let mut wanted = None;
    for p in marker_paths(app) {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let _ = std::fs::remove_file(&p);
        if let Some(slug) = slug_from_installer_name(text.trim()) {
            wanted = Some(slug);
            break;
        }
    }
    if let Some(slug) = &wanted {
        push_route(app, serde_json::json!({ "kind": "install", "slug": slug, "firstRun": true }));
    }
    if crate::store::setting(app, "handoffSent").is_some() {
        return;
    }
    let _ = crate::store::settings_set(app.clone(), "handoffSent".into(), chrono::Utc::now().to_rfc3339());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let platform = if cfg!(target_os = "macos") { "mac" } else { "win" };
        let path = format!("/api/v1/desktop/handoff?platform={platform}");
        let Ok(res) = crate::api::send(&app, "GET", &path, None).await else { return };
        // The site's guess only helps someone new: skip it if they already have tools.
        if wanted.is_some() || res.status != 200 || !crate::store::library(&app).is_empty() {
            return;
        }
        let slug = serde_json::from_str::<Value>(&res.body).ok().and_then(|v| v["toolSlug"].as_str().map(String::from));
        if let Some(slug) = slug.filter(|s| !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')) {
            push_route(&app, serde_json::json!({ "kind": "install", "slug": slug, "firstRun": true }));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::slug_from_installer_name as s;

    #[test]
    fn reads_slug() {
        assert_eq!(s("Toolbay-Setup--toolbay-flow.exe").as_deref(), Some("toolbay-flow"));
        assert_eq!(s("Toolbay-Setup--toolbay-flow (1).exe").as_deref(), Some("toolbay-flow"));
        assert_eq!(s("Toolbay-Setup--toolbay-flow(2).pkg").as_deref(), Some("toolbay-flow"));
        assert_eq!(s(r"C:\Users\a\Downloads\Toolbay-Setup--notes.exe").as_deref(), Some("notes"));
        assert_eq!(s("/Users/a/Downloads/Toolbay-Setup--toolbay-flow.pkg").as_deref(), Some("toolbay-flow"));
    }

    #[test]
    fn ignores_plain_names() {
        assert_eq!(s("Toolbay_x64-setup.exe"), None);
        assert_eq!(s("Toolbay_0.1.1_aarch64.pkg"), None);
        assert_eq!(s("Toolbay-Setup--.exe"), None);
        assert_eq!(s("Toolbay-Setup--Evil$.exe"), None);
    }
}
