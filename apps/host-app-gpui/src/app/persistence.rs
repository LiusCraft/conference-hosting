use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::mcp::McpServerConfig;

const APP_SETTINGS_PATH_ENV: &str = "HOST_APP_CONFIG_PATH";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct PersistedAppSettings {
    #[serde(default)]
    pub(crate) ws: PersistedWsSettings,
    #[serde(default)]
    pub(crate) ui: PersistedUiSettings,
    #[serde(default)]
    pub(crate) mcp_servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct PersistedWsSettings {
    #[serde(default)]
    pub(crate) server_url: String,
    #[serde(default)]
    pub(crate) device_id: String,
    #[serde(default)]
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) auth_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct PersistedUiSettings {
    #[serde(default)]
    pub(crate) aec_enabled: Option<bool>,
    #[serde(default)]
    pub(crate) show_ai_emotion_messages: Option<bool>,
}

pub(crate) fn load_persisted_app_settings() -> PersistedAppSettings {
    let path = persisted_settings_path();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            eprintln!(
                "[mcp][load] settings file not found, use defaults path={}",
                path.display()
            );
            return PersistedAppSettings::default();
        }
        Err(error) => {
            eprintln!(
                "[mcp][load] read settings failed path={} error={error}",
                path.display()
            );
            return PersistedAppSettings::default();
        }
    };

    match serde_json::from_str::<PersistedAppSettings>(&raw) {
        Ok(settings) => {
            let enabled_count = settings
                .mcp_servers
                .iter()
                .filter(|server| server.enabled)
                .count();
            eprintln!(
                "[mcp][load] loaded settings path={} servers={} enabled={} aliases={}",
                path.display(),
                settings.mcp_servers.len(),
                enabled_count,
                settings
                    .mcp_servers
                    .iter()
                    .map(|server| server.alias.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            settings
        }
        Err(error) => {
            eprintln!(
                "[mcp][load] parse settings failed path={} error={error}",
                path.display()
            );
            PersistedAppSettings::default()
        }
    }
}

pub(crate) fn save_persisted_app_settings(settings: &PersistedAppSettings) -> Result<(), String> {
    let path = persisted_settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("create settings dir `{}` failed: {error}", parent.display())
        })?;
    }

    let payload = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("serialize app settings failed: {error}"))?;
    fs::write(&path, payload)
        .map_err(|error| format!("write app settings `{}` failed: {error}", path.display()))?;

    let enabled_count = settings
        .mcp_servers
        .iter()
        .filter(|server| server.enabled)
        .count();
    eprintln!(
        "[mcp][save] persisted settings path={} servers={} enabled={}",
        path.display(),
        settings.mcp_servers.len(),
        enabled_count
    );
    Ok(())
}

fn persisted_settings_path() -> PathBuf {
    if let Ok(explicit_path) = std::env::var(APP_SETTINGS_PATH_ENV) {
        let explicit_path = explicit_path.trim();
        if !explicit_path.is_empty() {
            return PathBuf::from(explicit_path);
        }
    }

    if let Some(home_dir) = std::env::var_os("HOME").map(PathBuf::from) {
        return home_dir
            .join(".conference-hosting")
            .join("host-app-gpui")
            .join("settings.json");
    }

    PathBuf::from("host-app-gpui.settings.json")
}
