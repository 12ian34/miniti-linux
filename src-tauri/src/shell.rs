//! Desktop shell: tray icon with elapsed timer and Smart-meeting actions, the
//! non-activating floating recording surface, desktop notifications (used
//! when miniti is not frontmost or the surface is off), deep-link handling
//! for OAuth returns, and close-to-tray so miniti stays reachable after the
//! main window is closed (port of the macOS menu bar / NSPanel behaviour).

use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};
#[cfg(not(target_os = "linux"))]
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
    let open = MenuItemBuilder::with_id("open", "Open miniti").build(app)?;
    let decision = MenuItemBuilder::with_id("decision", "")
        .enabled(false)
        .build(app)?;
    let decision_primary = MenuItemBuilder::with_id("decision_primary", "").build(app)?;
    let decision_secondary = MenuItemBuilder::with_id("decision_secondary", "").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit miniti").build(app)?;
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
        .tooltip("miniti");
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
            .set_tooltip(Some(format!("miniti — recording {title}")));
        let _ = h.status.set_text(match stream_state {
            Some("reconnecting") => format!("Recording {title} · reconnecting…"),
            Some("failed") => format!("Recording {title} · transcription failed"),
            _ => format!("Recording {title}"),
        });
        let _ = h.toggle.set_text("Stop meeting");
    } else {
        let _ = h.tray.set_title(None::<String>);
        let _ = h.tray.set_tooltip(Some("miniti"));
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
/// undecorated, not in the taskbar, never takes keyboard focus (so clicking
/// "end meeting" does not pull focus from the call). Default position is the
/// top-right corner with a 16 px inset (macOS `positionInDefaultCorner`); the
/// webview restores a user-moved position from its own storage and sizes the
/// window to its content. On Wayland the surface becomes a layer-shell overlay
/// when the compositor supports it (Hyprland, Sway, KDE); GNOME falls back to a
/// normal window whose placement the compositor decides.
pub const PRESENCE_W: f64 = 236.0;
pub const PRESENCE_H: f64 = 52.0;
pub const PRESENCE_MAX_W: f64 = 360.0;
pub const PRESENCE_MAX_H: f64 = 320.0;
const PRESENCE_MARGIN: f64 = 16.0;
const SCREEN_MARGIN: f64 = 8.0;

pub fn show_presence(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.show();
        return;
    }
    let url = WebviewUrl::App("index.html#presence".into());
    let mut builder = WebviewWindowBuilder::new(app, PRESENCE_LABEL, url)
        .title("miniti")
        .inner_size(PRESENCE_W, PRESENCE_H)
        .min_inner_size(200.0, 44.0)
        .decorations(false)
        .background_color(tauri::window::Color(0x09, 0x09, 0x0b, 0xff))
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .visible(false);
    // Top-right of the primary monitor, computed before the window exists so
    // it never flashes in the centre.
    if let Ok(Some(mon)) = app.primary_monitor() {
        let scale = mon.scale_factor();
        let size = mon.size();
        let pos = mon.position();
        let x = pos.x as f64 / scale + size.width as f64 / scale - PRESENCE_W - PRESENCE_MARGIN;
        let y = pos.y as f64 / scale + PRESENCE_MARGIN;
        builder = builder.position(x.max(0.0), y.max(0.0));
    }
    match builder.build() {
        Ok(w) => {
            #[cfg(target_os = "linux")]
            linux_surface::apply(&w);
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

/// Raise the surface for a decision and ask the UI to expand (never activates
/// the app). macOS `RecordingIndicatorWindowPolicy.shouldOrderFront`.
pub fn raise_presence(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PRESENCE_LABEL) {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
    let _ = app.emit("presence_attention", ());
}

/// A decision cleared: let the UI collapse if it auto-expanded.
pub fn shrink_presence(app: &AppHandle) {
    let _ = app.emit("presence_settle", ());
}

/// Whether the surface is a layer-shell overlay (position fixed by the
/// compositor, so drag and clamping are skipped).
pub fn presence_is_layer_surface() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_surface::is_layer_surface()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Webview-driven resize to fit content (the SwiftUI panel sizes to its ideal
/// size), then keep the whole window on its monitor with an 8 px margin.
#[tauri::command]
pub fn resize_presence(app: AppHandle, width: f64, height: f64) {
    let Some(w) = app.get_webview_window(PRESENCE_LABEL) else {
        return;
    };
    let size = tauri::LogicalSize::new(
        width.clamp(200.0, PRESENCE_MAX_W),
        height.clamp(44.0, PRESENCE_MAX_H),
    );
    let _ = w.set_size(size);
    if !presence_is_layer_surface() {
        clamp_to_monitor(&w, size);
    }
}

/// macOS `RecordingIndicatorGeometry.constrainedFrame`: never let the expanded
/// controls hang off the visible screen.
fn clamp_to_monitor(w: &tauri::WebviewWindow, size: tauri::LogicalSize<f64>) {
    let Ok(Some(mon)) = w.current_monitor() else {
        return;
    };
    let scale = mon.scale_factor();
    let Ok(pos) = w.outer_position() else { return };
    let (x, y) = (pos.x as f64 / scale, pos.y as f64 / scale);
    let (mx, my) = (
        mon.position().x as f64 / scale,
        mon.position().y as f64 / scale,
    );
    let (mw, mh) = (
        mon.size().width as f64 / scale,
        mon.size().height as f64 / scale,
    );
    let (min_x, min_y) = (mx + SCREEN_MARGIN, my + SCREEN_MARGIN);
    let max_x = (mx + mw - SCREEN_MARGIN - size.width).max(min_x);
    let max_y = (my + mh - SCREEN_MARGIN - size.height).max(min_y);
    let nx = x.clamp(min_x, max_x);
    let ny = y.clamp(min_y, max_y);
    if (nx - x).abs() > 0.5 || (ny - y).abs() > 0.5 {
        let _ = w.set_position(tauri::LogicalPosition::new(nx, ny));
    }
}

/// GTK-level behaviour for the floating surface on Linux.
#[cfg(target_os = "linux")]
mod linux_surface {
    use std::sync::atomic::{AtomicBool, Ordering};

    use gtk::prelude::*;

    static LAYER_SURFACE: AtomicBool = AtomicBool::new(false);

    pub fn is_layer_surface() -> bool {
        LAYER_SURFACE.load(Ordering::Relaxed)
    }

    /// Must run after `build()` (window exists, not yet realized because it
    /// was created hidden) and before `show()`.
    pub fn apply(w: &tauri::WebviewWindow) {
        let Ok(window) = w.gtk_window() else { return };
        // Utility windows are not focus targets for the WM and never appear in
        // alt-tab; keep-above + stick mirror the NSPanel floating level.
        window.set_type_hint(gtk::gdk::WindowTypeHint::Utility);
        window.set_accept_focus(false);
        window.set_focus_on_map(false);
        window.set_keep_above(true);
        window.stick();
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var("MINITI_NO_LAYER_SHELL").is_err()
        {
            match layer_shell::init(&window) {
                Ok(()) => {
                    LAYER_SURFACE.store(true, Ordering::Relaxed);
                    tracing::info!("floating surface: wlr-layer-shell overlay");
                }
                Err(e) => tracing::info!(
                    "floating surface: layer-shell unavailable ({e}); compositor decides placement"
                ),
            }
        }
    }

    /// Minimal dlopen binding to gtk-layer-shell so it stays an optional
    /// runtime dependency (`libgtk-layer-shell.so.0`). Enum values follow
    /// gtk-layer-shell.h: layers background=0 bottom=1 top=2 overlay=3; edges
    /// left=0 right=1 top=2 bottom=3; keyboard mode none=0.
    mod layer_shell {
        use gtk::glib::translate::ToGlibPtr;
        use gtk::prelude::*;
        use libloading::{Library, Symbol};

        type GtkWindowPtr = *mut gtk::ffi::GtkWindow;
        const LAYER_TOP: i32 = 2;
        const EDGE_RIGHT: i32 = 1;
        const EDGE_TOP: i32 = 2;
        const KEYBOARD_NONE: i32 = 0;

        pub fn init(window: &gtk::ApplicationWindow) -> Result<(), String> {
            // SAFETY: loading a well-known system library by soname; every
            // symbol is used with the C signature from gtk-layer-shell.h.
            unsafe {
                let lib = Library::new("libgtk-layer-shell.so.0").map_err(|e| e.to_string())?;
                let is_supported: Symbol<unsafe extern "C" fn() -> i32> = lib
                    .get(b"gtk_layer_is_supported\0")
                    .map_err(|e| e.to_string())?;
                if is_supported() == 0 {
                    return Err("compositor does not support wlr-layer-shell".into());
                }
                let init_for_window: Symbol<unsafe extern "C" fn(GtkWindowPtr)> = lib
                    .get(b"gtk_layer_init_for_window\0")
                    .map_err(|e| e.to_string())?;
                let set_layer: Symbol<unsafe extern "C" fn(GtkWindowPtr, i32)> = lib
                    .get(b"gtk_layer_set_layer\0")
                    .map_err(|e| e.to_string())?;
                let set_anchor: Symbol<unsafe extern "C" fn(GtkWindowPtr, i32, i32)> = lib
                    .get(b"gtk_layer_set_anchor\0")
                    .map_err(|e| e.to_string())?;
                let set_margin: Symbol<unsafe extern "C" fn(GtkWindowPtr, i32, i32)> = lib
                    .get(b"gtk_layer_set_margin\0")
                    .map_err(|e| e.to_string())?;
                let set_keyboard: Symbol<unsafe extern "C" fn(GtkWindowPtr, i32)> = lib
                    .get(b"gtk_layer_set_keyboard_mode\0")
                    .map_err(|e| e.to_string())?;
                let set_namespace: Result<
                    Symbol<unsafe extern "C" fn(GtkWindowPtr, *const std::os::raw::c_char)>,
                    _,
                > = lib.get(b"gtk_layer_set_namespace\0");
                let ptr: GtkWindowPtr = window.upcast_ref::<gtk::Window>().to_glib_none().0;
                init_for_window(ptr);
                if let Ok(set_namespace) = set_namespace {
                    set_namespace(ptr, c"miniti-presence".as_ptr());
                }
                set_layer(ptr, LAYER_TOP);
                set_anchor(ptr, EDGE_TOP, 1);
                set_anchor(ptr, EDGE_RIGHT, 1);
                set_margin(ptr, EDGE_TOP, super::super::PRESENCE_MARGIN as i32);
                set_margin(ptr, EDGE_RIGHT, super::super::PRESENCE_MARGIN as i32);
                set_keyboard(ptr, KEYBOARD_NONE);
                // Keep the library loaded for the life of the process.
                std::mem::forget(lib);
            }
            Ok(())
        }
    }
}

pub fn main_is_focused(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN_LABEL)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false)
}

/// Desktop notification. On Linux it goes straight to
/// org.freedesktop.Notifications with the `desktop-entry` hint, so the
/// desktop attributes it to miniti (per-app settings, muting, the app icon);
/// elsewhere the Tauri plugin does the job.
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        let result = notify_rust::Notification::new()
            .appname("miniti")
            .summary(title)
            .body(body)
            .icon("miniti")
            .hint(notify_rust::Hint::DesktopEntry("miniti".into()))
            .show();
        if let Err(e) = result {
            tracing::info!("notification unavailable: {e}");
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        if let Err(e) = app.notification().builder().title(title).body(body).show() {
            tracing::info!("notification unavailable: {e}");
        }
    }
}

/// A Smart-meeting prompt as a notification. On Linux the notification
/// carries the prompt's buttons as actions (and "join and take notes" when
/// the event has a call link); clicking the body counts as the primary
/// choice, the way the Apple reminder works. The choice is routed back
/// through the same decision path as the surface buttons. Servers without
/// action support (some minimal bars) still show the text.
pub fn notify_prompt(app: &AppHandle, prompt: &crate::smart::SmartPrompt) {
    #[cfg(target_os = "linux")]
    {
        let app = app.clone();
        let prompt = prompt.clone();
        std::thread::Builder::new()
            .name("miniti-notify-prompt".into())
            .spawn(move || {
                let mut n = notify_rust::Notification::new();
                n.appname("miniti")
                    .summary(&prompt.message)
                    .icon("miniti")
                    .hint(notify_rust::Hint::DesktopEntry("miniti".into()))
                    .timeout(notify_rust::Timeout::Milliseconds(25_000));
                if prompt.join_url.is_some() {
                    n.action("join", "Join and take notes");
                }
                n.action("primary", &prompt.primary).action("secondary", &prompt.secondary);
                if let Some(t) = &prompt.tertiary {
                    n.action("tertiary", t);
                }
                n.action("default", &prompt.primary);
                let handle = match n.show() {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::info!("notification unavailable: {e}");
                        return;
                    }
                };
                handle.wait_for_action(|action| {
                    let choice = match action {
                        "join" => "join",
                        "default" | "primary" => "primary",
                        "tertiary" => "tertiary",
                        "secondary" => "secondary",
                        _ => return, // dismissed or expired: the prompt stays on the surface
                    };
                    tracing::info!("notification action: {choice}");
                    let app = app.clone();
                    let choice = choice.to_string();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::state::decide_from_shell(&app, &choice).await {
                            tracing::info!("notification decision ignored: {e}");
                        }
                    });
                });
            })
            .ok();
    }
    #[cfg(not(target_os = "linux"))]
    {
        notify(
            app,
            "miniti",
            &format!("{} ({} / {})", prompt.message, prompt.primary, prompt.secondary),
        );
    }
}

/// Surface-aware delivery contract: when miniti is frontmost the floating
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

/// Schemes this app answers for (mirrors `plugins.deep-link` in tauri.conf.json).
pub const DEEP_LINK_SCHEMES: [&str; 3] = ["miniti-google", "miniti-attio", "miniti-twenty"];

/// Deliver one opened URL to the UI. Returns false for URLs that are not ours
/// (a second instance forwards argv here, so it must not trust it blindly).
pub fn handle_open_url(app: &AppHandle, raw: &str) -> bool {
    let Some(ev) = parse_deep_link(raw) else {
        return false;
    };
    if !DEEP_LINK_SCHEMES.contains(&ev.scheme.as_str()) {
        return false;
    }
    tracing::info!("deep link: {}://{}", ev.scheme, ev.host);
    let _ = app.emit("deep_link", &ev);
    show_main(app);
    true
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
            handle_open_url(&handle, u.as_str());
        }
    });
}

// ---- Launch at login (XDG autostart) -------------------------------------------

/// `~/.config/autostart/miniti.desktop` (honours `XDG_CONFIG_HOME`).
pub fn autostart_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("autostart")
        .join("miniti.desktop")
}

/// Desktop entry that starts miniti to the tray. `exec` is the absolute
/// binary path so it works for tarball installs outside `$PATH`.
pub fn autostart_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=miniti\n\
         Comment=AI meeting assistant\n\
         Exec={exec} --hidden\n\
         Icon=miniti\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n\
         X-KDE-autostart-after=panel\n"
    )
}

pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
    let path = autostart_path();
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    let exe = std::env::current_exe()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, autostart_entry(&exec_quote(&exe.to_string_lossy())))
}

/// Quote one Exec argument as the desktop entry spec requires: reserved
/// characters force double quotes, four of them are backslash-escaped inside,
/// and a literal `%` is written `%%` (field codes).
pub fn exec_quote(arg: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    let escaped_percent = arg.replace('%', "%%");
    if !arg.contains(RESERVED) {
        return escaped_percent;
    }
    let mut out = String::from("\"");
    for c in escaped_percent.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

pub fn autostart_enabled() -> bool {
    std::fs::read_to_string(autostart_path())
        .map(|s| !s.lines().any(|l| l.trim() == "Hidden=true"))
        .unwrap_or(false)
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

    #[test]
    fn autostart_entry_starts_hidden_with_the_given_binary() {
        let e = autostart_entry("/usr/bin/miniti");
        assert!(e.starts_with("[Desktop Entry]\n"));
        assert!(e.contains("Exec=/usr/bin/miniti --hidden\n"));
        assert!(e.contains("X-GNOME-Autostart-enabled=true"));
        assert!(!e.contains("Hidden=true"));
    }

    #[test]
    fn exec_quoting_follows_the_desktop_entry_spec() {
        assert_eq!(exec_quote("/usr/bin/miniti"), "/usr/bin/miniti");
        assert_eq!(exec_quote("/opt/my apps/miniti"), "\"/opt/my apps/miniti\"");
        assert_eq!(exec_quote("/home/ian/it's/miniti"), "\"/home/ian/it's/miniti\"");
        assert_eq!(exec_quote("/x/a(b)/miniti"), "\"/x/a(b)/miniti\"");
        assert_eq!(exec_quote("/x/say \"hi\"/m"), "\"/x/say \\\"hi\\\"/m\"");
        assert_eq!(exec_quote("/x/$HOME`/m"), "\"/x/\\$HOME\\`/m\"");
        assert_eq!(exec_quote("/x/100%/miniti"), "/x/100%%/miniti");
    }
}
