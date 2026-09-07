# desktop integration

How miniti behaves as a Linux desktop citizen, and every door a script, bar,
key daemon or another app can use to talk to it. The rule for all of it: one
state model, several transports, identical semantics.

## the `miniti` command line

The desktop app and the CLI are the same binary. With no command, `miniti`
opens the app; if one is already running it is raised instead, and any
`miniti-google://…` style link on the command line is delivered to it (this is
also the single-instance guard: an OAuth return no longer opens a second copy).

```
miniti status [--waybar]      recording state, elapsed, questions, pending prompt
miniti start [TITLE]          start a meeting (launches the app if needed)
miniti stop                   stop and save
miniti toggle                 start if idle, stop if recording
miniti show | open [ID]       raise the window, optionally on a meeting (`last`)
miniti questions              questions worth asking right now
miniti meetings [-n N]        recent meetings
miniti export [ID] [-o FILE]  Markdown export (`last` by default)
miniti decide primary|secondary|tertiary
miniti watch                  stream state changes (JSON lines with --json)
miniti quit
miniti autostart [on|off]     launch at login
miniti paths                  where everything lives
miniti completions SHELL      bash | zsh | fish | elvish
```

`--json` on any command prints machine-readable output. Exit codes: 0 ok,
1 the app refused, 2 usage, 3 miniti is not running. `man miniti` has the
full reference; packages install completions for bash, zsh and fish.

## where things live (XDG base directories)

| What | Path |
|---|---|
| Preferences | `$XDG_CONFIG_HOME/miniti/prefs.json` (`~/.config/miniti`) |
| Meetings database, device id | `$XDG_DATA_HOME/miniti/` (`~/.local/share/miniti`) |
| Logs (seven rolling days) | `$XDG_STATE_HOME/miniti/logs/` (`~/.local/state/miniti/logs`) |
| Control socket, live state | `$XDG_RUNTIME_DIR/miniti/` (per session, owner-only) |
| Launch-at-login entry | `$XDG_CONFIG_HOME/autostart/miniti.desktop` |
| Account credentials, device id | the secret service (GNOME Keyring, KWallet) with a file fallback |

`miniti paths` prints the resolved locations on the current machine.

## the control socket

`$XDG_RUNTIME_DIR/miniti/miniti.sock` (override with `MINITI_SOCKET`). One
JSON object per line in each direction:

```
{"cmd":"status"}
{"cmd":"start","title":"Weekly sync"}
{"cmd":"stop"}   {"cmd":"toggle"}   {"cmd":"show"}   {"cmd":"quit"}   {"cmd":"ping"}
{"cmd":"open-meeting","id":"last"}
{"cmd":"open-url","url":"miniti-google://oauth-callback?status=success"}
{"cmd":"questions"}   {"cmd":"meetings","limit":20}   {"cmd":"meeting","id":"…"}
{"cmd":"export","id":"last"}
{"cmd":"decide","choice":"primary"}
{"cmd":"subscribe"}
```

Replies are `{"ok":true,"data":…}` or `{"ok":false,"error":"…"}`. `subscribe`
turns the connection into a stream: the current snapshot at once, then one
line per change. From a shell:

```bash
printf '{"cmd":"status"}\n' | socat - UNIX-CONNECT:"$XDG_RUNTIME_DIR/miniti/miniti.sock"
```

## the state file

`$XDG_RUNTIME_DIR/miniti/state.json` holds the latest snapshot and is replaced
atomically whenever something changes (once a second while recording, since
the elapsed time is part of it; otherwise only on real changes). Watch it with
inotify or a Quickshell `FileView` when holding a socket open is inconvenient.
Both files are removed when miniti exits.

Snapshot shape (`schema` 1):

```json
{
  "schema": 1, "app_version": "0.5.0", "pid": 4242, "updated_at": 1788770000,
  "is_recording": true, "elapsed_seconds": 754.2, "elapsed_text": "12:34",
  "meeting_id": "…", "meeting_title": "Weekly sync",
  "lifecycle_status": "Zoom call active", "call_app_name": "Zoom",
  "transcription_status": "Transcription: live", "stream_state": "connected",
  "audio_status": "Audio: healthy", "audio_health": "healthy",
  "grace_remaining_seconds": null, "grace_app_name": null,
  "questions": [{"question": "Who owns the budget decision?", "type": "clarify", "context": "…", "priority": "high"}],
  "summary": "…rolling summary…",
  "prompt": {"id": "3", "kind": "ending", "message": "Zoom call ended", "primary": "Keep recording", "secondary": "End now", "countdown": 8},
  "nudge": {"kind": "question", "title": "…", "body": "…", "at": 1788770000}
}
```

`prompt` is the Smart-meeting decision currently shown in the tray and on the
floating surface (null when none); answer it with `decide`. `nudge` is the
latest live-guidance hint and clears when the meeting stops.

## D-Bus

Bus name `com.miniti.linux`, object `/com/miniti/linux`, interface
`com.miniti.linux.Control`:

| Member | Signature | Meaning |
|---|---|---|
| `Status()` | `→ s` | snapshot JSON |
| `Start(title)` | `s → s` | meeting id |
| `Stop()` | `→ s` | saved meeting id |
| `Toggle()` | `→ b` | new recording state |
| `Show()` | | raise the window |
| `OpenMeeting(id)` | `s` | raise on a meeting (`last`) |
| `Decide(choice)` | `s` | primary / secondary / tertiary |
| `Recording`, `ElapsedSeconds`, `MeetingTitle`, `Version` | properties | |
| `StateChanged(state)` | signal, `s` | snapshot JSON on every change |

```bash
busctl --user call com.miniti.linux /com/miniti/linux com.miniti.linux.Control Toggle
gdbus monitor --session --dest com.miniti.linux
```

The bus name is not activatable: it exists while the app runs.

## staying awake

While a meeting records, miniti asks the session not to idle or sleep: first
through `org.freedesktop.ScreenSaver` (KDE, hypridle, swayidle, GNOME's legacy
proxy), then through the desktop portal's `Inhibit` interface. The inhibit is
released when the meeting stops.

## launch at login

Settings → General → "Launch at login", or `miniti autostart on`, writes a
standard XDG autostart entry that runs `miniti --hidden`: the app starts into
the tray without showing a window.

## Waybar

```jsonc
"custom/miniti": {
  "exec": "miniti status --waybar",
  "return-type": "json",
  "interval": 1,
  "on-click": "miniti toggle",
  "on-click-right": "miniti show",
  "on-click-middle": "miniti decide primary"
}
```

The module hides itself when nothing records (`text` is empty) and carries the
elapsed time, the meeting title and the transcription state in the tooltip.
`class` is `idle`, `recording`, `recording degraded` (transcription
reconnecting) or `stopped` (app not running), for CSS such as
`#custom-miniti.recording { color: #3fb950; }`.

## Hyprland, Sway and other keybinds

```
bind = SUPER, M, exec, miniti toggle
bind = SUPER SHIFT, M, exec, miniti show
bind = SUPER, Y, exec, miniti decide primary
```

Any launcher that runs a command works the same way. The floating recording
surface is a `wlr-layer-shell` overlay with namespace `miniti-presence` on
compositors that support it, so layer rules can target it.

## Omarchy

The app installs from the Omarchy package repository (`sudo pacman -S
miniti-bin`). A bar widget with a live panel (timer, questions worth asking,
the pending prompt, start/stop) lives in its own repository, `miniti-omarchy`,
because the Omarchy marketplace indexes one plugin per repository. It reads the
state file above and drives the app through the CLI.

## desktop files installed by the packages

| File | Purpose |
|---|---|
| `/usr/share/applications/miniti.desktop` | launcher entry with start / stop / toggle actions and the OAuth scheme handlers |
| `/usr/share/metainfo/com.miniti.linux.metainfo.xml` | AppStream metadata for software centres |
| `/usr/share/icons/hicolor/{32x32,128x128,256x256@2,512x512}/apps/miniti.png` | raster icons |
| `/usr/share/icons/hicolor/scalable/apps/miniti.svg` | scalable icon |
| `/usr/share/icons/hicolor/symbolic/apps/miniti-symbolic.svg` | monochrome icon for status areas |
| `/usr/share/man/man1/miniti.1` | manual |
| `/usr/share/{bash-completion/completions,zsh/site-functions,fish/vendor_completions.d}` | shell completions |

The Wayland `app_id` and X11 `WM_CLASS` are `miniti`, matching the desktop
file name, so window rules and dock icons resolve. CI validates the desktop
entry with `desktop-file-validate` and the metadata with `appstreamcli`.
