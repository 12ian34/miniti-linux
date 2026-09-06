//! User preferences persisted as JSON under the XDG config dir
//! (`~/.config/miniti/prefs.json`). BYOK keys live here on disk (0600); the
//! device UUID lives separately (see `device_id`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Terms version the app currently requires acceptance of (gate 2).
pub const CURRENT_TERMS_VERSION: &str = "1.0";

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
    /// Filler overrides for the current language (empty = language default).
    pub filler_overrides: Vec<String>,
    pub smart_meetings_enabled: bool,
    pub show_tray: bool,
    pub show_floating_indicator: bool,
    pub capture_system_audio: bool,
    /// Terms version the user accepted (None = never).
    pub accepted_terms_version: Option<String>,
    pub onboarding_complete: bool,
    /// Live insights during recording (Summary / Questions / …).
    pub live_insights_enabled: bool,
    /// New meetings start with Sales (MEDDPICC) analysis enabled.
    pub sales_insights_default: bool,
    /// Folder used for codebase investigations.
    pub codebase_root: Option<String>,
    /// Personal dictionary terms sent as Deepgram keyterms.
    pub personal_dictionary: Vec<String>,
    /// Desktop notifications for Smart-meeting decisions and live guidance.
    pub notifications_enabled: bool,
    /// Opt-in live guidance (question / monologue / filler nudges).
    pub live_guidance_enabled: bool,
    /// Silence auto-stop in minutes (0 = off; macOS offers 3/5/10/15).
    pub auto_stop_minutes: i64,
    /// Calendar automation: auto-start upcoming events / auto-stop at the end.
    pub calendar_auto_start: bool,
    pub calendar_auto_stop: bool,
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
            accepted_terms_version: None,
            onboarding_complete: false,
            live_insights_enabled: true,
            sales_insights_default: false,
            codebase_root: None,
            personal_dictionary: Vec::new(),
            notifications_enabled: true,
            live_guidance_enabled: false,
            auto_stop_minutes: 0,
            calendar_auto_start: false,
            calendar_auto_stop: false,
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
        std::fs::write(path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn load() -> Self {
        Self::load_from(&prefs_path())
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&prefs_path())
    }

    pub fn byok_deepgram_key(&self) -> Option<&str> {
        self.byok_deepgram_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
    }

    /// True when the app can transcribe: managed always can; BYOK needs a key.
    pub fn can_transcribe(&self) -> bool {
        match self.app_mode {
            AppMode::Managed => true,
            AppMode::Byok => self.byok_deepgram_key().is_some(),
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
        assert!(p.accepted_terms_version.is_none());
        assert!(!p.onboarding_complete);
    }

    #[test]
    fn byok_requires_key() {
        let mut p = Prefs {
            app_mode: AppMode::Byok,
            ..Default::default()
        };
        assert!(!p.can_transcribe());
        p.byok_deepgram_key = Some("   ".into());
        assert!(!p.can_transcribe(), "whitespace is not a key");
        p.byok_deepgram_key = Some("dg_key".into());
        assert!(p.can_transcribe());
    }

    #[test]
    fn roundtrip_and_missing_file_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        assert_eq!(Prefs::load_from(&path).language, "en");

        let mut p = Prefs::default();
        p.app_mode = AppMode::Byok;
        p.webhook_url = Some("https://example.com/hook".into());
        p.accepted_terms_version = Some("1.0".into());
        p.save_to(&path).unwrap();

        let loaded = Prefs::load_from(&path);
        assert_eq!(loaded.app_mode, AppMode::Byok);
        assert_eq!(loaded.webhook_url.as_deref(), Some("https://example.com/hook"));
        assert_eq!(loaded.accepted_terms_version.as_deref(), Some("1.0"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "keys on disk must be owner-only");
        }
    }

    #[test]
    fn older_prefs_files_load_with_new_fields_defaulted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        std::fs::write(&path, r#"{"app_mode":"byok","language":"de"}"#).unwrap();
        let p = Prefs::load_from(&path);
        assert_eq!(p.language, "de");
        assert!(!p.onboarding_complete);
    }

    #[test]
    fn app_mode_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&AppMode::Byok).unwrap(), "\"byok\"");
        assert_eq!(serde_json::to_string(&AppMode::Managed).unwrap(), "\"managed\"");
    }
}
