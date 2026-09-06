//! Markdown export — port of `Meeting.fullMeetingAsMarkdown()` and friends
//! (`../miniti/Miniti/Models/Meeting.swift`). Same section order and
//! formatting so exports from Linux and macOS look identical.

use std::collections::HashMap;

use serde_json::Value;

use crate::coaching::{self, TrainingMetrics};
use crate::db::{Meeting, TranscriptSegment};

fn arr(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn labels(meeting: &Meeting) -> (Option<HashMap<String, String>>, Option<Vec<i64>>) {
    let names: HashMap<String, String> =
        serde_json::from_str(&meeting.speaker_names).unwrap_or_default();
    (
        if names.is_empty() { None } else { Some(names) },
        meeting.self_speaker_ids(),
    )
}

pub fn transcript_as_markdown(meeting: &Meeting, segments: &[TranscriptSegment]) -> String {
    let (names, self_ids) = labels(meeting);
    let mut sorted: Vec<&TranscriptSegment> = segments.iter().collect();
    sorted.sort_by(|a, b| {
        a.start_s
            .partial_cmp(&b.start_s)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut md = String::from("## Transcript\n\n");
    let mut current: Option<String> = None;
    for s in sorted {
        let label =
            coaching::resolved_speaker_label(s.speaker, names.as_ref(), self_ids.as_deref());
        if current.as_deref() != Some(label.as_str()) {
            current = Some(label.clone());
            md.push_str(&format!("\n**{label}:**\n"));
        }
        md.push_str(&s.text);
        md.push(' ');
    }
    md.trim().to_string()
}

pub fn notes_as_markdown(meeting: &Meeting) -> String {
    if meeting.notes.trim().is_empty() {
        String::new()
    } else {
        format!("## Notes\n\n{}", meeting.notes)
    }
}

pub fn insights_as_markdown(meeting: &Meeting) -> String {
    let mut md = String::from("## Insights\n\n");
    if !meeting.summary.trim().is_empty() {
        md.push_str(&format!("### Summary\n\n{}\n\n", meeting.summary));
    }
    let flow = arr(&meeting.discussion_flow);
    if !flow.is_empty() {
        md.push_str("### Discussion Flow\n\n");
        for (i, item) in flow.iter().enumerate() {
            md.push_str(&format!("{}. {item}\n", i + 1));
        }
        md.push('\n');
    }
    let actions = arr(&meeting.action_items);
    if !actions.is_empty() {
        md.push_str("### Action Items\n\n");
        for a in &actions {
            md.push_str(&format!("- [ ] {a}\n"));
        }
        md.push('\n');
    }
    let decisions = arr(&meeting.key_decisions);
    if !decisions.is_empty() {
        md.push_str("### Key Decisions\n\n");
        for d in &decisions {
            md.push_str(&format!("- {d}\n"));
        }
        md.push('\n');
    }
    let topics = arr(&meeting.topics);
    if !topics.is_empty() {
        md.push_str("### Topics\n\n");
        for t in &topics {
            md.push_str(&format!("- {t}\n"));
        }
        md.push('\n');
    }
    let questions: Vec<Value> =
        serde_json::from_str(&meeting.suggested_questions).unwrap_or_default();
    if !questions.is_empty() {
        md.push_str("### Suggested Questions\n\n");
        for q in &questions {
            let question = q
                .get("question")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            let context = q
                .get("context")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            md.push_str(&format!("- **{question}**\n  _{context}_\n"));
        }
        md.push('\n');
    }
    let docs: Vec<Value> = serde_json::from_str(&meeting.docs).unwrap_or_default();
    if !docs.is_empty() {
        md.push_str("### Docs\n\n");
        for card in &docs {
            md.push_str(&format!(
                "- **{}**\n  {}\n",
                card.get("topic")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default(),
                card.get("answer")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
            ));
            for c in card
                .get("citations")
                .and_then(|c| c.as_array())
                .into_iter()
                .flatten()
            {
                let title = c
                    .get("title")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Documentation");
                match c
                    .get("url")
                    .and_then(|s| s.as_str())
                    .filter(|u| !u.is_empty())
                {
                    Some(url) => md.push_str(&format!("  - [{title}]({url})\n")),
                    None => md.push_str(&format!("  - {title}\n")),
                }
            }
        }
        md.push('\n');
    }
    let med: HashMap<String, Value> = serde_json::from_str(&meeting.meddpicc).unwrap_or_default();
    let fields = [
        ("Metrics", "metrics"),
        ("Economic Buyer", "economic_buyer"),
        ("Decision Criteria", "decision_criteria"),
        ("Decision Process", "decision_process"),
        ("Paper Process", "paper_process"),
        ("Identified Pain", "identified_pain"),
        ("Champion", "champion"),
        ("Competition", "competition"),
    ];
    let has_med = fields.iter().any(|(_, k)| {
        med.get(*k)
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    });
    if has_med {
        md.push_str("### MEDDPICC\n\n");
        for (label, key) in fields {
            if let Some(v) = med
                .get(key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
            {
                md.push_str(&format!("**{label}:** {v}\n\n"));
            }
        }
    }
    if let Some(template) = crate::insights::templates::find(&meeting.template_id) {
        let sections = crate::insights::templates::parse_sections(&meeting.template_sections);
        let block = crate::insights::templates::markdown(&template, &sections, 3);
        if !block.is_empty() {
            md.push_str(&block);
        }
    }
    md.trim().to_string()
}

pub fn training_as_markdown(
    meeting: &Meeting,
    segments: &[TranscriptSegment],
    fillers: &[String],
) -> String {
    let duration = meeting.duration_seconds();
    if duration <= 0 {
        return String::new();
    }
    let (names, self_ids) = labels(meeting);
    let turns: Vec<coaching::Segment> = segments
        .iter()
        .map(|s| coaching::Segment {
            text: s.text.clone(),
            speaker: s.speaker,
            is_final: true,
            timestamp: s.start_s,
        })
        .collect();
    let m: TrainingMetrics = coaching::compute(
        &turns,
        duration as f64,
        fillers,
        names.as_ref(),
        self_ids.as_deref(),
    );
    if m.speakers.is_empty() {
        return String::new();
    }
    let mut md = format!("## Coaching\n\n**Duration:** {:.1} min", m.duration_minutes);
    if let Some(you) = m.speakers.iter().find(|s| s.is_local_mic) {
        let total: usize = m.speakers.iter().map(|s| s.word_count).sum();
        let ratio = if total > 0 {
            (you.word_count as f64 / total as f64 * 100.0) as i64
        } else {
            0
        };
        md.push_str(&format!(" | **Talk Ratio (You):** {ratio}%"));
    }
    md.push_str("\n\n");
    for s in &m.speakers {
        md.push_str(&format!("### {}\n", s.speaker_label));
        md.push_str(&format!("- Pace: {} wpm\n", s.words_per_minute as i64));
        md.push_str(&format!("- Fillers: {:.1}/min", s.fillers_per_minute));
        if !s.fillers.is_empty() {
            let top = s
                .fillers
                .iter()
                .take(5)
                .map(|f| format!("{}: {}", f.word, f.count))
                .collect::<Vec<_>>()
                .join(", ");
            md.push_str(&format!(" ({top})"));
        }
        md.push('\n');
        md.push_str(&format!(
            "- Longest monologue: {} words\n",
            s.longest_monologue_words
        ));
        md.push_str(&format!("- Questions asked: {}\n", s.questions_asked));
        md.push_str(&format!(
            "- Clarity: {} words/turn\n\n",
            s.avg_words_per_turn as i64
        ));
    }
    md.trim().to_string()
}

fn provenance_name(source: &str) -> Option<&'static str> {
    if source.starts_with("granola") {
        Some("Granola")
    } else {
        None
    }
}

/// Port of `fullMeetingAsMarkdown`.
pub fn full_meeting_markdown(
    meeting: &Meeting,
    segments: &[TranscriptSegment],
    fillers: &[String],
) -> String {
    let start = meeting.started_at.unwrap_or(meeting.created_at);
    let date = chrono::DateTime::<chrono::Utc>::from_timestamp(start, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%-d %B %Y at %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let mut md = format!("# {}\n\n_{date}_\n\n", meeting.display_title());
    if let Some(p) = meeting.import_source.as_deref().and_then(provenance_name) {
        md.push_str(&format!("_Imported from {p}_\n\n"));
    }
    md.push_str("---\n\n");
    let notes = notes_as_markdown(meeting);
    if !notes.is_empty() {
        md.push_str(&notes);
        md.push_str("\n\n---\n\n");
    }
    md.push_str(&insights_as_markdown(meeting));
    let training = training_as_markdown(meeting, segments, fillers);
    if !training.is_empty() {
        md.push_str("\n\n---\n\n");
        md.push_str(&training);
    }
    md.push_str("\n\n---\n\n");
    md.push_str(&transcript_as_markdown(meeting, segments));
    md
}

/// Safe file name from a title.
pub fn export_file_name(meeting: &Meeting) -> String {
    let start = meeting.started_at.unwrap_or(meeting.created_at);
    let date = chrono::DateTime::<chrono::Utc>::from_timestamp(start, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default();
    let title: String = meeting
        .display_title()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    format!(
        "{date}-{}.md",
        if title.is_empty() {
            "meeting".into()
        } else {
            title
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_has_macos_section_order() {
        let mut m = Meeting::new("Roadmap sync", "en");
        m.started_at = Some(1_700_000_000);
        m.ended_at = Some(1_700_000_600);
        m.notes = "prep".into();
        m.summary = "We planned Q4.".into();
        m.action_items = r#"["Ship it"]"#.into();
        m.discussion_flow = r#"["Intro","Plan"]"#.into();
        m.suggested_questions =
            r#"[{"question":"Why now?","type":"deeper","context":"timing"}]"#.into();
        m.meddpicc = r#"{"champion":"Sam"}"#.into();
        m.speaker_names = r#"{"0":"Alex"}"#.into();
        let segs = vec![
            TranscriptSegment::new(&m.id, 1000, "um hello", 0.0, 1.0, "microphone"),
            TranscriptSegment::new(&m.id, 1000, "again", 1.0, 2.0, "microphone"),
            TranscriptSegment::new(&m.id, 0, "hi", 2.0, 3.0, "system"),
        ];
        let fillers: Vec<String> = coaching::default_fillers("en")
            .into_iter()
            .map(String::from)
            .collect();
        let md = full_meeting_markdown(&m, &segs, &fillers);
        assert!(md.starts_with("# Roadmap sync\n\n_"));
        let order = [
            "## Notes",
            "## Insights",
            "### Summary",
            "### Discussion Flow",
            "### Action Items",
            "### Suggested Questions",
            "### MEDDPICC",
            "## Coaching",
            "## Transcript",
        ];
        let mut last = 0;
        for h in order {
            let i = md.find(h).unwrap_or_else(|| panic!("missing {h}"));
            assert!(i > last, "{h} out of order");
            last = i;
        }
        assert!(md.contains("- [ ] Ship it"));
        assert!(md.contains("- **Why now?**\n  _timing_"));
        assert!(md.contains("**Champion:** Sam"));
        assert!(
            md.contains("**You:**\num hello again"),
            "consecutive turns merge"
        );
        assert!(md.contains("**Alex:**\nhi"));
        assert!(md.contains("**Talk Ratio (You):** 75%"));
    }

    #[test]
    fn empty_meeting_still_exports() {
        let m = Meeting::new("", "en");
        let md = full_meeting_markdown(&m, &[], &[]);
        assert!(md.starts_with("# Untitled meeting"));
        assert!(md.contains("## Transcript"));
        assert!(!md.contains("## Notes"));
        assert!(!md.contains("## Coaching"));
    }

    #[test]
    fn file_names_are_safe() {
        let mut m = Meeting::new("Q4: Plan / Review?", "en");
        m.started_at = Some(1_700_000_000);
        let f = export_file_name(&m);
        assert!(f.ends_with("-q4-plan-review.md"), "{f}");
        assert!(!f.contains('/') && !f.contains('?'));
    }
}
