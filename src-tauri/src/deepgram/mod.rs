//! Deepgram live transcription client (PLAN.md §6). Pure helpers (URL/query,
//! message parsing, speaker-identity mapping, per-word speaker segmentation,
//! backoff) are unit-tested here; [`live`] owns the WebSocket.
//!
//! The speaker contract is a port of the Apple `SpeakerIdentityState` +
//! `segmentBySpeaker` pair: system speakers keep low IDs, microphone speakers
//! live at `1000 + n`, and speaker switches / brand-new speakers only promote
//! after sustained evidence.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod echo;
pub mod live;

pub const DEEPGRAM_WS_BASE: &str = "wss://api.deepgram.com/v1/listen";

/// First microphone app speaker id. All mic speakers are `>= MIC_SPEAKER_ID`;
/// system speakers are below it (see Apple `DeepgramService.micSpeakerID`).
pub const MIC_SPEAKER_ID: i64 = 1000;
/// Kept for callers that still use the old name.
pub const MIC_SPEAKER_OFFSET: i64 = MIC_SPEAKER_ID;

pub const MIC_CHANNEL_INDEX: u32 = 0;
pub const SYSTEM_CHANNEL_INDEX: u32 = 1;

/// Max key terms sent on the query string (Deepgram caps request size).
pub const KEYTERM_CAP: usize = 100;

pub fn is_mic_app_speaker_id(id: i64) -> bool {
    id >= MIC_SPEAKER_ID
}

#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub language: String,
    /// When true, stream `channels=2&multichannel=true` (mic ch0 + system ch1).
    pub multichannel: bool,
    pub keyterms: Vec<String>,
    /// `find:replace` pairs (dictionary corrections), sent after the keyterms.
    pub replacements: Vec<String>,
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            multichannel: false,
            keyterms: Vec::new(),
            replacements: Vec::new(),
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
        // diarize_model alone, never with the deprecated diarize=true: Deepgram
        // returns 400 when both are present (matches the macOS client).
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
    for pair in cfg.replacements.iter().take(crate::corrections::CAP) {
        params.push(("replace".into(), pair.clone()));
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

/// Tells Deepgram the audio is finished; it then flushes the remaining finals
/// and a trailing `Metadata` frame before closing.
pub fn close_stream_message() -> String {
    r#"{"type":"CloseStream"}"#.to_string()
}

pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// How long to wait for finals after `CloseStream` before giving up.
pub const CLOSE_DRAIN_TIMEOUT: Duration = Duration::from_millis(1500);

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
    pub words: Vec<WireWord>,
}

#[derive(Debug, Deserialize)]
pub struct WireWord {
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
    #[serde(default)]
    pub speaker_confidence: Option<f64>,
}

/// Parse a text frame from Deepgram into a [`ServerMessage`].
pub fn parse_message(text: &str) -> Result<ServerMessage, serde_json::Error> {
    serde_json::from_str(text)
}

/// Promote a segment to the transcript only on `is_final || speech_final`
/// (interims never promote speakers — same rule as the Apple apps).
pub fn should_promote(is_final: bool, speech_final: bool) -> bool {
    is_final || speech_final
}

// ---- Source + speaker identity ----------------------------------------------

/// Which capture lane a transcript segment came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentSource {
    Microphone,
    System,
    /// Malformed multichannel response (missing / out-of-range channel index).
    Unknown,
}

impl SegmentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentSource::Microphone => "microphone",
            SegmentSource::System => "system",
            SegmentSource::Unknown => "unknown",
        }
    }
}

/// Resolve the source for a response (port of `responseSource`).
pub fn response_source(
    multichannel: bool,
    stream_channel: Option<u32>,
    mono_source: SegmentSource,
) -> SegmentSource {
    if !multichannel {
        return mono_source;
    }
    match stream_channel {
        Some(MIC_CHANNEL_INDEX) => SegmentSource::Microphone,
        Some(SYSTEM_CHANNEL_INDEX) => SegmentSource::System,
        _ => SegmentSource::Unknown,
    }
}

/// Allocates collision-free app speaker IDs from Deepgram's channel-local,
/// socket-local speaker numbers (port of Apple `SpeakerIdentityState`).
///
/// - Microphone speakers occupy `1000 + n` by first-seen ordinal.
/// - System speakers on the first socket keep their provider number; later
///   sockets allocate fresh sequential low IDs.
/// - Reconnect continuity: while at most one mic speaker (`1000`) has been
///   seen, the first mic speaker of the next socket maps back to `1000`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeakerIdentityState {
    assignments: HashMap<(SegmentSource, i64, u64), i64>,
    next_system_app_id: i64,
    next_mic_offset: i64,
    allocated_mic_app_ids: BTreeSet<i64>,
    generation: u64,
    mic_continuity_available: bool,
}

impl SpeakerIdentityState {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Begin a new WebSocket connection. `preserving` keeps earlier allocations
    /// (reconnect within one meeting); `false` resets for a fresh recording.
    pub fn begin_connection(&mut self, preserving: bool) {
        if !preserving {
            *self = Self::default();
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.mic_continuity_available = self.allocated_mic_app_ids.is_empty()
            || (self.allocated_mic_app_ids.len() == 1
                && self.allocated_mic_app_ids.contains(&MIC_SPEAKER_ID));
    }

    /// Register app speaker IDs restored from a persisted meeting so later
    /// allocations cannot collide with them.
    pub fn seed_restored_app_speaker_ids(&mut self, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            if is_mic_app_speaker_id(id) {
                self.allocated_mic_app_ids.insert(id);
                self.next_mic_offset = self.next_mic_offset.max(id - MIC_SPEAKER_ID + 1);
            } else {
                self.next_system_app_id = self.next_system_app_id.max(id + 1);
            }
        }
    }

    pub fn app_speaker_id(&mut self, source: SegmentSource, provider_id: i64) -> i64 {
        if source == SegmentSource::Unknown {
            return provider_id;
        }
        let key = (source, provider_id, self.generation);
        if let Some(existing) = self.assignments.get(&key) {
            return *existing;
        }
        let app_id = match source {
            SegmentSource::Microphone => {
                let id = if self.mic_continuity_available {
                    self.mic_continuity_available = false;
                    MIC_SPEAKER_ID
                } else {
                    let mut candidate = MIC_SPEAKER_ID + self.next_mic_offset;
                    while self.allocated_mic_app_ids.contains(&candidate) {
                        self.next_mic_offset += 1;
                        candidate = MIC_SPEAKER_ID + self.next_mic_offset;
                    }
                    self.next_mic_offset += 1;
                    candidate
                };
                self.allocated_mic_app_ids.insert(id);
                id
            }
            SegmentSource::System => {
                if self.generation == 0 {
                    self.next_system_app_id = self.next_system_app_id.max(provider_id + 1);
                    provider_id
                } else {
                    let id = self.next_system_app_id;
                    self.next_system_app_id += 1;
                    id
                }
            }
            SegmentSource::Unknown => provider_id,
        };
        self.assignments.insert(key, app_id);
        app_id
    }

    pub fn mic_app_speaker_ids(&self) -> &BTreeSet<i64> {
        &self.allocated_mic_app_ids
    }
}

// ---- Per-word segmentation ----------------------------------------------------

/// A word with its app speaker id already resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f64,
    pub speaker: i64,
    pub speaker_confidence: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PendingSpeakerEvidence {
    word_count: usize,
    duration: f64,
    speaker_confidence_sum: f64,
    speaker_confidence_count: usize,
}

impl PendingSpeakerEvidence {
    fn add(&mut self, words: &[Word]) {
        if words.is_empty() {
            return;
        }
        self.word_count += words.len();
        if let (Some(first), Some(last)) = (words.first(), words.last()) {
            self.duration += (last.end - first.start).max(0.0);
        }
        for w in words {
            if let Some(c) = w.speaker_confidence {
                self.speaker_confidence_sum += c;
                self.speaker_confidence_count += 1;
            }
        }
    }

    fn average_speaker_confidence(&self) -> Option<f64> {
        if self.speaker_confidence_count == 0 {
            None
        } else {
            Some(self.speaker_confidence_sum / self.speaker_confidence_count as f64)
        }
    }
}

/// Speaker-switch confirmation state. Keys are app speaker IDs, already
/// source-scoped by the identity mapping (mic ≥ 1000, system < 1000).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SegmentationState {
    pub confirmed_speaker_ids: BTreeSet<i64>,
    pub pending_speaker_evidence: HashMap<i64, PendingSpeakerEvidence>,
}

/// A stabilized transcript segment ready to render / persist.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptEvent {
    pub text: String,
    pub speaker_id: i64,
    /// Seconds from meeting start (socket time + per-connection offset).
    pub start: f64,
    pub end: f64,
    pub is_final: bool,
    pub confidence: f64,
    pub source: SegmentSource,
    pub channel_index: Option<u32>,
}

const MIN_WORDS_FOR_SPEAKER_CHANGE: usize = 4;
const MIN_DURATION_FOR_SPEAKER_CHANGE: f64 = 0.85;
const MIN_AVG_SPEAKER_CONFIDENCE_FOR_SWITCH: f64 = 0.58;
const MIN_WORDS_FOR_NEW_SPEAKER_PROMOTION: usize = 6;
const MIN_DURATION_FOR_NEW_SPEAKER_PROMOTION: f64 = 1.50;
const MIN_AVG_SPEAKER_CONFIDENCE_FOR_NEW_SPEAKER: f64 = 0.65;

fn avg_speaker_confidence(words: &[Word]) -> Option<f64> {
    let confs: Vec<f64> = words.iter().filter_map(|w| w.speaker_confidence).collect();
    if confs.is_empty() {
        None
    } else {
        Some(confs.iter().sum::<f64>() / confs.len() as f64)
    }
}

fn make_segment(
    speaker: i64,
    words: &[Word],
    start: f64,
    is_final: bool,
    confidence: f64,
    channel_index: Option<u32>,
    source: SegmentSource,
) -> TranscriptEvent {
    TranscriptEvent {
        text: words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        speaker_id: speaker,
        start,
        end: words.last().map(|w| w.end).unwrap_or(start),
        is_final,
        confidence,
        source,
        channel_index,
    }
}

/// Split one alternative's words into speaker segments, confirming switches
/// only on sustained evidence (port of Apple `segmentBySpeaker`).
pub fn segment_by_speaker(
    words: &[Word],
    is_final: bool,
    confidence: f64,
    channel_index: Option<u32>,
    source: SegmentSource,
    state: &mut SegmentationState,
) -> Vec<TranscriptEvent> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut current_speaker = words[0].speaker;
    let mut current_words: Vec<Word> = Vec::new();
    let mut start_time = words[0].start;

    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        if word.speaker == current_speaker {
            current_words.push(word.clone());
            i += 1;
            continue;
        }

        let new_speaker = word.speaker;
        let mut look_ahead = i;
        while look_ahead < words.len() && words[look_ahead].speaker == new_speaker {
            look_ahead += 1;
        }
        let candidate = &words[i..look_ahead];
        let candidate_duration = match (candidate.first(), candidate.last()) {
            (Some(f), Some(l)) => (l.end - f.start).max(0.0),
            _ => 0.0,
        };
        let candidate_avg_conf = avg_speaker_confidence(candidate);

        let passes_general = candidate.len() >= MIN_WORDS_FOR_SPEAKER_CHANGE
            && candidate_duration >= MIN_DURATION_FOR_SPEAKER_CHANGE;
        // The primary mic identity keeps its exemption only while it is the
        // sole mic speaker; once another mic speaker is confirmed it must pass
        // the same confidence gate as everyone else.
        let other_mic_confirmed = state
            .confirmed_speaker_ids
            .iter()
            .any(|&id| is_mic_app_speaker_id(id) && id != MIC_SPEAKER_ID);
        let mic_primary_privileged = new_speaker == MIC_SPEAKER_ID && !other_mic_confirmed;
        let passes_confidence = mic_primary_privileged
            || candidate_avg_conf.unwrap_or(1.0) >= MIN_AVG_SPEAKER_CONFIDENCE_FOR_SWITCH;
        let is_known = state.confirmed_speaker_ids.contains(&new_speaker) || mic_primary_privileged;

        let mut allow_switch = passes_general && passes_confidence;
        if allow_switch && !is_known {
            let evidence = state
                .pending_speaker_evidence
                .entry(new_speaker)
                .or_default();
            evidence.add(candidate);
            let promoted = evidence.word_count >= MIN_WORDS_FOR_NEW_SPEAKER_PROMOTION
                && evidence.duration >= MIN_DURATION_FOR_NEW_SPEAKER_PROMOTION
                && evidence.average_speaker_confidence().unwrap_or(1.0)
                    >= MIN_AVG_SPEAKER_CONFIDENCE_FOR_NEW_SPEAKER;
            if promoted {
                state.confirmed_speaker_ids.insert(new_speaker);
                state.pending_speaker_evidence.remove(&new_speaker);
            } else {
                allow_switch = false;
            }
        }

        if allow_switch {
            if !current_words.is_empty() {
                segments.push(make_segment(
                    current_speaker,
                    &current_words,
                    start_time,
                    is_final,
                    confidence,
                    channel_index,
                    source,
                ));
            }
            current_speaker = new_speaker;
            current_words = candidate.to_vec();
            start_time = candidate.first().map(|w| w.start).unwrap_or(word.start);
        } else {
            current_words.extend_from_slice(candidate);
        }
        i = look_ahead;
    }

    if !current_words.is_empty() {
        segments.push(make_segment(
            current_speaker,
            &current_words,
            start_time,
            is_final,
            confidence,
            channel_index,
            source,
        ));
    }
    segments
}

/// Stateful response processor for one meeting: identity mapping + speaker
/// segmentation across sockets (port of `processTranscriptJSON`).
#[derive(Debug, Clone)]
pub struct Processor {
    pub multichannel: bool,
    pub mono_source: SegmentSource,
    pub segmentation: SegmentationState,
    pub identities: SpeakerIdentityState,
}

impl Processor {
    pub fn new(multichannel: bool, mono_source: SegmentSource) -> Self {
        Self {
            multichannel,
            mono_source,
            segmentation: SegmentationState::default(),
            identities: SpeakerIdentityState::default(),
        }
    }

    /// Call before each socket connect. `preserving` = reconnect within a meeting.
    pub fn begin_connection(&mut self, preserving: bool) {
        self.identities.begin_connection(preserving);
        if !preserving {
            self.segmentation = SegmentationState::default();
        }
    }

    /// Convert a `Results` frame into zero or more stabilized events.
    /// `time_offset` shifts socket-relative times onto the meeting timeline.
    pub fn process(&mut self, res: &Results, time_offset: f64) -> Vec<TranscriptEvent> {
        let Some(alt) = res.channel.alternatives.first() else {
            return Vec::new();
        };
        if alt.transcript.trim().is_empty() {
            return Vec::new();
        }
        let is_final = should_promote(res.is_final, res.speech_final);
        let stream_channel = res.channel_index.first().copied();
        let source = response_source(self.multichannel, stream_channel, self.mono_source);

        let words: Vec<Word> = if alt.words.is_empty() {
            // No word timings: one segment at the response granularity.
            vec![Word {
                text: alt.transcript.clone(),
                start: res.start,
                end: res.start + res.duration,
                confidence: alt.confidence,
                speaker: self.identities.app_speaker_id(source, 0),
                speaker_confidence: None,
            }]
        } else {
            alt.words
                .iter()
                .map(|w| Word {
                    text: w.punctuated_word.clone().unwrap_or_else(|| w.word.clone()),
                    start: w.start,
                    end: w.end,
                    confidence: w.confidence,
                    speaker: self
                        .identities
                        .app_speaker_id(source, w.speaker.unwrap_or(0)),
                    speaker_confidence: w.speaker_confidence,
                })
                .collect()
        };

        let mut segments = segment_by_speaker(
            &words,
            is_final,
            alt.confidence,
            stream_channel,
            source,
            &mut self.segmentation,
        );
        if is_final {
            for s in &segments {
                self.segmentation.confirmed_speaker_ids.insert(s.speaker_id);
                self.segmentation
                    .pending_speaker_evidence
                    .remove(&s.speaker_id);
            }
        }
        for s in &mut segments {
            s.start += time_offset;
            s.end += time_offset;
        }
        segments
    }
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
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

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

    fn word(text: &str, start: f64, speaker: i64, conf: Option<f64>) -> Word {
        Word {
            text: text.into(),
            start,
            end: start + 0.3,
            confidence: 0.9,
            speaker,
            speaker_confidence: conf,
        }
    }

    fn run(text: &str, speaker: i64, start: f64, n: usize) -> Vec<Word> {
        (0..n)
            .map(|i| word(text, start + i as f64 * 0.3, speaker, Some(0.9)))
            .collect()
    }

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
        assert!(
            !url.contains("punctuate="),
            "must not send punctuate with smart_format"
        );
        assert!(url.contains("diarize_model=latest"));
        assert!(
            !url.contains("diarize=true"),
            "deprecated diarize=true with diarize_model → 400"
        );
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
    fn url_puts_replacements_after_keyterms() {
        let cfg = DeepgramConfig {
            keyterms: vec!["Lightdash".into()],
            replacements: vec!["light dash:Lightdash".into(), "sasha:Sascha".into()],
            ..DeepgramConfig::default()
        };
        let url = build_ws_url(&cfg);
        let k = url.find("keyterm=Lightdash").unwrap();
        let r = url.find("replace=light%20dash%3ALightdash").unwrap();
        assert!(k < r, "replace items follow the keyterms");
        assert_eq!(url.matches("replace=").count(), 2);
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
    fn control_messages_have_expected_shape() {
        assert_eq!(keepalive_message(), r#"{"type":"KeepAlive"}"#);
        assert_eq!(close_stream_message(), r#"{"type":"CloseStream"}"#);
    }

    #[test]
    fn promotion_rule() {
        assert!(should_promote(true, false));
        assert!(should_promote(false, true));
        assert!(!should_promote(false, false));
    }

    #[test]
    fn response_source_mapping() {
        assert_eq!(
            response_source(true, Some(0), SegmentSource::Microphone),
            SegmentSource::Microphone
        );
        assert_eq!(
            response_source(true, Some(1), SegmentSource::Microphone),
            SegmentSource::System
        );
        assert_eq!(
            response_source(true, Some(7), SegmentSource::Microphone),
            SegmentSource::Unknown
        );
        assert_eq!(
            response_source(true, None, SegmentSource::Microphone),
            SegmentSource::Unknown
        );
        assert_eq!(
            response_source(false, None, SegmentSource::System),
            SegmentSource::System
        );
    }

    // ---- SpeakerIdentityState ----

    #[test]
    fn mono_mic_speakers_live_in_mic_range() {
        // iOS/macOS contract: mic-only sessions map every speaker to 1000 + n.
        let mut ids = SpeakerIdentityState::default();
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 0), 1000);
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 1), 1001);
        assert_eq!(
            ids.app_speaker_id(SegmentSource::Microphone, 0),
            1000,
            "stable"
        );
    }

    #[test]
    fn system_speakers_keep_provider_ids_on_first_socket() {
        let mut ids = SpeakerIdentityState::default();
        assert_eq!(ids.app_speaker_id(SegmentSource::System, 0), 0);
        assert_eq!(ids.app_speaker_id(SegmentSource::System, 2), 2);
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 0), 1000);
    }

    #[test]
    fn reconnect_keeps_single_mic_identity_and_allocates_fresh_system_ids() {
        let mut ids = SpeakerIdentityState::default();
        ids.app_speaker_id(SegmentSource::Microphone, 0); // 1000
        ids.app_speaker_id(SegmentSource::System, 0); // 0
        ids.app_speaker_id(SegmentSource::System, 1); // 1

        ids.begin_connection(true);
        // Provider numbers restart at 0 on the new socket.
        assert_eq!(
            ids.app_speaker_id(SegmentSource::Microphone, 0),
            1000,
            "single mic continuity"
        );
        assert_eq!(
            ids.app_speaker_id(SegmentSource::System, 0),
            2,
            "fresh system id, no collision"
        );
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 1), 1001);
    }

    #[test]
    fn reconnect_with_multiple_mic_speakers_gets_fresh_identities() {
        let mut ids = SpeakerIdentityState::default();
        ids.app_speaker_id(SegmentSource::Microphone, 0); // 1000
        ids.app_speaker_id(SegmentSource::Microphone, 1); // 1001
        ids.begin_connection(true);
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 0), 1002);
    }

    #[test]
    fn seeded_ids_are_never_reallocated() {
        let mut ids = SpeakerIdentityState::default();
        ids.seed_restored_app_speaker_ids([1000, 1001, 3]);
        ids.begin_connection(true);
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 0), 1002);
        assert_eq!(ids.app_speaker_id(SegmentSource::System, 0), 4);
    }

    #[test]
    fn fresh_connection_resets_state() {
        let mut ids = SpeakerIdentityState::default();
        ids.app_speaker_id(SegmentSource::Microphone, 0);
        ids.app_speaker_id(SegmentSource::Microphone, 1);
        ids.begin_connection(false);
        assert_eq!(ids.app_speaker_id(SegmentSource::Microphone, 0), 1000);
    }

    // ---- segment_by_speaker ----

    #[test]
    fn short_interjection_is_absorbed_into_current_speaker() {
        let mut words = run("a", 1000, 0.0, 6);
        words.extend(run("yeah", 1001, 2.0, 1)); // 1 word, below switch threshold
        words.extend(run("b", 1000, 2.5, 3));
        let mut st = SegmentationState::default();
        let segs = segment_by_speaker(&words, true, 0.9, None, SegmentSource::Microphone, &mut st);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker_id, 1000);
        assert_eq!(segs[0].text.split(' ').count(), 10);
    }

    #[test]
    fn new_speaker_needs_sustained_evidence_then_promotes() {
        let mut st = SegmentationState::default();
        // First response: 4 words / 1.2s from a new speaker — passes the switch
        // gate but not promotion (needs 6 words + 1.5s). Absorbed.
        let mut words = run("a", 0, 0.0, 5);
        words.extend(run("b", 1, 3.0, 4));
        let segs = segment_by_speaker(&words, true, 0.9, Some(1), SegmentSource::System, &mut st);
        assert_eq!(segs.len(), 1, "not yet promoted");
        assert!(st.pending_speaker_evidence.contains_key(&1));

        // Second response: more evidence accumulates and promotes speaker 1.
        let mut words = run("a", 0, 10.0, 5);
        words.extend(run("b", 1, 13.0, 4));
        let segs = segment_by_speaker(&words, true, 0.9, Some(1), SegmentSource::System, &mut st);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].speaker_id, 1);
        assert!(st.confirmed_speaker_ids.contains(&1));
    }

    #[test]
    fn known_speaker_switches_immediately_with_enough_words() {
        let mut st = SegmentationState::default();
        st.confirmed_speaker_ids.insert(0);
        st.confirmed_speaker_ids.insert(1);
        let mut words = run("a", 0, 0.0, 5);
        words.extend(run("b", 1, 3.0, 4));
        let segs = segment_by_speaker(&words, true, 0.9, Some(1), SegmentSource::System, &mut st);
        assert_eq!(segs.len(), 2);
        assert_eq!((segs[0].speaker_id, segs[1].speaker_id), (0, 1));
        assert!((segs[1].start - 3.0).abs() < 1e-9);
    }

    #[test]
    fn low_speaker_confidence_blocks_switch() {
        let mut st = SegmentationState::default();
        st.confirmed_speaker_ids.insert(0);
        st.confirmed_speaker_ids.insert(1);
        let mut words = run("a", 0, 0.0, 5);
        let shaky: Vec<Word> = (0..5)
            .map(|i| word("b", 3.0 + i as f64 * 0.3, 1, Some(0.3)))
            .collect();
        words.extend(shaky);
        let segs = segment_by_speaker(&words, true, 0.9, Some(1), SegmentSource::System, &mut st);
        assert_eq!(
            segs.len(),
            1,
            "low speaker_confidence must not flip speakers"
        );
    }

    #[test]
    fn primary_mic_speaker_is_privileged_while_alone() {
        let mut st = SegmentationState::default();
        st.confirmed_speaker_ids.insert(0);
        let mut words = run("a", 0, 0.0, 5);
        // Switch to 1000 with poor confidence still passes (sole mic speaker).
        let back: Vec<Word> = (0..5)
            .map(|i| word("me", 3.0 + i as f64 * 0.3, 1000, Some(0.2)))
            .collect();
        words.extend(back);
        let segs = segment_by_speaker(&words, true, 0.9, None, SegmentSource::Microphone, &mut st);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].speaker_id, 1000);
    }

    // ---- Processor ----

    fn results_json(channel_index: &str, is_final: bool, words: &[(&str, f64, i64)]) -> String {
        let words_json: Vec<String> = words
            .iter()
            .map(|(w, s, sp)| {
                format!(
                    r#"{{"word":"{w}","punctuated_word":"{w}","start":{s},"end":{},"speaker":{sp},"confidence":0.95,"speaker_confidence":0.9}}"#,
                    s + 0.3
                )
            })
            .collect();
        let transcript: Vec<&str> = words.iter().map(|(w, _, _)| *w).collect();
        format!(
            r#"{{"type":"Results","channel_index":{channel_index},"is_final":{is_final},"speech_final":false,
                "start":{},"duration":1.0,
                "channel":{{"alternatives":[{{"transcript":"{}","confidence":0.97,"words":[{}]}}]}}}}"#,
            words.first().map(|w| w.1).unwrap_or(0.0),
            transcript.join(" "),
            words_json.join(",")
        )
    }

    fn results(json: &str) -> Results {
        match parse_message(json).unwrap() {
            ServerMessage::Results(r) => r,
            other => panic!("expected Results, got {other:?}"),
        }
    }

    #[test]
    fn processor_mono_maps_to_mic_range_and_uses_punctuated_words() {
        let mut p = Processor::new(false, SegmentSource::Microphone);
        let res = results(&results_json(
            "[0]",
            true,
            &[("Hello", 1.0, 0), ("there", 1.3, 0)],
        ));
        let evs = p.process(&res, 0.0);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].speaker_id, 1000);
        assert_eq!(evs[0].source, SegmentSource::Microphone);
        assert_eq!(evs[0].text, "Hello there");
        assert!(evs[0].is_final);
        assert!((evs[0].start - 1.0).abs() < 1e-9);
        assert!((evs[0].end - 1.6).abs() < 1e-9);
    }

    #[test]
    fn processor_multichannel_routes_channels_and_applies_offset() {
        let mut p = Processor::new(true, SegmentSource::Microphone);
        let mic = results(&results_json("[0,2]", true, &[("hi", 1.0, 0)]));
        let sys = results(&results_json("[1,2]", true, &[("yo", 2.0, 0)]));
        let a = p.process(&mic, 10.0);
        let b = p.process(&sys, 10.0);
        assert_eq!(a[0].speaker_id, 1000);
        assert_eq!(a[0].source, SegmentSource::Microphone);
        assert!((a[0].start - 11.0).abs() < 1e-9, "offset applied");
        assert_eq!(b[0].speaker_id, 0);
        assert_eq!(b[0].source, SegmentSource::System);
    }

    #[test]
    fn processor_ignores_empty_transcripts_and_confirms_speakers_on_finals() {
        let mut p = Processor::new(false, SegmentSource::Microphone);
        let empty = results(
            r#"{"type":"Results","channel_index":[0],"is_final":true,"speech_final":false,"start":0,"duration":0.1,
                "channel":{"alternatives":[{"transcript":"   ","confidence":0.9,"words":[]}]}}"#,
        );
        assert!(p.process(&empty, 0.0).is_empty());

        let interim = results(&results_json("[0]", false, &[("a", 0.0, 0)]));
        p.process(&interim, 0.0);
        assert!(
            p.segmentation.confirmed_speaker_ids.is_empty(),
            "interims never confirm"
        );

        let fin = results(&results_json("[0]", true, &[("a", 0.0, 0)]));
        p.process(&fin, 0.0);
        assert!(p.segmentation.confirmed_speaker_ids.contains(&1000));
    }

    #[test]
    fn processor_low_confidence_is_not_dropped() {
        // macOS never gates on alternative confidence; neither do we.
        let mut p = Processor::new(false, SegmentSource::Microphone);
        let res = results(
            r#"{"type":"Results","channel_index":[0],"is_final":true,"speech_final":false,"start":0,"duration":0.1,
                "channel":{"alternatives":[{"transcript":"maybe","confidence":0.2,"words":[]}]}}"#,
        );
        assert_eq!(p.process(&res, 0.0).len(), 1);
    }

    #[test]
    fn unknown_and_control_messages_parse() {
        assert!(matches!(
            parse_message(r#"{"type":"SpeechStarted"}"#).unwrap(),
            ServerMessage::SpeechStarted(_)
        ));
        assert!(matches!(
            parse_message(r#"{"type":"Metadata","request_id":"x"}"#).unwrap(),
            ServerMessage::Metadata(_)
        ));
        assert!(matches!(
            parse_message(r#"{"type":"Something"}"#).unwrap(),
            ServerMessage::Unknown
        ));
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
