//! Dictionary corrections: "heard" → "correct" pairs the user adds when the
//! transcript gets a name or a product wrong (Apple 2.7.0 parity, issue #3).
//!
//! Three consumers, all fed from the same list in prefs:
//! - Deepgram: `replace=heard:correct` query items after the keyterms, fixed
//!   for the life of a socket (Nova-3 cannot change them mid-stream);
//! - the local corrector: word-boundary, case-insensitive rewrite of every
//!   final and interim as it arrives, so a correction takes effect at once
//!   without a reconnect;
//! - a retroactive pass over the current meeting's saved segments when the
//!   user asks to fix earlier mentions. Never clears insights.

use regex::{NoExpand, Regex};
use serde::{Deserialize, Serialize};

pub const CAP: usize = 100;
/// A selection longer than this is not a dictionary term.
pub const MAX_HEARD_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correction {
    pub heard: String,
    pub correct: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    Added,
    Replaced,
    Invalid(&'static str),
    Full,
}

/// Collapse whitespace, trim, drop colons (Deepgram splits `replace=` on the
/// colon), and lowercase the heard side (Deepgram lowercases the find term).
pub fn normalize_heard(s: &str) -> String {
    collapse(s).to_lowercase()
}

pub fn normalize_correct(s: &str) -> String {
    collapse(s)
}

fn collapse(s: &str) -> String {
    s.replace(':', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Add or replace a pair, deduplicated by the heard side.
pub fn upsert(list: &mut Vec<Correction>, heard: &str, correct: &str) -> Upsert {
    let heard = normalize_heard(heard);
    let correct = normalize_correct(correct);
    if heard.is_empty() || correct.is_empty() {
        return Upsert::Invalid("both sides are needed");
    }
    if heard.chars().count() > MAX_HEARD_CHARS {
        return Upsert::Invalid("that is too long for a dictionary term");
    }
    // A case-only change ("lightdash" → "Lightdash") is a real correction:
    // Deepgram lowercases the find term, and brand casing is what people fix.
    if heard == correct {
        return Upsert::Invalid("heard and corrected are the same word");
    }
    if let Some(existing) = list.iter_mut().find(|c| c.heard == heard) {
        existing.correct = correct;
        return Upsert::Replaced;
    }
    if list.len() >= CAP {
        return Upsert::Full;
    }
    list.push(Correction { heard, correct });
    Upsert::Added
}

pub fn remove(list: &mut Vec<Correction>, heard: &str) -> bool {
    let heard = normalize_heard(heard);
    let before = list.len();
    list.retain(|c| c.heard != heard);
    list.len() != before
}

/// `find:replace` items for Deepgram, in list order, capped.
pub fn deepgram_replace_items(list: &[Correction]) -> Vec<String> {
    list.iter()
        .take(CAP)
        .map(|c| format!("{}:{}", c.heard, c.correct))
        .collect()
}

/// Precompiled rewrite rules; `None` when there is nothing to do so the hot
/// path pays nothing for users without corrections.
#[derive(Debug, Clone)]
pub struct Corrector {
    rules: Vec<(Regex, String)>,
}

impl Corrector {
    pub fn new(list: &[Correction]) -> Option<Self> {
        let mut sorted: Vec<&Correction> = list.iter().filter(|c| !c.heard.is_empty()).collect();
        // Longest phrase first so "google meet" wins over "meet".
        sorted.sort_by(|a, b| b.heard.len().cmp(&a.heard.len()).then(a.heard.cmp(&b.heard)));
        let rules: Vec<(Regex, String)> = sorted
            .into_iter()
            .filter_map(|c| {
                let escaped = regex::escape(&c.heard);
                let lead = if c.heard.starts_with(|ch: char| ch.is_alphanumeric()) { r"\b" } else { "" };
                let trail = if c.heard.ends_with(|ch: char| ch.is_alphanumeric()) { r"\b" } else { "" };
                Regex::new(&format!("(?i){lead}{escaped}{trail}"))
                    .ok()
                    .map(|re| (re, c.correct.clone()))
            })
            .collect();
        if rules.is_empty() {
            None
        } else {
            Some(Self { rules })
        }
    }

    /// The corrected text and whether anything changed.
    pub fn apply(&self, text: &str) -> (String, bool) {
        let mut out = std::borrow::Cow::Borrowed(text);
        let mut changed = false;
        for (re, correct) in &self.rules {
            if re.is_match(&out) {
                out = std::borrow::Cow::Owned(re.replace_all(&out, NoExpand(correct)).into_owned());
                changed = true;
            }
        }
        (out.into_owned(), changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(pairs: &[(&str, &str)]) -> Vec<Correction> {
        let mut v = Vec::new();
        for (h, c) in pairs {
            assert!(matches!(upsert(&mut v, h, c), Upsert::Added));
        }
        v
    }

    #[test]
    fn store_normalizes_dedupes_and_caps() {
        let mut v = Vec::new();
        assert_eq!(upsert(&mut v, "  Light  Dash ", "Lightdash"), Upsert::Added);
        assert_eq!(v[0].heard, "light dash");
        assert_eq!(upsert(&mut v, "light dash", "LightDash"), Upsert::Replaced);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].correct, "LightDash");
        assert!(matches!(upsert(&mut v, "", "x"), Upsert::Invalid(_)));
        assert!(matches!(upsert(&mut v, "same", "same"), Upsert::Invalid(_)));
        assert!(matches!(upsert(&mut v, "lightdash", "Lightdash"), Upsert::Added));
        assert!(matches!(upsert(&mut v, "a:b", "c:d"), Upsert::Added));
        assert_eq!(v[2].heard, "ab");
        assert_eq!(v[2].correct, "cd");
        for i in 0..CAP {
            let _ = upsert(&mut v, &format!("w{i}"), "x");
        }
        assert_eq!(v.len(), CAP);
        assert_eq!(upsert(&mut v, "one more", "x"), Upsert::Full);
        assert!(remove(&mut v, "AB"));
        assert!(!remove(&mut v, "ab"));
        assert_eq!(deepgram_replace_items(&list(&[("acme corp", "Acme")]))[0], "acme corp:Acme");
    }

    #[test]
    fn corrector_respects_word_boundaries_case_and_phrase_length() {
        let c = Corrector::new(&list(&[("meet", "Meet"), ("google meet", "Google Meet"), ("sasha", "Sascha")])).unwrap();
        assert_eq!(c.apply("we meet on google meet with sasha.").0, "we Meet on Google Meet with Sascha.");
        assert_eq!(c.apply("the meeting starts").0, "the meeting starts", "meet must not touch meeting");
        assert_eq!(c.apply("SASHA, hi").0, "Sascha, hi");
        assert!(!c.apply("nothing here").1);
        assert!(Corrector::new(&[]).is_none());
        let dollars = Corrector::new(&list(&[("acme", "A$1\\B")])).unwrap();
        assert_eq!(dollars.apply("acme rocks").0, "A$1\\B rocks", "no template expansion");
        let symbol = Corrector::new(&list(&[("c sharp", "C#")])).unwrap();
        assert_eq!(symbol.apply("I write c sharp daily").0, "I write C# daily");
    }
}
