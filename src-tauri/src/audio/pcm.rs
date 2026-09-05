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

/// Linear-interpolation resampler. Adequate for the Phase 0 spike; replace with
/// a windowed-sinc resampler if quality proves insufficient.
pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
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

/// Root-mean-square level in [0.0, 1.0], used to drive the waveform meters.
pub fn rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        // stereo: [L=1.0,R=0.0, L=0.5,R=0.5]
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
        assert_eq!(out.len(), 16_000);
    }

    #[test]
    fn resample_upsamples_length() {
        let input = vec![0.0f32; 8_000];
        let out = resample_linear(&input, 8_000, 16_000);
        assert_eq!(out.len(), 16_000);
    }

    #[test]
    fn f32_to_pcm16_endianness_and_clamp() {
        let bytes = f32_to_pcm16le(&[0.0, 1.0, -1.0, 2.0]);
        assert_eq!(&bytes[0..2], &0i16.to_le_bytes());
        assert_eq!(&bytes[2..4], &i16::MAX.to_le_bytes());
        assert_eq!(&bytes[4..6], &(-i16::MAX).to_le_bytes());
        // 2.0 clamps to +1.0 -> i16::MAX
        assert_eq!(&bytes[6..8], &i16::MAX.to_le_bytes());
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
    }

    #[test]
    fn rms_of_full_scale_is_one() {
        assert!((rms_level(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
    }
}
