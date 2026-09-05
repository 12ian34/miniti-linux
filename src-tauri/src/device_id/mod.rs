//! Stable device UUID for the `X-Device-ID` header (PLAN.md §5).
//!
//! Preferred storage is libsecret (see TODO); this ships the documented file
//! fallback under the XDG data dir so the id is stable across launches without
//! requiring a secret service (e.g. headless / minimal desktops).

use std::path::{Path, PathBuf};

/// `~/.local/share/miniti` (honors `XDG_DATA_HOME`).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("miniti")
}

pub fn device_id_path() -> PathBuf {
    data_dir().join("device_id")
}

/// Read the device id from `path`, creating and persisting a new UUID if absent.
pub fn get_or_create_at(path: &Path) -> std::io::Result<String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &id)?;
    Ok(id)
}

/// Default location. TODO: prefer libsecret via the `keyring` crate when a
/// secret service is available, falling back to this file otherwise.
pub fn get_or_create() -> std::io::Result<String> {
    get_or_create_at(&device_id_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_then_reuses_stable_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device_id");
        let a = get_or_create_at(&path).unwrap();
        let b = get_or_create_at(&path).unwrap();
        assert_eq!(a, b, "id must be stable across calls");
        assert_eq!(a.len(), 36, "uuid v4 string length");
    }

    #[test]
    fn regenerates_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device_id");
        std::fs::write(&path, "   ").unwrap();
        let id = get_or_create_at(&path).unwrap();
        assert_eq!(id.len(), 36);
    }
}
