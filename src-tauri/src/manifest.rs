//! toolbay.json: how a tool tells the hub what to install.
//!
//! Every tool repo ships a toolbay.json at its root and attaches the same file
//! to each GitHub Release. The hub reads it from the latest release, picks the
//! entry for this computer, and installs everything in one go (program + files
//! such as AI models). Agreed with the toolbay-flow session; see AGENTS.md.
//!
//! {
//!   "schema": 1,
//!   "id": "flow",
//!   "name": "Toolbay Flow",
//!   "version": "0.1.0",
//!   "repo": "sriptcollector/toolbay-flow",
//!   "identifier": "ai.toolbay.flow",
//!   "platforms": {
//!     "windows-x86_64": { "asset": "Toolbay.Flow_0.1.0_x64-setup.exe", "kind": "nsis",
//!                         "size": 1234, "sha256": "…", "silentArgs": ["/S"], "exe": "toolbay-flow.exe" },
//!     "darwin-aarch64": { "asset": "Toolbay.Flow_0.1.0_aarch64.dmg", "kind": "dmg", "app": "Toolbay Flow.app" }
//!   },
//!   "files": [ { "url": "https://…/ggml-base.en.bin", "size": 147964211, "sha256": "…",
//!                "path": "models/ggml-base.en.bin" } ],
//!   "launch": { "args": ["--from-toolbay"] }
//! }
use crate::api;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default = "one")]
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub repo: String,
    /// The tool's own app identifier; extra files go into its app data dir.
    #[serde(default)]
    pub identifier: Option<String>,
    pub platforms: HashMap<String, PlatformEntry>,
    #[serde(default)]
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub launch: Launch,
}

fn one() -> u32 {
    1
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlatformEntry {
    /// Release asset name. Or give `url` directly.
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// nsis | msi | zip | exe | dmg
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub silent_args: Vec<String>,
    /// Program to start, relative to the install folder (Windows / zip).
    #[serde(default)]
    pub exe: Option<String>,
    /// .app bundle name inside the dmg (macOS).
    #[serde(default)]
    pub app: Option<String>,
    /// Where the installer puts itself by default, e.g. "%LOCALAPPDATA%\Toolbay Flow". The hub asks
    /// for its own folder with /D=, but an installer that finds an earlier install reuses that folder.
    #[serde(default)]
    pub install_dir: Option<String>,
    /// Uninstaller file name in the install folder (NSIS). Defaults to uninstall.exe.
    #[serde(default)]
    pub uninstaller: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub url: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    /// Relative to the tool's app data dir.
    pub path: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Launch {
    #[serde(default)]
    pub args: Vec<String>,
}

/// This computer, in the manifest's platform-key format.
pub fn platform_key() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    format!("{os}-{arch}")
}

/// A manifest plus the resolved download URL for this computer.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub manifest: Manifest,
    pub entry: PlatformEntry,
    pub download_url: String,
    pub release_tag: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

const NOT_AVAILABLE: &str = "This tool isn't available to download yet. Please check back soon.";
const NO_INTERNET: &str = "Couldn't reach the download server. Check your internet connection and try again.";

async fn get_text(url: &str) -> Result<(u16, String), String> {
    if let Some(path) = url.strip_prefix("file://") {
        // Local manifests, for development and tests.
        let path = path.trim_start_matches('/');
        let path = if cfg!(windows) { path.to_string() } else { format!("/{path}") };
        return std::fs::read_to_string(&path).map(|s| (200, s)).map_err(|_| NOT_AVAILABLE.to_string());
    }
    let res = api::client()
        .get(url)
        .header("Accept", "application/vnd.github+json, application/json")
        .send()
        .await
        .map_err(|_| NO_INTERNET.to_string())?;
    let status = res.status().as_u16();
    Ok((status, res.text().await.map_err(|_| NO_INTERNET.to_string())?))
}

/// Resolve from a GitHub repo's latest release ("owner/repo"),
/// or from a manifest URL (https:// or file://) whose entries carry `url`.
pub async fn resolve(source: &str) -> Result<Resolved, String> {
    let (manifest, release, tag) = if source.contains("://") {
        let (status, body) = get_text(source).await?;
        if status != 200 {
            return Err(NOT_AVAILABLE.into());
        }
        (parse(&body)?, None, None)
    } else {
        let repo = source.trim_matches('/');
        if repo.split('/').count() != 2 {
            return Err(NOT_AVAILABLE.into());
        }
        let (status, body) = get_text(&format!("https://api.github.com/repos/{repo}/releases/latest")).await?;
        if status == 403 || status == 429 {
            return Err("Too many update checks right now. Please try again in a few minutes.".into());
        }
        if status != 200 {
            return Err(NOT_AVAILABLE.into());
        }
        let release: Release = serde_json::from_str(&body).map_err(|_| NOT_AVAILABLE.to_string())?;
        let m = release
            .assets
            .iter()
            .find(|a| a.name == "toolbay.json")
            .ok_or_else(|| NOT_AVAILABLE.to_string())?;
        let (status, body) = get_text(&m.browser_download_url).await?;
        if status != 200 {
            return Err(NOT_AVAILABLE.into());
        }
        let tag = release.tag_name.clone();
        (parse(&body)?, Some(release), Some(tag))
    };

    let key = platform_key();
    let mut entry = manifest
        .platforms
        .get(&key)
        .cloned()
        .ok_or_else(|| format!("{} isn't available for this computer yet.", manifest.name))?;
    let download_url = match (&entry.url, &entry.asset, &release) {
        (Some(u), _, _) => u.clone(),
        (None, Some(name), Some(rel)) => {
            let a = rel.assets.iter().find(|a| &a.name == name).ok_or_else(|| NOT_AVAILABLE.to_string())?;
            entry.size = entry.size.or(Some(a.size));
            a.browser_download_url.clone()
        }
        _ => return Err(NOT_AVAILABLE.into()),
    };
    Ok(Resolved { manifest, entry, download_url, release_tag: tag })
}

fn parse(body: &str) -> Result<Manifest, String> {
    let m: Manifest = serde_json::from_str(body).map_err(|_| NOT_AVAILABLE.to_string())?;
    if m.schema != 1 {
        return Err("This tool needs a newer version of Toolbay. Update Toolbay from Settings, then try again.".into());
    }
    Ok(m)
}

/// "v0.10.2" > "0.9.9". Non-numeric parts compare as 0.
pub fn newer(candidate: &str, installed: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(candidate) > parts(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("v0.1.1", "0.1.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "v0.2.0"));
    }

    #[test]
    fn parses_flow_manifest() {
        let m = parse(include_str!("../../src/fixtures/toolbay-flow.toolbay.json")).unwrap();
        assert_eq!(m.id, "flow");
        assert!(m.platforms.contains_key("windows-x86_64"));
    }
}
