//! Local coaching metrics — a port of the Apple `TrainingMetrics` +
//! `CoachingAdvisor` pair (`../miniti/Miniti/Services/InsightsService.swift`).
//! Computed entirely on-device; never sent to `/api/insights`.
//!
//! Metric taxonomy (must stay consistent with macOS): fillers (per minute),
//! pace (words per minute), clarity (average words per turn), questions
//! (per 30 minutes), talk ratio (share of words that are yours), monologue
//! (longest uninterrupted run in words).

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::deepgram::{is_mic_app_speaker_id, MIC_SPEAKER_ID};

// ---- Default filler vocabularies (verbatim from `TranscriptionLanguage`) -----

/// Per-language default fillers. English hesitation entries match Deepgram's
/// filler vocabulary verbatim (uh, um, mhmm, uh huh, …) — it never emits
/// "hmm", "hm" or "er".
pub fn default_fillers(language: &str) -> Vec<&'static str> {
    match language {
        "es" => vec![
            "eh", "este", "bueno", "o sea", "pues", "es que", "digamos", "entonces", "a ver",
        ],
        "sv" => vec![
            "eh", "öh", "liksom", "typ", "alltså", "asså", "va", "ju", "ba",
        ],
        "el" => vec![
            "ε",
            "εε",
            "δηλαδή",
            "κοίτα",
            "λοιπόν",
            "ας πούμε",
            "τέλος πάντων",
        ],
        "fr" => vec![
            "euh", "ben", "genre", "en fait", "du coup", "voilà", "quoi", "bah", "bon",
        ],
        "de" => vec![
            "äh",
            "ähm",
            "halt",
            "also",
            "sozusagen",
            "quasi",
            "irgendwie",
            "na ja",
            "genau",
        ],
        "pt" => vec![
            "é",
            "né",
            "tipo",
            "assim",
            "então",
            "bom",
            "quer dizer",
            "enfim",
        ],
        "it" => vec![
            "ehm",
            "cioè",
            "tipo",
            "allora",
            "praticamente",
            "insomma",
            "diciamo",
            "boh",
        ],
        "nl" => vec![
            "eh",
            "uhm",
            "eigenlijk",
            "zeg maar",
            "weet je",
            "dus",
            "nou",
            "gewoon",
        ],
        "pl" => vec![
            "ee",
            "no",
            "w sumie",
            "jakby",
            "znaczy",
            "generalnie",
            "w zasadzie",
            "tak naprawdę",
        ],
        "ru" => vec![
            "эм",
            "ну",
            "вот",
            "типа",
            "короче",
            "как бы",
            "в общем",
            "значит",
            "так сказать",
        ],
        _ => vec![
            "um",
            "uh",
            "mhmm",
            "mhm",
            "ah",
            "like",
            "basically",
            "literally",
            "actually",
            "honestly",
            "uh huh",
            "you know",
            "i mean",
            "kind of",
            "sort of",
        ],
    }
}

/// Effective filler list: user overrides (normalized) or the language default.
pub fn effective_fillers(language: &str, overrides: &[String]) -> Vec<String> {
    let normalized = normalize_fillers(overrides);
    if normalized.is_empty() {
        default_fillers(language)
            .into_iter()
            .map(String::from)
            .collect()
    } else {
        normalized
    }
}

/// Lowercase, trim, collapse whitespace, dedupe (port of `normalizedFillers`).
pub fn normalize_fillers(raw: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for f in raw {
        let n = f
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !n.is_empty() && seen.insert(n.clone()) {
            out.push(n);
        }
    }
    out
}

// ---- Speaker labels (port of `resolvedSpeakerLabel`) -----------------------

/// Resolve a display label. `self_ids == None` means "self context unknown →
/// legacy mic default (1000 is You)"; an empty set means several people share
/// the mic and nobody is assumed to be the user.
pub fn resolved_speaker_label(
    speaker: i64,
    names: Option<&HashMap<String, String>>,
    self_ids: Option<&[i64]>,
) -> String {
    if let Some(ids) = self_ids {
        if ids.contains(&speaker) {
            return "You".into();
        }
    }
    if let Some(mapped) = names
        .and_then(|n| n.get(&speaker.to_string()))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return mapped.to_string();
    }
    if self_ids.is_none() && speaker == MIC_SPEAKER_ID {
        return "You".into();
    }
    if is_mic_app_speaker_id(speaker) {
        return format!("Speaker {} (mic)", speaker - MIC_SPEAKER_ID + 1);
    }
    format!("Speaker {}", speaker + 1)
}

// ---- TrainingMetrics ---------------------------------------------------------

/// A transcript segment as coaching input.
#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub speaker: i64,
    pub is_final: bool,
    /// Seconds from meeting start.
    pub timestamp: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FillerEntry {
    pub word: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpeakerStats {
    pub speaker_label: String,
    pub is_local_mic: bool,
    pub word_count: usize,
    pub segment_count: usize,
    pub fillers: Vec<FillerEntry>,
    pub total_fillers: usize,
    pub fillers_per_minute: f64,
    pub words_per_minute: f64,
    pub longest_monologue_words: usize,
    pub questions_asked: usize,
    pub avg_words_per_turn: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrainingMetrics {
    pub speakers: Vec<SpeakerStats>,
    /// Share of all words spoken by "You", in [0, 1].
    pub talk_ratio_you: f64,
    pub duration_minutes: f64,
}

/// Lowercase, strip everything but letters/numbers/whitespace/apostrophes, split.
pub fn tokenize(text: &str) -> Vec<String> {
    let cleaned: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().map(String::from).collect()
}

/// Count (possibly overlapping) occurrences of `phrase` in `tokens`.
pub fn count_phrase_occurrences(phrase: &[String], tokens: &[String]) -> usize {
    if phrase.is_empty() || tokens.len() < phrase.len() {
        return 0;
    }
    (0..=tokens.len() - phrase.len())
        .filter(|&i| tokens[i..i + phrase.len()] == phrase[..])
        .count()
}

/// Longest run of words by any speaker in `speakers` without interruption.
pub fn longest_monologue_words(speakers: &[i64], finals: &[Segment]) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for seg in finals {
        if speakers.contains(&seg.speaker) {
            current += tokenize(&seg.text).len();
        } else {
            longest = longest.max(current);
            current = 0;
        }
    }
    longest.max(current)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Group {
    SelfSpeaker,
    Other(i64),
}

/// Compute coaching metrics (port of `TrainingMetrics.compute`).
///
/// - `duration_s`: reported meeting duration; effective duration is capped at
///   the last spoken timestamp so trailing silence does not dilute rates.
/// - `self_ids`: see [`resolved_speaker_label`]. `None` ⇒ `[1000]`.
pub fn compute(
    segments: &[Segment],
    duration_s: f64,
    fillers: &[String],
    names: Option<&HashMap<String, String>>,
    self_ids: Option<&[i64]>,
) -> TrainingMetrics {
    let legacy_self = [MIC_SPEAKER_ID];
    let effective_self: &[i64] = self_ids.unwrap_or(&legacy_self);

    let mut finals: Vec<(usize, &Segment)> = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_final && !s.text.trim().is_empty())
        .collect();
    finals.sort_by(|a, b| {
        a.1.timestamp
            .partial_cmp(&b.1.timestamp)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let finals: Vec<Segment> = finals.into_iter().map(|(_, s)| s.clone()).collect();

    let reported = duration_s.max(0.0);
    let last_spoken = finals.last().map(|s| s.timestamp).unwrap_or(0.0);
    let effective_seconds = if last_spoken > 0.0 {
        if reported > 0.0 {
            reported.min(last_spoken)
        } else {
            last_spoken
        }
    } else {
        reported
    };
    let duration_minutes = (effective_seconds / 60.0).max(0.01);

    let filler_phrases: Vec<(Vec<String>, String)> = fillers
        .iter()
        .map(|p| (tokenize(p), p.clone()))
        .filter(|(t, _)| !t.is_empty())
        .collect();

    let mut grouped: BTreeMap<Group, Vec<&Segment>> = BTreeMap::new();
    for seg in &finals {
        let key = if effective_self.contains(&seg.speaker) {
            Group::SelfSpeaker
        } else {
            Group::Other(seg.speaker)
        };
        grouped.entry(key).or_default().push(seg);
    }

    let mut total_words_all = 0usize;
    let mut you_word_count = 0usize;
    let mut speakers = Vec::new();

    for (group, segs) in &grouped {
        let is_local_mic = *group == Group::SelfSpeaker;
        let label = match group {
            Group::SelfSpeaker => "You".to_string(),
            Group::Other(id) => resolved_speaker_label(*id, names, Some(effective_self)),
        };

        let mut word_count = 0usize;
        let mut filler_map: HashMap<String, usize> = HashMap::new();
        let mut questions = 0usize;
        for seg in segs {
            let tokens = tokenize(&seg.text);
            word_count += tokens.len();
            questions += seg.text.chars().filter(|c| *c == '?').count();
            for (phrase, label) in &filler_phrases {
                let n = count_phrase_occurrences(phrase, &tokens);
                if n > 0 {
                    *filler_map.entry(label.clone()).or_insert(0) += n;
                }
            }
        }
        total_words_all += word_count;
        if is_local_mic {
            you_word_count = word_count;
        }

        let total_fillers: usize = filler_map.values().sum();
        let mut filler_entries: Vec<FillerEntry> = filler_map
            .into_iter()
            .map(|(word, count)| FillerEntry { word, count })
            .collect();
        filler_entries.sort_by(|a, b| b.count.cmp(&a.count).then(a.word.cmp(&b.word)));

        let monologue_ids: Vec<i64> = match group {
            Group::SelfSpeaker => effective_self.to_vec(),
            Group::Other(id) => vec![*id],
        };

        speakers.push(SpeakerStats {
            speaker_label: label,
            is_local_mic,
            word_count,
            segment_count: segs.len(),
            fillers: filler_entries,
            total_fillers,
            fillers_per_minute: total_fillers as f64 / duration_minutes,
            words_per_minute: word_count as f64 / duration_minutes,
            longest_monologue_words: longest_monologue_words(&monologue_ids, &finals),
            questions_asked: questions,
            avg_words_per_turn: if segs.is_empty() {
                0.0
            } else {
                word_count as f64 / segs.len() as f64
            },
        });
    }

    TrainingMetrics {
        speakers,
        talk_ratio_you: if total_words_all > 0 {
            you_word_count as f64 / total_words_all as f64
        } else {
            0.0
        },
        duration_minutes,
    }
}

// ---- CoachingSnapshot / CoachingAdvisor ------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CoachingMetric {
    Fillers,
    Pace,
    Clarity,
    Questions,
    TalkRatio,
    Monologue,
}

impl CoachingMetric {
    pub const ALL: [CoachingMetric; 6] = [
        CoachingMetric::Fillers,
        CoachingMetric::Pace,
        CoachingMetric::Clarity,
        CoachingMetric::Questions,
        CoachingMetric::TalkRatio,
        CoachingMetric::Monologue,
    ];
}

/// A comparable, meeting-level view of the user's metrics. Questions are
/// normalized to a 30-minute meeting.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoachingSnapshot {
    pub meeting_id: String,
    pub meeting_title: String,
    /// Unix seconds.
    pub date: i64,
    pub fillers_per_minute: f64,
    pub words_per_minute: f64,
    pub avg_words_per_turn: f64,
    pub questions_per_30_minutes: f64,
    pub talk_ratio: Option<f64>,
    pub longest_monologue_words: f64,
    pub top_filler: Option<String>,
}

impl CoachingSnapshot {
    /// Build from the "You" bucket of a meeting's metrics; `None` when the user
    /// did not speak (no self bucket).
    pub fn from_metrics(
        meeting_id: &str,
        meeting_title: &str,
        date: i64,
        metrics: &TrainingMetrics,
    ) -> Option<Self> {
        let you = metrics.speakers.iter().find(|s| s.is_local_mic)?;
        let minutes = metrics.duration_minutes.max(0.01);
        Some(Self {
            meeting_id: meeting_id.to_string(),
            meeting_title: meeting_title.to_string(),
            date,
            fillers_per_minute: you.fillers_per_minute,
            words_per_minute: you.words_per_minute,
            avg_words_per_turn: you.avg_words_per_turn,
            questions_per_30_minutes: you.questions_asked as f64 / minutes * 30.0,
            talk_ratio: if metrics.speakers.len() > 1 {
                Some(metrics.talk_ratio_you)
            } else {
                None
            },
            longest_monologue_words: you.longest_monologue_words as f64,
            top_filler: you.fillers.first().map(|f| f.word.clone()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachingTrend {
    Improving,
    Steady,
    NeedsAttention,
    BuildingBaseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachingMetricStatus {
    Strong,
    Balanced,
    Focus,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoachingMetricSummary {
    pub metric: CoachingMetric,
    pub status: CoachingMetricStatus,
    pub trend: CoachingTrend,
    pub recent_value: String,
    pub previous_value: Option<String>,
    pub headline: String,
    pub observation: String,
    pub tip: String,
    #[serde(skip)]
    priority: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoachingReport {
    pub meeting_count: usize,
    pub focus: CoachingMetricSummary,
    pub strengths: Vec<CoachingMetricSummary>,
    pub summaries: Vec<CoachingMetricSummary>,
}

fn metric_value(metric: CoachingMetric, s: &CoachingSnapshot) -> Option<f64> {
    match metric {
        CoachingMetric::Fillers => Some(s.fillers_per_minute),
        CoachingMetric::Pace => Some(s.words_per_minute),
        CoachingMetric::Clarity => Some(s.avg_words_per_turn),
        CoachingMetric::Questions => Some(s.questions_per_30_minutes),
        CoachingMetric::TalkRatio => s.talk_ratio,
        CoachingMetric::Monologue => Some(s.longest_monologue_words),
    }
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// Distance from a broad conversational range (prompts for reflection, not
/// quality scores).
pub fn severity(metric: CoachingMetric, value: f64) -> f64 {
    match metric {
        CoachingMetric::Fillers => ((value - 3.0) / 4.0).max(0.0),
        CoachingMetric::Pace => {
            if value < 110.0 {
                (110.0 - value) / 55.0
            } else if value > 180.0 {
                (value - 180.0) / 55.0
            } else {
                0.0
            }
        }
        CoachingMetric::Clarity => {
            if value < 5.0 {
                (5.0 - value) / 5.0
            } else if value > 20.0 {
                (value - 20.0) / 15.0
            } else {
                0.0
            }
        }
        CoachingMetric::Questions => ((3.0 - value) / 3.0).max(0.0),
        CoachingMetric::TalkRatio => {
            if value < 0.35 {
                (0.35 - value) / 0.25
            } else if value > 0.65 {
                (value - 0.65) / 0.25
            } else {
                0.0
            }
        }
        CoachingMetric::Monologue => ((value - 150.0) / 180.0).max(0.0),
    }
}

fn status_for(severity: f64) -> CoachingMetricStatus {
    if severity <= 0.1 {
        CoachingMetricStatus::Strong
    } else if severity < 0.75 {
        CoachingMetricStatus::Balanced
    } else {
        CoachingMetricStatus::Focus
    }
}

fn trend_for(metric: CoachingMetric, recent: f64, previous: Option<f64>) -> CoachingTrend {
    let Some(previous) = previous else {
        return CoachingTrend::BuildingBaseline;
    };
    let r = severity(metric, recent);
    let p = severity(metric, previous);
    if r < p - 0.12 {
        CoachingTrend::Improving
    } else if r > p + 0.12 {
        CoachingTrend::NeedsAttention
    } else {
        CoachingTrend::Steady
    }
}

pub fn formatted_value(metric: CoachingMetric, value: f64) -> String {
    match metric {
        CoachingMetric::Fillers => format!("{value:.1} / min"),
        CoachingMetric::Pace => format!("{} wpm", value.round() as i64),
        CoachingMetric::Clarity => format!("{value:.1} words / turn"),
        CoachingMetric::Questions => format!("{value:.1} / 30 min"),
        CoachingMetric::TalkRatio => format!("{}% you", (value * 100.0).round() as i64),
        CoachingMetric::Monologue => format!("{} words", value.round() as i64),
    }
}

fn copy_for(metric: CoachingMetric, value: f64) -> (&'static str, &'static str, &'static str) {
    match metric {
        CoachingMetric::Fillers => {
            if value <= 3.0 {
                ("Your pauses are working",
                 "Filler use is low enough that your ideas can carry the emphasis.",
                 "Keep using a quiet beat before an important answer instead of rushing to fill it.")
            } else {
                ("Make pauses do the work",
                 "Fillers are softening otherwise clear delivery.",
                 "Choose one filler to notice next meeting. When it arrives, replace only that word with one silent breath.")
            }
        }
        CoachingMetric::Pace => {
            if value > 180.0 {
                ("Give ideas room to land",
                 "Your recent pace is energetic, but listeners may have less time to absorb each point.",
                 "After each key sentence, pause for one full beat. Aim to slow the important 20%, not the whole meeting.")
            } else if value < 110.0 {
                ("Lead with the point",
                 "Your recent pace is deliberate and may occasionally lose momentum.",
                 "Start answers with the conclusion in one sentence, then add the context that earns it.")
            } else {
                ("Your pace is easy to follow",
                 "You are sitting in a broadly conversational range.",
                 "Keep varying pace on purpose: slower for decisions, slightly quicker for familiar context.")
            }
        }
        CoachingMetric::Clarity => {
            if value > 20.0 {
                ("Shorten the next answer",
                 "Your turns are becoming dense, which can hide the main point.",
                 "Use a one-point-per-turn rule: make the point, give one example, then hand the conversation back.")
            } else if value < 5.0 {
                ("Connect the short answers",
                 "Your turns are very brief and may sometimes sound fragmented.",
                 "Add one sentence of reasoning after a short answer so the listener gets both the decision and why.")
            } else {
                (
                    "Your turns are concise",
                    "Your average answer length is in a clear conversational range.",
                    "Protect that clarity by stating the point before the supporting detail.",
                )
            }
        }
        CoachingMetric::Questions => {
            if value < 3.0 {
                ("Invite one level deeper",
                 "You are asking relatively few questions; that can be fine for presentations but limits discovery in conversations.",
                 "Prepare one follow-up prompt: “What makes that important now?” Use it once when the other person raises a priority.")
            } else {
                ("You are creating curiosity",
                 "Your recent meetings include a healthy cadence of questions.",
                 "Keep improving question quality: follow one factual answer with a why, impact, or trade-off question.")
            }
        }
        CoachingMetric::TalkRatio => {
            if value > 0.65 {
                ("Create more room",
                 "You have been carrying most of the conversation. That may fit a demo, but it can limit discovery.",
                 "After your next explanation, ask “What stands out to you?” and wait through the first quiet beat.")
            } else if value < 0.35 {
                ("Claim a little more space",
                 "You are listening generously, though your own point of view may be getting less airtime.",
                 "Before the meeting, write down the one perspective only you can add and make sure you state it clearly.")
            } else {
                ("The conversation has room to breathe",
                 "Your recent talk ratio is broadly balanced for a two-way discussion.",
                 "Keep checking the format: discovery should leave more room; a demo can reasonably ask more of your voice.")
            }
        }
        CoachingMetric::Monologue => {
            if value > 150.0 {
                ("Turn explanations into dialogue",
                 "Your longest stretches are doing a lot of work before anyone else enters.",
                 "Break long explanations into two-minute chapters and add a quick check-in between them.")
            } else {
                ("You are handing the conversation back",
                 "Your longest speaking stretches stay compact enough for regular participation.",
                 "Keep ending explanations with a real hand-off, not a rhetorical “does that make sense?”")
            }
        }
    }
}

/// Port of `CoachingAdvisor.analyze`: recent vs previous window per metric,
/// one focus, up to two strengths.
pub fn analyze(snapshots: &[CoachingSnapshot]) -> Option<CoachingReport> {
    let mut sorted: Vec<&CoachingSnapshot> = snapshots.iter().collect();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.date));
    if sorted.is_empty() {
        return None;
    }

    let recent_talk_ratio = average(
        &sorted
            .iter()
            .take(3)
            .filter_map(|s| s.talk_ratio)
            .collect::<Vec<_>>(),
    );

    let mut summaries = Vec::new();
    for metric in CoachingMetric::ALL {
        let values: Vec<f64> = sorted
            .iter()
            .filter_map(|s| metric_value(metric, s))
            .collect();
        if values.is_empty() {
            continue;
        }
        let comparison_window = if values.len() >= 4 {
            Some(3.min(values.len() / 2))
        } else {
            None
        };
        let recent_window = comparison_window.unwrap_or(3.min(values.len()));
        let recent = average(&values[..recent_window]).unwrap_or(0.0);
        let previous = comparison_window.and_then(|w| {
            let slice: Vec<f64> = values.iter().skip(w).take(w).copied().collect();
            average(&slice)
        });
        let sev = severity(metric, recent);
        let trend = trend_for(metric, recent, previous);
        let mut priority = sev;
        if trend == CoachingTrend::NeedsAttention {
            priority += 0.3;
        }
        if trend == CoachingTrend::Improving {
            priority -= 0.1;
        }
        if metric == CoachingMetric::Questions && recent_talk_ratio.unwrap_or(0.0) < 0.65 {
            priority = priority.min(0.65);
        }
        let (headline, observation, tip) = copy_for(metric, recent);
        summaries.push(CoachingMetricSummary {
            metric,
            status: status_for(sev),
            trend: if previous.is_none() {
                CoachingTrend::BuildingBaseline
            } else {
                trend
            },
            recent_value: formatted_value(metric, recent),
            previous_value: previous.map(|p| formatted_value(metric, p)),
            headline: headline.into(),
            observation: observation.into(),
            tip: tip.into(),
            priority,
        });
    }

    let focus = summaries
        .iter()
        .max_by(|a, b| {
            a.priority
                .partial_cmp(&b.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?
        .clone();
    let mut strengths: Vec<CoachingMetricSummary> = summaries
        .iter()
        .filter(|s| {
            s.metric != focus.metric
                && (s.status == CoachingMetricStatus::Strong || s.trend == CoachingTrend::Improving)
        })
        .cloned()
        .collect();
    strengths.sort_by(|a, b| {
        if a.status != b.status {
            return if a.status == CoachingMetricStatus::Strong {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        a.priority
            .partial_cmp(&b.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    strengths.truncate(2);

    Some(CoachingReport {
        meeting_count: sorted.len(),
        focus,
        strengths,
        summaries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(speaker: i64, text: &str, ts: f64) -> Segment {
        Segment {
            text: text.into(),
            speaker,
            is_final: true,
            timestamp: ts,
        }
    }

    fn en() -> Vec<String> {
        default_fillers("en")
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn english_fillers_match_deepgram_vocabulary() {
        let f = default_fillers("en");
        for must in ["um", "uh", "mhmm", "uh huh", "you know"] {
            assert!(f.contains(&must), "missing {must}");
        }
        for never in ["hmm", "hm", "er"] {
            assert!(!f.contains(&never), "{never} never emitted by Deepgram");
        }
        assert_eq!(
            default_fillers("xx"),
            default_fillers("en"),
            "unknown lang falls back"
        );
        assert!(!default_fillers("de").is_empty());
    }

    #[test]
    fn overrides_replace_defaults_when_present() {
        let eff = effective_fillers("en", &["  Like ".into(), "like".into(), "".into()]);
        assert_eq!(eff, vec!["like"]);
        assert_eq!(
            effective_fillers("en", &[]).len(),
            default_fillers("en").len()
        );
    }

    #[test]
    fn tokenize_strips_punctuation_but_keeps_apostrophes() {
        assert_eq!(
            tokenize("Hello, there! It's me."),
            vec!["hello", "there", "it's", "me"]
        );
    }

    #[test]
    fn phrase_counting_handles_multiword_and_overlap() {
        let t = tokenize("you know, you know what I mean");
        assert_eq!(count_phrase_occurrences(&tokenize("you know"), &t), 2);
        assert_eq!(count_phrase_occurrences(&tokenize("i mean"), &t), 1);
        assert_eq!(count_phrase_occurrences(&tokenize("nope"), &t), 0);
        // Overlap is counted independently, as in the Swift implementation.
        let t2 = tokenize("uh huh");
        assert_eq!(count_phrase_occurrences(&tokenize("uh"), &t2), 1);
        assert_eq!(count_phrase_occurrences(&tokenize("uh huh"), &t2), 1);
    }

    #[test]
    fn speaker_labels_follow_macos_contract() {
        assert_eq!(resolved_speaker_label(1000, None, None), "You");
        assert_eq!(resolved_speaker_label(1001, None, None), "Speaker 2 (mic)");
        assert_eq!(resolved_speaker_label(0, None, None), "Speaker 1");
        let mut names = HashMap::new();
        names.insert("0".to_string(), "Alex".to_string());
        assert_eq!(resolved_speaker_label(0, Some(&names), None), "Alex");
        // Explicit self ids win over names; empty self set removes implicit You.
        assert_eq!(resolved_speaker_label(0, Some(&names), Some(&[0])), "You");
        assert_eq!(
            resolved_speaker_label(1000, None, Some(&[])),
            "Speaker 1 (mic)"
        );
    }

    #[test]
    fn compute_groups_self_and_others_with_per_minute_rates() {
        let segs = vec![
            seg(1000, "um so we should ship this you know", 0.0),
            seg(0, "agreed", 10.0),
            seg(1000, "great", 60.0),
        ];
        let m = compute(&segs, 120.0, &en(), None, None);
        // Effective duration capped at last spoken timestamp (60s = 1 min).
        assert!((m.duration_minutes - 1.0).abs() < 1e-9);
        assert_eq!(m.speakers.len(), 2);
        let you = &m.speakers[0];
        assert!(you.is_local_mic);
        assert_eq!(you.speaker_label, "You");
        assert_eq!(you.word_count, 9);
        assert_eq!(you.total_fillers, 2, "um + you know");
        assert!((you.fillers_per_minute - 2.0).abs() < 1e-9);
        assert!((you.words_per_minute - 9.0).abs() < 1e-9);
        assert_eq!(you.segment_count, 2);
        assert!((you.avg_words_per_turn - 4.5).abs() < 1e-9);
        assert_eq!(you.fillers[0].word, "um"); // sorted by count then word
        let other = &m.speakers[1];
        assert_eq!(other.speaker_label, "Speaker 1");
        assert!((m.talk_ratio_you - 0.9).abs() < 1e-9);
    }

    #[test]
    fn longest_monologue_counts_words_across_adjacent_self_turns() {
        let segs = vec![
            seg(1000, "a b c", 0.0),
            seg(1000, "d e", 1.0),
            seg(0, "x", 2.0),
            seg(1000, "f", 3.0),
        ];
        let m = compute(&segs, 10.0, &en(), None, None);
        assert_eq!(m.speakers[0].longest_monologue_words, 5);
    }

    #[test]
    fn split_diarization_ids_merge_into_one_self_bucket() {
        let segs = vec![
            seg(1000, "one two", 0.0),
            seg(1001, "three", 1.0),
            seg(0, "x", 2.0),
        ];
        let m = compute(&segs, 10.0, &en(), None, Some(&[1000, 1001]));
        assert_eq!(m.speakers.len(), 2);
        assert_eq!(m.speakers[0].word_count, 3);
        assert_eq!(m.speakers[0].longest_monologue_words, 3);
    }

    #[test]
    fn interims_and_empty_segments_are_ignored() {
        let segs = vec![
            Segment {
                text: "interim".into(),
                speaker: 1000,
                is_final: false,
                timestamp: 0.0,
            },
            seg(1000, "   ", 1.0),
        ];
        let m = compute(&segs, 10.0, &en(), None, None);
        assert!(m.speakers.is_empty());
        assert_eq!(m.talk_ratio_you, 0.0);
    }

    #[test]
    fn questions_are_counted_per_question_mark() {
        let segs = vec![seg(1000, "does this work? really? yes", 0.0)];
        let m = compute(&segs, 60.0, &en(), None, None);
        assert_eq!(m.speakers[0].questions_asked, 2);
    }

    #[test]
    fn snapshot_normalizes_questions_to_30_minutes() {
        let segs = vec![seg(1000, "why? how?", 0.0), seg(0, "ok", 600.0)];
        let m = compute(&segs, 600.0, &en(), None, None);
        let snap = CoachingSnapshot::from_metrics("m", "T", 0, &m).unwrap();
        assert!((snap.questions_per_30_minutes - 6.0).abs() < 1e-9);
        assert_eq!(snap.talk_ratio, Some(m.talk_ratio_you));
    }

    #[test]
    fn severity_ranges_match_macos() {
        assert_eq!(severity(CoachingMetric::Fillers, 3.0), 0.0);
        assert!((severity(CoachingMetric::Fillers, 7.0) - 1.0).abs() < 1e-9);
        assert_eq!(severity(CoachingMetric::Pace, 140.0), 0.0);
        assert!(severity(CoachingMetric::Pace, 55.0) > 0.99);
        assert_eq!(severity(CoachingMetric::TalkRatio, 0.5), 0.0);
        assert!(severity(CoachingMetric::Monologue, 330.0) > 0.99);
        assert_eq!(formatted_value(CoachingMetric::TalkRatio, 0.5), "50% you");
        assert_eq!(formatted_value(CoachingMetric::Pace, 139.6), "140 wpm");
    }

    #[test]
    fn analyze_picks_highest_priority_focus_and_strengths() {
        let snap = |date: i64, fillers: f64, wpm: f64| CoachingSnapshot {
            meeting_id: "m".into(),
            meeting_title: "t".into(),
            date,
            fillers_per_minute: fillers,
            words_per_minute: wpm,
            avg_words_per_turn: 10.0,
            questions_per_30_minutes: 5.0,
            talk_ratio: Some(0.5),
            longest_monologue_words: 40.0,
            top_filler: Some("um".into()),
        };
        assert!(analyze(&[]).is_none());

        let report = analyze(&[
            snap(3, 9.0, 140.0),
            snap(2, 8.0, 140.0),
            snap(1, 8.0, 140.0),
        ])
        .unwrap();
        assert_eq!(report.meeting_count, 3);
        assert_eq!(report.focus.metric, CoachingMetric::Fillers);
        assert_eq!(report.focus.status, CoachingMetricStatus::Focus);
        assert_eq!(
            report.focus.trend,
            CoachingTrend::BuildingBaseline,
            "< 4 meetings"
        );
        assert!(report.strengths.len() <= 2);
        assert!(report
            .strengths
            .iter()
            .all(|s| s.status == CoachingMetricStatus::Strong));

        // With 4+ meetings a previous window exists and trends are computed.
        let report = analyze(&[
            snap(4, 2.0, 140.0),
            snap(3, 2.0, 140.0),
            snap(2, 8.0, 140.0),
            snap(1, 8.0, 140.0),
        ])
        .unwrap();
        let fillers = report
            .summaries
            .iter()
            .find(|s| s.metric == CoachingMetric::Fillers)
            .unwrap();
        assert_eq!(fillers.trend, CoachingTrend::Improving);
        assert!(fillers.previous_value.is_some());
    }
}
