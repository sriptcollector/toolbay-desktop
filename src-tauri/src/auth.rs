//! Browser sign-in, the way Wispr Flow does it:
//! 1. the app opens {base}/desktop/auth?state=<uuid> in the default browser,
//! 2. the site signs the user in and redirects to toolbay://auth?code=...&state=...,
//! 3. the OS hands that URL to this app (deep link), we check state and
//!    exchange the code at POST /api/v1/desktop/token for {token, user}.
use crate::{api, store};
use serde_json::Value;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
pub struct AuthState {
    pending: Mutex<Option<String>>,
}

#[tauri::command]
pub fn auth_start(app: AppHandle, state: tauri::State<'_, AuthState>) -> Result<(), String> {
    let nonce = uuid::Uuid::new_v4().to_string();
    *state.pending.lock().unwrap() = Some(nonce.clone());
    let url = format!("{}/desktop/auth?state={}", api::base_url(&app), nonce);
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "Couldn't open your web browser. Please try again.".to_string())
}

#[tauri::command]
pub fn auth_user(app: AppHandle) -> Option<Value> {
    api::token()?;
    store::setting(&app, "user").and_then(|u| serde_json::from_str(&u).ok())
}

#[tauri::command]
pub async fn auth_sign_out(app: AppHandle) -> Result<(), String> {
    // Revoke on the server too; sign out locally even if that fails (offline).
    let _ = api::send(&app, "DELETE", "/api/v1/desktop/token", None).await;
    api::set_token(None)?;
    store::settings_set(app.clone(), "user".into(), String::new())?;
    app.emit("auth-changed", Value::Null).ok();
    Ok(())
}

/// True for links that should start a tool without showing the hub window
/// (desktop shortcuts use toolbay://launch/<slug>).
pub fn is_launch_link(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|u| u.host_str() == Some("launch"))
}

/// Called for every toolbay:// URL the OS hands us.
pub fn handle_deep_link(app: &AppHandle, raw: &str) {
    // The OS can deliver the same link twice (argv and the deep-link event); one action per click.
    {
        static LAST: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);
        let mut last = LAST.lock().unwrap();
        let key = raw.trim_end_matches('/').to_string();
        if let Some((prev, at)) = last.as_ref() {
            if *prev == key && at.elapsed() < std::time::Duration::from_secs(2) {
                return;
            }
        }
        *last = Some((key, std::time::Instant::now()));
    }
    let Ok(url) = url::Url::parse(raw) else { return };
    let host = url.host_str().unwrap_or_default().to_string();
    let segments: Vec<String> = url
        .path_segments()
        .map(|s| s.filter(|x| !x.is_empty()).map(String::from).collect())
        .unwrap_or_default();

    // toolbay://launch/<slug>: open the tool, keep the hub in the tray.
    if host == "launch" {
        if let Some(slug) = segments.first() {
            if let Err(e) = crate::installer::tool_launch(app.clone(), slug.clone()) {
                show(app);
                app.emit("open-route", serde_json::json!({ "kind": "tool", "slug": slug })).ok();
                app.emit("auth-error", e).ok();
            }
        }
        return;
    }
    show(app);
    match host.as_str() {
        // toolbay://auth?code=..&state=..
        "auth" => {
            let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let Some(code) = q.get("code").cloned() else {
                app.emit("auth-error", "Sign-in didn't finish. Please try again.").ok();
                return;
            };
            let expected = app.state::<AuthState>().pending.lock().unwrap().take();
            if let (Some(got), Some(want)) = (q.get("state"), expected.as_ref()) {
                if got != want {
                    app.emit("auth-error", "That sign-in link was for a different window. Please sign in again.").ok();
                    return;
                }
            }
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = exchange(&app, &code).await {
                    app.emit("auth-error", e).ok();
                }
            });
        }
        // toolbay://open/tools/<slug> (the website's "Open in the app"), toolbay://tool/<slug>
        "open" | "tool" | "tools" => {
            let slug = if host == "open" { segments.get(1) } else { segments.first() };
            if let Some(slug) = slug {
                app.emit("open-route", serde_json::json!({ "kind": "tool", "slug": slug })).ok();
            }
        }
        _ => {}
    }
}

fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

async fn exchange(app: &AppHandle, code: &str) -> Result<(), String> {
    let body = serde_json::json!({ "code": code }).to_string();
    let res = api::send(app, "POST", "/api/v1/desktop/token", Some(body))
        .await
        .map_err(|_| "Couldn't reach toolbay.ai. Check your internet connection and try again.")?;
    if res.status != 200 {
        return Err("Sign-in didn't finish. Please try again.".into());
    }
    let json: Value = serde_json::from_str(&res.body).map_err(|_| "Sign-in didn't finish. Please try again.")?;
    let token = json["token"].as_str().ok_or("Sign-in didn't finish. Please try again.")?;
    api::set_token(Some(token))?;
    let user = json["user"].clone();
    store::settings_set(app.clone(), "user".into(), user.to_string())?;
    app.emit("auth-changed", user).ok();
    Ok(())
}
