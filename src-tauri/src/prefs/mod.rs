//! User preferences persisted as JSON under the XDG config dir
//! (`~/.config/miniti/prefs.json`). Secrets (BYOK keys) live here on disk; the
//! device UUID lives separately (see `device_id`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    #[default]
    Managed,
    Byok,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub app_mode: AppMode,
    pub language: String,
    pub byok_deepgram_key: Option<String>,
    pub byok_openai_key: Option<String>,
    pub webhook_url: Option<String>,
    pub docs_mcp_url: Option<String>,
    pub export_folder: Option<String>,
    pub filler_overrides: Vec<String>,
    pub smart_meetings_enabled: bool,
    pub show_tray: bool,
    pub show_floating_indicator: bool,
    pub capture_system_audio: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            app_mode: AppMode::Managed,
            language: "en".to_string(),
            byok_deepgram_key: None,
            byok_openai_key: None,
            webhook_url: None,
            docs_mcp_url: None,
            export_folder: None,
            filler_overrides: Vec::new(),
            smart_meetings_enabled: false,
            show_tray: true,
            show_floating_indicator: true,
            capture_system_audio: true,
        }
    }
}

/// `~/.config/miniti` (honors `XDG_CONFIG_HOME`).
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("miniti")
}

pub fn prefs_path() -> PathBuf {
    config_dir().join("prefs.json")
}

impl Prefs {
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Prefs::default(),
        }
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).expect("prefs serialize");
        std::fs::write(path, json)
    }

    pub fn load() -> Self {
        Self::load_from(&prefs_path())
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&prefs_path())
    }

    /// True when the app can transcribe: managed always can; BYOK needs a key.
    pub fn can_transcribe(&self) -> bool {
        match self.app_mode {
            AppMode::Managed => true,
            AppMode::Byok => self
                .byok_deepgram_key
                .as_deref()
                .map(|k| !k.trim().is_empty())
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let p = Prefs::default();
        assert_eq!(p.app_mode, AppMode::Managed);
        assert_eq!(p.language, "en");
        assert!(p.show_tray);
        assert!(p.can_transcribe(), "managed can always transcribe");
    }

    #[test]
    fn byok_requires_key() {
        let mut p = Prefs {
            app_mode: AppMode::Byok,
            ..Default::default()
        };
        assert!(!p.can_transcribe());
        p.byok_deepgram_key = Some("dg_key".into());
        assert!(p.can_transcribe());
    }

    #[test]
    fn roundtrip_and_missing_file_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        assert_eq!(Prefs::load_from(&path).language, "en"); // missing -> default

        let mut p = Prefs::default();
        p.app_mode = AppMode::Byok;
        p.webhook_url = Some("https://example.com/hook".into());
        p.save_to(&path).unwrap();

        let loaded = Prefs::load_from(&path);
        assert_eq!(loaded.app_mode, AppMode::Byok);
        assert_eq!(loaded.webhook_url.as_deref(), Some("https://example.com/hook"));
    }

    #[test]
    fn app_mode_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&AppMode::Byok).unwrap(), "\"byok\"");
        assert_eq!(serde_json::to_string(&AppMode::Managed).unwrap(), "\"managed\"");
    }
}
