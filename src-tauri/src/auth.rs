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
    let url = format!(
        "{}/desktop/auth?state={}&redirect_uri={}",
        api::base_url(&app),
        nonce,
        urlencode("toolbay://auth")
    );
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[tauri::command]
pub fn auth_user(app: AppHandle) -> Option<Value> {
    api::token()?;
    store::setting(&app, "user").and_then(|u| serde_json::from_str(&u).ok())
}

#[tauri::command]
pub fn auth_sign_out(app: AppHandle) -> Result<(), String> {
    api::set_token(None)?;
    store::settings_set(app.clone(), "user".into(), String::new())?;
    app.emit("auth-changed", Value::Null).ok();
    Ok(())
}

/// Called for every toolbay:// URL the OS hands us.
pub fn handle_deep_link(app: &AppHandle, raw: &str) {
    let Ok(url) = url::Url::parse(raw) else { return };
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    // toolbay://auth?code=..  -> host "auth"
    // toolbay://tool/<slug>   -> open a tool page
    match url.host_str() {
        Some("auth") => {
            let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let Some(code) = q.get("code").cloned() else {
                app.emit("auth-error", "Sign-in link had no code.").ok();
                return;
            };
            let expected = app.state::<AuthState>().pending.lock().unwrap().take();
            // If the site echoes state back, it must match the one we sent.
            if let (Some(got), Some(want)) = (q.get("state"), expected.as_ref()) {
                if got != want {
                    app.emit("auth-error", "Sign-in link did not match this app. Try again.").ok();
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
        Some("tool") | Some("skill") => {
            let kind = url.host_str().unwrap_or("tool").to_string();
            let slug = url.path().trim_start_matches('/').to_string();
            app.emit("open-route", serde_json::json!({ "kind": kind, "slug": slug })).ok();
        }
        _ => {}
    }
}

async fn exchange(app: &AppHandle, code: &str) -> Result<(), String> {
    let body = serde_json::json!({ "code": code }).to_string();
    let res = api::send(app, "POST", "/api/v1/desktop/token", Some(body)).await?;
    if res.status != 200 {
        return Err(format!("Sign-in failed (HTTP {}).", res.status));
    }
    let json: Value = serde_json::from_str(&res.body).map_err(|_| "Sign-in failed: bad response.")?;
    let token = json["token"].as_str().ok_or("Sign-in failed: no token.")?;
    api::set_token(Some(token))?;
    let user = json["user"].clone();
    store::settings_set(app.clone(), "user".into(), user.to_string())?;
    app.emit("auth-changed", user).ok();
    Ok(())
}
