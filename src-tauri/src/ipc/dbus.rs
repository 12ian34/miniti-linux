//! Session-bus interface `com.miniti.linux.Control` at `/com/miniti/linux`
//! (bus name `com.miniti.linux`): the desktop-native way for launchers, key
//! daemons and other apps to drive miniti without knowing the socket.
//!
//! ```text
//! busctl --user call com.miniti.linux /com/miniti/linux com.miniti.linux.Control Toggle
//! gdbus call --session -d com.miniti.linux -o /com/miniti/linux -m com.miniti.linux.Control.Status
//! ```
//!
//! Method semantics are exactly those of the socket protocol (`server::handle`).

use serde_json::Value;
use tauri::AppHandle;
use zbus::object_server::SignalEmitter;
use zbus::{fdo, interface, Connection};

use super::protocol::{Request, Response};

pub struct Control {
    app: AppHandle,
}

fn to_fdo(resp: Response) -> fdo::Result<Value> {
    if resp.ok {
        Ok(resp.data)
    } else {
        Err(fdo::Error::Failed(resp.error.unwrap_or_default()))
    }
}

#[interface(name = "com.miniti.linux.Control")]
impl Control {
    /// Current snapshot as JSON (same shape as `state.json`).
    async fn status(&self) -> String {
        serde_json::to_string(&super::snapshot::current(&self.app)).unwrap_or_default()
    }

    /// Start a meeting; returns the new meeting id. Empty title = automatic.
    async fn start(&self, title: &str) -> fdo::Result<String> {
        let title = (!title.trim().is_empty()).then(|| title.to_string());
        let data = to_fdo(super::server::handle(&self.app, Request::Start { title }).await)?;
        Ok(data["meeting_id"].as_str().unwrap_or_default().to_string())
    }

    /// Stop the meeting; returns the saved meeting id ("" when nothing ran).
    async fn stop(&self) -> fdo::Result<String> {
        let data = to_fdo(super::server::handle(&self.app, Request::Stop).await)?;
        Ok(data["meeting_id"].as_str().unwrap_or_default().to_string())
    }

    /// Start if idle, stop if recording; returns the new recording state.
    async fn toggle(&self) -> fdo::Result<bool> {
        let data = to_fdo(super::server::handle(&self.app, Request::Toggle).await)?;
        Ok(data["recording"].as_bool().unwrap_or(false))
    }

    /// Raise the main window.
    async fn show(&self) {
        let _ = super::server::handle(&self.app, Request::Show).await;
    }

    /// Raise the main window on a saved meeting (`last` = most recent).
    async fn open_meeting(&self, id: &str) -> fdo::Result<()> {
        to_fdo(super::server::handle(&self.app, Request::OpenMeeting { id: id.into() }).await)
            .map(|_| ())
    }

    /// Answer the pending Smart-meeting prompt: primary | secondary | tertiary.
    async fn decide(&self, choice: &str) -> fdo::Result<()> {
        to_fdo(super::server::handle(&self.app, Request::Decide { choice: choice.into() }).await)
            .map(|_| ())
    }

    #[zbus(property)]
    async fn recording(&self) -> bool {
        crate::state::build_presence(&self.app).is_recording
    }

    #[zbus(property)]
    async fn elapsed_seconds(&self) -> u32 {
        crate::state::build_presence(&self.app).elapsed_seconds as u32
    }

    #[zbus(property)]
    async fn meeting_title(&self) -> String {
        crate::state::build_presence(&self.app).meeting_title
    }

    #[zbus(property)]
    async fn version(&self) -> String {
        crate::state::APP_VERSION.to_string()
    }

    /// Emitted with the full snapshot JSON whenever it changes.
    #[zbus(signal)]
    async fn state_changed(emitter: &SignalEmitter<'_>, state: &str) -> zbus::Result<()>;
}

/// Claim the bus name and export the object. Fails quietly without a session
/// bus; the socket keeps working.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        match serve(app.clone()).await {
            Ok(conn) => {
                if let Some(hub) = super::snapshot::hub(&app) {
                    if let Ok(mut slot) = hub.dbus.lock() {
                        *slot = Some(conn);
                    }
                }
                tracing::info!("d-bus: {} at {}", super::BUS_NAME, super::OBJECT_PATH);
            }
            Err(e) => tracing::info!("d-bus interface unavailable: {e}"),
        }
    });
}

async fn serve(app: AppHandle) -> zbus::Result<Connection> {
    zbus::connection::Builder::session()?
        .name(super::BUS_NAME)?
        .serve_at(super::OBJECT_PATH, Control { app })?
        .build()
        .await
}

/// Push a snapshot to signal listeners and refresh the changed properties.
pub async fn broadcast(conn: &Connection, json: &str, recording_changed: bool) {
    let Ok(iface) = conn
        .object_server()
        .interface::<_, Control>(super::OBJECT_PATH)
        .await
    else {
        return;
    };
    let emitter = iface.signal_emitter();
    let _ = Control::state_changed(emitter, json).await;
    let guard = iface.get().await;
    if recording_changed {
        let _ = guard.recording_changed(emitter).await;
        let _ = guard.meeting_title_changed(emitter).await;
    }
    let _ = guard.elapsed_seconds_changed(emitter).await;
}
