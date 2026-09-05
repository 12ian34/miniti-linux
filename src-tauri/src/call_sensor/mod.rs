//! Call detection policy (PLAN.md §8). Linux has no Core Audio process HAL, so
//! v1 combines a known-app directory with live PipeWire/Pulse capture clients.
//! The policy is pure and tested here; the live scan runs at runtime.

use std::process::Command;

/// How strongly an app implies an active call when it is capturing audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAppKind {
    /// Dedicated conferencing app (Zoom, Teams, Meet desktop, …).
    Strong,
    /// Browser or generic app that *might* be in a call.
    Weak,
    /// Not a known call app.
    None,
}

/// Classify a process binary / desktop id against the known-app directory.
pub fn classify(identifier: &str) -> CallAppKind {
    let id = identifier.to_lowercase();
    const STRONG: &[&str] = &[
        "zoom", "zoom.us", "teams", "microsoft teams", "webex", "slack", "discord",
        "skype", "google meet", "meet",
    ];
    const WEAK: &[&str] = &[
        "firefox", "chrome", "chromium", "brave", "vivaldi", "epiphany", "edge",
    ];
    if STRONG.iter().any(|s| id.contains(s)) {
        CallAppKind::Strong
    } else if WEAK.iter().any(|w| id.contains(w)) {
        CallAppKind::Weak
    } else {
        CallAppKind::None
    }
}

/// A capture client observed on the audio server.
#[derive(Debug, Clone)]
pub struct CaptureClient {
    pub app_name: String,
    pub is_capturing: bool,
}

/// Decide whether a call is likely in progress.
///
/// A capturing Strong app ⇒ in-call. A capturing Weak app alone is treated as a
/// weak signal (not decisive) — quiet/calendar fallbacks cover the rest.
pub fn likely_in_call(clients: &[CaptureClient]) -> bool {
    clients
        .iter()
        .any(|c| c.is_capturing && classify(&c.app_name) == CallAppKind::Strong)
}

/// True when there is at least a weak signal (a capturing weak app) but no
/// strong one — the caller may escalate to quiet/calendar heuristics.
pub fn weak_signal_only(clients: &[CaptureClient]) -> bool {
    let strong = clients
        .iter()
        .any(|c| c.is_capturing && classify(&c.app_name) == CallAppKind::Strong);
    let weak = clients
        .iter()
        .any(|c| c.is_capturing && classify(&c.app_name) == CallAppKind::Weak);
    weak && !strong
}

/// Live scan of source-output (capture) clients via `pactl`. Best-effort:
/// returns an empty list when the audio server is unavailable.
pub fn scan_capture_clients() -> Vec<CaptureClient> {
    let out = match Command::new("pactl")
        .args(["list", "source-outputs"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut clients = Vec::new();
    for block in text.split("Source Output #").skip(1) {
        let app_name = block
            .lines()
            .find_map(|l| {
                let l = l.trim();
                l.strip_prefix("application.name = ")
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_default();
        if !app_name.is_empty() {
            clients.push(CaptureClient {
                app_name,
                is_capturing: true,
            });
        }
    }
    clients
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_apps() {
        assert_eq!(classify("zoom.us"), CallAppKind::Strong);
        assert_eq!(classify("Microsoft Teams"), CallAppKind::Strong);
        assert_eq!(classify("firefox"), CallAppKind::Weak);
        assert_eq!(classify("gnome-text-editor"), CallAppKind::None);
    }

    #[test]
    fn strong_capturing_app_means_in_call() {
        let clients = vec![
            CaptureClient { app_name: "Zoom".into(), is_capturing: true },
            CaptureClient { app_name: "firefox".into(), is_capturing: false },
        ];
        assert!(likely_in_call(&clients));
        assert!(!weak_signal_only(&clients));
    }

    #[test]
    fn weak_only_is_not_decisive() {
        let clients = vec![CaptureClient {
            app_name: "chromium".into(),
            is_capturing: true,
        }];
        assert!(!likely_in_call(&clients));
        assert!(weak_signal_only(&clients));
    }

    #[test]
    fn non_capturing_strong_app_is_not_in_call() {
        let clients = vec![CaptureClient {
            app_name: "zoom".into(),
            is_capturing: false,
        }];
        assert!(!likely_in_call(&clients));
    }
}
