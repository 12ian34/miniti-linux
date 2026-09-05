//! Local coaching metrics (PLAN.md §7). Computed entirely on-device — never sent
//! to `/api/insights`. Ports the intent of Apple `TrainingMetrics`/`CoachingAdvisor`:
//! talk ratio, fillers, pace, monologue length, questions, clarity.

use serde::Serialize;

/// One contiguous speech turn used as coaching input.
#[derive(Debug, Clone)]
pub struct SpeechTurn {
    pub speaker: i64,
    pub text: String,
    pub start_s: f64,
    pub end_s: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoachingMetrics {
    /// Fraction of words spoken by "self" speakers, in [0, 1].
    pub talk_ratio: f64,
    pub self_words: usize,
    pub total_words: usize,
    pub filler_count: usize,
    /// Fillers per self word, in [0, 1].
    pub filler_rate: f64,
    /// Self speaking pace in words per minute.
    pub pace_wpm: f64,
    /// Longest uninterrupted self monologue, in seconds.
    pub longest_monologue_s: f64,
    pub questions_asked: usize,
    /// Blended 0–100 clarity score (higher is better).
    pub clarity: f64,
}

/// Default filler lists. Exact per-language lists should be ported from Apple
/// `InsightsService`/`TranscriptionLanguage` when implementing all 11 languages.
pub fn default_fillers(language: &str) -> Vec<&'static str> {
    match language {
        "es" => vec!["eh", "este", "o sea", "pues"],
        "fr" => vec!["euh", "ben", "genre", "quoi"],
        "de" => vec!["äh", "ähm", "halt", "also"],
        _ => vec!["um", "uh", "like", "you know", "sort of", "kind of", "basically", "actually"],
    }
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().filter(|w| !w.is_empty()).count()
}

fn count_fillers(text: &str, fillers: &[&str]) -> usize {
    let lower = text.to_lowercase();
    let mut count = 0;
    for f in fillers {
        if f.contains(' ') {
            // Multi-word filler: count non-overlapping phrase occurrences.
            let mut hay = lower.as_str();
            while let Some(idx) = hay.find(f) {
                count += 1;
                hay = &hay[idx + f.len()..];
            }
        } else {
            count += lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|tok| tok == f)
                .count();
        }
    }
    count
}

fn count_questions(text: &str) -> usize {
    text.split(['?'])
        .count()
        .saturating_sub(1)
        .max(0)
}

/// Compute coaching metrics for the given turns and set of "self" speaker ids.
pub fn compute(turns: &[SpeechTurn], self_speakers: &[i64], fillers: &[&str]) -> CoachingMetrics {
    let is_self = |s: i64| self_speakers.contains(&s);

    let mut self_words = 0usize;
    let mut total_words = 0usize;
    let mut filler_count = 0usize;
    let mut self_seconds = 0.0f64;
    let mut questions = 0usize;

    // Longest contiguous self monologue across adjacent self turns.
    let mut longest = 0.0f64;
    let mut current_start: Option<f64> = None;
    let mut current_end = 0.0f64;

    for turn in turns {
        let w = count_words(&turn.text);
        total_words += w;
        if is_self(turn.speaker) {
            self_words += w;
            self_seconds += (turn.end_s - turn.start_s).max(0.0);
            filler_count += count_fillers(&turn.text, fillers);
            questions += count_questions(&turn.text);
            match current_start {
                Some(_) => current_end = turn.end_s,
                None => {
                    current_start = Some(turn.start_s);
                    current_end = turn.end_s;
                }
            }
        } else if let Some(start) = current_start.take() {
            longest = longest.max(current_end - start);
        }
    }
    if let Some(start) = current_start {
        longest = longest.max(current_end - start);
    }

    let talk_ratio = if total_words == 0 {
        0.0
    } else {
        self_words as f64 / total_words as f64
    };
    let filler_rate = if self_words == 0 {
        0.0
    } else {
        filler_count as f64 / self_words as f64
    };
    let pace_wpm = if self_seconds <= 0.0 {
        0.0
    } else {
        self_words as f64 / (self_seconds / 60.0)
    };

    // Clarity: penalize high filler rate and extreme pace (ideal ~140 wpm).
    let filler_penalty = (filler_rate * 100.0).min(40.0);
    let pace_penalty = if pace_wpm == 0.0 {
        0.0
    } else {
        ((pace_wpm - 140.0).abs() / 10.0).min(40.0)
    };
    let clarity = (100.0 - filler_penalty - pace_penalty).clamp(0.0, 100.0);

    CoachingMetrics {
        talk_ratio,
        self_words,
        total_words,
        filler_count,
        filler_rate,
        pace_wpm,
        longest_monologue_s: longest,
        questions_asked: questions,
        clarity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(sp: i64, text: &str, start: f64, end: f64) -> SpeechTurn {
        SpeechTurn {
            speaker: sp,
            text: text.into(),
            start_s: start,
            end_s: end,
        }
    }

    #[test]
    fn talk_ratio_and_word_counts() {
        let turns = vec![
            turn(1000, "hello there team", 0.0, 2.0), // self: 3 words
            turn(1, "hi", 2.0, 3.0),                  // other: 1 word
        ];
        let m = compute(&turns, &[1000], &default_fillers("en"));
        assert_eq!(m.self_words, 3);
        assert_eq!(m.total_words, 4);
        assert!((m.talk_ratio - 0.75).abs() < 1e-9);
    }

    #[test]
    fn counts_fillers_single_and_multiword() {
        let turns = vec![turn(1000, "um so like you know we ship", 0.0, 4.0)];
        let m = compute(&turns, &[1000], &default_fillers("en"));
        // "um", "like", "you know" = 3
        assert_eq!(m.filler_count, 3);
    }

    #[test]
    fn longest_monologue_spans_adjacent_self_turns() {
        let turns = vec![
            turn(1000, "a b", 0.0, 5.0),
            turn(1000, "c d", 5.0, 12.0), // contiguous self: 0..12 = 12s
            turn(1, "ok", 12.0, 13.0),
            turn(1000, "e", 13.0, 15.0), // 2s
        ];
        let m = compute(&turns, &[1000], &default_fillers("en"));
        assert!((m.longest_monologue_s - 12.0).abs() < 1e-9);
    }

    #[test]
    fn pace_and_questions() {
        // 140 words over 60s -> 140 wpm, high clarity.
        let text = "word ".repeat(140);
        let turns = vec![turn(1000, text.trim(), 0.0, 60.0)];
        let m = compute(&turns, &[1000], &default_fillers("en"));
        assert!((m.pace_wpm - 140.0).abs() < 1e-6);
        assert!(m.clarity > 95.0);

        let q = vec![turn(1000, "does this work? really? yes", 0.0, 3.0)];
        let mq = compute(&q, &[1000], &default_fillers("en"));
        assert_eq!(mq.questions_asked, 2);
    }

    #[test]
    fn empty_input_is_zeroed_not_nan() {
        let m = compute(&[], &[1000], &default_fillers("en"));
        assert_eq!(m.talk_ratio, 0.0);
        assert_eq!(m.pace_wpm, 0.0);
        assert_eq!(m.clarity, 100.0);
    }
}
