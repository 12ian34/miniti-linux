//! Credential storage and token lifecycle for device-bound authorization.
//!
//! Credentials (installation key secret, canonical recovery key, tokens) are
//! stored as one JSON document in the desktop secret service under
//! `com.miniti.linux` / `device-auth`. When no secret service is available the
//! document falls back to `~/.local/share/miniti/auth.json` with mode 0600
//! (the roadmap's "Keychain-backed fallback" equivalent); `AuthStatus.storage`
//! reports which one is in use so the UI can say so.
//!
//! Enrollment and token grants talk to `/api/auth/*` directly (no bearer);
//! everything else goes through `api::ApiClient`, which asks this manager for
//! a fresh access token and a request proof per call.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{
    format_recovery_key, generate_recovery_key, new_jti, parse_recovery_key, request_target,
    InstallationKey,
};
use crate::api::{endpoint_url, map_error, ApiError, HTTP_TIMEOUT, PLATFORM};
use crate::device_id::data_dir;
use crate::prefs::AppMode;

const KEYRING_SERVICE: &str = "com.miniti.linux";
const KEYRING_USER: &str = "device-auth";
/// Refresh the access token when within this many seconds of expiry.
const ACCESS_REFRESH_MARGIN_SECS: i64 = 60;
/// How long a secret-service call may block before we fall back to the file store.
const KEYRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Credentials {
    /// base64url P-256 secret scalar.
    pub installation_key: String,
    /// Canonical recovery key (`M1…`), shown only on explicit reveal.
    pub recovery_key: String,
    pub account_id: String,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub access_expires_at: Option<i64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub device_cap: Option<u32>,
    #[serde(default)]
    pub enrolled_at: String,
    /// Written before the enrollment request so a lost response cannot lose
    /// the only recovery key. A draft is never an enrollment: it has no
    /// account, no tokens, and a key the backend has not seen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
}

impl Credentials {
    /// Drafts from this version carry the flag; older drafts are recognised
    /// by having no account and no tokens.
    pub fn is_draft(&self) -> bool {
        self.draft
            || (self.account_id.is_empty()
                && self.access_token.is_none()
                && self.refresh_token.is_none())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageKind {
    SecretService,
    File,
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub enrolled: bool,
    /// The secret service has not answered yet; `enrolled` may still flip to true.
    pub keyring_pending: bool,
    pub account_id: Option<String>,
    pub device_cap: Option<u32>,
    pub storage: StorageKind,
    pub enrolled_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChallengeResponse {
    challenge_id: String,
    nonce: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    device_cap: Option<u32>,
}

// ---- Storage --------------------------------------------------------------------

pub fn auth_file_path() -> PathBuf {
    data_dir().join("auth.json")
}

/// Run a secret-service call on its own thread with a deadline.
///
/// The `keyring` secret-service backend blocks on a private tokio runtime,
/// which panics when invoked from a tokio worker (every async Tauri command
/// runs on one). A locked keyring can also raise a GUI unlock prompt that never
/// appears under a bare window manager, so a call that does not answer within
/// the deadline is treated as "no secret service" and the file store is used.
fn keyring_op<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("miniti-keyring".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .ok()?;
    match rx.recv_timeout(KEYRING_TIMEOUT) {
        Ok(v) => Some(v),
        Err(_) => {
            tracing::info!(
                "secret service did not answer within {}s; using file fallback",
                KEYRING_TIMEOUT.as_secs()
            );
            None
        }
    }
}

fn keyring_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

/// What the secret service said. `Ok(None)` is a definite "nothing stored";
/// `Err` is "could not ask" (no daemon, locked keyring, D-Bus timeout), which
/// must not be mistaken for "not enrolled": the credentials may well be there.
#[allow(clippy::disallowed_methods)]
fn read_keyring() -> Result<Option<Credentials>, String> {
    let outcome = keyring_op(|| match keyring_entry() {
        Some(entry) => entry.get_password().map(Some),
        None => Ok(None),
    });
    match outcome {
        None => Err("secret service did not answer in time".into()),
        Some(Ok(None)) => Ok(None),
        Some(Ok(Some(raw))) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| format!("stored credentials are unreadable: {e}")),
        Some(Err(keyring::Error::NoEntry)) => Ok(None),
        Some(Err(e)) => Err(e.to_string()),
    }
}

#[allow(clippy::disallowed_methods)]
fn write_keyring(creds: &Credentials) -> bool {
    let Ok(json) = serde_json::to_string(creds) else {
        return false;
    };
    match keyring_op(move || keyring_entry().map(|e| e.set_password(&json))) {
        Some(Some(Ok(()))) => true,
        Some(Some(Err(e))) => {
            tracing::info!("secret service unavailable for device auth ({e}); using file fallback");
            false
        }
        _ => false,
    }
}

#[allow(clippy::disallowed_methods)]
fn delete_keyring() {
    let _ = keyring_op(|| keyring_entry().map(|e| e.delete_credential()));
}

pub fn read_file(path: &Path) -> Option<Credentials> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// The file copy, but only if it is a finished enrollment. An interrupted
/// "create account" leaves a draft behind; loading that as credentials made
/// every signed request fail with `invalid_proof` (the backend never saw the
/// draft's key) while the real credentials sat in the secret service.
pub fn read_file_enrolled(path: &Path) -> Option<Credentials> {
    let creds = read_file(path)?;
    if creds.is_draft() {
        tracing::info!("device auth: ignoring an unfinished enrollment draft at {}", path.display());
        return None;
    }
    Some(creds)
}

pub fn write_file(path: &Path, creds: &Credentials) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(creds).map_err(std::io::Error::other)?;
    std::fs::write(path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

// ---- Manager ----------------------------------------------------------------------

pub struct AuthManager {
    base: String,
    device_id: String,
    app_version: String,
    http: reqwest::Client,
    creds: Mutex<Option<Credentials>>,
    storage: Mutex<StorageKind>,
    file_path: PathBuf,
    /// False in tests so nothing touches the real secret service.
    use_keyring: bool,
    /// The secret service could not be asked at load and no file copy exists:
    /// the answer to "enrolled?" is not known yet (see `retry_keyring`).
    keyring_pending: std::sync::atomic::AtomicBool,
    /// Serializes concurrent refreshes so one expiry triggers one grant.
    refresh_gate: tokio::sync::Mutex<()>,
}

impl AuthManager {
    /// Load stored credentials (secret service first, then the file fallback).
    pub fn load(
        base: impl Into<String>,
        device_id: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Self {
        let file_path = auth_file_path();
        let keyring = read_keyring();
        let (creds, storage) = match &keyring {
            Ok(Some(c)) => (Some(c.clone()), StorageKind::SecretService),
            _ => match read_file_enrolled(&file_path) {
                Some(c) => (Some(c), StorageKind::File),
                None => (None, StorageKind::None),
            },
        };
        let pending = creds.is_none() && keyring.is_err();
        if let Err(e) = &keyring {
            if pending {
                tracing::warn!("device auth: secret service unavailable ({e}); will keep asking before treating this device as new");
            } else {
                tracing::info!("device auth: secret service unavailable ({e}); using the file copy");
            }
        }
        let mut manager =
            Self::with_credentials(base, device_id, app_version, creds, storage, file_path);
        manager.use_keyring = true;
        manager
            .keyring_pending
            .store(pending, std::sync::atomic::Ordering::Relaxed);
        manager
    }

    /// True while the secret service has not yet answered whether this
    /// device holds credentials. The UI must not offer "create an account"
    /// in that state: the backend already knows this device.
    pub fn keyring_pending(&self) -> bool {
        self.keyring_pending.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Ask the secret service again. Returns true when credentials turned up
    /// (a keyring that unlocked after login, a daemon that started late).
    pub fn retry_keyring(&self) -> bool {
        if !self.keyring_pending() || !self.use_keyring {
            return false;
        }
        match read_keyring() {
            Ok(Some(c)) => {
                if let Ok(mut s) = self.storage.lock() {
                    *s = StorageKind::SecretService;
                }
                if let Ok(mut cur) = self.creds.lock() {
                    *cur = Some(c);
                }
                self.keyring_pending
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                tracing::info!("device auth: credentials found in the secret service on retry");
                true
            }
            Ok(None) => {
                self.keyring_pending
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                tracing::info!("device auth: secret service answered; this device holds no credentials");
                false
            }
            Err(e) => {
                tracing::debug!("device auth: secret service still unavailable ({e})");
                false
            }
        }
    }

    /// File-only manager (no secret service); used by tests and the live check.
    pub fn with_credentials(
        base: impl Into<String>,
        device_id: impl Into<String>,
        app_version: impl Into<String>,
        creds: Option<Credentials>,
        storage: StorageKind,
        file_path: PathBuf,
    ) -> Self {
        let app_version = app_version.into();
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(format!("miniti-Linux/{app_version}"))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base: base.into(),
            device_id: device_id.into(),
            app_version,
            http,
            creds: Mutex::new(creds),
            storage: Mutex::new(storage),
            file_path,
            use_keyring: false,
            keyring_pending: std::sync::atomic::AtomicBool::new(false),
            refresh_gate: tokio::sync::Mutex::new(()),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn is_enrolled(&self) -> bool {
        self.creds.lock().map(|c| c.is_some()).unwrap_or(false)
    }

    pub fn status(&self) -> AuthStatus {
        let creds = self.creds.lock().ok().and_then(|c| c.clone());
        AuthStatus {
            enrolled: creds.is_some(),
            keyring_pending: self.keyring_pending(),
            account_id: creds.as_ref().map(|c| c.account_id.clone()),
            device_cap: creds.as_ref().and_then(|c| c.device_cap),
            storage: self.storage.lock().map(|s| *s).unwrap_or(StorageKind::None),
            enrolled_at: creds
                .as_ref()
                .map(|c| c.enrolled_at.clone())
                .filter(|s| !s.is_empty()),
        }
    }

    /// Formatted recovery key for an explicit reveal.
    pub fn recovery_key(&self) -> Option<String> {
        self.creds
            .lock()
            .ok()?
            .as_ref()
            .map(|c| format_recovery_key(&c.recovery_key))
    }

    fn installation_key(&self) -> Result<InstallationKey, ApiError> {
        let creds = self.creds.lock().map_err(|_| ApiError::NotEnrolled)?;
        let creds = creds.as_ref().ok_or(ApiError::NotEnrolled)?;
        InstallationKey::from_secret_b64(&creds.installation_key).ok_or(ApiError::NotEnrolled)
    }

    /// Persist credentials: secret service when available, else the 0600 file.
    fn persist(&self, creds: Credentials) -> Result<(), ApiError> {
        let kind = if self.use_keyring && write_keyring(&creds) {
            // Never leave a stale plaintext copy behind once the secret service works.
            let _ = std::fs::remove_file(&self.file_path);
            StorageKind::SecretService
        } else {
            write_file(&self.file_path, &creds)
                .map_err(|e| ApiError::Network(format!("could not store credentials: {e}")))?;
            StorageKind::File
        };
        if let Ok(mut s) = self.storage.lock() {
            *s = kind;
        }
        if let Ok(mut c) = self.creds.lock() {
            *c = Some(creds);
        }
        Ok(())
    }

    fn update<F: FnOnce(&mut Credentials)>(&self, f: F) -> Result<(), ApiError> {
        let mut creds = {
            let guard = self.creds.lock().map_err(|_| ApiError::NotEnrolled)?;
            guard.clone().ok_or(ApiError::NotEnrolled)?
        };
        f(&mut creds);
        self.persist(creds)
    }

    /// Forget local credentials (after revoke / delete, or when the server says revoked).
    pub fn clear_local(&self) {
        if self.use_keyring {
            delete_keyring();
        }
        let _ = std::fs::remove_file(&self.file_path);
        if let Ok(mut c) = self.creds.lock() {
            *c = None;
        }
        if let Ok(mut s) = self.storage.lock() {
            *s = StorageKind::None;
        }
    }

    // ---- Unauthenticated calls ---------------------------------------------------

    fn common_headers(
        &self,
        req: reqwest::RequestBuilder,
        app_mode: AppMode,
    ) -> reqwest::RequestBuilder {
        req.header("X-Device-ID", &self.device_id)
            .header("X-App-Version", &self.app_version)
            .header("X-Platform", PLATFORM)
            .header(
                "X-App-Mode",
                match app_mode {
                    AppMode::Byok => "byok",
                    AppMode::Managed => "managed",
                },
            )
    }

    async fn post_public<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &serde_json::Value,
        app_mode: AppMode,
    ) -> Result<T, ApiError> {
        let url = endpoint_url(&self.base, path);
        let resp = self
            .common_headers(self.http.post(url), app_mode)
            .json(body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if let Some(err) = map_error(status, &text) {
            return Err(err);
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))
    }

    async fn challenge(
        &self,
        purpose: &str,
        app_mode: AppMode,
    ) -> Result<ChallengeResponse, ApiError> {
        self.post_public(
            "api/auth/challenge",
            &serde_json::json!({ "device_id": self.device_id, "purpose": purpose }),
            app_mode,
        )
        .await
    }

    async fn enroll(
        &self,
        path: &str,
        key: &InstallationKey,
        recovery_key: &str,
        label: Option<&str>,
        app_mode: AppMode,
    ) -> Result<TokenResponse, ApiError> {
        let challenge = self.challenge("enroll", app_mode).await?;
        let proof = key.challenge_proof(&challenge.nonce, "enroll", &self.device_id, now());
        self.post_public(
            path,
            &serde_json::json!({
                "device_id": self.device_id,
                "recovery_key": recovery_key,
                "public_key_jwk": key.public_jwk(),
                "challenge_id": challenge.challenge_id,
                "proof": proof,
                "label": label,
            }),
            app_mode,
        )
        .await
    }

    fn store_enrollment(
        &self,
        key: &InstallationKey,
        recovery_key: &str,
        tokens: TokenResponse,
    ) -> Result<(), ApiError> {
        let creds = Credentials {
            installation_key: key.secret_b64(),
            recovery_key: recovery_key.to_string(),
            account_id: tokens.account_id.clone().unwrap_or_default(),
            access_token: Some(tokens.access_token),
            access_expires_at: Some(now() + tokens.expires_in.unwrap_or(3600)),
            refresh_token: tokens.refresh_token,
            device_cap: tokens.device_cap,
            enrolled_at: chrono::Utc::now().to_rfc3339(),
            draft: false,
        };
        self.persist(creds)
    }

    /// Create a new anonymous account. Returns the formatted recovery key,
    /// which the UI must show once and ask the user to save.
    pub async fn create_account(
        &self,
        label: Option<&str>,
        app_mode: AppMode,
    ) -> Result<String, ApiError> {
        self.retry_keyring();
        if self.is_enrolled() {
            return Err(ApiError::AlreadyEnrolled);
        }
        let key = InstallationKey::generate();
        let recovery_key = generate_recovery_key();
        // Keep a local copy before the request so a lost response cannot lose the only key.
        let draft = Credentials {
            installation_key: key.secret_b64(),
            recovery_key: recovery_key.clone(),
            account_id: String::new(),
            access_token: None,
            access_expires_at: None,
            refresh_token: None,
            device_cap: None,
            enrolled_at: String::new(),
            draft: true,
        };
        let _ = write_file(&self.file_path, &draft);
        let tokens = self
            .enroll(
                "api/auth/account/create",
                &key,
                &recovery_key,
                label,
                app_mode,
            )
            .await?;
        self.store_enrollment(&key, &recovery_key, tokens)?;
        tracing::info!("device auth: account created");
        Ok(format_recovery_key(&recovery_key))
    }

    /// Attach this installation to an existing account with its recovery key.
    pub async fn restore_account(
        &self,
        recovery_key_input: &str,
        label: Option<&str>,
        app_mode: AppMode,
    ) -> Result<(), ApiError> {
        self.retry_keyring();
        if self.is_enrolled() {
            return Err(ApiError::AlreadyEnrolled);
        }
        let recovery_key =
            parse_recovery_key(recovery_key_input).ok_or(ApiError::InvalidRecoveryKey)?;
        let key = InstallationKey::generate();
        let tokens = self
            .enroll(
                "api/auth/account/restore",
                &key,
                &recovery_key,
                label,
                app_mode,
            )
            .await?;
        self.store_enrollment(&key, &recovery_key, tokens)?;
        tracing::info!("device auth: account restored");
        Ok(())
    }

    /// Record a rotated recovery key after the server accepted it.
    pub fn set_recovery_key(&self, canonical: &str) -> Result<(), ApiError> {
        let canonical = canonical.to_string();
        self.update(|c| c.recovery_key = canonical)
    }

    // ---- Tokens ---------------------------------------------------------------------------

    fn cached_token(&self) -> Option<(String, i64)> {
        let creds = self.creds.lock().ok()?;
        let creds = creds.as_ref()?;
        Some((creds.access_token.clone()?, creds.access_expires_at?))
    }

    /// A valid access token, refreshing when within the margin of expiry.
    pub async fn access_token(&self) -> Result<String, ApiError> {
        if !self.is_enrolled() {
            return Err(ApiError::NotEnrolled);
        }
        if let Some((token, exp)) = self.cached_token() {
            if exp - now() > ACCESS_REFRESH_MARGIN_SECS {
                return Ok(token);
            }
        }
        self.refresh(false).await
    }

    /// Discard the cached token and obtain a new one (after a 401).
    pub async fn force_refresh(&self) -> Result<String, ApiError> {
        self.refresh(true).await
    }

    async fn refresh(&self, force: bool) -> Result<String, ApiError> {
        let _gate = self.refresh_gate.lock().await;
        if !force {
            if let Some((token, exp)) = self.cached_token() {
                if exp - now() > ACCESS_REFRESH_MARGIN_SECS {
                    return Ok(token);
                }
            }
        }
        let refresh_token = self
            .creds
            .lock()
            .ok()
            .and_then(|c| c.as_ref().and_then(|c| c.refresh_token.clone()));
        let app_mode = AppMode::Managed;

        let result = match refresh_token {
            Some(rt) => match self.grant_refresh(&rt, app_mode).await {
                Ok(t) => Ok(t),
                Err(ApiError::Http { status: 401, .. }) | Err(ApiError::Unauthorized) => {
                    self.grant_challenge(app_mode).await
                }
                Err(e) => Err(e),
            },
            None => self.grant_challenge(app_mode).await,
        };
        match result {
            Ok(tokens) => {
                let access = tokens.access_token.clone();
                self.update(|c| {
                    c.access_token = Some(tokens.access_token);
                    c.access_expires_at = Some(now() + tokens.expires_in.unwrap_or(3600));
                    if tokens.refresh_token.is_some() {
                        c.refresh_token = tokens.refresh_token;
                    }
                    if tokens.device_cap.is_some() {
                        c.device_cap = tokens.device_cap;
                    }
                    if let Some(id) = tokens.account_id {
                        c.account_id = id;
                    }
                })?;
                Ok(access)
            }
            Err(ApiError::Http {
                status: 401, code, ..
            }) if code == "installation_revoked" => {
                tracing::warn!(
                    "device auth: installation revoked by the server; clearing local credentials"
                );
                self.clear_local();
                Err(ApiError::Revoked)
            }
            Err(e) => Err(e),
        }
    }

    async fn grant_refresh(
        &self,
        refresh_token: &str,
        app_mode: AppMode,
    ) -> Result<TokenResponse, ApiError> {
        self.post_public(
            "api/auth/token",
            &serde_json::json!({ "grant_type": "refresh", "device_id": self.device_id, "refresh_token": refresh_token }),
            app_mode,
        )
        .await
    }

    async fn grant_challenge(&self, app_mode: AppMode) -> Result<TokenResponse, ApiError> {
        let key = self.installation_key()?;
        let challenge = self.challenge("token", app_mode).await?;
        let proof = key.challenge_proof(&challenge.nonce, "token", &self.device_id, now());
        self.post_public(
            "api/auth/token",
            &serde_json::json!({ "grant_type": "challenge", "device_id": self.device_id, "challenge_id": challenge.challenge_id, "proof": proof }),
            app_mode,
        )
        .await
    }

    /// `X-Request-Proof` for an authenticated call.
    pub fn request_proof(
        &self,
        access_token: &str,
        method: &str,
        url: &str,
        body: &[u8],
    ) -> Result<String, ApiError> {
        let key = self.installation_key()?;
        Ok(key.request_proof(
            access_token,
            method,
            &request_target(url),
            body,
            now(),
            &new_jti(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Credentials {
        Credentials {
            installation_key: InstallationKey::generate().secret_b64(),
            recovery_key: generate_recovery_key(),
            account_id: "acct_x".into(),
            access_token: Some("tok".into()),
            access_expires_at: Some(now() + 3600),
            refresh_token: Some("mrt_x".into()),
            device_cap: Some(5),
            enrolled_at: "2026-09-06T00:00:00Z".into(),
            draft: false,
        }
    }

    #[test]
    fn file_round_trip_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let c = creds();
        write_file(&path, &c).unwrap();
        assert_eq!(read_file(&path), Some(c));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_file(&path), None);
    }

    #[test]
    fn a_draft_on_disk_is_not_an_enrollment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let mut draft = creds();
        draft.account_id.clear();
        draft.access_token = None;
        draft.refresh_token = None;
        draft.draft = true;
        write_file(&path, &draft).unwrap();
        assert!(read_file(&path).unwrap().is_draft());
        assert!(read_file_enrolled(&path).is_none(), "drafts must not load as credentials");
        // A draft written by an older version has no flag but the same shape.
        std::fs::write(
            &path,
            r#"{"installation_key":"x","recovery_key":"y","account_id":"","enrolled_at":""}"#,
        )
        .unwrap();
        assert!(read_file_enrolled(&path).is_none());
        // A real enrollment still loads, and the flag stays out of its JSON.
        write_file(&path, &creds()).unwrap();
        assert!(read_file_enrolled(&path).is_some());
        assert!(!std::fs::read_to_string(&path).unwrap().contains("draft"));
    }

    #[test]
    fn keyring_pending_is_off_for_file_backed_managers_and_retry_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let m = AuthManager::with_credentials(
            "https://api.test",
            "550e8400-e29b-41d4-a716-446655440000",
            "0.5.0",
            None,
            StorageKind::None,
            dir.path().join("auth.json"),
        );
        assert!(!m.status().keyring_pending);
        assert!(!m.retry_keyring(), "nothing to retry without a secret service");
        assert!(!m.is_enrolled());
    }

    #[test]
    fn status_and_reveal_reflect_loaded_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let c = creds();
        let m = AuthManager::with_credentials(
            "https://api.test",
            "550e8400-e29b-41d4-a716-446655440000",
            "0.3.0",
            Some(c.clone()),
            StorageKind::File,
            dir.path().join("auth.json"),
        );
        assert!(m.is_enrolled());
        let s = m.status();
        assert_eq!(s.account_id.as_deref(), Some("acct_x"));
        assert_eq!(s.storage, StorageKind::File);
        assert_eq!(
            m.recovery_key().as_deref(),
            Some(format_recovery_key(&c.recovery_key).as_str())
        );
        let proof = m
            .request_proof("tok", "POST", "https://api.test/api/session?x=1", b"{}")
            .unwrap();
        assert_eq!(proof.split('.').count(), 3);

        let empty = AuthManager::with_credentials(
            "https://api.test",
            "d",
            "0.3.0",
            None,
            StorageKind::None,
            dir.path().join("none.json"),
        );
        assert!(!empty.is_enrolled());
        assert!(matches!(
            empty.request_proof("t", "GET", "https://api.test/x", b""),
            Err(ApiError::NotEnrolled)
        ));
    }

    /// End-to-end interop check against the deployed backend: enroll a
    /// throwaway device, make a proof-bearing call, list devices, delete the
    /// account. Run with `cargo test live_ -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_enroll_against_backend() {
        use crate::api::{ApiClient, HeaderContext, DEFAULT_BASE_URL};
        let base =
            std::env::var("MINITI_API_BASE").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let dir = tempfile::tempdir().unwrap();
        let device_id = uuid::Uuid::new_v4().to_string();
        let manager = std::sync::Arc::new(AuthManager::with_credentials(
            &base,
            &device_id,
            "0.3.0",
            None,
            StorageKind::None,
            dir.path().join("auth.json"),
        ));

        let recovery = manager
            .create_account(Some("live-test (Linux)"), AppMode::Managed)
            .await
            .expect("create account");
        assert!(
            super::parse_recovery_key(&recovery).is_some(),
            "server accepted our key format"
        );
        assert!(manager.is_enrolled());
        assert!(matches!(
            manager.create_account(None, AppMode::Managed).await,
            Err(ApiError::AlreadyEnrolled)
        ));

        let client = ApiClient::new(
            &base,
            HeaderContext {
                device_id: device_id.clone(),
                app_version: "0.3.0".into(),
                app_mode: AppMode::Managed,
            },
            manager.clone(),
        );
        let usage = client.get_usage().await.expect("usage with bearer token");
        assert!(usage.minutes_limit.is_some() || usage.tier.is_some());
        let version = client.get_version().await.expect("version");
        assert!(!version.min_version.is_empty());
        let devices = client
            .auth_devices()
            .await
            .expect("devices (GET, no proof)");
        assert_eq!(devices.devices.len(), 1);
        assert!(devices.devices[0].current);

        // Proof-bearing POST: rotate the recovery key, then confirm the old one no longer restores.
        let rotated = super::generate_recovery_key();
        client
            .auth_rotate_recovery_key(&rotated)
            .await
            .expect("rotate (POST + proof)");
        manager.set_recovery_key(&rotated).unwrap();
        assert_eq!(
            manager.recovery_key().as_deref(),
            Some(super::format_recovery_key(&rotated).as_str())
        );

        // Forced refresh exercises the refresh grant and the challenge fallback path.
        let first = manager.access_token().await.unwrap();
        let second = manager.force_refresh().await.expect("refresh grant");
        assert_ne!(first, second);
        assert!(client.get_usage().await.is_ok());

        client
            .auth_delete_account()
            .await
            .expect("delete account (DELETE + proof)");
        manager.clear_local();
        assert!(!manager.is_enrolled());
    }

    /// Regression: the secret-service backend blocks on a private tokio runtime,
    /// which panics if it runs on a tokio worker. `keyring_op` must keep it off
    /// the runtime. Linux only: the Apple backend never had the problem, and a
    /// read of a probe entry touches nothing in the developer's keyring.
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn keyring_calls_from_the_async_runtime_do_not_panic() {
        #[allow(clippy::disallowed_methods)]
        let probed = keyring_op(|| {
            keyring::Entry::new(KEYRING_SERVICE, "test-probe")
                .map(|e| e.get_password().is_ok())
                .unwrap_or(false)
        });
        // Either answer is fine (no secret service on CI); what matters is that we got here.
        let _ = probed;
    }
}
