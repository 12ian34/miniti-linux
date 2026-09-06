//! Live Deepgram WebSocket streaming with graceful close and bounded reconnect.
//!
//! - On stop (`pcm_rx` closed) we send `CloseStream` and drain the finals
//!   Deepgram flushes before it closes the socket, so the last seconds of a
//!   meeting are never lost.
//! - On transport errors [`run_stream`] reconnects with backoff, preserving
//!   speaker identities and shifting socket-relative times onto the meeting
//!   timeline (wall-clock offset, as macOS does for reconnects).

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use super::{
    build_ws_url, close_stream_message, keepalive_message, parse_message, Auth, Backoff,
    DeepgramConfig, Processor, ServerMessage, TranscriptEvent, CLOSE_DRAIN_TIMEOUT,
    KEEPALIVE_INTERVAL,
};

/// Connection lifecycle reported to the UI.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StreamStatus {
    Connecting,
    Connected { generation: u64 },
    Reconnecting { attempt: u32, reason: String },
    /// Graceful end after `CloseStream` drained.
    Ended,
    /// Gave up after repeated connect failures.
    Failed { reason: String },
}

#[derive(Debug)]
pub enum StreamError {
    /// Handshake / auth failure — never connected.
    Connect(String),
    /// Connected, then the transport dropped.
    Transport(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Connect(e) => write!(f, "connect failed: {e}"),
            StreamError::Transport(e) => write!(f, "transport dropped: {e}"),
        }
    }
}

/// Consecutive connect failures tolerated before giving up on a meeting.
pub const MAX_CONSECUTIVE_CONNECT_FAILURES: u32 = 6;

/// Connect and stream until `pcm_rx` closes (graceful) or the socket errors.
///
/// - `pcm_rx`: 16 kHz PCM16 LE byte frames (mono or interleaved stereo). It is
///   borrowed so a reconnecting caller can keep the same receiver.
/// - `processor`: speaker identity + segmentation state for this meeting.
/// - `time_offset`: seconds to add to socket-relative times.
/// - `on_connected`: invoked once the handshake succeeds.
pub async fn connect_and_stream(
    cfg: &DeepgramConfig,
    auth: &Auth,
    pcm_rx: &mut Receiver<Vec<u8>>,
    events_tx: &Sender<TranscriptEvent>,
    processor: &mut Processor,
    time_offset: f64,
    on_connected: &mut (dyn FnMut() + Send),
) -> Result<(), StreamError> {
    let url = build_ws_url(cfg);
    let mut request = url
        .into_client_request()
        .map_err(|e| StreamError::Connect(format!("bad deepgram url: {e}")))?;
    request.headers_mut().insert(
        "Authorization",
        auth.header_value()
            .parse()
            .map_err(|_| StreamError::Connect("invalid auth header".into()))?,
    );

    let (ws, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| StreamError::Connect(describe_connect_error(e)))?;
    on_connected();
    let (mut write, mut read) = ws.split();

    let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let forward = |txt: &str, processor: &mut Processor| -> Result<bool, StreamError> {
        // Returns Ok(true) when the message was a trailing Metadata frame.
        match parse_message(txt) {
            Ok(ServerMessage::Results(res)) => {
                for ev in processor.process(&res, time_offset) {
                    // Receiver gone = UI/persistence shut down; treat as closed.
                    if events_tx.try_send(ev).is_err() {
                        tracing::warn!("transcript event channel full or closed; dropping");
                    }
                }
                Ok(false)
            }
            Ok(ServerMessage::Metadata(_)) => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => {
                tracing::debug!("unparsed deepgram frame: {e}");
                Ok(false)
            }
        }
    };

    loop {
        tokio::select! {
            maybe_pcm = pcm_rx.recv() => {
                match maybe_pcm {
                    Some(bytes) => {
                        if let Err(e) = write.send(Message::Binary(bytes)).await {
                            return Err(StreamError::Transport(format!("send failed: {e}")));
                        }
                    }
                    None => {
                        // Upstream finished: ask Deepgram to flush, then drain.
                        let _ = write.send(Message::Text(close_stream_message())).await;
                        let deadline = tokio::time::Instant::now() + CLOSE_DRAIN_TIMEOUT;
                        loop {
                            let next = tokio::time::timeout_at(deadline, read.next()).await;
                            match next {
                                Ok(Some(Ok(Message::Text(txt)))) => {
                                    if forward(&txt, processor)? {
                                        break; // Metadata = flush complete
                                    }
                                }
                                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
                                Ok(Some(Ok(_))) => {}
                                Ok(Some(Err(e))) => {
                                    tracing::debug!("read during close drain: {e}");
                                    break;
                                }
                                Err(_) => {
                                    tracing::info!("close drain timed out after {:?}", CLOSE_DRAIN_TIMEOUT);
                                    break;
                                }
                            }
                        }
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            _ = keepalive.tick() => {
                if let Err(e) = write.send(Message::Text(keepalive_message())).await {
                    return Err(StreamError::Transport(format!("keepalive failed: {e}")));
                }
            }
            maybe_msg = read.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(txt))) => { forward(&txt, processor)?; }
                    Some(Ok(Message::Close(frame))) => {
                        return Err(StreamError::Transport(format!("server closed: {frame:?}")));
                    }
                    None => return Err(StreamError::Transport("socket ended".into())),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(StreamError::Transport(format!("read error: {e}"))),
                }
            }
        }
    }
}

/// Include Deepgram's HTTP status and error body on handshake rejections so a
/// 400/401 says *why* (bad query param, expired grant, …).
fn describe_connect_error(e: tokio_tungstenite::tungstenite::Error) -> String {
    use tokio_tungstenite::tungstenite::Error;
    match e {
        Error::Http(resp) => {
            let status = resp.status();
            let body = resp
                .body()
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).trim().to_string())
                .filter(|b| !b.is_empty())
                .unwrap_or_default();
            if body.is_empty() {
                format!("HTTP {status}")
            } else {
                format!("HTTP {status}: {body}")
            }
        }
        other => other.to_string(),
    }
}

/// Produces a credential for each (re)connect. Managed mode refreshes the
/// session grant when it is near expiry; BYOK returns the same key.
pub type AuthProvider = Box<
    dyn FnMut() -> Pin<Box<dyn Future<Output = Result<Auth, String>> + Send>> + Send,
>;

/// Drive a meeting's transcription: connect, stream, reconnect on transport
/// drops, and finish gracefully when `pcm_rx` closes. Status changes are
/// reported on `status_tx` (best effort).
pub async fn run_stream(
    cfg: DeepgramConfig,
    mut auth_provider: AuthProvider,
    mut pcm_rx: Receiver<Vec<u8>>,
    events_tx: Sender<TranscriptEvent>,
    status_tx: Sender<StreamStatus>,
    mut processor: Processor,
    meeting_started: Instant,
) {
    let mut backoff = Backoff::default();
    let mut consecutive_connect_failures = 0u32;
    let mut first = true;

    loop {
        let _ = status_tx.try_send(StreamStatus::Connecting);
        let auth = match auth_provider().await {
            Ok(a) => a,
            Err(e) => {
                let _ = status_tx.try_send(StreamStatus::Failed { reason: e });
                return;
            }
        };

        processor.begin_connection(!first);
        // First socket: Deepgram time ≈ meeting time. Reconnects: wall-clock
        // recording duration at connect time (per macOS audio.md).
        let time_offset = if first {
            0.0
        } else {
            meeting_started.elapsed().as_secs_f64()
        };
        first = false;

        let generation = backoff.generation;
        let status_for_connect = status_tx.clone();
        let mut on_connected = move || {
            let _ = status_for_connect.try_send(StreamStatus::Connected { generation });
        };

        match connect_and_stream(
            &cfg,
            &auth,
            &mut pcm_rx,
            &events_tx,
            &mut processor,
            time_offset,
            &mut on_connected,
        )
        .await
        {
            Ok(()) => {
                let _ = status_tx.try_send(StreamStatus::Ended);
                return;
            }
            Err(StreamError::Connect(reason)) => {
                consecutive_connect_failures += 1;
                if consecutive_connect_failures >= MAX_CONSECUTIVE_CONNECT_FAILURES {
                    tracing::error!("deepgram: giving up after {consecutive_connect_failures} connect failures: {reason}");
                    let _ = status_tx.try_send(StreamStatus::Failed { reason });
                    return;
                }
                let delay = backoff.next_delay();
                tracing::warn!("deepgram connect failed ({reason}); retry in {delay:?}");
                let _ = status_tx.try_send(StreamStatus::Reconnecting {
                    attempt: backoff.attempt(),
                    reason,
                });
                tokio::time::sleep(delay).await;
            }
            Err(StreamError::Transport(reason)) => {
                if pcm_rx.is_closed() {
                    // Stop raced the drop; nothing more to send.
                    let _ = status_tx.try_send(StreamStatus::Ended);
                    return;
                }
                consecutive_connect_failures = 0;
                backoff.reset();
                let delay = backoff.next_delay();
                tracing::warn!("deepgram transport dropped ({reason}); reconnecting in {delay:?}");
                let _ = status_tx.try_send(StreamStatus::Reconnecting {
                    attempt: backoff.attempt(),
                    reason,
                });
                tokio::time::sleep(delay.min(Duration::from_secs(30))).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_with_state_tag() {
        let s = serde_json::to_value(StreamStatus::Reconnecting {
            attempt: 2,
            reason: "x".into(),
        })
        .unwrap();
        assert_eq!(s["state"], "reconnecting");
        assert_eq!(s["attempt"], 2);
        assert_eq!(serde_json::to_value(StreamStatus::Ended).unwrap()["state"], "ended");
    }
}
