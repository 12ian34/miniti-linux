//! Audio subsystem: PCM conversion helpers and live capture sources.

pub mod capture;
pub mod dual;
pub mod pcm;

pub use capture::{
    frame_channel, microphone_available, start_microphone, start_system, system_audio_available,
    CaptureHandle, PcmFrame, Source,
};
