//! Keep the machine awake while a meeting records. Tries the widely
//! implemented `org.freedesktop.ScreenSaver` inhibit (KDE, GNOME's legacy
//! proxy, hypridle, swayidle) and falls back to the desktop portal
//! (`org.freedesktop.portal.Inhibit`), which every portal-backed desktop has.
//! Best effort: without a session bus (or on a dev box) recording continues
//! uninhibited and a single log line says so.

use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::Connection;

const REASON: &str = "Recording a meeting";

/// Both inhibit mechanisms are tied to the bus connection that asked
/// (cookies are dropped on `NameOwnerChanged`, portal requests close with the
/// sender), so the connection lives here for as long as the inhibit does.
#[derive(Default)]
pub struct Inhibitor {
    conn: Option<Connection>,
    screensaver_cookie: Option<u32>,
    portal_request: Option<OwnedObjectPath>,
}

impl Inhibitor {
    pub fn active(&self) -> bool {
        self.screensaver_cookie.is_some() || self.portal_request.is_some()
    }

    pub async fn acquire(&mut self, conn: Connection) {
        if self.active() {
            return;
        }
        match screensaver_inhibit(&conn).await {
            Ok(cookie) => {
                self.screensaver_cookie = Some(cookie);
                self.conn = Some(conn);
                tracing::info!("idle inhibit: org.freedesktop.ScreenSaver (cookie {cookie})");
                return;
            }
            Err(e) => tracing::debug!("ScreenSaver inhibit unavailable: {e}"),
        }
        match portal_inhibit(&conn).await {
            Ok(handle) => {
                tracing::info!("idle inhibit: desktop portal ({handle})");
                self.portal_request = Some(handle);
                self.conn = Some(conn);
            }
            Err(e) => tracing::info!("idle inhibit unavailable ({e}); the session may sleep mid-meeting"),
        }
    }

    pub async fn release(&mut self) {
        let Some(conn) = self.conn.take() else {
            self.screensaver_cookie = None;
            self.portal_request = None;
            return;
        };
        if let Some(cookie) = self.screensaver_cookie.take() {
            if let Err(e) = screensaver_uninhibit(&conn, cookie).await {
                tracing::debug!("ScreenSaver uninhibit: {e}");
            }
        }
        if let Some(handle) = self.portal_request.take() {
            if let Err(e) = portal_close(&conn, &handle).await {
                tracing::debug!("portal inhibit close: {e}");
            }
        }
    }
}

async fn screensaver_inhibit(conn: &Connection) -> zbus::Result<u32> {
    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    )
    .await?;
    proxy.call("Inhibit", &("miniti", REASON)).await
}

async fn screensaver_uninhibit(conn: &Connection, cookie: u32) -> zbus::Result<()> {
    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    )
    .await?;
    proxy.call::<_, _, ()>("UnInhibit", &(cookie,)).await
}

/// Portal flag bit for "idle" (the spec: 1 logout, 2 user switch, 4 suspend, 8 idle).
const PORTAL_INHIBIT_IDLE: u32 = 8;

async fn portal_inhibit(conn: &Connection) -> zbus::Result<OwnedObjectPath> {
    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Inhibit",
    )
    .await?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("reason", Value::from(REASON));
    proxy
        .call("Inhibit", &("", PORTAL_INHIBIT_IDLE, options))
        .await
}

async fn portal_close(conn: &Connection, handle: &OwnedObjectPath) -> zbus::Result<()> {
    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        handle.as_ref(),
        "org.freedesktop.portal.Request",
    )
    .await?;
    proxy.call::<_, _, ()>("Close", &()).await
}

/// Follow the recording state: inhibit while recording, release otherwise.
pub async fn follow(app: &tauri::AppHandle, recording: bool) {
    let Some(hub) = super::snapshot::hub(app) else { return };
    let mut inhibitor = hub.inhibit.lock().await;
    if !recording {
        inhibitor.release().await;
        return;
    }
    // Prefer the interface's connection; without it (name taken, or the
    // first tick raced the D-Bus setup) open one that the inhibitor keeps.
    let conn = hub.dbus.lock().ok().and_then(|c| c.clone());
    let conn = match conn {
        Some(c) => c,
        None => match Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                tracing::info!("idle inhibit unavailable (no session bus: {e})");
                return;
            }
        },
    };
    inhibitor.acquire(conn).await;
}
