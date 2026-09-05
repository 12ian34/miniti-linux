//! Miniti backend client (PLAN.md §5). Header construction, URL building, auth
//! obfuscation, token-refresh timing, and status mapping are pure + tested; the
//! reqwest methods perform live calls and need network + credentials.

use serde::{Deserialize, Serialize};

use crate::prefs::AppMode;

pub const DEFAULT_BASE_URL: &str = "https://api.miniti.app";
pub const PLATFORM: &str = "linux";
/// Refresh the session token if within this many seconds of expiry.
pub const REFRESH_MARGIN_SECS: i64 = 60;

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

/// Endpoints Linux needs (PLAN.md §5).
pub fn endpoint_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

// ---- Response models ------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VersionInfo {
    #[serde(default)]
    pub min_version: String,
    #[serde(default)]
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub minutes_used: f64,
    #[serde(default)]
    pub minutes_limit: Option<f64>,
    #[serde(default)]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Session {
    /// Preferred managed Deepgram grant.
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
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

    /// Absolute expiry (unix secs) derived from `expires_at` or `expires_in`.
    pub fn expiry_at(&self, now: i64) -> Option<i64> {
        self.expires_at.or_else(|| self.expires_in.map(|d| now + d))
    }
}

/// True when the token should be refreshed before reconnecting.
pub fn needs_refresh(expiry_at: i64, now: i64) -> bool {
    expiry_at - now <= REFRESH_MARGIN_SECS
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InsightsMeta {
    #[serde(default)]
    pub degraded: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InsightsResponse {
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub meta: InsightsMeta,
}

// ---- Errors ---------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("device disabled")]
    DeviceDisabled,
    #[error("managed limit reached")]
    LimitReached,
    #[error("rate limited")]
    RateLimited,
    #[error("http status {0}")]
    Http(u16),
    #[error("network error: {0}")]
    Network(String),
    #[error("decode error: {0}")]
    Decode(String),
}

/// Map a well-known HTTP status to a domain error (PLAN.md §5):
/// 403 device_disabled, 402 limit reached, 429 rate limited.
pub fn map_status(status: u16) -> Option<ApiError> {
    match status {
        200..=299 => None,
        402 => Some(ApiError::LimitReached),
        403 => Some(ApiError::DeviceDisabled),
        429 => Some(ApiError::RateLimited),
        other => Some(ApiError::Http(other)),
    }
}

// ---- Live client ----------------------------------------------------------

/// Live HTTP client. Construction is cheap; calls need network + credentials.
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
    ctx: HeaderContext,
}

impl ApiClient {
    pub fn new(base: impl Into<String>, ctx: HeaderContext) -> Self {
        Self {
            base: base.into(),
            http: reqwest::Client::new(),
            ctx,
        }
    }

    fn apply_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in build_header_pairs(&self.ctx) {
            req = req.header(k, v);
        }
        req
    }

    async fn send_json<T: for<'de> Deserialize<'de>>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T, ApiError> {
        let resp = self
            .apply_headers(req)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if let Some(err) = map_status(resp.status().as_u16()) {
            return Err(err);
        }
        resp.json::<T>()
            .await
            .map_err(|e| ApiError::Decode(e.to_string()))
    }

    pub async fn get_version(&self) -> Result<VersionInfo, ApiError> {
        let url = endpoint_url(&self.base, "api/version");
        self.send_json(self.http.get(url)).await
    }

    pub async fn get_usage(&self) -> Result<Usage, ApiError> {
        let url = endpoint_url(&self.base, "api/usage");
        self.send_json(self.http.get(url)).await
    }

    pub async fn create_session(&self, body: &serde_json::Value) -> Result<Session, ApiError> {
        let url = endpoint_url(&self.base, "api/session");
        self.send_json(self.http.post(url).json(body)).await
    }

    pub async fn end_session(&self, body: &serde_json::Value) -> Result<(), ApiError> {
        let url = endpoint_url(&self.base, "api/session/end");
        let resp = self
            .apply_headers(self.http.post(url).json(body))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match map_status(resp.status().as_u16()) {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    pub async fn post_insights(
        &self,
        body: &serde_json::Value,
    ) -> Result<InsightsResponse, ApiError> {
        let url = endpoint_url(&self.base, "api/insights");
        self.send_json(self.http.post(url).json(body)).await
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
    fn status_mapping() {
        assert!(map_status(200).is_none());
        assert!(matches!(map_status(402), Some(ApiError::LimitReached)));
        assert!(matches!(map_status(403), Some(ApiError::DeviceDisabled)));
        assert!(matches!(map_status(429), Some(ApiError::RateLimited)));
        assert!(matches!(map_status(500), Some(ApiError::Http(500))));
    }

    #[test]
    fn session_prefers_access_token_and_computes_expiry() {
        let s = Session {
            access_token: Some("jwt".into()),
            temp_api_key: Some("legacy".into()),
            expires_in: Some(3600),
            ..Default::default()
        };
        assert_eq!(s.token(), Some("jwt"));
        assert_eq!(s.expiry_at(1000), Some(4600));

        let legacy = Session {
            temp_api_key: Some("legacy".into()),
            ..Default::default()
        };
        assert_eq!(legacy.token(), Some("legacy"));
    }

    #[test]
    fn refresh_window() {
        assert!(needs_refresh(1000, 950)); // 50s left -> refresh
        assert!(!needs_refresh(1000, 800)); // 200s left -> ok
    }
}
