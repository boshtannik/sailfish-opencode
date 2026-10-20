//! User settings — the single source of the settings JSON key names.
//!
//! `serde` generates every key from a field name here, so neither the worker
//! nor the bridge writes a `["workdir"]` magic string anywhere.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_workdir")]
    pub workdir: String,
    /// Server protocol adapter to use: `"v1"` (opencode 1.x) or `"v2"`
    /// (opencode 2.x, default). Kept as a free string so an unknown value can be
    /// logged and safely fall back to v1 instead of failing to load.
    #[serde(default = "default_api_version")]
    pub api_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            workdir: default_workdir(),
            api_version: default_api_version(),
        }
    }
}

/// Default working directory for `opencode serve`.
pub fn default_workdir() -> String {
    "/home/defaultuser/opencode-home".to_string()
}

/// Default protocol adapter — v2 (opencode 2.x).
pub fn default_api_version() -> String {
    "v2".to_string()
}

fn settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/defaultuser".into());
    PathBuf::from(home)
        .join(".config/harbour-opencode")
        .join("settings.json")
}

impl Settings {
    /// An empty workdir breaks `opencode serve`: the child's `chdir("")`
    /// fails with ENOENT and ServerGuard never starts. `serde(default)`
    /// only covers a MISSING key — a stale `{"workdir":""}` (written by the
    /// settings page when the field loses focus while empty) slipped through.
    fn sanitize(&mut self) {
        if self.workdir.trim().is_empty() {
            log::warn!("settings: workdir is empty — falling back to the default");
            self.workdir = default_workdir();
        }
    }

    /// Read settings from disk; missing keys/file fall back to defaults.
    pub fn load() -> Settings {
        let p = settings_path();
        let mut st: Settings = std::fs::read_to_string(&p)
            .ok()
            .and_then(|txt| serde_json::from_str(&txt).ok())
            .unwrap_or_default();
        st.sanitize();
        st
    }

    /// Write settings to disk, creating the directory if needed.
    pub fn save(&mut self) {
        self.sanitize();
        let p = settings_path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(txt) = serde_json::to_string(self) {
            let _ = std::fs::write(p, txt);
        }
    }

    /// JSON representation handed to QML.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
