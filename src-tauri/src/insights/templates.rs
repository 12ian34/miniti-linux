//! Insight templates (port of macOS `InsightTemplates.swift`).
//!
//! A template is a small fixed set of named sections the Templates specialist
//! view fills from the transcript. The definition travels with every request,
//! so the backend needs no catalog. Limits mirror the backend's
//! `validateTemplateDefinition`; keep them in sync.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const MAX_SECTIONS: usize = 8;
pub const MAX_POINTS_PER_SECTION: usize = 4;
pub const MAX_SECTION_VALUE_CHARS: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateSection {
    pub key: &'static str,
    pub title: &'static str,
    pub guidance: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InsightTemplate {
    pub id: &'static str,
    /// Full name shown in menus and section headers ("Interview scorecard").
    pub name: &'static str,
    /// Compact name for the tab strip ("Interview").
    pub short_name: &'static str,
    /// One line explaining what the template is for.
    pub summary: &'static str,
    pub sections: Vec<TemplateSection>,
}

const fn s(key: &'static str, title: &'static str, guidance: &'static str) -> TemplateSection {
    TemplateSection {
        key,
        title,
        guidance,
    }
}

pub fn builtin() -> Vec<InsightTemplate> {
    vec![
        InsightTemplate {
            id: "bant",
            name: "BANT qualification",
            short_name: "BANT",
            summary: "Budget, authority, need, and timeline",
            sections: vec![
                s("budget", "budget", "Money that has been mentioned, approved, or is missing: amounts, ranges, who controls it, and whether it exists yet."),
                s("authority", "authority", "Who decides, who signs, and who else must approve. Include names and roles when stated."),
                s("need", "need", "The problem in the customer's words, its impact, and what happens if nothing changes."),
                s("timeline", "timeline", "Dates, deadlines, and events driving the decision, plus anything that could slip it."),
                s("next_steps", "next steps", "What was agreed to happen next, by whom, and by when."),
            ],
        },
        InsightTemplate {
            id: "spin",
            name: "SPIN discovery",
            short_name: "SPIN",
            summary: "Situation, problem, implication, need-payoff",
            sections: vec![
                s("situation", "situation", "Facts about how things work today: team, tools, process, volumes."),
                s("problem", "problem", "Difficulties and dissatisfactions the customer stated with the current situation."),
                s("implication", "implication", "Consequences of those problems that were discussed: cost, time, risk, morale."),
                s("need_payoff", "need-payoff", "Value the customer said a solution would bring, in their own words."),
                s("next_steps", "next steps", "Agreed follow-ups with owners and dates."),
            ],
        },
        InsightTemplate {
            id: "interview",
            name: "Interview scorecard",
            short_name: "Interview",
            summary: "Evidence-based notes for a hiring interview",
            sections: vec![
                s("strengths", "strengths", "Skills and qualities the candidate demonstrated, each tied to something they actually said or described."),
                s("concerns", "concerns", "Gaps, risks, or unclear areas that came up. State the evidence, not a verdict."),
                s("examples", "examples", "Concrete stories, projects, or results the candidate cited, with outcomes where given."),
                s("open_questions", "open questions", "Things still unknown that a later interview should probe."),
                s("candidate_questions", "candidate questions", "What the candidate asked about the role, team, or company."),
                s("next_steps", "next steps", "Process commitments made to the candidate and internal follow-ups."),
            ],
        },
        InsightTemplate {
            id: "customer_check_in",
            name: "Customer check-in",
            short_name: "Check-in",
            summary: "Health, wins, risks, and asks from an account call",
            sections: vec![
                s("health", "health", "Signals about adoption, satisfaction, and sentiment that were expressed, positive or negative."),
                s("wins", "wins", "Outcomes and successes the customer reported."),
                s("risks", "risks", "Churn or expansion risks: blockers, frustrations, competitor mentions, budget or champion changes."),
                s("requests", "requests", "Feature requests, support asks, and questions the customer raised."),
                s("commitments", "commitments", "What each side promised to do."),
                s("next_steps", "next steps", "Agreed follow-ups with owners and dates."),
            ],
        },
        InsightTemplate {
            id: "standup",
            name: "Stand-up",
            short_name: "Stand-up",
            summary: "Done, next, blockers, and decisions",
            sections: vec![
                s("done", "done", "Work completed since the last stand-up, attributed to the person who reported it."),
                s("next", "next", "What each person plans to do next."),
                s("blockers", "blockers", "Anything stopping progress and who is needed to unblock it."),
                s("decisions", "decisions", "Decisions made during the stand-up."),
                s("follow_ups", "follow-ups", "Conversations to take offline, with the people involved."),
            ],
        },
        InsightTemplate {
            id: "one_on_one",
            name: "1:1",
            short_name: "1:1",
            summary: "Updates, wins, challenges, feedback, and growth",
            sections: vec![
                s("updates", "updates", "Status on ongoing work and priorities discussed."),
                s("wins", "wins", "Things that went well and were called out."),
                s("challenges", "challenges", "Difficulties, frustrations, or risks raised by either person."),
                s("feedback", "feedback", "Feedback given in either direction, as stated."),
                s("growth", "growth", "Career, learning, and development topics."),
                s("actions", "actions", "Commitments made, with owners and dates."),
            ],
        },
    ]
}

pub fn find(id: &str) -> Option<InsightTemplate> {
    builtin().into_iter().find(|t| t.id == id)
}

impl InsightTemplate {
    /// Wire shape for `POST /api/insights` (`template`).
    pub fn request_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "sections": self.sections.iter().map(|s| json!({ "key": s.key, "title": s.title, "guidance": s.guidance })).collect::<Vec<_>>(),
        })
    }

    /// Sections with a value, in template order.
    pub fn ordered<'a>(
        &'a self,
        values: &'a Map<String, Value>,
    ) -> Vec<(&'a TemplateSection, &'a str)> {
        self.sections
            .iter()
            .filter_map(|sec| {
                values
                    .get(sec.key)
                    .and_then(Value::as_str)
                    .filter(|v| has_value(v))
                    .map(|v| (sec, v))
            })
            .collect()
    }
}

/// A section counts as filled when it holds real text rather than a null-ish placeholder.
pub fn has_value(value: &str) -> bool {
    let t = value.trim().to_lowercase();
    !t.is_empty()
        && !matches!(
            t.trim_end_matches('.'),
            "null" | "n/a" | "none" | "not discussed" | "not mentioned"
        )
}

fn strip_bullet(line: &str) -> &str {
    let t = line.trim();
    for prefix in ["- ", "• ", "– ", "* "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return rest.trim();
        }
    }
    t
}

/// Keep only the template's keys (in order), strip bullet prefixes, cap points,
/// and represent empty sections as null (backend `normalizeTemplateSections`).
pub fn normalize_sections(raw: &Value, template: &InsightTemplate) -> Map<String, Value> {
    let source = raw.as_object();
    let mut out = Map::new();
    for sec in &template.sections {
        let cleaned = source
            .and_then(|m| m.get(sec.key))
            .and_then(Value::as_str)
            .map(|v| {
                v.lines()
                    .map(strip_bullet)
                    .filter(|l| !l.is_empty())
                    .take(MAX_POINTS_PER_SECTION)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|v| has_value(v))
            .map(|v| v.chars().take(MAX_SECTION_VALUE_CHARS).collect::<String>());
        out.insert(
            sec.key.to_string(),
            cleaned.map(Value::String).unwrap_or(Value::Null),
        );
    }
    out
}

/// Sections with at least one filled value.
pub fn filled(sections: &Map<String, Value>) -> bool {
    sections
        .values()
        .any(|v| v.as_str().map(has_value).unwrap_or(false))
}

/// Markdown block shared by exports and webhooks.
pub fn markdown(
    template: &InsightTemplate,
    sections: &Map<String, Value>,
    heading_level: usize,
) -> String {
    let entries = template.ordered(sections);
    if entries.is_empty() {
        return String::new();
    }
    let h = "#".repeat(heading_level.clamp(1, 6));
    let mut md = format!("{h} {}\n\n", template.name);
    for (sec, value) in entries {
        md.push_str(&format!("**{}:**\n", sec.title));
        for line in value.lines().filter(|l| !l.trim().is_empty()) {
            md.push_str(&format!("- {}\n", line.trim()));
        }
        md.push('\n');
    }
    md
}

/// Parse stored `template_sections` JSON.
pub fn parse_sections(json: &str) -> Map<String, Value> {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_definitions_respect_backend_limits() {
        let key_ok = |k: &str| {
            let mut c = k.chars();
            c.next().map(|f| f.is_ascii_lowercase()).unwrap_or(false)
                && k.len() <= 32
                && k.chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        };
        let all = builtin();
        assert_eq!(all.len(), 6);
        for t in &all {
            assert!(
                !t.sections.is_empty() && t.sections.len() <= MAX_SECTIONS,
                "{}",
                t.id
            );
            assert!(t.name.len() <= 60);
            let mut keys = std::collections::HashSet::new();
            for s in &t.sections {
                assert!(key_ok(s.key), "{}.{}", t.id, s.key);
                assert!(keys.insert(s.key), "duplicate key {}.{}", t.id, s.key);
                assert!(s.title.len() <= 60 && s.guidance.len() <= 300);
            }
        }
        assert_eq!(find("bant").unwrap().short_name, "BANT");
        assert!(find("nope").is_none());
    }

    #[test]
    fn normalize_keeps_order_strips_bullets_and_drops_placeholders() {
        let t = find("standup").unwrap();
        let raw = json!({
            "next": "- ship it\n• test it\nthird\nfourth\nfifth",
            "done": "null",
            "blockers": "  ",
            "unknown": "ignored",
            "decisions": "Use SQLite."
        });
        let n = normalize_sections(&raw, &t);
        // serde_json::Map is sorted; rendering order comes from the template, not the map.
        let mut keys: Vec<_> = n.keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["blockers", "decisions", "done", "follow_ups", "next"]
        );
        assert_eq!(n["next"], "ship it\ntest it\nthird\nfourth");
        assert!(n["done"].is_null() && n["blockers"].is_null() && n["follow_ups"].is_null());
        assert!(filled(&n));
        let md = markdown(&t, &n, 3);
        assert!(md.starts_with("### Stand-up\n\n**next:**\n- ship it\n"));
        assert!(md.contains("**decisions:**\n- Use SQLite.\n"));
        assert_eq!(markdown(&t, &Map::new(), 3), "");
        assert_eq!(t.request_json()["sections"].as_array().unwrap().len(), 5);
    }
}
