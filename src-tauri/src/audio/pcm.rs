//! PCM conversion helpers for the Deepgram contract (PLAN.md §6): 16 kHz
//! PCM16 LE, mono (`channels=1`) or stereo-interleaved mic/system
//! (`channels=2&multichannel=true`, mic = ch0, system = ch1).

/// Target sample rate streamed to Deepgram.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Downmix an interleaved multi-channel `f32` frame to mono by averaging.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    let frames = interleaved.len() / ch;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let base = f * ch;
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += interleaved[base + c];
        }
        out.push(sum / ch as f32);
    }
    out
}

/// Stateless linear-interpolation resampler for one-off buffers (tests, files).
/// Live capture uses [`LinearResampler`], which carries phase across callbacks.
pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || input.is_empty() {
        return input.to_vec();
    }
    let mut r = LinearResampler::new(from_hz, to_hz);
    r.process(input)
}

/// Streaming linear resampler. Keeps the fractional read position and the last
/// input sample between calls so consecutive audio-callback buffers join
/// without the phase reset (and click) a per-buffer resample introduces.
#[derive(Debug, Clone)]
pub struct LinearResampler {
    step: f64,
    /// Fractional position of the next output sample relative to `prev`.
    pos: f64,
    /// Last input sample of the previous buffer (`None` before any input).
    prev: Option<f32>,
    identity: bool,
}

impl LinearResampler {
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        Self {
            step: from_hz as f64 / to_hz as f64,
            pos: 0.0,
            prev: None,
            identity: from_hz == to_hz,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.identity || input.is_empty() {
            return input.to_vec();
        }
        // Virtual buffer = [prev, input...]; positions are relative to it.
        let has_prev = self.prev.is_some();
        let virt_len = input.len() + usize::from(has_prev);
        let sample = |i: usize| -> f32 {
            if has_prev {
                if i == 0 {
                    self.prev.unwrap()
                } else {
                    input[i - 1]
                }
            } else {
                input[i]
            }
        };

        let mut out = Vec::with_capacity((input.len() as f64 / self.step).ceil() as usize + 1);
        let mut pos = self.pos;
        while (pos.floor() as usize) + 1 < virt_len {
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = sample(idx);
            let b = sample(idx + 1);
            out.push(a + (b - a) * frac);
            pos += self.step;
        }
        // Rebase so the last input sample becomes `prev` at index 0.
        self.pos = pos - (virt_len - 1) as f64;
        self.prev = input.last().copied();
        out
    }
}

/// Clamp and convert `f32` samples in [-1.0, 1.0] to little-endian PCM16 bytes.
pub fn f32_to_pcm16le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Interleave mic (ch0) and system (ch1) PCM16 samples for Deepgram
/// multichannel. The shorter stream is padded with silence so a stalled
/// source never desynchronizes the timeline.
pub fn interleave_stereo_pcm16(mic: &[i16], system: &[i16]) -> Vec<i16> {
    let frames = mic.len().max(system.len());
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        out.push(mic.get(i).copied().unwrap_or(0));
        out.push(system.get(i).copied().unwrap_or(0));
    }
    out
}

/// PCM16 samples to little-endian bytes.
pub fn pcm16_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Little-endian bytes to PCM16 samples (length must be even).
pub fn le_bytes_to_pcm16(bytes: &[u8]) -> Vec<i16> {
    // `chunks_exact` rather than `as_chunks` to stay within the Rust 1.85 floor.
    #[allow(clippy::manual_slice_size_calculation)]
    bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// Root-mean-square level in [0.0, 1.0], used to drive the waveform meters.
pub fn rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

/// RMS directly from PCM16 (avoids a float copy on the hot path).
pub fn rms_level_pcm16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let f = s as f64 / i16::MAX as f64;
            f * f
        })
        .sum();
    ((sum_sq / samples.len() as f64).sqrt() as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let out = downmix_to_mono(&[1.0, 0.0, 0.5, 0.5], 2);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn downmix_mono_is_identity() {
        assert_eq!(downmix_to_mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let x = vec![0.0, 0.5, -0.5];
        assert_eq!(resample_linear(&x, 16_000, 16_000), x);
    }

    #[test]
    fn resample_downsamples_length() {
        let input = vec![0.0f32; 48_000];
        let out = resample_linear(&input, 48_000, 16_000);
        assert!((out.len() as i64 - 16_000).abs() <= 1, "got {}", out.len());
    }

    #[test]
    fn resample_upsamples_length() {
        let input = vec![0.0f32; 8_000];
        let out = resample_linear(&input, 8_000, 16_000);
        assert!((out.len() as i64 - 16_000).abs() <= 2, "got {}", out.len());
    }

    #[test]
    fn streaming_resampler_matches_whole_buffer_within_tolerance() {
        // A ramp resampled in one shot vs in chunks must agree (no phase reset).
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 / 4800.0) * 2.0 - 1.0).collect();
        let whole = resample_linear(&input, 48_000, 16_000);

        let mut r = LinearResampler::new(48_000, 16_000);
        let mut chunked = Vec::new();
        for chunk in input.chunks(480) {
            chunked.extend(r.process(chunk));
        }
        assert!((whole.len() as i64 - chunked.len() as i64).abs() <= 1);
        let n = whole.len().min(chunked.len());
        for i in 0..n {
            assert!(
                (whole[i] - chunked[i]).abs() < 1e-4,
                "sample {i}: {} vs {}",
                whole[i],
                chunked[i]
            );
        }
    }

    #[test]
    fn streaming_resampler_output_is_monotone_for_ramp() {
        // Any click at a chunk boundary shows up as a non-monotone step.
        let input: Vec<f32> = (0..9600).map(|i| i as f32 / 9600.0).collect();
        let mut r = LinearResampler::new(48_000, 16_000);
        let mut out = Vec::new();
        for chunk in input.chunks(441) {
            out.extend(r.process(chunk));
        }
        for w in out.windows(2) {
            assert!(w[1] >= w[0] - 1e-6, "non-monotone at {:?}", w);
        }
    }

    #[test]
    fn f32_to_pcm16_endianness_and_clamp() {
        let bytes = f32_to_pcm16le(&[0.0, 1.0, -1.0, 2.0]);
        assert_eq!(&bytes[0..2], &0i16.to_le_bytes());
        assert_eq!(&bytes[2..4], &i16::MAX.to_le_bytes());
        assert_eq!(&bytes[4..6], &(-i16::MAX).to_le_bytes());
        assert_eq!(&bytes[6..8], &i16::MAX.to_le_bytes());
    }

    #[test]
    fn pcm16_bytes_roundtrip() {
        let s = [1i16, -2, 32767, -32768];
        assert_eq!(le_bytes_to_pcm16(&pcm16_to_le_bytes(&s)), s.to_vec());
    }

    #[test]
    fn interleave_pads_shorter_system_with_silence() {
        let mic = [10i16, 20, 30];
        let system = [1i16];
        let out = interleave_stereo_pcm16(&mic, &system);
        assert_eq!(out, vec![10, 1, 20, 0, 30, 0]);
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms_level(&[0.0; 128]), 0.0);
        assert_eq!(rms_level_pcm16(&[0; 128]), 0.0);
    }

    #[test]
    fn rms_of_full_scale_is_one() {
        assert!((rms_level(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
        assert!((rms_level_pcm16(&[i16::MAX, -i16::MAX]) - 1.0).abs() < 1e-6);
    }
}
