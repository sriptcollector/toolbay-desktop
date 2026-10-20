//! HTTP to the Toolbay site API. All requests go through here so the bearer
//! token stays in Rust and the webview never deals with CORS.
use crate::store;
use serde::Serialize;
use std::sync::OnceLock;
use tauri::AppHandle;

pub const DEFAULT_BASE: &str = "https://toolbay.ai";
const KEYRING_SERVICE: &str = "ai.toolbay.desktop";
const KEYRING_USER: &str = "session";

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("ToolbayDesktop/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("http client")
    })
}

/// Base URL: TOOLBAY_API_BASE env var, then the "apiBase" setting, then toolbay.ai.
pub fn base_url(app: &AppHandle) -> String {
    std::env::var("TOOLBAY_API_BASE")
        .ok()
        .or_else(|| store::setting(app, "apiBase"))
        .unwrap_or_else(|| DEFAULT_BASE.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

pub fn token() -> Option<String> {
    entry()?.get_password().ok()
}

pub fn set_token(token: Option<&str>) -> Result<(), String> {
    let e = entry().ok_or("keychain unavailable")?;
    match token {
        Some(t) => e.set_password(t).map_err(|e| e.to_string()),
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        },
    }
}

#[derive(Serialize)]
pub struct RawResponse {
    pub status: u16,
    pub body: String,
}

pub async fn send(
    app: &AppHandle,
    method: &str,
    path: &str,
    body: Option<String>,
) -> Result<RawResponse, String> {
    if !path.starts_with('/') {
        return Err("path must start with /".into());
    }
    let url = format!("{}{}", base_url(app), path);
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = client()
        .request(method, url)
        .header("Accept", "application/json");
    if let Some(t) = token() {
        req = req.bearer_auth(t);
    }
    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b);
    }
    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let body = res.text().await.map_err(|e| e.to_string())?;
    Ok(RawResponse { status, body })
}

#[tauri::command]
pub async fn api_request(
    app: AppHandle,
    method: String,
    path: String,
    body: Option<String>,
) -> Result<RawResponse, String> {
    // Only the site API is reachable from the webview through this command.
    if !path.starts_with("/api/") {
        return Err("only /api/ paths are allowed".into());
    }
    send(&app, &method, &path, body).await
}
