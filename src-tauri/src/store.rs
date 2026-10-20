//! Small JSON files in the app data dir: settings.json and library.json.
//! (Tokens live in the OS keychain, see api.rs.)
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn data_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app data dir");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn tools_dir(app: &AppHandle) -> PathBuf {
    let dir = data_dir(app).join("tools");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: PathBuf) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let body = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

// ---- settings ----

pub type Settings = BTreeMap<String, String>;

pub fn settings(app: &AppHandle) -> Settings {
    read_json(data_dir(app).join("settings.json"))
}

pub fn setting(app: &AppHandle, key: &str) -> Option<String> {
    settings(app).get(key).cloned().filter(|v| !v.is_empty())
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    settings(&app)
}

#[tauri::command]
pub fn settings_set(app: AppHandle, key: String, value: String) -> Result<(), String> {
    let mut s = settings(&app);
    s.insert(key, value);
    write_json(data_dir(&app).join("settings.json"), &s)
}

// ---- library (installed tools) ----

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTool {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub path: String,
    pub launch_path: Option<String>,
    #[serde(default)]
    pub launch_args: Vec<String>,
    pub installed_at: String,
}

pub fn library(app: &AppHandle) -> Vec<InstalledTool> {
    read_json(data_dir(app).join("library.json"))
}

pub fn save_library(app: &AppHandle, lib: &[InstalledTool]) -> Result<(), String> {
    write_json(data_dir(app).join("library.json"), &lib)
}

#[tauri::command]
pub fn library_list(app: AppHandle) -> Vec<InstalledTool> {
    library(&app)
}
