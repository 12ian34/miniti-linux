//! Desktop shell: tray icon with elapsed timer and Smart-meeting actions, the
//! non-activating floating recording surface, desktop notifications (used
//! when Miniti is not frontmost or the surface is off), deep-link handling
//! for OAuth returns, and close-to-tray so Miniti stays reachable after the
//! main window is closed (port of the macOS menu bar / NSPanel behaviour).

use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_notification::NotificationExt;

pub const PRESENCE_LABEL: &str = "presence";
pub const MAIN_LABEL: &str = "main";

pub struct TrayHandles {
    pub tray: TrayIcon<Wry>,
    pub status: MenuItem<Wry>,
    pub toggle: MenuItem<Wry>,
    pub decision: MenuItem<Wry>,
    pub decision_primary: MenuItem<Wry>,
    pub decision_secondary: MenuItem<Wry>,
}

pub type TraySlot = Mutex<Option<TrayHandles>>;

fn build_menu(app: &AppHandle) -> Result<(Menu<Wry>, TrayHandles, TrayIcon<Wry>), tauri::Error> {
    let status = MenuItemBuilder::with_id("status", "Not recording")
        .enabled(false)
        .build(app)?;
    let toggle = MenuItemBuilder::with_id("toggle", "Start meeting").build(app)?;
    let open = MenuItemBuilder::with_id("open", "Open Miniti").build(app)?;
    let decision = MenuItemBuilder::with_id("decision", "")
        .enabled(false)
        .build(app)?;
    let decision_primary = MenuItemBuilder::with_id("decision_primary", "").build(app)?;
    let decision_secondary = MenuItemBuilder::with_id("decision_secondary", "").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Miniti").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&status)
        .item(&toggle)
        .item(&open)
        .separator()
        .item(&decision)
        .item(&decision_primary)
        .item(&decision_secondary)
        .separator()
        .item(&quit)
        .build()?;
    let mut builder = TrayIconBuilder::with_id("miniti")
        .menu(&menu)
        .tooltip("Miniti");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::state::toggle_recording_from_shell(app).await;
                });
            }
            "open" => show_main(app),
            "decision_primary" => {
                let _ = app.emit("smart_decision", "primary");
            }
            "decision_secondary" => {
                let _ = app.emit("smart_decision", "secondary");
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    // Decision rows start hidden.
    let _ = decision.set_enabled(false);
    let _ = decision_primary.set_enabled(false);
    let _ = decision_secondary.set_enabled(false);
    let handles = TrayHandles {
        tray: tray.clone(),
        status,
        toggle,
        decision,
        decision_primary,
        decision_secondary,
    };
    Ok((menu, handles, tray))
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Build the tray if enabled in prefs. Safe to call once at setup.
pub fn setup_tray(app: &AppHandle, enabled: bool) {
    if !enabled {
        return;
    }
    match build_menu(app) {
        Ok((_menu, handles, _tray)) => {
            if let Some(slot) = app.try_state::<TraySlot>() {
                if let Ok(mut s) = slot.lock() {
                    *s = Some(handles);
                }
            }
        }
        Err(e) => tracing::warn!("tray unavailable: {e}"),
    }
}

/// Reflect recording state in the tray: timer title, status row, toggle label.
pub fn update_tray(
    app: &AppHandle,
    recording: bool,
    elapsed_seconds: f64,
    stream_state: Option<&str>,
) {
    let Some(slot) = app.try_state::<TraySlot>() else {
        return;
    };
    let Ok(guard) = slot.lock() else { return };
    let Some(h) = guard.as_ref() else { return };
    if recording {
        let s = elapsed_seconds as u64;
        let title = if s >= 3600 {
            format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
        } else {
            format!("{}:{:02}", s / 60, s % 60)
        };
        let _ = h.tray.set_title(Some(format!("● {title}")));
        let _ = h
            .tray
            .set_tooltip(Some(format!("Miniti — recording {title}")));
        let _ = h.status.set_text(match stream_state {
            Some("reconnecting") => format!("Recording {title} · reconnecting…"),
            Some("failed") => format!("Recording {title} · transcription failed"),
            _ => format!("Recording {title}"),
        });
        let _ = h.toggle.set_text("Stop meeting");
    } else {
        let _ = h.tray.set_title(None::<String>);
        let _ = h.tray.set_tooltip(Some("Miniti"));
        let _ = h.status.set_text("Not recording");
        let _ = h.toggle.set_text("Start meeting");
    }
}

/// Show or clear a Smart-meeting decision in the tray menu.
pub fn set_tray_decision(app: &AppHandle, decision: Option<(&str, &str, &str)>) {
    let Some(slot) = app.try_state::<TraySlot>() else {
        return;
    };
    let Ok(guard) = slot.lock() else { return };
    let Some(h) = guard.as_ref() else { return };
    match decision {
        Some((title, primary, secondary)) => {
            let _ = h.decision.set_text(title);
            let _ = h.decision_primary.set_text(primary);
            let _ = h.decision_secondary.set_text(secondary);
            let _ = h.decision_primary.set_enabled(true);
            let _ = h.decision_secondary.set_enabled(true);
        }
        None => {
            let _ = h.decision.set_text("");
            let _ = h.decision_primary.set_text("");
            let _ = h.decision_secondary.set_text("");
            let _ = h.decision_primary.set_enabled(false);
            let _ = h.decision_secondary.set_enabled(false);
        }
    }
}

/// Create (once) and show the floating recording surface: always on top,
/// undecorated, not in the taskbar, never steals focus. Default position is
/// the top-right corner with a 16 px inset (macOS `positionInDefaultCorner`);
/// the webview restores a user-moved position from its own storage. Wayland
/// compositors may ignore positioning; that is the documented Linux limit.
pub const PRESENCE_W: f64 = 236.0;
pub const PRESENCE_H: f64 = 52.0;

pub fn show_presence(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.show();
        return;
    }
    let url = WebviewUrl::App("index.html#presence".into());
    let mut builder = WebviewWindowBuilder::new(app, PRESENCE_LABEL, url)
        .title("Miniti")
        .inner_size(PRESENCE_W, PRESENCE_H)
        .min_inner_size(200.0, 44.0)
        .decorations(false)
        .background_color(tauri::window::Color(0x09, 0x09, 0x0b, 0xff))
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false);
    // Top-right of the primary monitor, computed before the window exists so
    // it never flashes in the centre.
    if let Ok(Some(mon)) = app.primary_monitor() {
        let scale = mon.scale_factor();
        let size = mon.size();
        let pos = mon.position();
        let x = pos.x as f64 / scale + size.width as f64 / scale - PRESENCE_W - 16.0;
        let y = pos.y as f64 / scale + 16.0;
        builder = builder.position(x.max(0.0), y.max(0.0));
    }
    match builder.build() {
        Ok(w) => {
            let _ = w.show();
        }
        Err(e) => tracing::warn!("floating surface unavailable: {e}"),
    }
}

pub fn hide_presence(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.hide();
    }
}

/// Expand + raise the surface for a decision (never activates the app).
pub fn raise_presence(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.set_size(tauri::LogicalSize::new(360.0, 150.0));
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
}

pub fn shrink_presence(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.set_size(tauri::LogicalSize::new(PRESENCE_W, PRESENCE_H));
    }
}

/// Webview-driven resize (expand for decisions/nudges, collapse back).
#[tauri::command]
pub fn resize_presence(app: AppHandle, width: f64, height: f64) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.set_size(tauri::LogicalSize::new(
            width.clamp(200.0, 360.0),
            height.clamp(44.0, 260.0),
        ));
    }
}

pub fn main_is_focused(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN_LABEL)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false)
}

/// Desktop notification (libnotify / portal via the notification plugin).
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::info!("notification unavailable: {e}");
    }
}

/// Surface-aware delivery contract: when Miniti is frontmost the floating
/// surface / in-app UI carries the message; otherwise Notification Center.
pub fn deliver_guidance(app: &AppHandle, title: &str, body: &str, surface_enabled: bool) {
    if surface_enabled && main_is_focused(app) {
        return;
    }
    notify(app, title, body);
}

#[derive(Debug, Clone, Serialize)]
pub struct DeepLinkEvent {
    pub scheme: String,
    pub host: String,
    pub query: std::collections::HashMap<String, String>,
    pub url: String,
}

pub fn parse_deep_link(raw: &str) -> Option<DeepLinkEvent> {
    let u = url::Url::parse(raw).ok()?;
    Some(DeepLinkEvent {
        scheme: u.scheme().to_string(),
        host: u.host_str().unwrap_or_default().to_string(),
        query: u
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        url: raw.to_string(),
    })
}

/// Register `miniti-google://`, `miniti-attio://`, `miniti-twenty://` handlers
/// and forward opened URLs to the UI as `deep_link` events.
pub fn setup_deep_links(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = app.deep_link().register_all() {
            tracing::info!("deep link registration skipped: {e}");
        }
    }
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        for u in event.urls() {
            if let Some(ev) = parse_deep_link(u.as_str()) {
                tracing::info!("deep link: {}://{}", ev.scheme, ev.host);
                let _ = handle.emit("deep_link", &ev);
                show_main(&handle);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_links_parse_oauth_returns() {
        let ev = parse_deep_link("miniti-google://oauth-callback?status=success").unwrap();
        assert_eq!(ev.scheme, "miniti-google");
        assert_eq!(ev.host, "oauth-callback");
        assert_eq!(ev.query.get("status").map(String::as_str), Some("success"));
        let err = parse_deep_link(
            "miniti-attio://oauth-callback?status=error&message=denied%20by%20user",
        )
        .unwrap();
        assert_eq!(
            err.query.get("message").map(String::as_str),
            Some("denied by user")
        );
        assert!(parse_deep_link("not a url").is_none());
    }
}
