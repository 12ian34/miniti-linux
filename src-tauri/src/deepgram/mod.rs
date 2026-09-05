//! Deepgram live transcription client (PLAN.md §6). Pure helpers (URL/query,
//! message parsing, speaker-ID mapping, stabilization, backoff) are unit-tested;
//! [`connect`] opens the live WebSocket and needs a valid credential at runtime.

use std::time::Duration;

use serde::Deserialize;

pub mod live;

pub const DEEPGRAM_WS_BASE: &str = "wss://api.deepgram.com/v1/listen";

/// Base speaker id offset for the mic channel when running multichannel so mic
/// and system speaker spaces never collide (see Apple `SpeakerIdentityState`).
pub const MIC_SPEAKER_OFFSET: i64 = 1000;

/// Max key terms sent on the query string (Deepgram caps request size).
pub const KEYTERM_CAP: usize = 100;

#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub language: String,
    /// When true, stream `channels=2&multichannel=true` (mic ch0 + system ch1).
    pub multichannel: bool,
    pub keyterms: Vec<String>,
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            multichannel: false,
            keyterms: Vec::new(),
        }
    }
}

/// Auth scheme: managed grants a JWT (`Bearer`), BYOK uses the raw key (`Token`).
#[derive(Debug, Clone)]
pub enum Auth {
    Bearer(String),
    Token(String),
}

impl Auth {
    pub fn header_value(&self) -> String {
        match self {
            Auth::Bearer(t) => format!("Bearer {t}"),
            Auth::Token(t) => format!("Token {t}"),
        }
    }
}

/// Build the Deepgram listen URL with the macOS-aligned query contract.
pub fn build_ws_url(cfg: &DeepgramConfig) -> String {
    let mut params: Vec<(String, String)> = vec![
        ("model".into(), "nova-3".into()),
        ("smart_format".into(), "true".into()),
        ("filler_words".into(), "true".into()),
        ("diarize".into(), "true".into()),
        ("diarize_model".into(), "latest".into()),
        ("interim_results".into(), "true".into()),
        ("utterance_end_ms".into(), "1000".into()),
        ("vad_events".into(), "true".into()),
        ("endpointing".into(), "300".into()),
        ("encoding".into(), "linear16".into()),
        ("sample_rate".into(), "16000".into()),
        ("language".into(), cfg.language.clone()),
    ];
    if cfg.multichannel {
        params.push(("channels".into(), "2".into()));
        params.push(("multichannel".into(), "true".into()));
    } else {
        params.push(("channels".into(), "1".into()));
    }
    for term in cfg.keyterms.iter().take(KEYTERM_CAP) {
        params.push(("keyterm".into(), term.clone()));
    }

    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{DEEPGRAM_WS_BASE}?{query}")
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The periodic keepalive Deepgram expects (~every 5s while connected).
pub fn keepalive_message() -> String {
    r#"{"type":"KeepAlive"}"#.to_string()
}

pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

// ---- Wire message types ---------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Results(Results),
    Metadata(serde_json::Value),
    SpeechStarted(serde_json::Value),
    UtteranceEnd(serde_json::Value),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
pub struct Results {
    #[serde(default)]
    pub channel_index: Vec<u32>,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub speech_final: bool,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub duration: f64,
    pub channel: Channel,
}

#[derive(Debug, Deserialize)]
pub struct Channel {
    #[serde(default)]
    pub alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
pub struct Alternative {
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Debug, Deserialize)]
pub struct Word {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub punctuated_word: Option<String>,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub speaker: Option<i64>,
    #[serde(default)]
    pub confidence: f64,
}

/// Which capture lane a transcript segment came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentSource {
    Microphone,
    System,
    Mono,
}

/// A stabilized transcript segment ready to render / persist.
#[derive(Debug, Clone)]
pub struct TranscriptEvent {
    pub text: String,
    pub speaker_id: i64,
    pub start: f64,
    pub end: f64,
    pub is_final: bool,
    pub confidence: f64,
    pub source: SegmentSource,
}

/// Map a raw Deepgram speaker id onto Miniti's speaker space.
///
/// Multichannel: channel 0 = mic → `MIC_SPEAKER_OFFSET + raw`; channel 1 =
/// system → `raw` (low ids). Mono: mic range only.
pub fn map_speaker(channel_index: u32, raw_speaker: i64, multichannel: bool) -> i64 {
    if multichannel && channel_index == 0 {
        MIC_SPEAKER_OFFSET + raw_speaker
    } else {
        raw_speaker
    }
}

/// Promote a segment to the transcript only on `is_final || speech_final`
/// (interims never promote speakers — same rule as the Apple apps).
pub fn should_promote(is_final: bool, speech_final: bool) -> bool {
    is_final || speech_final
}

/// Convert a `Results` message into a stabilized event, or `None` when the
/// alternative is empty or below the confidence floor.
pub fn results_to_event(
    res: &Results,
    multichannel: bool,
    min_confidence: f64,
) -> Option<TranscriptEvent> {
    let alt = res.alternatives_first()?;
    if alt.transcript.trim().is_empty() || alt.confidence < min_confidence {
        return None;
    }
    let channel = res.channel_index.first().copied().unwrap_or(0);
    let raw_speaker = alt
        .words
        .first()
        .and_then(|w| w.speaker)
        .unwrap_or(0);
    let source = if !multichannel {
        SegmentSource::Mono
    } else if channel == 0 {
        SegmentSource::Microphone
    } else {
        SegmentSource::System
    };
    Some(TranscriptEvent {
        text: alt.transcript.clone(),
        speaker_id: map_speaker(channel, raw_speaker, multichannel),
        start: res.start,
        end: res.start + res.duration,
        is_final: res.is_final,
        confidence: alt.confidence,
        source,
    })
}

impl Results {
    fn alternatives_first(&self) -> Option<&Alternative> {
        self.channel.alternatives.first()
    }
}

/// Parse a text frame from Deepgram into a [`ServerMessage`].
pub fn parse_message(text: &str) -> Result<ServerMessage, serde_json::Error> {
    serde_json::from_str(text)
}

// ---- Reconnect backoff ----------------------------------------------------

/// Bounded exponential backoff with a generation counter so late results from a
/// stale socket can be discarded by the caller.
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: u32,
    base: Duration,
    max: Duration,
    pub generation: u64,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            attempt: 0,
            base: Duration::from_millis(500),
            max: Duration::from_secs(30),
            generation: 0,
        }
    }
}

impl Backoff {
    pub fn next_delay(&mut self) -> Duration {
        let shift = self.attempt.min(6);
        let delay = self.base.saturating_mul(1 << shift);
        self.attempt = self.attempt.saturating_add(1);
        delay.min(self.max)
    }

    /// Call on a successful connect: resets delay and bumps the generation.
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_has_core_contract_params() {
        let url = build_ws_url(&DeepgramConfig::default());
        for p in [
            "model=nova-3",
            "smart_format=true",
            "filler_words=true",
            "encoding=linear16",
            "sample_rate=16000",
            "language=en",
            "channels=1",
        ] {
            assert!(url.contains(p), "missing {p} in {url}");
        }
        assert!(!url.contains("punctuate="), "must not send punctuate with smart_format");
    }

    #[test]
    fn url_multichannel_toggles_channels() {
        let url = build_ws_url(&DeepgramConfig {
            multichannel: true,
            ..Default::default()
        });
        assert!(url.contains("channels=2"));
        assert!(url.contains("multichannel=true"));
    }

    #[test]
    fn url_caps_and_encodes_keyterms() {
        let cfg = DeepgramConfig {
            keyterms: vec!["Acme Corp".into(); KEYTERM_CAP + 50],
            ..Default::default()
        };
        let url = build_ws_url(&cfg);
        assert_eq!(url.matches("keyterm=").count(), KEYTERM_CAP);
        assert!(url.contains("keyterm=Acme%20Corp"));
    }

    #[test]
    fn auth_header_scheme() {
        assert_eq!(Auth::Bearer("j".into()).header_value(), "Bearer j");
        assert_eq!(Auth::Token("k".into()).header_value(), "Token k");
    }

    #[test]
    fn speaker_mapping_multichannel() {
        assert_eq!(map_speaker(0, 2, true), MIC_SPEAKER_OFFSET + 2);
        assert_eq!(map_speaker(1, 3, true), 3);
        assert_eq!(map_speaker(0, 2, false), 2);
    }

    #[test]
    fn promotion_rule() {
        assert!(should_promote(true, false));
        assert!(should_promote(false, true));
        assert!(!should_promote(false, false));
    }

    #[test]
    fn parses_results_and_builds_event() {
        let raw = r#"{
            "type":"Results","channel_index":[0,1],"is_final":true,"speech_final":true,
            "start":1.0,"duration":0.5,
            "channel":{"alternatives":[{"transcript":"hello there","confidence":0.97,
              "words":[{"word":"hello","punctuated_word":"Hello","start":1.0,"end":1.2,"speaker":0,"confidence":0.98}]}]}
        }"#;
        let msg = parse_message(raw).unwrap();
        let ServerMessage::Results(res) = msg else {
            panic!("expected Results");
        };
        let ev = results_to_event(&res, true, 0.5).expect("event");
        assert_eq!(ev.text, "hello there");
        assert_eq!(ev.speaker_id, MIC_SPEAKER_OFFSET); // mic channel, raw 0
        assert_eq!(ev.source, SegmentSource::Microphone);
        assert!(ev.is_final);
        assert!((ev.end - 1.5).abs() < 1e-9);
    }

    #[test]
    fn low_confidence_is_dropped() {
        let raw = r#"{"type":"Results","channel_index":[0],"is_final":true,"speech_final":false,
            "start":0.0,"duration":0.1,
            "channel":{"alternatives":[{"transcript":"maybe","confidence":0.2,"words":[]}]}}"#;
        let ServerMessage::Results(res) = parse_message(raw).unwrap() else {
            panic!()
        };
        assert!(results_to_event(&res, false, 0.5).is_none());
    }

    #[test]
    fn unknown_and_control_messages_parse() {
        assert!(matches!(
            parse_message(r#"{"type":"SpeechStarted"}"#).unwrap(),
            ServerMessage::SpeechStarted(_)
        ));
        assert!(matches!(
            parse_message(r#"{"type":"Something"}"#).unwrap(),
            ServerMessage::Unknown
        ));
    }

    #[test]
    fn keepalive_shape() {
        assert_eq!(keepalive_message(), r#"{"type":"KeepAlive"}"#);
    }

    #[test]
    fn backoff_grows_and_resets() {
        let mut b = Backoff::default();
        let d0 = b.next_delay();
        let d1 = b.next_delay();
        assert!(d1 >= d0);
        assert!(b.next_delay() <= Duration::from_secs(30));
        let gen = b.generation;
        b.reset();
        assert_eq!(b.generation, gen + 1);
        assert!(b.next_delay() <= Duration::from_secs(1));
    }
}
