//! Live capture sources. Mic uses `cpal` (ALSA/PipeWire); system audio reads the
//! default sink monitor via `parec` (PipeWire-pulse / PulseAudio). Both normalize
//! to 16 kHz mono PCM16 frames matching the Deepgram contract.
//!
//! Runtime requires an audio server + devices; in headless environments the
//! start calls return an error the UI surfaces as "mic/system unavailable".

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::pcm::{self, LinearResampler};

/// A chunk of 16 kHz mono PCM16 samples plus its RMS level in [0, 1].
#[derive(Debug, Clone)]
pub struct PcmFrame {
    pub samples: Vec<i16>,
    pub level: f32,
}

/// Which source a capture handle represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Microphone,
    System,
}

/// Owns a running capture; dropping or calling [`CaptureHandle::stop`] tears it down.
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    child: Option<Child>,
    pub source: Source,
}

impl CaptureHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start microphone capture. Frames are pushed to `sink` until the handle drops.
pub fn start_microphone(sink: Sender<PcmFrame>) -> Result<CaptureHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    // The cpal stream is !Send, so build and own it entirely on this thread.
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let join = thread::Builder::new()
        .name("miniti-mic".into())
        .spawn(move || {
            let stream = match build_input_stream(sink) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!("mic stream play failed: {e}")));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            while !stop_thread.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            drop(stream);
        })
        .map_err(|e| e.to_string())?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(CaptureHandle {
            stop,
            join: Some(join),
            child: None,
            source: Source::Microphone,
        }),
        Ok(Err(e)) => {
            let _ = join.join();
            Err(e)
        }
        Err(_) => Err("mic capture thread exited before signaling readiness".into()),
    }
}

fn build_input_stream(sink: Sender<PcmFrame>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("no default input config: {e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let stream_config: cpal::StreamConfig = config.config();

    macro_rules! stream_for {
        ($t:ty, $to_f32:expr) => {{
            let sink = sink.clone();
            // One resampler per stream so phase carries across callbacks.
            let mut resampler = LinearResampler::new(sample_rate, pcm::TARGET_SAMPLE_RATE);
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[$t], _: &cpal::InputCallbackInfo| {
                        let floats: Vec<f32> = data.iter().map($to_f32).collect();
                        let mono = pcm::downmix_to_mono(&floats, channels);
                        let resampled = resampler.process(&mono);
                        if resampled.is_empty() {
                            return;
                        }
                        let level = pcm::rms_level(&resampled);
                        let samples: Vec<i16> = resampled
                            .iter()
                            .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                            .collect();
                        let _ = sink.send(PcmFrame { samples, level });
                    },
                    |e| tracing::error!("mic stream error: {e}"),
                    None,
                )
                .map_err(|e| format!("build_input_stream: {e}"))
        }};
    }

    match config.sample_format() {
        cpal::SampleFormat::F32 => stream_for!(f32, |s: &f32| *s),
        cpal::SampleFormat::I16 => stream_for!(i16, |s: &i16| *s as f32 / i16::MAX as f32),
        cpal::SampleFormat::U16 => {
            stream_for!(u16, |s: &u16| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0)
        }
        other => Err(format!("unsupported sample format: {other:?}")),
    }
}

/// Resolve the default sink's monitor source name via `pactl`.
pub fn default_monitor_source() -> Result<String, String> {
    let out = Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .map_err(|e| format!("pactl not available: {e}"))?;
    if !out.status.success() {
        return Err("pactl get-default-sink failed".into());
    }
    let sink = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sink.is_empty() {
        return Err("no default sink".into());
    }
    Ok(format!("{sink}.monitor"))
}

/// ~50 ms of 16 kHz mono s16le = 800 samples = 1600 bytes per frame.
const SYSTEM_FRAME_BYTES: usize = 1600;

/// Start system-audio capture from the default sink monitor via `parec`.
pub fn start_system(sink: Sender<PcmFrame>) -> Result<CaptureHandle, String> {
    let monitor = default_monitor_source()?;
    let mut child = Command::new("parec")
        .args([
            "--format=s16le",
            &format!("--rate={}", pcm::TARGET_SAMPLE_RATE),
            "--channels=1",
            "--latency-msec=50",
            "-d",
            &monitor,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("parec not available: {e}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "parec produced no stdout".to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let join = thread::Builder::new()
        .name("miniti-system".into())
        .spawn(move || {
            // Pipes deliver arbitrary byte counts. Always read whole frames so a
            // partial read can never leave the sample stream byte-misaligned.
            let mut buf = [0u8; SYSTEM_FRAME_BYTES];
            while !stop_thread.load(Ordering::SeqCst) {
                if stdout.read_exact(&mut buf).is_err() {
                    break; // EOF (parec killed) or pipe error
                }
                let samples = pcm::le_bytes_to_pcm16(&buf);
                let level = pcm::rms_level_pcm16(&samples);
                if sink.send(PcmFrame { samples, level }).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;

    Ok(CaptureHandle {
        stop,
        join: Some(join),
        child: Some(child),
        source: Source::System,
    })
}

/// True when a default input device is present (used for capability reporting).
pub fn microphone_available() -> bool {
    cpal::default_host().default_input_device().is_some()
}

/// True when PipeWire/Pulse exposes a default sink monitor.
pub fn system_audio_available() -> bool {
    default_monitor_source().is_ok()
}

/// Convenience: create the frame channel used by the recording engine.
pub fn frame_channel() -> (Sender<PcmFrame>, Receiver<PcmFrame>) {
    mpsc::channel()
}
