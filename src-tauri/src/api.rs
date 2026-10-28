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

/// macOS: the token is a 0600 file, `~/Library/Application Support/ai.toolbay.desktop/session`,
/// that the Toolbay tools read too. Not the keychain: the apps are ad-hoc signed, so every
/// update looks like a new app to the keychain and macOS asked for the login password.
#[cfg(target_os = "macos")]
fn token_file() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join("Library/Application Support").join(KEYRING_SERVICE).join(KEYRING_USER))
}

#[cfg(target_os = "macos")]
pub fn token() -> Option<String> {
    let t = std::fs::read_to_string(token_file()?).ok()?;
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(target_os = "macos")]
pub fn set_token(token: Option<&str>) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = token_file().ok_or("no home folder")?;
    match token {
        Some(t) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let tmp = path.with_extension("tmp");
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            f.write_all(t.as_bytes()).map_err(|e| e.to_string())?;
            drop(f);
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

#[cfg(not(target_os = "macos"))]
fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

#[cfg(not(target_os = "macos"))]
pub fn token() -> Option<String> {
    entry()?.get_password().ok()
}

#[cfg(not(target_os = "macos"))]
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

const PACKAGE_MAX: u64 = 80 * 1024 * 1024;

/// Upload an app package (.zip) to toolbay.ai (POST /api/v1/packages, multipart field "package").
/// Done here because the webview can't attach the Bearer token to a cross-origin upload.
#[tauri::command]
pub async fn package_upload(app: AppHandle, path: String) -> Result<RawResponse, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        return Err("That isn't a .zip file. Drag in the zip your AI made.".into());
    }
    let meta = std::fs::metadata(&p).map_err(|_| "Couldn't read that file. Try dragging it in again.".to_string())?;
    if meta.len() > PACKAGE_MAX {
        return Err("That zip is over 80 MB. Ask your AI to leave out big files it doesn't need.".into());
    }
    if token().is_none() {
        return Err("Sign in first, then drag the zip in again.".into());
    }
    let bytes = tokio::fs::read(&p).await.map_err(|_| "Couldn't read that file. Try again.".to_string())?;
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "package.zip".into());
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(name)
        .mime_str("application/zip")
        .map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new().part("package", part);
    let res = reqwest::Client::builder()
        .user_agent(concat!("ToolbayDesktop/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?
        .post(format!("{}/api/v1/packages", base_url(&app)))
        .bearer_auth(token().unwrap_or_default())
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await
        .map_err(|_| "The upload stopped. Check your internet connection and try again.".to_string())?;
    let status = res.status().as_u16();
    let body = res.text().await.map_err(|e| e.to_string())?;
    Ok(RawResponse { status, body })
}
