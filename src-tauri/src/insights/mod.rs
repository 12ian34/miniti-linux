//! `/api/insights` request/response shaping, matching
//! `../miniti-api/docs/agents/04-api-reference.md` exactly. Coaching is local
//! only and never hits the API. Managed mode: the server owns prompts and
//! ignores `model`; BYOK (direct OpenAI) uses the pinned models below.
//!
//! `engine` runs the live cadence + final pass; `provider` talks to the backend
//! (managed) or OpenAI (BYOK); `mcp` is the Docs MCP client for Playbook.

use serde::{Deserialize, Serialize};

use crate::prefs::AppMode;

pub mod engine;
pub mod mcp;
pub mod provider;

/// Port of `buildDocsSearchQuery`: last ~600 chars of transcript as the query.
pub fn build_docs_search_query(transcript: &str) -> String {
    let n = transcript.chars().count();
    transcript.chars().skip(n.saturating_sub(600)).collect::<String>().trim().to_string()
}

/// Every `mode` value the backend accepts. Coaching is intentionally absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightMode {
    Standard,
    Meddpicc,
    Questions,
    SpeakerNames,
    Catchup,
    Investigation,
    Docs,
    DocsTopics,
}

impl InsightMode {
    pub fn api_mode(self) -> &'static str {
        match self {
            InsightMode::Standard => "standard",
            InsightMode::Meddpicc => "meddpicc",
            InsightMode::Questions => "questions",
            InsightMode::SpeakerNames => "speaker_names",
            InsightMode::Catchup => "catchup",
            InsightMode::Investigation => "investigation",
            InsightMode::Docs => "docs",
            InsightMode::DocsTopics => "docs_topics",
        }
    }

    /// Modes that accept `incremental_payload`.
    pub fn supports_incremental(self) -> bool {
        matches!(
            self,
            InsightMode::Standard | InsightMode::Meddpicc | InsightMode::Questions
        )
    }

    /// Metered on managed-free (docs lookups allowance).
    pub fn is_metered_docs_lookup(self) -> bool {
        self == InsightMode::Docs
    }
}

/// Question sub-types returned in `questions` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    Deeper,
    Challenge,
    Reframe,
    Clarify,
    Explore,
    FollowUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationScope {
    Web,
    Codebase,
}

/// Pinned model ids (contract: `/api/insights` § model).
pub const MODEL_MINI: &str = "gpt-5-mini-2025-08-07";
pub const MODEL_MINI_LARGE: &str = "gpt-5.4-mini-2026-03-17";

/// Resolve the model for a mode. Managed: mirrors what the server will use
/// (it ignores the field). BYOK: the model the client calls OpenAI with.
pub fn model_for(mode: InsightMode, app_mode: AppMode, incremental: bool) -> &'static str {
    match app_mode {
        AppMode::Byok => match mode {
            InsightMode::Investigation => MODEL_MINI_LARGE,
            _ => MODEL_MINI,
        },
        AppMode::Managed => {
            if incremental && mode.supports_incremental() {
                return MODEL_MINI;
            }
            match mode {
                InsightMode::Standard
                | InsightMode::Meddpicc
                | InsightMode::Questions
                | InsightMode::Docs
                | InsightMode::Investigation => MODEL_MINI_LARGE,
                InsightMode::SpeakerNames | InsightMode::Catchup | InsightMode::DocsTopics => {
                    MODEL_MINI
                }
            }
        }
    }
}

/// `attendees[]` entry: `{ name, domain, role? }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attendee {
    pub name: String,
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// `incremental_payload` (strategy `delta_recent_window_v1`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IncrementalPayload {
    pub strategy: String,
    pub full_segment_count: usize,
    pub acked_segment_count: usize,
    pub delta_segment_count: usize,
    pub recent_segment_count: usize,
    pub transcript_delta: String,
    pub recent_transcript: String,
    /// Sparse rolling state is accepted; missing fields default server-side.
    pub rolling_state: serde_json::Value,
}

impl IncrementalPayload {
    pub const STRATEGY: &'static str = "delta_recent_window_v1";
}

/// Request body for `POST /api/insights`. Optional fields are omitted when unset.
#[derive(Debug, Clone, Serialize, Default)]
pub struct InsightRequest {
    pub transcript: String,
    pub mode: &'static str,
    pub model: &'static str,
    pub language: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<Attendee>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_title: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub incremental: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental_payload: Option<IncrementalPayload>,
    /// `speaker_names` only: attendee display-name hints.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    /// `docs` only (required there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs_mcp_url: Option<String>,
    /// `docs` only: focused single-topic lookup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// `investigation` only (required there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investigation_scope: Option<InvestigationScope>,
    /// `investigation` only (required there, ≤ 1,000 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    /// codebase investigation only (≤ 50,000 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codebase_context: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub referenced_files: Vec<String>,
    /// `catchup` only: whole transcript as background; `transcript` is the recent window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_transcript: Option<String>,
}

pub const FOCUS_MAX_CHARS: usize = 1_000;
pub const CODEBASE_CONTEXT_MAX_CHARS: usize = 50_000;
pub const REFERENCED_FILES_MAX: usize = 20;
/// Server truncates to the last ~100KB; trim client-side to the same.
pub const TRANSCRIPT_MAX_BYTES: usize = 100 * 1024;

/// Keep the *tail* of the transcript within the server budget.
pub fn trim_transcript(transcript: &str) -> String {
    if transcript.len() <= TRANSCRIPT_MAX_BYTES {
        return transcript.to_string();
    }
    let mut start = transcript.len() - TRANSCRIPT_MAX_BYTES;
    while !transcript.is_char_boundary(start) {
        start += 1;
    }
    transcript[start..].to_string()
}

/// Build a request for `mode`. Returns `Err` when a mode-specific required
/// field is missing so callers never send a request the server will 400.
#[allow(clippy::too_many_arguments)]
pub fn build_request(
    mode: InsightMode,
    app_mode: AppMode,
    transcript: &str,
    language: &str,
    attendees: &[Attendee],
    incremental: Option<IncrementalPayload>,
    request_seq: Option<i64>,
) -> InsightRequest {
    let is_incremental = incremental.is_some() && mode.supports_incremental();
    InsightRequest {
        transcript: trim_transcript(transcript),
        mode: mode.api_mode(),
        model: model_for(mode, app_mode, is_incremental),
        language: language.to_string(),
        attendees: attendees.to_vec(),
        request_seq,
        incremental: is_incremental,
        incremental_payload: if is_incremental { incremental } else { None },
        ..Default::default()
    }
}

/// Validate mode-specific required fields before sending.
pub fn validate(req: &InsightRequest) -> Result<(), String> {
    match req.mode {
        "docs" if req.docs_mcp_url.as_deref().map(str::is_empty).unwrap_or(true) => {
            Err("docs mode requires docs_mcp_url".into())
        }
        "investigation" => {
            if req.investigation_scope.is_none() {
                return Err("investigation requires investigation_scope".into());
            }
            match req.focus.as_deref() {
                None | Some("") => return Err("investigation requires focus".into()),
                Some(f) if f.chars().count() > FOCUS_MAX_CHARS => {
                    return Err(format!("focus exceeds {FOCUS_MAX_CHARS} characters"))
                }
                _ => {}
            }
            if req.investigation_scope == Some(InvestigationScope::Codebase) {
                match req.codebase_context.as_deref() {
                    None | Some("") => return Err("codebase investigation requires codebase_context".into()),
                    Some(c) if c.chars().count() > CODEBASE_CONTEXT_MAX_CHARS => {
                        return Err(format!("codebase_context exceeds {CODEBASE_CONTEXT_MAX_CHARS} characters"))
                    }
                    _ => {}
                }
            }
            if req.referenced_files.len() > REFERENCED_FILES_MAX {
                return Err(format!("referenced_files exceeds {REFERENCED_FILES_MAX} entries"));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ---- Response models ---------------------------------------------------------

/// Flat schema returned by `standard` / `meddpicc` / `questions` / `speaker_names`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct StandardInsights {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub action_items: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub discussion_flow: Vec<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub questions: Vec<SuggestedQuestion>,
    /// `speaker_names` mode: `{ "<numeric_id>": "<Name>" }`.
    #[serde(default)]
    pub speakers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub metrics: Option<String>,
    #[serde(default)]
    pub economic_buyer: Option<String>,
    #[serde(default)]
    pub decision_criteria: Option<String>,
    #[serde(default)]
    pub decision_process: Option<String>,
    #[serde(default)]
    pub paper_process: Option<String>,
    #[serde(default)]
    pub identified_pain: Option<String>,
    #[serde(default)]
    pub champion: Option<String>,
    #[serde(default)]
    pub competition: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SuggestedQuestion {
    pub question: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub context: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocCitation {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocCard {
    pub topic: String,
    pub answer: String,
    #[serde(default)]
    pub citations: Vec<DocCitation>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InvestigationSource {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InvestigationResult {
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub sources: Vec<InvestigationSource>,
    #[serde(default)]
    pub referenced_files: Vec<String>,
}

/// Apply-safety rule (PLAN §7): a degraded response must not advance the
/// incremental ack cursor; a stale one must not be applied at all.
pub fn should_apply(meta_applied: bool, meta_stale: bool) -> bool {
    meta_applied && !meta_stale
}

pub fn should_advance_ack(meta_degraded: bool) -> bool {
    !meta_degraded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_modes_match_contract() {
        assert_eq!(InsightMode::Standard.api_mode(), "standard");
        assert_eq!(InsightMode::Meddpicc.api_mode(), "meddpicc");
        assert_eq!(InsightMode::Questions.api_mode(), "questions");
        assert_eq!(InsightMode::SpeakerNames.api_mode(), "speaker_names");
        assert_eq!(InsightMode::Catchup.api_mode(), "catchup");
        assert_eq!(InsightMode::Investigation.api_mode(), "investigation");
        assert_eq!(InsightMode::Docs.api_mode(), "docs");
        assert_eq!(InsightMode::DocsTopics.api_mode(), "docs_topics");
    }

    #[test]
    fn model_pins_follow_server_routing() {
        assert_eq!(model_for(InsightMode::Standard, AppMode::Managed, true), MODEL_MINI);
        assert_eq!(model_for(InsightMode::Standard, AppMode::Managed, false), MODEL_MINI_LARGE);
        assert_eq!(model_for(InsightMode::Meddpicc, AppMode::Managed, false), MODEL_MINI_LARGE);
        assert_eq!(model_for(InsightMode::Docs, AppMode::Managed, false), MODEL_MINI_LARGE);
        assert_eq!(model_for(InsightMode::SpeakerNames, AppMode::Managed, false), MODEL_MINI);
        assert_eq!(model_for(InsightMode::Catchup, AppMode::Managed, false), MODEL_MINI);
        assert_eq!(model_for(InsightMode::DocsTopics, AppMode::Managed, false), MODEL_MINI);
        assert_eq!(model_for(InsightMode::Standard, AppMode::Byok, false), MODEL_MINI);
        assert_eq!(model_for(InsightMode::Investigation, AppMode::Byok, false), MODEL_MINI_LARGE);
    }

    #[test]
    fn request_serializes_documented_shape() {
        let req = build_request(
            InsightMode::Standard,
            AppMode::Managed,
            "hello world",
            "en",
            &[Attendee { name: "Alex".into(), domain: "acme.com".into(), role: Some("VP".into()) }],
            Some(IncrementalPayload {
                strategy: IncrementalPayload::STRATEGY.into(),
                full_segment_count: 140,
                acked_segment_count: 120,
                delta_segment_count: 20,
                recent_segment_count: 35,
                transcript_delta: "d".into(),
                recent_transcript: "r".into(),
                rolling_state: serde_json::json!({"summary": "s"}),
            }),
            Some(42),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["mode"], "standard");
        assert_eq!(json["language"], "en");
        assert_eq!(json["incremental"], true);
        assert_eq!(json["request_seq"], 42);
        assert_eq!(json["attendees"][0]["name"], "Alex");
        assert_eq!(json["attendees"][0]["domain"], "acme.com");
        assert_eq!(json["attendees"][0]["role"], "VP");
        assert_eq!(json["incremental_payload"]["strategy"], "delta_recent_window_v1");
        assert_eq!(json["incremental_payload"]["acked_segment_count"], 120);
        assert_eq!(json["model"], MODEL_MINI, "incremental → lighter model");
        for absent in ["docs_mcp_url", "topic", "focus", "investigation_scope", "existing_summary"] {
            assert!(json.get(absent).is_none(), "{absent} must be omitted when unset");
        }
    }

    #[test]
    fn non_incremental_modes_drop_incremental_payload() {
        let req = build_request(
            InsightMode::SpeakerNames,
            AppMode::Managed,
            "t",
            "en",
            &[],
            Some(IncrementalPayload::default()),
            None,
        );
        assert!(!req.incremental);
        assert!(req.incremental_payload.is_none());
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("incremental").is_none());
        assert!(json.get("attendees").is_none());
    }

    #[test]
    fn validation_catches_missing_required_fields() {
        let mut docs = build_request(InsightMode::Docs, AppMode::Managed, "t", "en", &[], None, None);
        assert!(validate(&docs).is_err());
        docs.docs_mcp_url = Some("https://docs.example.com/mcp".into());
        assert!(validate(&docs).is_ok());

        let mut inv = build_request(InsightMode::Investigation, AppMode::Managed, "t", "en", &[], None, None);
        assert!(validate(&inv).is_err());
        inv.investigation_scope = Some(InvestigationScope::Codebase);
        inv.focus = Some("why".into());
        assert!(validate(&inv).is_err(), "codebase needs context");
        inv.codebase_context = Some("fn main() {}".into());
        assert!(validate(&inv).is_ok());
        inv.focus = Some("x".repeat(FOCUS_MAX_CHARS + 1));
        assert!(validate(&inv).is_err());
    }

    #[test]
    fn transcript_trim_keeps_tail_on_char_boundary() {
        let s = format!("{}é{}", "a".repeat(TRANSCRIPT_MAX_BYTES), "b".repeat(10));
        let t = trim_transcript(&s);
        assert!(t.len() <= TRANSCRIPT_MAX_BYTES);
        assert!(t.ends_with(&"b".repeat(10)));
        assert_eq!(trim_transcript("short"), "short");
    }

    #[test]
    fn responses_decode_documented_shapes() {
        let std: StandardInsights = serde_json::from_str(
            r#"{"summary":"s","action_items":["a"],"topics":[],"discussion_flow":[],"title":null,
                "questions":[{"question":"q","type":"deeper","context":"c"}],"speakers":{"1000":"Ian"},
                "metrics":null,"economic_buyer":null,"decision_criteria":null,"decision_process":null,
                "paper_process":null,"identified_pain":null,"champion":null,"competition":null,
                "meta":{"applied":true,"stale":false,"degraded":false,"fallback_reason":null,"request_seq":42}}"#,
        )
        .unwrap();
        assert_eq!(std.questions[0].kind, "deeper");
        assert_eq!(std.speakers["1000"], "Ian");

        let docs: Vec<DocCard> = serde_json::from_value(
            serde_json::json!([{"topic":"SSO","answer":"…","citations":[{"title":"t","url":"https://x","snippet":"s"}],"priority":"high"}]),
        )
        .unwrap();
        assert_eq!(docs[0].citations.len(), 1);

        assert!(should_apply(true, false));
        assert!(!should_apply(true, true));
        assert!(!should_advance_ack(true));
    }
}
