//! Settings that follow the person across computers (toolbay.ai tool-sync API, slug "toolbay-hub").
//!
//! Items:
//! - `settings`: hub preferences that should match everywhere ({"autoUpdate": bool}).
//! - `library/<computerId>`: {"computer": "<name>", "tools": ["toolbay-flow", ...]}, one per computer,
//!   so two computers never write the same key and Library can offer "On your other computers".
//!
//! Loop: pull on start and every 5 minutes; push `settings` when it changes and this computer's
//! library item when the library changes. Conflicts on `settings`: the local change wins (it's what
//! the person just did), retried against the server's version.
use crate::{api, store};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const PATH: &str = "/api/v1/me/tools/toolbay-hub/sync";
const SYNCED_SETTINGS: &[&str] = &["autoUpdate"];

fn computer_id(app: &AppHandle) -> String {
    if let Some(id) = store::setting(app, "computerId") {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = store::settings_set(app.clone(), "computerId".into(), id.clone());
    id
}

fn computer_name() -> String {
    // Mac apps get no HOSTNAME; the name people gave their Mac is in scutil.
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output() {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !name.is_empty() {
            return name;
        }
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Another computer".into())
}

fn version_of(app: &AppHandle, key: &str) -> u64 {
    store::setting(app, &format!("syncVersion:{key}")).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn set_version(app: &AppHandle, key: &str, v: u64) {
    let _ = store::settings_set(app.clone(), format!("syncVersion:{key}"), v.to_string());
}

/// Pull what changed since last time and apply it.
pub async fn pull(app: &AppHandle) {
    if api::token().is_none() {
        return;
    }
    let since = store::setting(app, "syncRev").unwrap_or_default();
    let path = if since.is_empty() { PATH.to_string() } else { format!("{PATH}?since={since}") };
    let Ok(res) = api::send(app, "GET", &path, None).await else { return };
    if res.status != 200 {
        return;
    }
    let Ok(doc) = serde_json::from_str::<Value>(&res.body) else { return };
    let mine = format!("library/{}", computer_id(app));
    let mut others: Value = store::setting(app, "remoteLibraries")
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));
    if doc["full"].as_bool() == Some(true) {
        others = json!({});
    }
    if let Some(items) = doc["items"].as_object() {
        for (key, item) in items {
            let version = item["version"].as_u64().unwrap_or(0);
            if key == "settings" {
                set_version(app, key, version);
                // Remember what the server has, so applying it doesn't trigger a push back.
                let _ = store::settings_set(app.clone(), "syncSettingsSeen".into(), item["value"].to_string());
                if let Some(obj) = item["value"].as_object() {
                    for k in SYNCED_SETTINGS {
                        if let Some(v) = obj.get(*k) {
                            let s = match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            let _ = store::settings_set(app.clone(), (*k).into(), s);
                        }
                    }
                }
                set_version(app, key, version);
            } else if key == &mine {
                set_version(app, key, version);
            } else if let Some(id) = key.strip_prefix("library/") {
                if item["deleted"].as_bool() == Some(true) || item["value"].is_null() {
                    others.as_object_mut().map(|o| o.remove(id));
                } else {
                    others[id] = item["value"].clone();
                }
            }
        }
    }
    let _ = store::settings_set(app.clone(), "remoteLibraries".into(), others.to_string());
    if let Some(rev) = doc["rev"].as_u64() {
        let _ = store::settings_set(app.clone(), "syncRev".into(), rev.to_string());
    }
    app.emit("sync-changed", ()).ok();
}

async fn put(app: &AppHandle, key: &str, value: Value) {
    if api::token().is_none() {
        return;
    }
    for _ in 0..3 {
        let body = json!({ "changes": [{ "key": key, "baseVersion": version_of(app, key), "value": value }] });
        let Ok(res) = api::send(app, "PUT", PATH, Some(body.to_string())).await else { return };
        if res.status != 200 {
            return;
        }
        let Ok(doc) = serde_json::from_str::<Value>(&res.body) else { return };
        if let Some(v) = doc["applied"][key]["version"].as_u64() {
            set_version(app, key, v);
            return;
        }
        match doc["conflicts"].get(key) {
            // Someone else wrote it first: retry on top of their version (our change wins).
            Some(Value::Object(server)) => set_version(app, key, server.get("version").and_then(Value::as_u64).unwrap_or(0)),
            Some(Value::Null) => set_version(app, key, 0),
            _ => return,
        }
    }
}

/// Push hub settings that should match on every computer.
pub fn push_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut v = json!({});
        for k in SYNCED_SETTINGS {
            if let Some(s) = store::setting(&app, k) {
                v[*k] = serde_json::from_str(&s).unwrap_or(Value::String(s));
            }
        }
        if store::setting(&app, "syncSettingsSeen").as_deref() == Some(v.to_string().as_str()) {
            return;
        }
        put(&app, "settings", v.clone()).await;
        let _ = store::settings_set(app.clone(), "syncSettingsSeen".into(), v.to_string());
    });
}

/// Push which tools this computer has.
pub fn push_library(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tools: Vec<String> = store::library(&app).into_iter().map(|t| t.slug).collect();
        tools.sort();
        let value = json!({ "computer": computer_name(), "tools": tools });
        // Only write when it changed since the last successful push.
        if store::setting(&app, "syncLibraryPushed").as_deref() == Some(value.to_string().as_str()) {
            return;
        }
        let key = format!("library/{}", computer_id(&app));
        put(&app, &key, value.clone()).await;
        if version_of(&app, &key) > 0 {
            let _ = store::settings_set(app.clone(), "syncLibraryPushed".into(), value.to_string());
        }
    });
}

pub fn is_synced_setting(key: &str) -> bool {
    SYNCED_SETTINGS.contains(&key)
}

/// Tools installed on the person's other computers but not this one: [{slug, computer}].
#[tauri::command]
pub fn sync_other_tools(app: AppHandle) -> Vec<Value> {
    let local: Vec<String> = store::library(&app).into_iter().map(|t| t.slug).collect();
    let others: Value = store::setting(&app, "remoteLibraries")
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));
    let mut out: Vec<Value> = vec![];
    for (_, lib) in others.as_object().into_iter().flatten() {
        let computer = lib["computer"].as_str().unwrap_or("another computer");
        for slug in lib["tools"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if !local.iter().any(|s| s == slug) && !out.iter().any(|o| o["slug"] == slug) {
                out.push(json!({ "slug": slug, "computer": computer }));
            }
        }
    }
    out
}

/// Pull now (e.g. right after sign-in) and every 5 minutes after.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            pull(&app).await;
            push_library(&app);
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    });
}
