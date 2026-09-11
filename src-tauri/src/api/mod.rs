//! Miniti backend client (PLAN.md §5, contract: `../miniti-api/docs/agents/04-api-reference.md`).
//! Header construction, URL building, session expiry parsing and error mapping
//! are pure + tested; the reqwest methods perform live calls. Authentication is
//! device-bound (`crate::auth`): no shared secret is compiled into the binary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::auth::manager::AuthManager;
use crate::prefs::AppMode;

pub const DEFAULT_BASE_URL: &str = "https://api.miniti.app";
pub const PLATFORM: &str = "linux";
/// Refresh the session token if within this many seconds of expiry.
pub const REFRESH_MARGIN_SECS: i64 = 60;
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// `/api/insights` runs on the node runtime with a 60 s budget.
pub const INSIGHTS_TIMEOUT: Duration = Duration::from_secs(75);
/// Back off at least this long after a 429 that carried no usable `Retry-After`.
pub const MIN_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(5);
/// Never hold an endpoint back longer than this, whatever the header said.
pub const MAX_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(600);

/// Values needed to identify every request. Authentication itself comes from
/// the device-bound `auth::manager::AuthManager` (bearer token + request proof).
#[derive(Debug, Clone)]
pub struct HeaderContext {
    pub device_id: String,
    pub app_version: String,
    pub app_mode: AppMode,
}

fn app_mode_header(mode: AppMode) -> &'static str {
    match mode {
        AppMode::Byok => "byok",
        AppMode::Managed => "managed",
    }
}

/// Build the identity header name/value pairs sent on every backend request.
pub fn build_header_pairs(ctx: &HeaderContext) -> Vec<(String, String)> {
    vec![
        ("X-Device-ID".into(), ctx.device_id.clone()),
        ("X-App-Version".into(), ctx.app_version.clone()),
        ("X-Platform".into(), PLATFORM.into()),
        ("X-App-Mode".into(), app_mode_header(ctx.app_mode).into()),
    ]
}

pub fn endpoint_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Parse an RFC 3339 / ISO 8601 timestamp (as the backend emits) to unix secs.
pub fn parse_iso8601(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.timestamp())
}

// ---- Response models ------------------------------------------------------

/// `GET /api/version`. Supported clients get `latest_version == their version`
/// and empty notes; only `min_version` gates.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VersionInfo {
    #[serde(default)]
    pub latest_version: String,
    #[serde(default)]
    pub min_version: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub release_notes: String,
}

/// `GET /api/usage`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub minutes_used: f64,
    #[serde(default)]
    pub minutes_limit: Option<f64>,
    /// ISO 8601.
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub subscription_status: Option<String>,
    /// Pro comes from another device on this account (the backend sends the
    /// field only then). This device has no subscription of its own, so the
    /// Polar portal would answer `404 no_subscription`.
    #[serde(default)]
    pub entitlement_via_account: bool,
    /// Only present on metered tiers; absent means unmetered (Pro).
    #[serde(default)]
    pub docs_lookups_used: Option<i64>,
    #[serde(default)]
    pub docs_lookups_limit: Option<i64>,
}

/// `POST /api/session`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Session {
    /// Managed Deepgram grant JWT; connect with `Authorization: Bearer`.
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    /// ISO 8601 (authoritative when present).
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub session_id: Option<String>,
    /// Deprecated alias for `access_token`; prefer `access_token`.
    #[serde(default)]
    pub temp_api_key: Option<String>,
}

impl Session {
    /// Preferred token: `access_token`, falling back to the deprecated alias.
    pub fn token(&self) -> Option<&str> {
        self.access_token
            .as_deref()
            .or(self.temp_api_key.as_deref())
    }

    /// Absolute expiry (unix secs): `expires_at` if parseable, else `now + expires_in`.
    pub fn expiry_at(&self, now: i64) -> Option<i64> {
        self.expires_at
            .as_deref()
            .and_then(parse_iso8601)
            .or_else(|| self.expires_in.filter(|d| *d > 0).map(|d| now + d))
    }
}

/// True when the token should be refreshed before reconnecting.
pub fn needs_refresh(expiry_at: i64, now: i64) -> bool {
    expiry_at - now <= REFRESH_MARGIN_SECS
}

/// `POST /api/session/end`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SessionEnd {
    #[serde(default)]
    pub minutes_used: f64,
    #[serde(default)]
    pub minutes_remaining: Option<f64>,
    #[serde(default)]
    pub session_finalized: bool,
    #[serde(default)]
    pub idempotent_replay: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResponseMeta {
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub fallback_reason: Option<String>,
    #[serde(default)]
    pub request_seq: Option<i64>,
}

/// Raw `/api/insights` response: the body shape differs per mode (flat
/// standard schema vs `docs` / `catchup` / `investigation`), so the caller
/// decodes `value` per mode. `meta` is common to all.
#[derive(Debug, Clone, Serialize)]
pub struct InsightsResponse {
    pub value: serde_json::Value,
    pub meta: ResponseMeta,
}

// ---- Errors ---------------------------------------------------------------

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiError {
    #[error("managed mode needs a miniti account on this device: create or restore a recovery key in Settings → Account")]
    NotEnrolled,
    #[error("this device is already enrolled")]
    AlreadyEnrolled,
    #[error("the miniti backend already knows this computer, but its credentials are not on this machine (a locked, reset, or missing keyring does that). Restore with the account's recovery key; if it is lost, remove this device from the account on another device first")]
    EnrolledElsewhere,
    #[error("that recovery key is not valid (check for typos; it starts with M1)")]
    InvalidRecoveryKey,
    #[error("this device's access was revoked; create or restore a recovery key to continue")]
    Revoked,
    #[error("this computer's stored key no longer matches its miniti account (an interrupted enrollment, or a restore from another copy of this installation). Sign out in Settings → Account, then restore with your recovery key")]
    KeyMismatch,
    #[error("this device has been disabled")]
    DeviceDisabled,
    #[error("that account is full{}. Remove a device from it first, then try again", cap_hint(.device_cap))]
    DeviceCapReached { device_cap: Option<u32> },
    #[error("both this device and that account hold a subscription. Cancel one of them before moving this device")]
    SubscriptionConflict,
    #[error("that account is busy with another change; try again in a moment")]
    Busy,
    #[error("no account uses that recovery key (check for typos; it starts with M1)")]
    RecoveryKeyUnknown,
    #[error("monthly managed minutes used up{}", resets_hint(.resets_at))]
    LimitReached { resets_at: Option<String> },
    #[error("rate limited{}", retry_hint(.retry_after_s))]
    RateLimited {
        /// Whole seconds from the backend's `Retry-After`, when it sent one.
        retry_after_s: Option<u64>,
    },
    #[error("backend rejected this device's credentials (unauthorized)")]
    Unauthorized,
    #[error("backend error {status}: {code}{}", message_hint(.message))]
    Http {
        status: u16,
        code: String,
        message: Option<String>,
    },
    #[error("network error: {0}")]
    Network(String),
    #[error("decode error: {0}")]
    Decode(String),
}

fn cap_hint(device_cap: &Option<u32>) -> String {
    device_cap
        .map(|c| format!(" ({c} devices)"))
        .unwrap_or_default()
}

/// "; try again in 12s" when the backend said how long, else a vague nudge.
fn retry_hint(retry_after_s: &Option<u64>) -> String {
    match retry_after_s {
        Some(s) if *s > 0 => format!("; try again in {s}s"),
        _ => "; try again shortly".to_string(),
    }
}

fn resets_hint(resets_at: &Option<String>) -> String {
    resets_at
        .as_ref()
        .map(|r| format!(" (resets {r})"))
        .unwrap_or_default()
}

fn message_hint(message: &Option<String>) -> String {
    message
        .as_ref()
        .filter(|m| !m.is_empty())
        .map(|m| format!(": {m}"))
        .unwrap_or_default()
}

#[derive(Debug, Deserialize, Default)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    device_cap: Option<u32>,
}

/// Map a non-2xx status + body to a domain error (contract §Error format).
pub fn map_error(status: u16, body: &str) -> Option<ApiError> {
    map_error_with_retry(status, body, None)
}

/// `Retry-After` in whole seconds, as every 429 from miniti-api carries.
/// A date form (RFC 7231 allows one) is not parsed; the caller falls back to
/// the client's own minimum backoff.
pub fn parse_retry_after(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

/// [`map_error`] plus the `Retry-After` header, which only 429 carries.
pub fn map_error_with_retry(
    status: u16,
    body: &str,
    retry_after_s: Option<u64>,
) -> Option<ApiError> {
    if (200..=299).contains(&status) {
        return None;
    }
    let parsed: ErrorBody = serde_json::from_str(body).unwrap_or_default();
    Some(match (status, parsed.error.as_str()) {
        (401, "unauthorized") | (401, "") => ApiError::Unauthorized,
        (401, "installation_revoked") | (401, "device_auth_required") => ApiError::Revoked,
        (401, "invalid_proof") => ApiError::KeyMismatch,
        (401, code) => ApiError::Http {
            status: 401,
            code: code.into(),
            message: parsed.message,
        },
        (402, _) => ApiError::LimitReached {
            resets_at: parsed.resets_at,
        },
        (403, _) => ApiError::DeviceDisabled,
        (400, "invalid_recovery_key") => ApiError::InvalidRecoveryKey,
        (404, "recovery_key_unknown") => ApiError::RecoveryKeyUnknown,
        (409, "device_already_enrolled") => ApiError::EnrolledElsewhere,
        (409, "device_cap_reached") => ApiError::DeviceCapReached {
            device_cap: parsed.device_cap,
        },
        (409, "subscription_conflict") => ApiError::SubscriptionConflict,
        (409, "busy") => ApiError::Busy,
        (429, _) => ApiError::RateLimited { retry_after_s },
        (s, code) => ApiError::Http {
            status: s,
            code: if code.is_empty() {
                "unknown".into()
            } else {
                code.into()
            },
            message: parsed.message,
        },
    })
}

/// Status-only mapping kept for callers without a body.
pub fn map_status(status: u16) -> Option<ApiError> {
    map_error(status, "")
}

// ---- Live client ----------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthDevice {
    pub installation_id: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub enrolled_at: String,
    #[serde(default)]
    pub last_auth_at: Option<String>,
    #[serde(default)]
    pub current: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AuthDevices {
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub device_cap: Option<u32>,
    #[serde(default)]
    pub recovery_version: Option<u32>,
    #[serde(default)]
    pub devices: Vec<AuthDevice>,
}

/// `POST /api/auth/account/attach`: the usual token object plus `moved`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AttachResult {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub device_cap: Option<u32>,
    /// False when the device was already on that account: nothing changed.
    #[serde(default)]
    pub moved: bool,
}

/// Live HTTP client. Construction is cheap; calls need network + an enrolled
/// installation (except `get_version`, which is public metadata).
/// Per-endpoint "do not call before" clock, shared by every clone of the
/// client. A 429 parks its endpoint for at least the backend's `Retry-After`,
/// so a UI that remounts in a loop costs one request instead of hundreds
/// (0.4.1 sent 974 device-list requests in three hours, 814 of them refused).
#[derive(Debug, Default)]
pub struct RateLimitGate {
    until: Mutex<HashMap<String, Instant>>,
}

impl RateLimitGate {
    /// Seconds still to wait before `key` may be called again.
    pub fn remaining_secs(&self, key: &str, now: Instant) -> Option<u64> {
        let map = self.until.lock().ok()?;
        let until = map.get(key)?;
        let left = until.saturating_duration_since(now);
        (!left.is_zero()).then(|| left.as_secs().max(1))
    }

    /// Park `key` for the backend's `Retry-After`, clamped to sane bounds.
    pub fn park(&self, key: &str, retry_after_s: Option<u64>, now: Instant) -> u64 {
        let wait = retry_after_s
            .map(Duration::from_secs)
            .unwrap_or(MIN_RATE_LIMIT_BACKOFF)
            .clamp(MIN_RATE_LIMIT_BACKOFF, MAX_RATE_LIMIT_BACKOFF);
        if let Ok(mut map) = self.until.lock() {
            let until = now + wait;
            // A later deadline wins; a retry must never shorten an active park.
            let entry = map.entry(key.to_string()).or_insert(until);
            if until > *entry {
                *entry = until;
            }
        }
        wait.as_secs()
    }

    /// Forget an endpoint's park (used after a success and in tests).
    pub fn clear(&self, key: &str) {
        if let Ok(mut map) = self.until.lock() {
            map.remove(key);
        }
    }
}

/// The gate key for a request: method + path, so `/api/usage` and
/// `/api/auth/account/devices` back off independently.
pub fn gate_key(method: &str, url: &str) -> String {
    let path = url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.to_string());
    format!("{method} {path}")
}

#[derive(Clone)]
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
    ctx: HeaderContext,
    auth: Arc<AuthManager>,
    gate: Arc<RateLimitGate>,
}

impl ApiClient {
    pub fn new(base: impl Into<String>, ctx: HeaderContext, auth: Arc<AuthManager>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(format!("miniti-Linux/{}", ctx.app_version))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base: base.into(),
            http,
            ctx,
            auth,
            gate: Arc::new(RateLimitGate::default()),
        }
    }

    pub fn rate_limit_gate(&self) -> &Arc<RateLimitGate> {
        &self.gate
    }

    pub fn context(&self) -> &HeaderContext {
        &self.ctx
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn auth(&self) -> &Arc<AuthManager> {
        &self.auth
    }

    /// GET an absolute URL and decode JSON (integrations).
    pub async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, ApiError> {
        self.send_json(reqwest::Method::GET, url, None, None).await
    }

    /// POST JSON to an absolute URL and decode JSON (integrations).
    pub async fn post_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<T, ApiError> {
        self.send_json(reqwest::Method::POST, url, Some(body), None)
            .await
    }

    /// DELETE an absolute URL and decode JSON.
    pub async fn delete_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
    ) -> Result<T, ApiError> {
        self.send_json(reqwest::Method::DELETE, url, None, None)
            .await
    }

    fn identity_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in build_header_pairs(&self.ctx) {
            req = req.header(k, v);
        }
        req
    }

    /// One authenticated attempt: bearer token + request proof (non-GET).
    /// Returns the status, the body, and `Retry-After` in seconds when sent.
    async fn attempt(
        &self,
        method: &reqwest::Method,
        url: &str,
        body: &Option<Vec<u8>>,
        timeout: Option<Duration>,
        token: Option<&str>,
    ) -> Result<(u16, String, Option<u64>), ApiError> {
        let mut req = self.identity_headers(self.http.request(method.clone(), url));
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        if let Some(token) = token {
            req = req.bearer_auth(token);
            if *method != reqwest::Method::GET && *method != reqwest::Method::HEAD {
                let proof = self.auth.request_proof(
                    token,
                    method.as_str(),
                    url,
                    body.as_deref().unwrap_or(&[]),
                )?;
                req = req.header("X-Request-Proof", proof);
            }
        }
        if let Some(bytes) = body {
            req = req
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(bytes.clone());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Ok((status, text, retry_after))
    }

    /// Authenticated request with one retry after a token rejection.
    async fn send_text(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<String, ApiError> {
        let bytes = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|e| ApiError::Decode(e.to_string()))?;
        // A parked endpoint is refused here, without a request: the backend
        // asked for the wait, and a caller in a loop must not spend it anyway.
        let key = gate_key(method.as_str(), url);
        if let Some(left) = self.gate.remaining_secs(&key, Instant::now()) {
            return Err(ApiError::RateLimited {
                retry_after_s: Some(left),
            });
        }
        let mut token = self.auth.access_token().await?;
        for attempt in 0..2 {
            let (status, text, retry_after) = self
                .attempt(&method, url, &bytes, timeout, Some(&token))
                .await?;
            if status == 429 {
                let waited = self.gate.park(&key, retry_after, Instant::now());
                tracing::warn!("{key} rate limited; backing off {waited}s");
                return Err(ApiError::RateLimited {
                    retry_after_s: Some(waited),
                });
            }
            if status == 401 && attempt == 0 {
                let code = error_code(&text);
                if code == "token_expired" || code == "invalid_token" {
                    token = self.auth.force_refresh().await?;
                    continue;
                }
                if code == "installation_revoked" {
                    self.auth.clear_local();
                    return Err(ApiError::Revoked);
                }
            }
            if let Some(err) = map_error_with_retry(status, &text, retry_after) {
                return Err(err);
            }
            self.gate.clear(&key);
            return Ok(text);
        }
        Err(ApiError::Unauthorized)
    }

    async fn send_json<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<T, ApiError> {
        let text = self.send_text(method, url, body, timeout).await?;
        serde_json::from_str::<T>(&text).map_err(|e| ApiError::Decode(e.to_string()))
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, ApiError> {
        self.send_json(
            reqwest::Method::POST,
            &endpoint_url(&self.base, path),
            Some(body),
            None,
        )
        .await
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        self.send_json(
            reqwest::Method::GET,
            &endpoint_url(&self.base, path),
            None,
            None,
        )
        .await
    }

    /// Public version metadata: authenticated when enrolled (so the device is
    /// touched for the dashboard), anonymous otherwise.
    pub async fn get_version(&self) -> Result<VersionInfo, ApiError> {
        let url = endpoint_url(&self.base, "api/version");
        if self.auth.is_enrolled() {
            if let Ok(v) = self.get::<VersionInfo>("api/version").await {
                return Ok(v);
            }
        }
        let (status, text, retry_after) = self
            .attempt(&reqwest::Method::GET, &url, &None, None, None)
            .await?;
        if let Some(err) = map_error_with_retry(status, &text, retry_after) {
            return Err(err);
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))
    }

    pub async fn get_usage(&self) -> Result<Usage, ApiError> {
        self.get("api/usage").await
    }

    /// Start a managed transcription session (Deepgram grant JWT).
    pub async fn create_session(&self, language: &str) -> Result<Session, ApiError> {
        self.post(
            "api/session",
            &serde_json::json!({ "model": "nova-3", "language": language }),
        )
        .await
    }

    /// Report session duration. Idempotent per `(device, session_id)`.
    pub async fn end_session(
        &self,
        session_id: &str,
        duration_minutes: f64,
    ) -> Result<SessionEnd, ApiError> {
        let body = serde_json::json!({
            "session_id": session_id,
            "duration_minutes": duration_minutes,
        });
        self.post("api/session/end", &body).await
    }

    /// Managed insights proxy. `body` should come from `insights::InsightRequest`.
    pub async fn post_insights(
        &self,
        body: &serde_json::Value,
    ) -> Result<InsightsResponse, ApiError> {
        let url = endpoint_url(&self.base, "api/insights");
        let text = self
            .send_text(
                reqwest::Method::POST,
                &url,
                Some(body),
                Some(INSIGHTS_TIMEOUT),
            )
            .await?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))?;
        let meta = value
            .get("meta")
            .cloned()
            .map(serde_json::from_value::<ResponseMeta>)
            .transpose()
            .map_err(|e| ApiError::Decode(e.to_string()))?
            .unwrap_or_default();
        Ok(InsightsResponse { value, meta })
    }

    /// Polar checkout URL for Pro (open in the system browser).
    pub async fn subscribe_url(&self) -> Result<String, ApiError> {
        #[derive(Deserialize)]
        struct R {
            checkout_url: String,
        }
        self.get::<R>("api/subscribe").await.map(|r| r.checkout_url)
    }

    /// Polar customer portal URL.
    pub async fn portal_url(&self) -> Result<String, ApiError> {
        #[derive(Deserialize)]
        struct R {
            portal_url: String,
        }
        self.get::<R>("api/portal").await.map(|r| r.portal_url)
    }

    /// Restore Pro on this device with a Polar license key.
    pub async fn restore(&self, license_key: &str) -> Result<serde_json::Value, ApiError> {
        self.post(
            "api/restore",
            &serde_json::json!({ "license_key": license_key }),
        )
        .await
    }

    /// Structured client reliability events (never transcript or audio).
    /// Batched by the caller; the backend rate limits to 30/min per device.
    pub async fn post_client_events(&self, body: &serde_json::Value) -> Result<(), ApiError> {
        self.post::<serde_json::Value>("api/client-events", body)
            .await
            .map(|_| ())
    }

    // ---- Account (device-bound auth) ------------------------------------------

    pub async fn auth_devices(&self) -> Result<AuthDevices, ApiError> {
        self.get("api/auth/account/devices").await
    }

    pub async fn auth_remove_device(&self, installation_id: &str) -> Result<(), ApiError> {
        let url = endpoint_url(
            &self.base,
            &format!("api/auth/account/devices/{installation_id}"),
        );
        self.delete_json::<serde_json::Value>(&url)
            .await
            .map(|_| ())
    }

    /// Ask the server to accept a new (client-generated, canonical) recovery key.
    pub async fn auth_rotate_recovery_key(&self, canonical: &str) -> Result<(), ApiError> {
        self.post::<serde_json::Value>(
            "api/auth/account/recovery-key/rotate",
            &serde_json::json!({ "new_recovery_key": canonical }),
        )
        .await
        .map(|_| ())
    }

    /// Move this installation to the account that owns `recovery_key`, without
    /// signing out: the device keeps its id, usage, integrations and local
    /// meetings, and only account membership changes.
    pub async fn auth_attach(&self, recovery_key: &str) -> Result<AttachResult, ApiError> {
        self.post(
            "api/auth/account/attach",
            &serde_json::json!({ "recovery_key": recovery_key }),
        )
        .await
    }

    /// Sign this installation out server-side.
    pub async fn auth_revoke(&self) -> Result<(), ApiError> {
        self.post::<serde_json::Value>("api/auth/revoke", &serde_json::json!({}))
            .await
            .map(|_| ())
    }

    /// Delete the anonymous account (revokes every installation).
    pub async fn auth_delete_account(&self) -> Result<(), ApiError> {
        let url = endpoint_url(&self.base, "api/auth/account");
        self.delete_json::<serde_json::Value>(&url)
            .await
            .map(|_| ())
    }
}

fn error_code(body: &str) -> String {
    serde_json::from_str::<ErrorBody>(body)
        .map(|b| b.error)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_include_linux_platform_and_mode() {
        let ctx = HeaderContext {
            device_id: "D".into(),
            app_version: "0.1.0".into(),
            app_mode: AppMode::Byok,
        };
        let pairs = build_header_pairs(&ctx);
        let get = |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("X-Platform").as_deref(), Some("linux"));
        assert_eq!(get("X-App-Mode").as_deref(), Some("byok"));
        assert!(get("X-API-Key").is_none(), "no shared secret is ever sent");
        assert_eq!(get("X-Device-ID").as_deref(), Some("D"));
    }

    #[test]
    fn endpoint_url_joins_cleanly() {
        assert_eq!(
            endpoint_url("https://api.miniti.app/", "/api/version"),
            "https://api.miniti.app/api/version"
        );
        assert_eq!(
            endpoint_url("https://api.miniti.app", "api/usage"),
            "https://api.miniti.app/api/usage"
        );
    }

    #[test]
    fn error_mapping_uses_status_and_body() {
        assert!(map_error(200, "").is_none());
        assert!(matches!(
            map_error(
                402,
                r#"{"error":"limit_reached","resets_at":"2026-10-01T00:00:00.000Z"}"#
            ),
            Some(ApiError::LimitReached { resets_at: Some(_) })
        ));
        assert!(matches!(
            map_error(403, r#"{"error":"device_disabled"}"#),
            Some(ApiError::DeviceDisabled)
        ));
        assert!(matches!(
            map_error(429, ""),
            Some(ApiError::RateLimited {
                retry_after_s: None
            })
        ));
        assert!(matches!(
            map_error(401, r#"{"error":"unauthorized"}"#),
            Some(ApiError::Unauthorized)
        ));
        assert!(matches!(
            map_error(409, r#"{"error":"device_already_enrolled","message":"This installation is already enrolled; restore or revoke it first"}"#),
            Some(ApiError::EnrolledElsewhere)
        ));
        assert!(matches!(
            map_error(401, r#"{"error":"invalid_proof","message":"proof of possession failed"}"#),
            Some(ApiError::KeyMismatch)
        ));
        match map_error(500, r#"{"error":"internal_error","message":"boom"}"#) {
            Some(ApiError::Http {
                status,
                code,
                message,
            }) => {
                assert_eq!(status, 500);
                assert_eq!(code, "internal_error");
                assert_eq!(message.as_deref(), Some("boom"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            map_status(500),
            Some(ApiError::Http { status: 500, .. })
        ));
    }

    #[test]
    fn retry_after_is_parsed_and_surfaced_in_the_message() {
        assert_eq!(parse_retry_after(" 12 "), Some(12));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        let err = map_error_with_retry(429, r#"{"error":"rate_limited"}"#, Some(12)).unwrap();
        assert!(matches!(
            err,
            ApiError::RateLimited {
                retry_after_s: Some(12)
            }
        ));
        assert!(
            err.to_string().contains("try again in 12s"),
            "the wait is shown where a user action triggered it: {err}"
        );
        let vague = map_error_with_retry(429, "", None).unwrap();
        assert!(vague.to_string().contains("try again shortly"));
    }

    #[test]
    fn a_parked_endpoint_is_refused_without_a_request() {
        let gate = RateLimitGate::default();
        let key = gate_key("GET", "https://api.miniti.app/api/auth/account/devices?x=1");
        assert_eq!(key, "GET /api/auth/account/devices");
        let t0 = Instant::now();
        assert_eq!(gate.remaining_secs(&key, t0), None, "clear by default");

        assert_eq!(gate.park(&key, Some(30), t0), 30);
        assert_eq!(gate.remaining_secs(&key, t0), Some(30));
        // Another endpoint is unaffected.
        assert_eq!(gate.remaining_secs(&gate_key("GET", "/api/usage"), t0), None);
        // A later 429 never shortens an active park.
        gate.park(&key, Some(5), t0);
        assert_eq!(gate.remaining_secs(&key, t0), Some(30));
        // The park expires on its own.
        assert_eq!(gate.remaining_secs(&key, t0 + Duration::from_secs(31)), None);

        // No header: the client's own minimum, and a silly value is clamped.
        let other = gate_key("POST", "https://api.miniti.app/api/session");
        assert_eq!(gate.park(&other, None, t0), MIN_RATE_LIMIT_BACKOFF.as_secs());
        gate.clear(&other);
        assert_eq!(
            gate.park(&other, Some(86_400), t0),
            MAX_RATE_LIMIT_BACKOFF.as_secs()
        );
        gate.clear(&other);
        assert_eq!(gate.remaining_secs(&other, t0), None);
    }

    #[test]
    fn attach_errors_map_to_their_own_messages() {
        assert!(matches!(
            map_error(400, r#"{"error":"invalid_recovery_key"}"#),
            Some(ApiError::InvalidRecoveryKey)
        ));
        assert!(matches!(
            map_error(404, r#"{"error":"recovery_key_unknown"}"#),
            Some(ApiError::RecoveryKeyUnknown)
        ));
        assert!(matches!(
            map_error(409, r#"{"error":"device_cap_reached","device_cap":3}"#),
            Some(ApiError::DeviceCapReached {
                device_cap: Some(3)
            })
        ));
        assert!(matches!(
            map_error(409, r#"{"error":"subscription_conflict"}"#),
            Some(ApiError::SubscriptionConflict)
        ));
        assert!(matches!(
            map_error(409, r#"{"error":"busy"}"#),
            Some(ApiError::Busy)
        ));
        // The existing enrollment conflict keeps its own meaning.
        assert!(matches!(
            map_error(409, r#"{"error":"device_already_enrolled"}"#),
            Some(ApiError::EnrolledElsewhere)
        ));
    }

    #[test]
    fn attach_decodes_the_token_object_and_moved_flag() {
        let r: AttachResult = serde_json::from_str(
            r#"{"access_token":"eyJ.jwt","expires_in":3600,"refresh_token":"rt",
                "account_id":"acct_2","device_cap":5,"moved":true}"#,
        )
        .unwrap();
        assert_eq!(r.account_id.as_deref(), Some("acct_2"));
        assert_eq!(r.device_cap, Some(5));
        assert!(r.moved);
        let already: AttachResult =
            serde_json::from_str(r#"{"access_token":"eyJ.jwt","moved":false}"#).unwrap();
        assert!(!already.moved, "already on that account");
    }

    #[test]
    fn session_decodes_real_backend_shape() {
        // Exactly what `POST /api/session` returns: expires_at is ISO 8601.
        let raw = r#"{
            "access_token": "eyJ.jwt",
            "token_type": "Bearer",
            "expires_in": 3600,
            "expires_at": "2026-07-20T22:00:00.000Z",
            "session_id": "sess_abc123def456",
            "temp_api_key": "eyJ.jwt"
        }"#;
        let s: Session = serde_json::from_str(raw).expect("must decode ISO expires_at");
        assert_eq!(s.token(), Some("eyJ.jwt"));
        assert_eq!(s.session_id.as_deref(), Some("sess_abc123def456"));
        let expected = parse_iso8601("2026-07-20T22:00:00.000Z").unwrap();
        assert_eq!(
            s.expiry_at(0),
            Some(expected),
            "expires_at is authoritative"
        );
    }

    #[test]
    fn session_falls_back_to_expires_in_and_legacy_alias() {
        let legacy = Session {
            temp_api_key: Some("legacy".into()),
            expires_in: Some(3600),
            ..Default::default()
        };
        assert_eq!(legacy.token(), Some("legacy"));
        assert_eq!(legacy.expiry_at(1000), Some(4600));

        let none = Session::default();
        assert_eq!(none.expiry_at(1000), None);
    }

    #[test]
    fn iso8601_parsing() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:10.000Z"), Some(10));
        assert_eq!(parse_iso8601("1970-01-01T01:00:00+01:00"), Some(0));
        assert_eq!(parse_iso8601("not a date"), None);
    }

    #[test]
    fn refresh_window() {
        assert!(needs_refresh(1000, 950)); // 50s left -> refresh
        assert!(!needs_refresh(1000, 800)); // 200s left -> ok
    }

    #[test]
    fn version_and_usage_decode_documented_shapes() {
        let v: VersionInfo = serde_json::from_str(
            r#"{"latest_version":"1.0.0","min_version":"1.0.0","download_url":"https://miniti.app","release_notes":""}"#,
        )
        .unwrap();
        assert_eq!(v.min_version, "1.0.0");
        let u: Usage = serde_json::from_str(
            r#"{"minutes_used":142.5,"minutes_limit":500,"resets_at":"2026-03-01T00:00:00.000Z","tier":"free",
                "subscription_status":null,"docs_lookups_used":3,"docs_lookups_limit":10}"#,
        )
        .unwrap();
        assert_eq!(u.minutes_limit, Some(500.0));
        assert_eq!(u.docs_lookups_limit, Some(10));
        assert!(!u.entitlement_via_account, "absent means this device's own");
        let sponsored: Usage = serde_json::from_str(
            r#"{"minutes_used":10,"minutes_limit":5000,"tier":"pro","entitlement_via_account":true}"#,
        )
        .unwrap();
        assert!(sponsored.entitlement_via_account);
        assert_eq!(sponsored.tier.as_deref(), Some("pro"));
    }
}
