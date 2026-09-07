# linux citizenship roadmap

What a reference-quality Linux desktop app does, checked against the
freedesktop specs, the GNOME and KDE guidelines and distribution packaging
rules, and where miniti stands. The full research (41 items with spec links
and the exact requirement for each) is the "Miniti Linux Citizenship
Checklist" report from 2026-09-07; this file is the working summary.

Legend: **done** shipped (0.5.0 unless noted) · **open** not started ·
**decided** deliberately not doing, with the reason.

## identity and desktop entry

| Item | Status |
|---|---|
| Desktop entry with `Keywords`, `GenericName`, `StartupNotify`, `SingleMainWindow`, `TryExec`, `X-GNOME-UsesNotifications`, one main category | done |
| Desktop actions (start / stop / toggle) in the launcher menu | done |
| URL scheme handlers for the OAuth returns | done (0.2.0) |
| Reverse-DNS desktop file and `app_id` (`app.miniti.Miniti`) | decided: keep `miniti`. Wayland `app_id`, `WM_CLASS`, icon name and desktop file already agree; renaming touches the WebKit data dir, the keyring service and every user's dock pin for no visible gain outside Flathub, which is off the ship list |
| `DBusActivatable=true` + `org.freedesktop.Application` | open; needs the reverse-DNS rename first |
| `X-Purism-FormFactor`, portal `Background` autostart | decided: not relevant outside Flatpak / phones |

## metadata, icons, files

| Item | Status |
|---|---|
| AppStream metainfo, validated in CI | done |
| Screenshots in the metainfo | done, but they are the Mac app's; replace with Linux captures before the AppStream listing matters |
| hicolor PNG ladder incl. 48 and 64 px, `256x256` slot fixed | done |
| Scalable SVG and symbolic icon | done (drawn from the hexagon; a designed SVG master would be better) |
| Tray uses the symbolic icon with a recording variant | open; Tauri's tray API takes an image, not an icon name |
| Man page, bash/zsh/fish completions, Debian copyright file | done |
| Logs under `$XDG_STATE_HOME`, runtime files under `$XDG_RUNTIME_DIR` | done |

## process behaviour

| Item | Status |
|---|---|
| Single instance; deep links forwarded to the running app | done (via the control socket) |
| Forward `XDG_ACTIVATION_TOKEN` so the raised window gets focus under Mutter/KWin focus-stealing prevention | open |
| D-Bus control interface (`com.miniti.linux.Control`) with properties and a change signal | done |
| `PropertiesChanged`-driven bar widgets, Waybar module, Omarchy plugin | done (Waybar via CLI; Omarchy in `../miniti-omarchy`) |
| CLI: `--help`, `--version`, subcommands, `--json`, exit codes | done |
| Idle / sleep inhibit while recording | done (ScreenSaver, then portal) |
| SIGTERM / logout: stop and save inside the session manager's budget | done (4 s stop, then exit) |
| Launch at login (XDG autostart) | done |
| Notifications carry the `desktop-entry` hint so GNOME/KDE attribute and can mute them per app | done (Linux only, notify-rust) |
| Notification actions ("Stop") and `replaces_id` for one live bubble | open |
| Detect a StatusNotifierHost; skip close-to-tray when there is none (GNOME without the extension) | open |
| GlobalShortcuts portal for a toggle hotkey | decided: `miniti toggle` bound in the compositor covers Hyprland/Sway/KDE today |
| PipeWire stream properties (`application.id`, `media.role=Communication`) | open; needs a native PipeWire backend instead of `parec` |
| Follow portal colour scheme | decided: the app is dark only |

## packaging

| Item | Status |
|---|---|
| Both PKGBUILDs install the same file set; `alsa-lib` added; versioned `provides` | done |
| No `.install` / postinst (pacman hooks and dpkg triggers refresh caches) | done |
| `namcap` and `lintian` in CI | open (needs an Arch and a Debian job) |
| Flatpak | decided: not shipping (see AGENTS.md); Flathub also rejects the `.linux` id suffix |
| Native PipeWire capture (real device names, default-device following) | open, largest remaining item |

## how to verify on a real desktop

```
miniti status; miniti status --waybar
busctl --user introspect com.miniti.linux /com/miniti/linux
gdbus monitor --session --dest com.miniti.linux      # then start a meeting
systemd-inhibit --list                                # shows miniti while recording
xdg-open 'miniti-google://oauth-callback?status=success'   # raises the running app
miniti autostart on && cat ~/.config/autostart/miniti.desktop
desktop-file-validate /usr/share/applications/miniti.desktop
appstreamcli validate /usr/share/metainfo/com.miniti.linux.metainfo.xml
```
