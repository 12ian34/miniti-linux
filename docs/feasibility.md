# Linux rewrite feasibility

Investigation snapshot (2026-09-02, decisions locked 2026-09-05). Goal: **macOS feature-complete** desktop rewrite in **Tauri 2 + Rust**, shipped as a **binary tarball + AUR** — not a Swift port, not Flatpak/AppImage.

## Verdict

| | |
|---|---|
| Nature | Full rewrite |
| Realistic parity | ~70–85% in year one |
| Effort | ~9–15 months (1–2 engineers) after green spike |
| Hardest gaps | PipeWire system audio reliability; call-app mic detection without Core Audio HAL |
| Stack | Tauri 2 + Rust (Qt only if spike fails) |
| Ship | GitHub Releases binary + AUR |

True 100% parity is blocked by platform physics (distro audio matrix + no Process Tap / HAL twin), not by missing insight screens.

## Apple surface that does not move

Roughly ~41k LOC of macOS/shared Swift. Strongly platform-bound (~14k+): `AudioCaptureService` (Process Tap), `NSPanel` / menu bar, TextKit transcript pane, CallActivityMonitor, Sparkle, Keychain bookmarks, SwiftUI chrome. Portable contracts (~15–25k equivalent): Deepgram, insights/API, Coaching math, call *policy*, webhooks, MCP, meeting schema, echo/diarization algorithms once PCM exists. `AppState` (~10k) is hybrid — split into domain services + OS adapters here.

## Feature map

| Feature | Parity | Difficulty | Linux path |
|---|---|---|---|
| Mic → 16 kHz PCM16 | full | easy | PipeWire / ALSA |
| System / loopback audio | partial | hard | PipeWire monitor or per-app nodes |
| Dual-channel + echo reconcile | near | hard | Port Apple algorithms once both streams exist |
| Deepgram live + naming | full | medium | Reimplement service contract |
| Insights / Sales / Questions / Playbook | full | medium | Same `/api/insights` + Docs MCP |
| Coaching metrics + overview | full | medium | Port local math; UI cost |
| Smart meetings call detection | partial | hard | PipeWire clients + known apps; policy portable |
| Quiet / calendar Smart prompts | near | medium | iOS-style fallbacks |
| Floating indicator + tray | near | medium | Tauri tray + always-on-top; Wayland quirks |
| Shortcuts / notifications | near | medium | In-app first; libnotify / portals |
| Calendar + Attio / Twenty | full | medium | Deep links + backend OAuth |
| History / trim / Granola / webhooks | full | medium | SQLite + file pickers |
| Markdown export + codebase investigate | near | medium | POSIX paths (easier than sandbox bookmarks) |
| Long selectable live transcript | near | hard | Virtualized editor / dedicated perf work |
| Managed Free / Pro / BYOK | full | easy | Polar web rail + `X-Platform: linux` |
| Updates | near | medium | Binary tarball + AUR; `/api/version` force gate |

## Hard gaps (document, don’t fake)

1. **System audio** — macOS `AudioHardwareCreateProcessTap` (“System Audio Recording Only”). Linux: PipeWire monitor / per-app capture (OBS-class). Expect a supported-distro matrix; native binary+AUR avoids Flatpak capture friction but not BT clock / Pulse leftover issues.
2. **Call detection** — no `kAudioProcessPropertyIsRunningInput`. Approximate; quiet + calendar cover the rest.
3. **Prebuilt glibc** — build release binaries on Ubuntu 22.04-class images. Arch source AUR builds ignore this; `-bin` users on older glibc do not.
4. **Transcript perf** — TextKit 2 role must be re-solved for multi-hour meetings in webview or a native pane.

## Stack choices

| Candidate | Fit |
|---|---|
| Tauri 2 + Rust + web UI | **Chosen** |
| Qt / C++ | Fallback if spike fails |
| Electron | Rejected |
| Swift on Linux | Rejected |

## Phases (calendar)

0. Spike 2–3w — PipeWire dual capture + Deepgram + Tauri shell (Arch + Ubuntu 22.04 binary)  
1. Core meeting 8–12w  
2. Insights parity 6–8w  
3. Desktop shell 6–10w  
4. Smart meetings 6–10w  
5. Ship hardening 4–6w — tarball CI, AUR PKGBUILDs, CRM/export/perf  

## What “complete” means on Linux

**Commit to ship:** mic + best-effort system audio on PipeWire desktops; full insight suite; history; Coaching; Calendar; CRM; Polar Pro; BYOK; webhooks; Docs MCP; Granola; markdown export; tray + floating presence; Smart meetings with call-sensor where available.

**Document as limited:** guaranteed Process-Tap-class capture everywhere; FaceTime-class call HAL; silent updates without pacman/GitHub; TextKit-identical selection with zero extra work.

## Sources

- `../miniti` AGENTS.md, docs/architecture.md, docs/audio.md, docs/monetization.md, docs/call-lifecycle-and-recording-presence-plan.md  
- `../miniti/ANDROID_PLAN.md` (API/schema patterns; Linux parity is *wider* than Android v1)  
- Tauri v2 AUR / Debian distribute docs  
- Investigation canvas (Cursor) locked Tauri + binary/AUR 2026-09-05  
