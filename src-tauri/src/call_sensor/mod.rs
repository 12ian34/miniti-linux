//! Call detection sensor (PLAN.md §8). Linux has no Core Audio process HAL, so
//! v1 combines a known-app directory with live PipeWire/Pulse capture clients.
//! The policy is pure and tested here; the live scan runs at runtime.
//!
//! Status: sensor + policy only. The Smart-meetings lifecycle engine that acts
//! on it (auto-start prompts, ending grace, quiet/calendar fallbacks) is Phase 4
//! and not wired yet.

use std::process::Command;

/// How strongly an app implies an active call when it is capturing audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAppKind {
    /// Dedicated conferencing app (Zoom, Teams, Webex, …).
    Strong,
    /// Browser or generic app that *might* be in a call.
    Weak,
    /// Not a known call app.
    None,
}

/// Our own capture clients, which must never count as "someone is in a call".
const SELF_MARKERS: &[&str] = &["miniti", "parec", "alsa plug-in [miniti"];

/// Whole-word directory. Matching is on lowercase word tokens so "meet" does
/// not match "meeting" and "teams" does not match "steams".
const STRONG: &[&str] = &[
    "zoom", "zoom.us", "teams", "microsoft teams", "webex", "slack", "discord", "skype",
    "google meet", "meet", "jitsi", "signal", "telegram", "whatsapp", "gotomeeting",
    "bluejeans", "ringcentral",
];
const WEAK: &[&str] = &[
    "firefox", "chrome", "chromium", "brave", "vivaldi", "epiphany", "edge", "opera",
    "librewolf", "zen",
];

fn tokens(identifier: &str) -> Vec<String> {
    identifier
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '.'))
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

fn matches_any(id_lower: &str, toks: &[String], dictionary: &[&str]) -> bool {
    dictionary.iter().any(|entry| {
        if entry.contains(' ') {
            id_lower.contains(entry)
        } else {
            toks.iter().any(|t| t == entry)
        }
    })
}

/// True for Miniti's own audio clients.
pub fn is_self(identifier: &str) -> bool {
    let id = identifier.to_lowercase();
    SELF_MARKERS.iter().any(|m| id.contains(m))
}

/// Classify a process binary / desktop id / `application.name` against the
/// known-app directory.
pub fn classify(identifier: &str) -> CallAppKind {
    if is_self(identifier) {
        return CallAppKind::None;
    }
    let id = identifier.to_lowercase();
    let toks = tokens(&id);
    if matches_any(&id, &toks, STRONG) {
        CallAppKind::Strong
    } else if matches_any(&id, &toks, WEAK) {
        CallAppKind::Weak
    } else {
        CallAppKind::None
    }
}

/// A capture client observed on the audio server.
#[derive(Debug, Clone)]
pub struct CaptureClient {
    pub app_name: String,
    /// False when the stream exists but is corked (paused).
    pub is_capturing: bool,
}

/// Decide whether a call is likely in progress: a capturing Strong app.
pub fn likely_in_call(clients: &[CaptureClient]) -> bool {
    clients
        .iter()
        .any(|c| c.is_capturing && classify(&c.app_name) == CallAppKind::Strong)
}

/// True when there is at least a weak signal (a capturing weak app) but no
/// strong one — the caller may escalate to quiet/calendar heuristics.
pub fn weak_signal_only(clients: &[CaptureClient]) -> bool {
    let strong = likely_in_call(clients);
    let weak = clients
        .iter()
        .any(|c| c.is_capturing && classify(&c.app_name) == CallAppKind::Weak);
    weak && !strong
}

/// Parse `pactl list source-outputs` output into capture clients. Corked
/// streams are reported as not capturing; Miniti's own streams are dropped.
pub fn parse_source_outputs(text: &str) -> Vec<CaptureClient> {
    let mut clients = Vec::new();
    for block in text.split("Source Output #").skip(1) {
        let mut app_name = String::new();
        let mut corked = false;
        for line in block.lines() {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("application.name = ") {
                app_name = v.trim_matches('"').to_string();
            } else if let Some(v) = l.strip_prefix("Corked:") {
                corked = v.trim().eq_ignore_ascii_case("yes");
            }
        }
        if app_name.is_empty() || is_self(&app_name) {
            continue;
        }
        clients.push(CaptureClient {
            app_name,
            is_capturing: !corked,
        });
    }
    clients
}

/// Live scan via `pactl`. `None` when the audio server / pactl is unavailable
/// (an unreliable reading, which the lifecycle engine treats as no information).
pub fn snapshot_capture_clients() -> Option<Vec<CaptureClient>> {
    match Command::new("pactl").args(["list", "source-outputs"]).output() {
        Ok(o) if o.status.success() => Some(parse_source_outputs(&String::from_utf8_lossy(&o.stdout))),
        _ => None,
    }
}

/// Best-effort variant: empty when unavailable.
pub fn scan_capture_clients() -> Vec<CaptureClient> {
    snapshot_capture_clients().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_apps_by_whole_word() {
        assert_eq!(classify("zoom.us"), CallAppKind::Strong);
        assert_eq!(classify("Microsoft Teams"), CallAppKind::Strong);
        assert_eq!(classify("teams-for-linux"), CallAppKind::Strong);
        assert_eq!(classify("firefox"), CallAppKind::Weak);
        assert_eq!(classify("gnome-text-editor"), CallAppKind::None);
        assert_eq!(classify("meeting-notes-app"), CallAppKind::None, "substring 'meet' must not match");
        assert_eq!(classify("steams"), CallAppKind::None);
    }

    #[test]
    fn own_capture_clients_are_ignored() {
        assert_eq!(classify("Miniti"), CallAppKind::None);
        assert_eq!(classify("parec"), CallAppKind::None);
        assert!(is_self("ALSA plug-in [miniti]"));
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
        let clients = vec![CaptureClient { app_name: "chromium".into(), is_capturing: true }];
        assert!(!likely_in_call(&clients));
        assert!(weak_signal_only(&clients));
    }

    #[test]
    fn corked_streams_do_not_count() {
        let out = r#"
Source Output #12
	Driver: protocol-native.c
	Corked: yes
	Properties:
		application.name = "ZOOM VoiceEngine"
Source Output #13
	Corked: no
	Properties:
		application.name = "Firefox"
Source Output #14
	Corked: no
	Properties:
		application.name = "parec"
"#;
        let clients = parse_source_outputs(out);
        assert_eq!(clients.len(), 2, "own parec dropped");
        assert!(!clients[0].is_capturing, "corked zoom");
        assert!(clients[1].is_capturing);
        assert!(!likely_in_call(&clients));
        assert!(weak_signal_only(&clients));
    }
}
