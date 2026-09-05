//! Live Deepgram WebSocket streaming. Requires a valid credential at runtime;
//! the pure protocol logic it relies on is unit-tested in the parent module.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use super::{
    build_ws_url, keepalive_message, parse_message, results_to_event, Auth, Backoff, DeepgramConfig,
    ServerMessage, TranscriptEvent, KEEPALIVE_INTERVAL,
};

/// Minimum confidence for a segment to be promoted to the transcript.
const MIN_CONFIDENCE: f64 = 0.3;

/// Connect and stream until `pcm_rx` closes or the socket errors.
///
/// - `pcm_rx`: 16 kHz PCM16 LE byte frames (mono or interleaved stereo).
/// - `events_tx`: stabilized transcript segments for the UI / persistence.
///
/// On success the caller receives events; on transport error this returns `Err`
/// and the caller decides whether to reconnect (see [`Backoff`]).
pub async fn connect_and_stream(
    cfg: &DeepgramConfig,
    auth: &Auth,
    mut pcm_rx: Receiver<Vec<u8>>,
    events_tx: Sender<TranscriptEvent>,
) -> Result<(), String> {
    let url = build_ws_url(cfg);
    let mut request = url
        .into_client_request()
        .map_err(|e| format!("bad deepgram url: {e}"))?;
    request.headers_mut().insert(
        "Authorization",
        auth.header_value()
            .parse()
            .map_err(|_| "invalid auth header".to_string())?,
    );

    let (ws, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("deepgram connect failed: {e}"))?;
    let (mut write, mut read) = ws.split();

    let multichannel = cfg.multichannel;
    let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            maybe_pcm = pcm_rx.recv() => {
                match maybe_pcm {
                    Some(bytes) => {
                        if write.send(Message::Binary(bytes)).await.is_err() {
                            return Err("deepgram send failed".into());
                        }
                    }
                    None => {
                        // Upstream closed: flush a graceful close and finish.
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            _ = keepalive.tick() => {
                if write.send(Message::Text(keepalive_message())).await.is_err() {
                    return Err("deepgram keepalive failed".into());
                }
            }
            maybe_msg = read.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(txt))) => {
                        if let Ok(ServerMessage::Results(res)) = parse_message(&txt) {
                            if let Some(ev) = results_to_event(&res, multichannel, MIN_CONFIDENCE) {
                                if events_tx.send(ev).await.is_err() {
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return Ok(()),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(format!("deepgram read error: {e}")),
                }
            }
        }
    }
}

/// Reconnect wrapper: retries with bounded backoff. `reconnect` is consulted
/// after each drop to decide whether to try again.
pub async fn stream_with_reconnect<F>(
    cfg: DeepgramConfig,
    auth: Auth,
    mut make_pcm_rx: impl FnMut() -> Receiver<Vec<u8>>,
    events_tx: Sender<TranscriptEvent>,
    mut should_continue: F,
) where
    F: FnMut() -> bool,
{
    let mut backoff = Backoff::default();
    while should_continue() {
        let pcm_rx = make_pcm_rx();
        match connect_and_stream(&cfg, &auth, pcm_rx, events_tx.clone()).await {
            Ok(()) => {
                backoff.reset();
                break;
            }
            Err(e) => {
                tracing::warn!("deepgram stream ended: {e}");
                let delay = backoff.next_delay();
                tokio::time::sleep(delay.min(Duration::from_secs(30))).await;
            }
        }
    }
}
