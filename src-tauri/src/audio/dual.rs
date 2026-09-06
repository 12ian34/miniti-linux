//! Dual-source (mic + system) engine — port of the macOS `AudioCaptureService`
//! stereo path: the system monitor feeds a ring buffer, and each mic frame
//! drains it to produce interleaved stereo PCM16 for Deepgram multichannel
//! (ch0 = mic, ch1 = system). Underruns are padded with silence so the channel
//! layout stays stable for the whole socket. A source-energy log records
//! Int16-scale RMS per frame so the transcript layer can tell playback echo
//! from genuine local speech.

use std::collections::VecDeque;

use super::pcm::{self, TARGET_SAMPLE_RATE};

/// ~500 ms at 16 kHz mono (samples, not bytes) — same as macOS.
pub const RING_CAPACITY: usize = 8_000;
/// ~60 s of 50 ms samples.
pub const SOURCE_LOG_CAPACITY: usize = 1_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Mic,
    System,
    Unknown,
}

/// One frame's worth of mic vs system energy on the recording timeline.
#[derive(Debug, Clone, Copy)]
pub struct SourceSample {
    /// Seconds from recording start.
    pub start: f64,
    pub end: f64,
    /// Int16-scale RMS.
    pub mic_energy: f32,
    pub sys_energy: f32,
}

/// Bounded chronological log of [`SourceSample`]s.
#[derive(Debug, Default)]
pub struct SourceEnergyLog {
    samples: VecDeque<SourceSample>,
}

impl SourceEnergyLog {
    pub fn push(&mut self, s: SourceSample) {
        if self.samples.len() >= SOURCE_LOG_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(s);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Port of `dominantSource(from:to:)`: energy-weighted comparison over the
    /// window, nearest sample within 1.5 s as fallback; the mic must reach a
    /// speech floor (200) and be > 2× the system to count as local speech.
    pub fn dominant_source(&self, start: f64, end: f64) -> Source {
        let (ws, we) = (start.min(end), start.max(end));
        let mid = (ws + we) * 0.5;
        let mut mic_w = 0.0f64;
        let mut sys_w = 0.0f64;
        let mut overlap_total = 0.0f64;
        let mut nearest: Option<(f64, SourceSample)> = None;
        for s in &self.samples {
            let os = ws.max(s.start);
            let oe = we.min(s.end);
            if oe > os {
                let sec = oe - os;
                mic_w += s.mic_energy as f64 * sec;
                sys_w += s.sys_energy as f64 * sec;
                overlap_total += sec;
                continue;
            }
            let d = if mid < s.start {
                s.start - mid
            } else if mid > s.end {
                mid - s.end
            } else {
                0.0
            };
            if nearest.map(|(nd, _)| d < nd).unwrap_or(true) {
                nearest = Some((d, *s));
            }
        }
        if overlap_total <= 0.0 {
            let Some((d, s)) = nearest else {
                return Source::Unknown;
            };
            if d > 1.5 {
                return Source::Unknown;
            }
            mic_w = s.mic_energy as f64;
            sys_w = s.sys_energy as f64;
            overlap_total = s.end - s.start;
        }
        let avg_mic = if overlap_total > 0.0 {
            mic_w / overlap_total
        } else {
            0.0
        };
        if avg_mic < 200.0 {
            return Source::System;
        }
        if mic_w > sys_w * 2.0 {
            Source::Mic
        } else {
            Source::System
        }
    }
}

/// Int16-scale RMS (0…32767), as the macOS energy log stores it.
pub fn rms_int16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// System ring + interleave state. One instance per recording; the system
/// thread calls [`push_system`], the mic thread calls [`interleave_mic`].
#[derive(Debug)]
pub struct DualMixer {
    ring: VecDeque<i16>,
    /// Mic samples emitted so far → recording timeline for the energy log.
    mic_samples_emitted: u64,
    pub underruns: u64,
    pub frames_with_system: u64,
    pub system_dropped: u64,
    pub log: SourceEnergyLog,
}

impl Default for DualMixer {
    fn default() -> Self {
        Self::new()
    }
}

impl DualMixer {
    pub fn new() -> Self {
        Self {
            ring: VecDeque::with_capacity(RING_CAPACITY),
            mic_samples_emitted: 0,
            underruns: 0,
            frames_with_system: 0,
            system_dropped: 0,
            log: SourceEnergyLog::default(),
        }
    }

    /// Append system samples; the oldest are dropped when the ring overflows
    /// (system runs ahead of mic — keep the freshest audio).
    pub fn push_system(&mut self, samples: &[i16]) {
        for &s in samples {
            if self.ring.len() >= RING_CAPACITY {
                self.ring.pop_front();
                self.system_dropped += 1;
            }
            self.ring.push_back(s);
        }
    }

    pub fn ring_len(&self) -> usize {
        self.ring.len()
    }

    /// Drain the ring against a mic frame and return interleaved stereo PCM16
    /// (ch0 = mic, ch1 = system, silence-padded). Also logs source energy.
    pub fn interleave_mic(&mut self, mic: &[i16]) -> Vec<i16> {
        let n = mic.len();
        let available = self.ring.len().min(n);
        let mut system: Vec<i16> = Vec::with_capacity(n);
        for _ in 0..available {
            system.push(self.ring.pop_front().unwrap_or(0));
        }
        if available < n {
            self.underruns += 1;
            system.resize(n, 0);
        } else {
            self.frames_with_system += 1;
        }
        let start = self.mic_samples_emitted as f64 / TARGET_SAMPLE_RATE as f64;
        self.mic_samples_emitted += n as u64;
        let end = self.mic_samples_emitted as f64 / TARGET_SAMPLE_RATE as f64;
        self.log.push(SourceSample {
            start,
            end,
            mic_energy: rms_int16(mic),
            sys_energy: rms_int16(&system),
        });
        pcm::interleave_stereo_pcm16(mic, &system)
    }

    /// Recording-timeline position of the mic stream, in seconds.
    pub fn elapsed(&self) -> f64 {
        self.mic_samples_emitted as f64 / TARGET_SAMPLE_RATE as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaves_and_pads_underruns() {
        let mut m = DualMixer::new();
        m.push_system(&[1, 2]);
        let out = m.interleave_mic(&[10, 20, 30]);
        assert_eq!(out, vec![10, 1, 20, 2, 30, 0]);
        assert_eq!(m.underruns, 1);
        assert_eq!(m.ring_len(), 0);
        m.push_system(&[5, 6, 7, 8]);
        let out = m.interleave_mic(&[1, 1]);
        assert_eq!(out, vec![1, 5, 1, 6]);
        assert_eq!(m.frames_with_system, 1);
        assert_eq!(m.ring_len(), 2, "leftover system stays queued");
    }

    #[test]
    fn ring_keeps_freshest_when_system_runs_ahead() {
        let mut m = DualMixer::new();
        let big: Vec<i16> = (0..(RING_CAPACITY as i16 + 10)).collect();
        m.push_system(&big);
        assert_eq!(m.ring_len(), RING_CAPACITY);
        assert_eq!(m.system_dropped, 10);
        let out = m.interleave_mic(&[0]);
        assert_eq!(out[1], 10, "oldest 10 samples were dropped");
    }

    #[test]
    fn energy_log_tracks_recording_timeline() {
        let mut m = DualMixer::new();
        m.push_system(&[10_000; 800]);
        m.interleave_mic(&[0; 800]); // 50 ms of silent mic, loud system
        m.interleave_mic(&[8_000; 800]); // 50 ms of loud mic, silent system
        assert!((m.elapsed() - 0.1).abs() < 1e-9);
        assert_eq!(m.log.dominant_source(0.0, 0.05), Source::System);
        assert_eq!(m.log.dominant_source(0.05, 0.1), Source::Mic);
        assert_eq!(
            m.log.dominant_source(5.0, 5.1),
            Source::Unknown,
            "too far from any sample"
        );
    }

    #[test]
    fn dominance_needs_speech_floor_and_2x_margin() {
        let mut log = SourceEnergyLog::default();
        log.push(SourceSample {
            start: 0.0,
            end: 1.0,
            mic_energy: 150.0,
            sys_energy: 0.0,
        });
        assert_eq!(
            log.dominant_source(0.0, 1.0),
            Source::System,
            "keyboard-level mic is not speech"
        );
        log.clear();
        log.push(SourceSample {
            start: 0.0,
            end: 1.0,
            mic_energy: 1_000.0,
            sys_energy: 600.0,
        });
        assert_eq!(
            log.dominant_source(0.0, 1.0),
            Source::System,
            "not clearly louder"
        );
        log.clear();
        log.push(SourceSample {
            start: 0.0,
            end: 1.0,
            mic_energy: 2_000.0,
            sys_energy: 600.0,
        });
        assert_eq!(log.dominant_source(0.0, 1.0), Source::Mic);
        // Fallback to the nearest sample within 1.5 s.
        assert_eq!(log.dominant_source(2.0, 2.2), Source::Mic);
        assert_eq!(log.dominant_source(3.0, 3.2), Source::Unknown);
    }
}
