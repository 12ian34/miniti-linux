//! Miniti backend client (PLAN.md §5, contract: `../miniti-api/docs/agents/04-api-reference.md`).
//! Header construction, URL building, key obfuscation, session expiry parsing and
//! error mapping are pure + tested; the reqwest methods perform live calls.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::prefs::AppMode;

pub const DEFAULT_BASE_URL: &str = "https://api.miniti.app";
pub const PLATFORM: &str = "linux";
/// Refresh the session token if within this many seconds of expiry.
pub const REFRESH_MARGIN_SECS: i64 = 60;
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// `/api/insights` runs on the node runtime with a 60 s budget.
pub const INSIGHTS_TIMEOUT: Duration = Duration::from_secs(75);

// Build-time embedded, XOR-obfuscated app secret (see build.rs). Empty when the
// build was produced without `MINITI_API_KEY`.
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/api_key.rs"));
}

/// The shared backend app secret compiled into this build, if any.
pub fn embedded_api_key() -> Option<String> {
    if embedded::API_KEY_OBF.is_empty() {
        return None;
    }
    let key = deobfuscate_api_key(embedded::API_KEY_OBF, embedded::API_KEY_MASK);
    if key.trim().is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Values needed to authenticate every request.
#[derive(Debug, Clone)]
pub struct HeaderContext {
    /// Deobfuscated shared app secret (`X-API-Key`).
    pub api_key: String,
    pub device_id: String,
    pub app_version: String,
    pub app_mode: AppMode,
}

/// XOR-obfuscate/deobfuscate the shared app key. This is *not* real security —
/// it only keeps the literal secret out of `strings` output, matching the other
/// clients. Symmetric: the same call encodes and decodes.
pub fn xor_transform(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect()
}

/// Recover the shared key from its obfuscated bytes.
pub fn deobfuscate_api_key(obfuscated: &[u8], xor_key: &[u8]) -> String {
    String::from_utf8_lossy(&xor_transform(obfuscated, xor_key)).to_string()
}

fn app_mode_header(mode: AppMode) -> &'static str {
    match mode {
        AppMode::Byok => "byok",
        AppMode::Managed => "managed",
    }
}

/// Build the header name/value pairs sent on authenticated routes.
pub fn build_header_pairs(ctx: &HeaderContext) -> Vec<(String, String)> {
    vec![
        ("X-API-Key".into(), ctx.api_key.clone()),
        ("X-Device-ID".into(), ctx.device_id.clone()),
        ("X-App-Version".into(), ctx.app_version.clone()),
        ("X-Platform".into(), PLATFORM.into()),
        ("X-App-Mode".into(), app_mode_header(ctx.app_mode).into()),
    ]
}

pub fn endpoint_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
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
    #[error("this build has no Miniti backend key; use BYOK or rebuild with MINITI_API_KEY")]
    NoApiKey,
    #[error("this device has been disabled")]
    DeviceDisabled,
    #[error("monthly managed minutes used up{}", resets_hint(.resets_at))]
    LimitReached { resets_at: Option<String> },
    #[error("rate limited; try again shortly")]
    RateLimited,
    #[error("backend rejected the app key (unauthorized)")]
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
}

/// Map a non-2xx status + body to a domain error (contract §Error format).
pub fn map_error(status: u16, body: &str) -> Option<ApiError> {
    if (200..=299).contains(&status) {
        return None;
    }
    let parsed: ErrorBody = serde_json::from_str(body).unwrap_or_default();
    Some(match (status, parsed.error.as_str()) {
        (401, "unauthorized") | (401, "") => ApiError::Unauthorized,
        (402, _) => ApiError::LimitReached {
            resets_at: parsed.resets_at,
        },
        (403, _) => ApiError::DeviceDisabled,
        (429, _) => ApiError::RateLimited,
        (s, code) => ApiError::Http {
            status: s,
            code: if code.is_empty() { "unknown".into() } else { code.into() },
            message: parsed.message,
        },
    })
}

/// Status-only mapping kept for callers without a body.
pub fn map_status(status: u16) -> Option<ApiError> {
    map_error(status, "")
}

// ---- Live client ----------------------------------------------------------

/// Live HTTP client. Construction is cheap; calls need network + credentials.
#[derive(Clone)]
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
    ctx: HeaderContext,
}

impl ApiClient {
    pub fn new(base: impl Into<String>, ctx: HeaderContext) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(format!("Miniti-Linux/{}", ctx.app_version))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base: base.into(),
            http,
            ctx,
        }
    }

    pub fn context(&self) -> &HeaderContext {
        &self.ctx
    }

    fn apply_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in build_header_pairs(&self.ctx) {
            req = req.header(k, v);
        }
        req
    }

    async fn send_text(&self, req: reqwest::RequestBuilder) -> Result<String, ApiError> {
        let resp = self
            .apply_headers(req)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if let Some(err) = map_error(status, &body) {
            return Err(err);
        }
        Ok(body)
    }

    async fn send_json<T: for<'de> Deserialize<'de>>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T, ApiError> {
        let body = self.send_text(req).await?;
        serde_json::from_str::<T>(&body).map_err(|e| ApiError::Decode(e.to_string()))
    }

    pub async fn get_version(&self) -> Result<VersionInfo, ApiError> {
        let url = endpoint_url(&self.base, "api/version");
        self.send_json(self.http.get(url)).await
    }

    pub async fn get_usage(&self) -> Result<Usage, ApiError> {
        let url = endpoint_url(&self.base, "api/usage");
        self.send_json(self.http.get(url)).await
    }

    /// Start a managed transcription session (Deepgram grant JWT).
    pub async fn create_session(&self, language: &str) -> Result<Session, ApiError> {
        let url = endpoint_url(&self.base, "api/session");
        let body = serde_json::json!({ "model": "nova-3", "language": language });
        self.send_json(self.http.post(url).json(&body)).await
    }

    /// Report session duration. Idempotent per `(device, session_id)`.
    pub async fn end_session(
        &self,
        session_id: &str,
        duration_minutes: f64,
    ) -> Result<SessionEnd, ApiError> {
        let url = endpoint_url(&self.base, "api/session/end");
        let body = serde_json::json!({
            "session_id": session_id,
            "duration_minutes": duration_minutes,
        });
        self.send_json(self.http.post(url).json(&body)).await
    }

    /// Managed insights proxy. `body` should come from `insights::InsightRequest`.
    pub async fn post_insights(
        &self,
        body: &serde_json::Value,
    ) -> Result<InsightsResponse, ApiError> {
        let url = endpoint_url(&self.base, "api/insights");
        let text = self
            .send_text(self.http.post(url).timeout(INSIGHTS_TIMEOUT).json(body))
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
        let url = endpoint_url(&self.base, "api/subscribe");
        self.send_json::<R>(self.http.get(url)).await.map(|r| r.checkout_url)
    }

    /// Polar customer portal URL.
    pub async fn portal_url(&self) -> Result<String, ApiError> {
        #[derive(Deserialize)]
        struct R {
            portal_url: String,
        }
        let url = endpoint_url(&self.base, "api/portal");
        self.send_json::<R>(self.http.get(url)).await.map(|r| r.portal_url)
    }

    /// Restore Pro on this device with a Polar license key.
    pub async fn restore(&self, license_key: &str) -> Result<serde_json::Value, ApiError> {
        let url = endpoint_url(&self.base, "api/restore");
        let body = serde_json::json!({ "license_key": license_key });
        self.send_json(self.http.post(url).json(&body)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_roundtrips() {
        let key = b"miniti-xor";
        let secret = b"super-secret-app-key";
        let obf = xor_transform(secret, key);
        assert_ne!(&obf, secret);
        assert_eq!(deobfuscate_api_key(&obf, key), "super-secret-app-key");
    }

    #[test]
    fn headers_include_linux_platform_and_mode() {
        let ctx = HeaderContext {
            api_key: "K".into(),
            device_id: "D".into(),
            app_version: "0.1.0".into(),
            app_mode: AppMode::Byok,
        };
        let pairs = build_header_pairs(&ctx);
        let get = |name: &str| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        assert_eq!(get("X-Platform").as_deref(), Some("linux"));
        assert_eq!(get("X-App-Mode").as_deref(), Some("byok"));
        assert_eq!(get("X-API-Key").as_deref(), Some("K"));
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
            map_error(402, r#"{"error":"limit_reached","resets_at":"2026-10-01T00:00:00.000Z"}"#),
            Some(ApiError::LimitReached { resets_at: Some(_) })
        ));
        assert!(matches!(map_error(403, r#"{"error":"device_disabled"}"#), Some(ApiError::DeviceDisabled)));
        assert!(matches!(map_error(429, ""), Some(ApiError::RateLimited)));
        assert!(matches!(map_error(401, r#"{"error":"unauthorized"}"#), Some(ApiError::Unauthorized)));
        match map_error(500, r#"{"error":"internal_error","message":"boom"}"#) {
            Some(ApiError::Http { status, code, message }) => {
                assert_eq!(status, 500);
                assert_eq!(code, "internal_error");
                assert_eq!(message.as_deref(), Some("boom"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(map_status(500), Some(ApiError::Http { status: 500, .. })));
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
        assert_eq!(s.expiry_at(0), Some(expected), "expires_at is authoritative");
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
    }

    #[test]
    fn embedded_key_absent_in_test_builds_unless_env_set() {
        // Tests run without MINITI_API_KEY unless the developer set it.
        match std::env::var("MINITI_API_KEY") {
            Ok(k) if !k.is_empty() => assert_eq!(embedded_api_key().as_deref(), Some(k.as_str())),
            _ => assert!(embedded_api_key().is_none()),
        }
    }
}
