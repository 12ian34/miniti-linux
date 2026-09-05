//! Insight request shaping (PLAN.md §7). Coaching is local-only and never hits
//! `/api/insights`. Managed mode sends transcript + mode + language (+ attendees)
//! and the server owns prompts; BYOK uses pinned models directly.

use serde::Serialize;

use crate::prefs::AppMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsightMode {
    Summary,
    Sales,
    Questions,
    Coaching,
    Playbook,
    CatchUp,
    Investigation,
}

impl InsightMode {
    /// The `mode` string sent to `/api/insights` (None when local-only).
    pub fn api_mode(self) -> Option<&'static str> {
        match self {
            InsightMode::Summary => Some("standard"),
            InsightMode::Sales => Some("meddpicc"),
            InsightMode::Questions => Some("questions"),
            InsightMode::Playbook => Some("playbook"),
            InsightMode::CatchUp => Some("catchup"),
            InsightMode::Investigation => Some("investigation"),
            InsightMode::Coaching => None,
        }
    }

    pub fn is_local(self) -> bool {
        matches!(self, InsightMode::Coaching)
    }
}

/// Question sub-types (PLAN.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    Deeper,
    Challenge,
    Reframe,
    Clarify,
    Explore,
    FollowUp,
}

/// Pinned model ids (PLAN.md §7 — verify against `miniti-api` before ship).
pub const MODEL_MINI: &str = "gpt-5-mini-2025-08-07";
pub const MODEL_MINI_LARGE: &str = "gpt-5.4-mini-2026-03-17";

/// Resolve the model for a mode. `incremental` selects the lighter model for
/// managed incremental summary/speaker-naming/catch-up/playbook-topic passes.
pub fn model_for(mode: InsightMode, app_mode: AppMode, incremental: bool) -> &'static str {
    match app_mode {
        AppMode::Byok => match mode {
            InsightMode::Investigation => MODEL_MINI_LARGE,
            _ => MODEL_MINI,
        },
        AppMode::Managed => match mode {
            InsightMode::Investigation
            | InsightMode::Sales
            | InsightMode::Questions => MODEL_MINI_LARGE,
            InsightMode::Summary if !incremental => MODEL_MINI_LARGE,
            InsightMode::Playbook if !incremental => MODEL_MINI_LARGE,
            _ => MODEL_MINI,
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InsightRequest {
    pub mode: String,
    pub language: String,
    pub transcript: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<String>,
    pub incremental: bool,
    pub model: String,
}

/// Build a managed/BYOK insight request. Returns `None` for local-only modes.
pub fn build_request(
    mode: InsightMode,
    app_mode: AppMode,
    transcript: &str,
    language: &str,
    attendees: &[String],
    incremental: bool,
) -> Option<InsightRequest> {
    let api_mode = mode.api_mode()?;
    Some(InsightRequest {
        mode: api_mode.to_string(),
        language: language.to_string(),
        transcript: transcript.to_string(),
        attendees: attendees.to_vec(),
        incremental,
        model: model_for(mode, app_mode, incremental).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coaching_is_local_only() {
        assert!(InsightMode::Coaching.is_local());
        assert!(InsightMode::Coaching.api_mode().is_none());
        assert!(build_request(
            InsightMode::Coaching,
            AppMode::Managed,
            "t",
            "en",
            &[],
            false
        )
        .is_none());
    }

    #[test]
    fn api_modes_match_contract() {
        assert_eq!(InsightMode::Summary.api_mode(), Some("standard"));
        assert_eq!(InsightMode::Sales.api_mode(), Some("meddpicc"));
        assert_eq!(InsightMode::Questions.api_mode(), Some("questions"));
        assert_eq!(InsightMode::Investigation.api_mode(), Some("investigation"));
    }

    #[test]
    fn model_pins() {
        // Managed incremental summary -> lighter model.
        assert_eq!(model_for(InsightMode::Summary, AppMode::Managed, true), MODEL_MINI);
        // Managed final summary -> larger model.
        assert_eq!(model_for(InsightMode::Summary, AppMode::Managed, false), MODEL_MINI_LARGE);
        // Managed MEDDPICC always larger.
        assert_eq!(model_for(InsightMode::Sales, AppMode::Managed, true), MODEL_MINI_LARGE);
        // BYOK default mini, investigations larger.
        assert_eq!(model_for(InsightMode::Summary, AppMode::Byok, false), MODEL_MINI);
        assert_eq!(model_for(InsightMode::Investigation, AppMode::Byok, false), MODEL_MINI_LARGE);
    }

    #[test]
    fn request_shape_serializes() {
        let req = build_request(
            InsightMode::Summary,
            AppMode::Managed,
            "hello world",
            "en",
            &["Alex".to_string()],
            true,
        )
        .unwrap();
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["mode"], "standard");
        assert_eq!(json["language"], "en");
        assert_eq!(json["incremental"], true);
        assert_eq!(json["attendees"][0], "Alex");
    }
}
