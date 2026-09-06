//! Device-bound client authorization (backend contract: `../miniti-api/docs/agents/04-api-reference.md`
//! § "Device-bound client authorization"; design: `../miniti/docs/roadmap.md` P0.1).
//!
//! This module is the pure half: the P-256 installation key, ES256 compact-JWS
//! proofs, and the anonymous recovery-key format. Everything here is verified
//! against the cross-language vectors in `vectors.json`, which the backend's
//! TypeScript tests also consume. Storage and the token lifecycle live in
//! [`manager`].

pub mod manager;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use sha2::{Digest, Sha256};

pub const PROOF_TYP: &str = "miniti-proof+jwt";
pub const RECOVERY_KEY_VERSION: &str = "M1";
const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const RECOVERY_RANDOM_BYTES: usize = 16;
const RECOVERY_CHECKSUM_BYTES: usize = 4;
const RECOVERY_BODY_CHARS: usize = (RECOVERY_RANDOM_BYTES + RECOVERY_CHECKSUM_BYTES) * 8 / 5;
const RECOVERY_CHECKSUM_DOMAIN: &[u8] = b"miniti-recovery-v1";

pub fn b64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).ok()
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// base64url SHA-256, as used for `ath` (access token) and `bh` (body) claims.
pub fn hash_b64(bytes: &[u8]) -> String {
    b64url(&sha256(bytes))
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    use rand_core::RngCore;
    let mut out = [0u8; N];
    rand_core::OsRng.fill_bytes(&mut out);
    out
}

/// Unique request id for proofs (16 random bytes, base64url).
pub fn new_jti() -> String {
    b64url(&random_bytes::<16>())
}

// ---- Installation key -------------------------------------------------------

/// Per-installation P-256 key. The secret scalar never leaves the device.
#[derive(Clone)]
pub struct InstallationKey {
    signing: SigningKey,
}

impl std::fmt::Debug for InstallationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallationKey")
            .field("jkt", &self.thumbprint())
            .finish()
    }
}

impl InstallationKey {
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::random(&mut rand_core::OsRng),
        }
    }

    /// Restore from the stored base64url secret scalar.
    pub fn from_secret_b64(secret: &str) -> Option<Self> {
        let bytes = b64url_decode(secret.trim())?;
        SigningKey::from_slice(&bytes)
            .ok()
            .map(|signing| Self { signing })
    }

    pub fn secret_b64(&self) -> String {
        b64url(&self.signing.to_bytes())
    }

    fn coordinates(&self) -> (String, String) {
        let point = self.signing.verifying_key().to_encoded_point(false);
        let x = point.x().map(|c| b64url(c)).unwrap_or_default();
        let y = point.y().map(|c| b64url(c)).unwrap_or_default();
        (x, y)
    }

    /// Public JWK sent at enrollment.
    pub fn public_jwk(&self) -> serde_json::Value {
        let (x, y) = self.coordinates();
        serde_json::json!({ "kty": "EC", "crv": "P-256", "x": x, "y": y })
    }

    /// RFC 7638 thumbprint (the `cnf.jkt` the backend binds tokens to).
    pub fn thumbprint(&self) -> String {
        let (x, y) = self.coordinates();
        jwk_thumbprint(&x, &y)
    }

    /// Compact JWS, `{"alg":"ES256","typ":"miniti-proof+jwt"}`, raw `r‖s` signature.
    pub fn sign_jws(&self, payload: &serde_json::Value) -> String {
        let header = b64url(br#"{"alg":"ES256","typ":"miniti-proof+jwt"}"#);
        let body = b64url(payload.to_string().as_bytes());
        let signing_input = format!("{header}.{body}");
        let signature: Signature = self.signing.sign(signing_input.as_bytes());
        format!("{signing_input}.{}", b64url(&signature.to_bytes()))
    }

    /// Proof over an enrollment / token challenge.
    pub fn challenge_proof(&self, nonce: &str, purpose: &str, device_id: &str, now: i64) -> String {
        self.sign_jws(&serde_json::json!({
            "purpose": purpose,
            "nonce": nonce,
            "device_id": device_id,
            "jkt": self.thumbprint(),
            "iat": now,
        }))
    }

    /// `X-Request-Proof` for a billable request. `target` is path + query.
    pub fn request_proof(
        &self,
        access_token: &str,
        method: &str,
        target: &str,
        body: &[u8],
        now: i64,
        jti: &str,
    ) -> String {
        self.sign_jws(&serde_json::json!({
            "ath": hash_b64(access_token.as_bytes()),
            "htm": method.to_ascii_uppercase(),
            "htu": target,
            "bh": hash_b64(body),
            "iat": now,
            "jti": jti,
        }))
    }
}

pub fn jwk_thumbprint(x: &str, y: &str) -> String {
    let canonical = format!(r#"{{"crv":"P-256","kty":"EC","x":"{x}","y":"{y}"}}"#);
    hash_b64(canonical.as_bytes())
}

// ---- Recovery keys ------------------------------------------------------------

pub fn crockford_encode(bytes: &[u8]) -> String {
    let mut bits = 0u32;
    let mut value = 0u64;
    let mut out = String::new();
    for &b in bytes {
        value = (value << 8) | b as u64;
        bits += 8;
        while bits >= 5 {
            out.push(CROCKFORD[((value >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(CROCKFORD[((value << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn crockford_decode(text: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut value = 0u64;
    let mut out = Vec::new();
    for ch in text.bytes() {
        let idx = CROCKFORD.iter().position(|&c| c == ch)? as u64;
        value = (value << 5) | idx;
        bits += 5;
        if bits >= 8 {
            out.push(((value >> (bits - 8)) & 0xff) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

fn recovery_checksum(random: &[u8]) -> [u8; RECOVERY_CHECKSUM_BYTES] {
    let mut material = Vec::with_capacity(RECOVERY_CHECKSUM_DOMAIN.len() + random.len());
    material.extend_from_slice(RECOVERY_CHECKSUM_DOMAIN);
    material.extend_from_slice(random);
    let digest = sha256(&material);
    let mut out = [0u8; RECOVERY_CHECKSUM_BYTES];
    out.copy_from_slice(&digest[..RECOVERY_CHECKSUM_BYTES]);
    out
}

/// Canonical key (`M1` + 32 Crockford chars) from 16 random bytes.
pub fn recovery_key_from_random(random: &[u8; RECOVERY_RANDOM_BYTES]) -> String {
    let mut body = Vec::with_capacity(RECOVERY_RANDOM_BYTES + RECOVERY_CHECKSUM_BYTES);
    body.extend_from_slice(random);
    body.extend_from_slice(&recovery_checksum(random));
    format!("{RECOVERY_KEY_VERSION}{}", crockford_encode(&body))
}

pub fn generate_recovery_key() -> String {
    recovery_key_from_random(&random_bytes::<RECOVERY_RANDOM_BYTES>())
}

/// Uppercase, drop separators, map `O→0` and `I/L→1`, optional `M1` prefix. No checksum check.
pub fn normalize_recovery_key(input: &str) -> Option<String> {
    let stripped: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    let body = if stripped.len() == RECOVERY_BODY_CHARS + RECOVERY_KEY_VERSION.len()
        && stripped.starts_with(RECOVERY_KEY_VERSION)
    {
        &stripped[RECOVERY_KEY_VERSION.len()..]
    } else {
        stripped.as_str()
    };
    if body.len() != RECOVERY_BODY_CHARS || !body.bytes().all(|b| CROCKFORD.contains(&b)) {
        return None;
    }
    Some(format!("{RECOVERY_KEY_VERSION}{body}"))
}

/// Normalize and verify the checksum. Returns the canonical key.
pub fn parse_recovery_key(input: &str) -> Option<String> {
    let canonical = normalize_recovery_key(input)?;
    let decoded = crockford_decode(&canonical[RECOVERY_KEY_VERSION.len()..])?;
    if decoded.len() != RECOVERY_RANDOM_BYTES + RECOVERY_CHECKSUM_BYTES {
        return None;
    }
    let (random, checksum) = decoded.split_at(RECOVERY_RANDOM_BYTES);
    (recovery_checksum(random) == checksum).then_some(canonical)
}

/// `M1-XXXX-XXXX-…` for display.
pub fn format_recovery_key(canonical: &str) -> String {
    let body = &canonical[RECOVERY_KEY_VERSION.len().min(canonical.len())..];
    let groups: Vec<String> = body
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect();
    std::iter::once(RECOVERY_KEY_VERSION.to_string())
        .chain(groups)
        .collect::<Vec<_>>()
        .join("-")
}

// ---- Request targets ---------------------------------------------------------

/// Path + query of a URL, exactly as the server compares it (`htu`).
pub fn request_target(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => match u.query() {
            Some(q) => format!("{}?{}", u.path(), q),
            None => u.path().to_string(),
        },
        Err(_) => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;

    fn vectors() -> serde_json::Value {
        serde_json::from_str(include_str!("vectors.json")).unwrap()
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn recovery_key_vectors_match_backend() {
        for v in vectors()["recovery_keys"].as_array().unwrap() {
            let random: [u8; 16] = hex(v["random_hex"].as_str().unwrap()).try_into().unwrap();
            let key = recovery_key_from_random(&random);
            assert_eq!(key, v["canonical"].as_str().unwrap());
            assert_eq!(format_recovery_key(&key), v["formatted"].as_str().unwrap());
            assert_eq!(
                parse_recovery_key(v["formatted"].as_str().unwrap()).as_deref(),
                Some(key.as_str())
            );
        }
        for v in vectors()["recovery_key_normalization"].as_array().unwrap() {
            assert_eq!(
                parse_recovery_key(v["input"].as_str().unwrap()).as_deref(),
                v["canonical"].as_str()
            );
        }
    }

    #[test]
    fn recovery_key_checksum_and_confusables() {
        let canonical = vectors()["recovery_keys"][0]["canonical"]
            .as_str()
            .unwrap()
            .to_string();
        let confusable = canonical.replace('0', "O").replace('1', "I");
        assert_eq!(
            parse_recovery_key(&confusable).as_deref(),
            Some(canonical.as_str())
        );
        let mut corrupted = canonical.clone();
        let last = corrupted.pop().unwrap();
        corrupted.push(if last == '0' { '1' } else { '0' });
        assert!(parse_recovery_key(&corrupted).is_none());
        assert!(parse_recovery_key("short").is_none());
        let generated = generate_recovery_key();
        assert_eq!(generated.len(), 34);
        assert_eq!(
            parse_recovery_key(&format_recovery_key(&generated)).as_deref(),
            Some(generated.as_str())
        );
    }

    #[test]
    fn thumbprint_and_hash_vectors_match_backend() {
        let v = vectors();
        let jwk = &v["jwk_thumbprint"]["jwk"];
        assert_eq!(
            jwk_thumbprint(jwk["x"].as_str().unwrap(), jwk["y"].as_str().unwrap()),
            v["jwk_thumbprint"]["jkt"].as_str().unwrap()
        );
        for b in v["body_hashes"].as_array().unwrap() {
            assert_eq!(
                hash_b64(b["body"].as_str().unwrap().as_bytes()),
                b["bh"].as_str().unwrap()
            );
        }
        assert_eq!(
            hash_b64(v["access_token_hash"]["token"].as_str().unwrap().as_bytes()),
            v["access_token_hash"]["ath"].as_str().unwrap()
        );
    }

    #[test]
    fn installation_key_round_trips_and_signs_verifiable_proofs() {
        let key = InstallationKey::generate();
        let restored = InstallationKey::from_secret_b64(&key.secret_b64()).unwrap();
        assert_eq!(restored.thumbprint(), key.thumbprint());
        let jwk = key.public_jwk();
        assert_eq!(jwk["kty"], "EC");
        assert_eq!(b64url_decode(jwk["x"].as_str().unwrap()).unwrap().len(), 32);
        assert_eq!(
            jwk_thumbprint(jwk["x"].as_str().unwrap(), jwk["y"].as_str().unwrap()),
            key.thumbprint()
        );

        let proof = key.request_proof(
            "tok",
            "post",
            "/api/session",
            b"{}",
            1_700_000_000,
            "jti-12345678",
        );
        let parts: Vec<&str> = proof.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["alg"], "ES256");
        assert_eq!(header["typ"], PROOF_TYP);
        let payload: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[1]).unwrap()).unwrap();
        assert_eq!(payload["htm"], "POST");
        assert_eq!(payload["htu"], "/api/session");
        assert_eq!(payload["ath"], hash_b64(b"tok"));
        assert_eq!(payload["bh"], hash_b64(b"{}"));
        let sig_bytes = b64url_decode(parts[2]).unwrap();
        assert_eq!(sig_bytes.len(), 64, "raw r||s as WebCrypto expects");
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        key.signing
            .verifying_key()
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .unwrap();

        let challenge =
            key.challenge_proof("nonce", "enroll", "550e8400-e29b-41d4-a716-446655440000", 1);
        let payload: serde_json::Value =
            serde_json::from_slice(&b64url_decode(challenge.split('.').nth(1).unwrap()).unwrap())
                .unwrap();
        assert_eq!(payload["purpose"], "enroll");
        assert_eq!(payload["jkt"], key.thumbprint());
    }

    #[test]
    fn request_target_keeps_query() {
        assert_eq!(
            request_target("https://api.miniti.app/api/session"),
            "/api/session"
        );
        assert_eq!(
            request_target("https://api.miniti.app/api/google/events?days=7"),
            "/api/google/events?days=7"
        );
    }
}
