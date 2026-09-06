//! Stable device UUID for the `X-Device-ID` header (PLAN.md §5).
//!
//! Preferred storage is the desktop secret service (libsecret / GNOME Keyring /
//! KWallet via the `keyring` crate). When no secret service is available the
//! id falls back to a file under the XDG data dir, and a file copy is always
//! kept so the id survives keyring resets. Both paths yield the same id.

use std::path::{Path, PathBuf};

const KEYRING_SERVICE: &str = "com.miniti.linux";
const KEYRING_USER: &str = "device-id";

/// `~/.local/share/miniti` (honors `XDG_DATA_HOME`).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("miniti")
}

pub fn device_id_path() -> PathBuf {
    data_dir().join("device_id")
}

fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

fn read_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| is_uuid(s))
}

fn write_file(path: &Path, id: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, id)
}

fn keyring_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

/// Secret-service calls run on their own thread with a deadline: the keyring
/// backend blocks on a private tokio runtime (panics inside an async Tauri
/// command) and a locked keyring may raise a prompt that never appears.
fn keyring_op<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("miniti-keyring".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .ok()?;
    rx.recv_timeout(std::time::Duration::from_secs(5)).ok()
}

#[allow(clippy::disallowed_methods)]
fn read_keyring() -> Option<String> {
    keyring_op(|| keyring_entry()?.get_password().ok())?
        .map(|s| s.trim().to_string())
        .filter(|s| is_uuid(s))
}

#[allow(clippy::disallowed_methods)]
fn write_keyring(id: &str) -> bool {
    let id = id.to_string();
    match keyring_op(move || keyring_entry().map(|e| e.set_password(&id))) {
        Some(Some(Ok(()))) => true,
        Some(Some(Err(e))) => {
            tracing::info!("secret service unavailable for device id ({e}); using file fallback");
            false
        }
        _ => false,
    }
}

/// Read the device id from `path`, creating and persisting a new UUID if absent.
/// File-only variant (no secret service) used by tests and as the fallback.
pub fn get_or_create_at(path: &Path) -> std::io::Result<String> {
    if let Some(existing) = read_file(path) {
        return Ok(existing);
    }
    let id = uuid::Uuid::new_v4().to_string();
    write_file(path, &id)?;
    Ok(id)
}

/// Keyring-first resolution with file mirror + fallback.
pub fn get_or_create() -> std::io::Result<String> {
    let path = device_id_path();
    if let Some(id) = read_keyring() {
        if read_file(&path).as_deref() != Some(id.as_str()) {
            let _ = write_file(&path, &id);
        }
        return Ok(id);
    }
    let id = get_or_create_at(&path)?;
    write_keyring(&id);
    Ok(id)
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
        assert!(is_uuid(&a));
    }

    #[test]
    fn regenerates_when_empty_or_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device_id");
        std::fs::write(&path, "   ").unwrap();
        assert!(is_uuid(&get_or_create_at(&path).unwrap()));
        std::fs::write(&path, "not-a-uuid").unwrap();
        let id = get_or_create_at(&path).unwrap();
        assert!(is_uuid(&id), "backend requires a valid UUID");
    }
}
