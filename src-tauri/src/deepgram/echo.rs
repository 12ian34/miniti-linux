//! Dual-channel final ordering + microphone playback-echo reconciliation —
//! a port of the macOS `AppState` logic (audio.md §7):
//!
//! - System finals and clearly mic-dominant mic finals commit immediately into
//!   a 350 ms ordering buffer (nearby callbacks rendezvous on Deepgram's clock).
//! - Ambiguous mic finals wait up to 4 s for the system channel to catch up;
//!   they are suppressed when their words closely match an overlapping system
//!   final, released early once the system channel finalizes past them, and
//!   flushed on deadline otherwise.
//! - System-dominant mic *interims* are hidden so playback never flashes as
//!   "You" before reconciliation.

use std::time::{Duration, Instant};

use crate::audio::dual::Source;

use super::{SegmentSource, TranscriptEvent};

pub const MIC_ECHO_RECONCILIATION_DELAY: Duration = Duration::from_secs(4);
pub const ECHO_SYSTEM_HISTORY_WINDOW: Duration = Duration::from_secs(8);
pub const DUAL_CHANNEL_ORDERING_DELAY: Duration = Duration::from_millis(350);
const SYSTEM_COVERAGE_MARGIN: f64 = 0.8;
const TIME_PADDING: f64 = 0.8;

fn echo_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

fn lcs_len(a: &[String], b: &[String]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for x in a {
        cur[0] = 0;
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y { prev[j] + 1 } else { prev[j + 1].max(cur[j]) };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn longest_contiguous_run(a: &[String], b: &[String]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut prev = vec![0usize; b.len() + 1];
    let mut longest = 0;
    for x in a {
        let mut cur = vec![0usize; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            if x == y {
                cur[j + 1] = prev[j] + 1;
                longest = longest.max(cur[j + 1]);
            }
        }
        prev = cur;
    }
    longest
}

#[derive(Debug, Clone, PartialEq)]
pub struct EchoSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Port of `isLikelyMicEcho`: text/time agreement is the primary signal;
/// source energy only relaxes the threshold for short or imperfect matches.
pub fn is_likely_mic_echo(mic: &EchoSegment, system: &[EchoSegment], system_dominant: bool) -> bool {
    let mut overlapping: Vec<&EchoSegment> = system
        .iter()
        .filter(|s| s.end >= mic.start - TIME_PADDING && s.start <= mic.end + TIME_PADDING)
        .collect();
    if overlapping.is_empty() {
        return false;
    }
    overlapping.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    let mic_tokens = echo_tokens(&mic.text);
    if mic_tokens.is_empty() {
        return false;
    }
    let sys_text = overlapping.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    let sys_tokens = echo_tokens(&sys_text);
    if sys_tokens.is_empty() {
        return false;
    }
    let matched = lcs_len(&mic_tokens, &sys_tokens);
    let contiguous = longest_contiguous_run(&mic_tokens, &sys_tokens);
    let coverage = matched as f64 / mic_tokens.len() as f64;
    if mic_tokens.len() <= 2 {
        return system_dominant && matched == mic_tokens.len();
    }
    if matched >= 3 && coverage >= 0.84 {
        return true;
    }
    system_dominant && matched >= 3 && (coverage >= 0.66 || (contiguous >= 3 && coverage >= 0.60))
}

struct PendingMic {
    ev: TranscriptEvent,
    deadline: Instant,
}

struct Ordered {
    ev: TranscriptEvent,
    seq: u64,
    deadline: Instant,
}

struct RecentSystem {
    seg: EchoSegment,
    received: Instant,
}

/// Stateful reconciler for one meeting. Interims pass through [`ingest`]
/// immediately (subject to hiding); finals come back from [`ingest`] /
/// [`flush_due`] / [`flush_all`] in committed order.
#[derive(Default)]
pub struct DualChannelReconciler {
    recent_system: Vec<RecentSystem>,
    pending_mic: Vec<PendingMic>,
    ordered: Vec<Ordered>,
    seq: u64,
    pub suppressed: u64,
}

impl DualChannelReconciler {
    pub fn new() -> Self {
        Self::default()
    }

    fn system_segments(&self) -> Vec<EchoSegment> {
        self.recent_system.iter().map(|r| r.seg.clone()).collect()
    }

    fn is_echo(&self, ev: &TranscriptEvent, dominant: Source) -> bool {
        is_likely_mic_echo(
            &EchoSegment { text: ev.text.clone(), start: ev.start, end: ev.end },
            &self.system_segments(),
            dominant == Source::System,
        )
    }

    fn enqueue_ordered(&mut self, events: Vec<TranscriptEvent>, now: Instant) {
        for ev in events {
            self.seq += 1;
            self.ordered.push(Ordered { ev, seq: self.seq, deadline: now + DUAL_CHANNEL_ORDERING_DELAY });
        }
    }

    fn prune_system_history(&mut self, now: Instant) {
        self.recent_system.retain(|r| now.duration_since(r.received) <= ECHO_SYSTEM_HISTORY_WINDOW);
    }

    fn suppress_pending_echoes<F: Fn(f64, f64) -> Source>(&mut self, dominant: &F) {
        let system = self.system_segments();
        let mut survivors = Vec::with_capacity(self.pending_mic.len());
        for p in self.pending_mic.drain(..) {
            let d = dominant(p.ev.start, p.ev.end);
            if is_likely_mic_echo(&EchoSegment { text: p.ev.text.clone(), start: p.ev.start, end: p.ev.end }, &system, d == Source::System) {
                self.suppressed += 1;
            } else {
                survivors.push(p);
            }
        }
        self.pending_mic = survivors;
    }

    /// Once the clean system channel has finalized beyond an ambiguous mic
    /// segment, a surviving non-match is genuine local speech.
    fn release_covered_by_system(&mut self, now: Instant) {
        let Some(watermark) = self.recent_system.iter().map(|r| r.seg.end).fold(None, |m: Option<f64>, e| Some(m.map_or(e, |x| x.max(e)))) else { return };
        let (covered, keep): (Vec<PendingMic>, Vec<PendingMic>) = self.pending_mic.drain(..).partition(|p| p.ev.end + SYSTEM_COVERAGE_MARGIN <= watermark);
        self.pending_mic = keep;
        if !covered.is_empty() {
            self.enqueue_ordered(covered.into_iter().map(|p| p.ev).collect(), now);
        }
    }

    /// Feed one batch of events from a Results frame. Returns
    /// `(interims_to_show, finals_committed_now)`.
    pub fn ingest<F: Fn(f64, f64) -> Source>(&mut self, events: Vec<TranscriptEvent>, now: Instant, dominant: &F) -> (Vec<TranscriptEvent>, Vec<TranscriptEvent>) {
        let mut interims = Vec::new();
        let mut system_finals = Vec::new();
        let mut mic_finals = Vec::new();
        let mut other_finals = Vec::new();
        for ev in events {
            if !ev.is_final {
                // Hide system-dominant mic interims (playback would flash as "You").
                if ev.source == SegmentSource::Microphone && dominant(ev.start, ev.end) == Source::System {
                    continue;
                }
                interims.push(ev);
                continue;
            }
            match ev.source {
                SegmentSource::System => system_finals.push(ev),
                SegmentSource::Microphone => mic_finals.push(ev),
                SegmentSource::Unknown => other_finals.push(ev),
            }
        }

        if !system_finals.is_empty() {
            for ev in &system_finals {
                self.recent_system.push(RecentSystem { seg: EchoSegment { text: ev.text.clone(), start: ev.start, end: ev.end }, received: now });
            }
            self.prune_system_history(now);
            self.suppress_pending_echoes(dominant);
            self.release_covered_by_system(now);
            self.enqueue_ordered(system_finals, now);
        }

        let mut immediate = Vec::new();
        for ev in mic_finals {
            let d = dominant(ev.start, ev.end);
            if self.is_echo(&ev, d) {
                self.suppressed += 1;
                continue;
            }
            match d {
                Source::Mic => immediate.push(ev),
                Source::System | Source::Unknown => self.pending_mic.push(PendingMic { ev, deadline: now + MIC_ECHO_RECONCILIATION_DELAY }),
            }
        }
        if !immediate.is_empty() {
            self.enqueue_ordered(immediate, now);
        }
        if !other_finals.is_empty() {
            // Never drop content merely because Deepgram omitted channel_index.
            self.enqueue_ordered(other_finals, now);
        }
        (interims, self.flush_due(now, dominant))
    }

    /// Commit ordered finals whose rendezvous window elapsed, and expire
    /// pending mic segments past their reconciliation deadline.
    pub fn flush_due<F: Fn(f64, f64) -> Source>(&mut self, now: Instant, dominant: &F) -> Vec<TranscriptEvent> {
        if self.pending_mic.iter().any(|p| p.deadline <= now) {
            self.suppress_pending_echoes(dominant);
            let (due, keep): (Vec<PendingMic>, Vec<PendingMic>) = self.pending_mic.drain(..).partition(|p| p.deadline <= now);
            self.pending_mic = keep;
            if !due.is_empty() {
                self.enqueue_ordered(due.into_iter().map(|p| p.ev).collect(), now);
            }
            self.prune_system_history(now);
        }
        let (due, keep): (Vec<Ordered>, Vec<Ordered>) = self.ordered.drain(..).partition(|o| o.deadline <= now);
        self.ordered = keep;
        Self::commit(due)
    }

    /// Stop / reconnect: everything pending comes out now.
    pub fn flush_all<F: Fn(f64, f64) -> Source>(&mut self, dominant: &F) -> Vec<TranscriptEvent> {
        self.suppress_pending_echoes(dominant);
        let pending: Vec<TranscriptEvent> = self.pending_mic.drain(..).map(|p| p.ev).collect();
        let now = Instant::now();
        self.enqueue_ordered(pending, now);
        let all: Vec<Ordered> = std::mem::take(&mut self.ordered);
        Self::commit(all)
    }

    /// Next time something becomes due (for the caller's timer).
    pub fn next_deadline(&self) -> Option<Instant> {
        self.ordered.iter().map(|o| o.deadline).chain(self.pending_mic.iter().map(|p| p.deadline)).min()
    }

    fn commit(mut batch: Vec<Ordered>) -> Vec<TranscriptEvent> {
        batch.sort_by(|a, b| a.ev.start.partial_cmp(&b.ev.start).unwrap_or(std::cmp::Ordering::Equal).then(a.seq.cmp(&b.seq)));
        batch.into_iter().map(|o| o.ev).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(source: SegmentSource, text: &str, start: f64, end: f64, is_final: bool) -> TranscriptEvent {
        TranscriptEvent {
            text: text.into(),
            speaker_id: if source == SegmentSource::Microphone { 1000 } else { 0 },
            start,
            end,
            is_final,
            confidence: 0.9,
            source,
            channel_index: Some(if source == SegmentSource::Microphone { 0 } else { 1 }),
        }
    }
    fn seg(text: &str, start: f64, end: f64) -> EchoSegment {
        EchoSegment { text: text.into(), start, end }
    }

    #[test]
    fn echo_detection_matches_macos_rules() {
        let sys = vec![seg("nobody alive today will remember this", 10.0, 13.0)];
        assert!(is_likely_mic_echo(&seg("nobody alive today will remember", 10.2, 12.0), &sys, false), "near-verbatim (≥84% coverage) needs no dominance");
        assert!(!is_likely_mic_echo(&seg("nobody alive today, though", 10.2, 12.0), &sys, false), "imperfect ASR variant kept without dominance");
        assert!(is_likely_mic_echo(&seg("nobody alive today, though", 10.2, 12.0), &sys, true), "…but suppressed once the clean system source dominates");
        assert!(!is_likely_mic_echo(&seg("let me check the calendar for tomorrow", 10.2, 12.0), &sys, true), "unrelated overlap kept");
        assert!(!is_likely_mic_echo(&seg("nobody alive today will remember this", 30.0, 33.0), &sys, true), "no time overlap");
        // 1–2 word interjections: exact match + system dominance only.
        assert!(is_likely_mic_echo(&seg("nobody alive", 10.5, 11.0), &sys, true));
        assert!(!is_likely_mic_echo(&seg("nobody alive", 10.5, 11.0), &sys, false));
        assert!(!is_likely_mic_echo(&seg("yeah right", 10.5, 11.0), &sys, true));
        // Fuzzy variant needs dominance.
        // 5 of 7 words match (71%): under the 84% verbatim bar, over the 66% dominance bar.
        let fuzzy = seg("nobody alive today will remember something else", 10.0, 13.0);
        assert!(!is_likely_mic_echo(&fuzzy, &sys, false));
        assert!(is_likely_mic_echo(&fuzzy, &sys, true));
    }

    #[test]
    fn clear_local_speech_commits_after_ordering_window() {
        let mut r = DualChannelReconciler::new();
        let t0 = Instant::now();
        let mic_dom = |_: f64, _: f64| Source::Mic;
        let (interims, finals) = r.ingest(vec![ev(SegmentSource::Microphone, "hello there everyone", 1.0, 2.0, true)], t0, &mic_dom);
        assert!(interims.is_empty());
        assert!(finals.is_empty(), "held for the 350 ms rendezvous");
        let out = r.flush_due(t0 + Duration::from_millis(400), &mic_dom);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "hello there everyone");
    }

    #[test]
    fn playback_echo_on_mic_is_suppressed() {
        let mut r = DualChannelReconciler::new();
        let t0 = Instant::now();
        let sys_dom = |_: f64, _: f64| Source::System;
        r.ingest(vec![ev(SegmentSource::System, "welcome to the world service news hour", 5.0, 8.0, true)], t0, &sys_dom);
        let (_, _) = r.ingest(vec![ev(SegmentSource::Microphone, "welcome to the world service news", 5.3, 8.1, true)], t0 + Duration::from_millis(50), &sys_dom);
        let out = r.flush_due(t0 + Duration::from_secs(5), &sys_dom);
        assert_eq!(out.len(), 1, "only the system final survives");
        assert_eq!(out[0].source, SegmentSource::System);
        assert_eq!(r.suppressed, 1);
    }

    #[test]
    fn ambiguous_mic_speech_is_released_when_system_passes_it() {
        let mut r = DualChannelReconciler::new();
        let t0 = Instant::now();
        let unknown = |_: f64, _: f64| Source::Unknown;
        // Genuine local interjection during quiet: ambiguous → pending.
        r.ingest(vec![ev(SegmentSource::Microphone, "can we move on to pricing", 10.0, 11.5, true)], t0, &unknown);
        assert!(r.flush_due(t0 + Duration::from_millis(400), &unknown).is_empty(), "still pending");
        // System finalizes well past it with unrelated text → released without waiting 4 s.
        let (_, finals) = r.ingest(vec![ev(SegmentSource::System, "the quarterly numbers look strong overall", 11.0, 13.0, true)], t0 + Duration::from_millis(500), &unknown);
        assert!(finals.is_empty());
        let out = r.flush_due(t0 + Duration::from_millis(900), &unknown);
        let texts: Vec<&str> = out.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["can we move on to pricing", "the quarterly numbers look strong overall"], "chronological");
    }

    #[test]
    fn pending_mic_flushes_on_deadline_when_system_is_silent() {
        let mut r = DualChannelReconciler::new();
        let t0 = Instant::now();
        let unknown = |_: f64, _: f64| Source::Unknown;
        r.ingest(vec![ev(SegmentSource::Microphone, "is anyone there", 1.0, 2.0, true)], t0, &unknown);
        assert!(r.flush_due(t0 + Duration::from_secs(3), &unknown).is_empty());
        // Deadline passes → moves into the 350 ms ordering buffer, then commits.
        assert!(r.flush_due(t0 + Duration::from_millis(4_100), &unknown).is_empty());
        let out = r.flush_due(t0 + Duration::from_millis(4_500), &unknown);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn system_dominant_mic_interims_are_hidden() {
        let mut r = DualChannelReconciler::new();
        let t0 = Instant::now();
        let sys_dom = |_: f64, _: f64| Source::System;
        let mic_dom = |_: f64, _: f64| Source::Mic;
        let (i1, _) = r.ingest(vec![ev(SegmentSource::Microphone, "partial", 1.0, 1.5, false)], t0, &sys_dom);
        assert!(i1.is_empty());
        let (i2, _) = r.ingest(vec![ev(SegmentSource::Microphone, "partial", 1.0, 1.5, false)], t0, &mic_dom);
        assert_eq!(i2.len(), 1);
        let (i3, _) = r.ingest(vec![ev(SegmentSource::System, "partial", 1.0, 1.5, false)], t0, &sys_dom);
        assert_eq!(i3.len(), 1, "system interims always show");
    }

    #[test]
    fn flush_all_drains_everything_in_order() {
        let mut r = DualChannelReconciler::new();
        let t0 = Instant::now();
        let unknown = |_: f64, _: f64| Source::Unknown;
        r.ingest(vec![ev(SegmentSource::Microphone, "second thing here", 5.0, 6.0, true)], t0, &unknown);
        r.ingest(vec![ev(SegmentSource::System, "first thing here", 1.0, 2.0, true)], t0, &unknown);
        let out = r.flush_all(&unknown);
        assert_eq!(out.iter().map(|e| e.start as i64).collect::<Vec<_>>(), vec![1, 5]);
        assert!(r.next_deadline().is_none());
    }
}
