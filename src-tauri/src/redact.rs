//! Secrets that must never appear in the log a user sends us.
//!
//! Two halves, as on Apple: call sites log a reduced form (a webhook's scheme
//! and host, never its path — a Zapier or Make hook URL carries its secret in
//! the path), and anything registered here is scrubbed out of the log viewer
//! and the exported log as a backstop for text we do not control, such as a
//! transport error that quotes the whole URL.

use std::sync::RwLock;

pub const PLACEHOLDER: &str = "[redacted]";
/// Below this a pattern is too short to replace safely (it would hit ordinary words).
const MIN_PATTERN_LEN: usize = 8;

fn registry() -> &'static RwLock<Vec<String>> {
    static REGISTRY: RwLock<Vec<String>> = RwLock::new(Vec::new());
    &REGISTRY
}

/// Register a secret string. Idempotent; short or empty patterns are ignored.
pub fn register(secret: &str) {
    let secret = secret.trim();
    if secret.len() < MIN_PATTERN_LEN {
        return;
    }
    if let Ok(mut reg) = registry().write() {
        if !reg.iter().any(|s| s == secret) {
            reg.push(secret.to_string());
        }
    }
}

/// Replace every registered secret in `text`.
pub fn scrub(text: &str) -> String {
    let Ok(reg) = registry().read() else {
        return text.to_string();
    };
    let mut out = text.to_string();
    for secret in reg.iter() {
        if out.contains(secret.as_str()) {
            out = out.replace(secret.as_str(), PLACEHOLDER);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_secrets_are_scrubbed_and_short_ones_ignored() {
        register("https://hooks.zapier.com/hooks/catch/123/abcdef");
        register("um"); // too short to replace safely
        let line = "webhook failed for https://hooks.zapier.com/hooks/catch/123/abcdef (um)";
        let scrubbed = scrub(line);
        assert!(!scrubbed.contains("abcdef"));
        assert!(scrubbed.contains(PLACEHOLDER));
        assert!(scrubbed.contains("(um)"), "short patterns are never applied");
    }
}
