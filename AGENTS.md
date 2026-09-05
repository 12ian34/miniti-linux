# Miniti Linux

Native Linux desktop client for [Miniti](https://miniti.app) — AI meeting assistant with live transcription, insights, and Smart meetings. Sibling of the Apple apps in `../miniti`, the Android port in `../miniti-android`, and the backend in `../miniti-api`.

This file is the canonical entry point for agents working here. Long-form delivery context lives in [PLAN.md](PLAN.md). Do not invent Apple/Swift APIs in this repo — reimplement product contracts in Rust + Tauri.

## Locked decisions (do not reopen without asking)

| Decision | Choice |
|---|---|
| Stack | **Tauri 2 + Rust** backend, web UI in system WebKitGTK |
| Parity target | **macOS feature-complete** (not the thinner iOS/Android mic-only cut) |
| Distribution | **GitHub Release binary tarball + AUR** (`miniti-bin`, `miniti` / `miniti-git`) |
| Not shipping | Flatpak, AppImage, Snap, Swift-on-Linux, Electron |
| Monetization | Managed Free + **Polar Pro** (same web rail as macOS) + BYOK |
| Platform header | `X-Platform: linux` |
| Repo | Separate from `../miniti` — no shared Swift sources |

Qt remains a **fallback only** if the PipeWire / long-transcript spike proves Tauri intractable.

## Sibling repos (local)

| Path | Role |
|---|---|
| `../miniti` | macOS + iOS Swift apps (source of truth for product behaviour) |
| `../miniti-api` | Vercel backend — API contract in that repo’s `AGENTS.md` |
| `../miniti-android` | Android port (mic-only; useful for API/schema patterns, not desktop audio) |

Do not modify sibling repos from this workflow unless the user explicitly asks. Backend work needed for Linux (`X-Platform: linux`, `LINUX_MIN_VERSION`, download URL) is listed in [PLAN.md](PLAN.md) § backend.

## Where to find things

| I need to… | Read |
|---|---|
| Build / ship the product | [PLAN.md](PLAN.md) |
| Set up / run the app locally | [README.md](README.md) § Develop; Cloud Agent env in `.cursor/environment.json` |
| Understand feasibility & gaps | [docs/feasibility.md](docs/feasibility.md) |
| Package binary + AUR | [docs/distribution.md](docs/distribution.md) |
| Backend endpoints / Polar | `../miniti-api/AGENTS.md` (and PLAN § API) |
| Current macOS audio / echo / multichannel | `../miniti/docs/audio.md` (reference only — reimplement here) |
| Call lifecycle policy semantics | `../miniti/docs/call-lifecycle-and-recording-presence-plan.md` |

## Product (user-facing target)

macOS-class desktop meeting assistant:

- mic + system audio (PipeWire), live Deepgram Nova-3, dual-channel attribution when both sources run
- live insights: Summary, Questions, Coaching, opt-in Sales (MEDDPICC), Playbook (Docs MCP)
- Smart meetings (call sensor where possible + quiet/calendar fallbacks), tray + floating presence
- Google Calendar, Attio/Twenty CRM, webhooks, Granola import, markdown export, codebase investigation
- Managed Free / Polar Pro / BYOK; force-update via `/api/version`

Honest Linux limits (document in UI/docs, don’t fake parity): Process-Tap-class capture on every distro; Core Audio–grade call HAL; Sparkle-smooth silent updates outside pacman.

## Non-negotiable conventions

- **Dark mode only**, terminal-adjacent visual language — port tokens from `../miniti/Miniti/Models/ColorPalette.swift` and `design.md`; no random hex in components.
- **PCM contract**: 16 kHz PCM16 LE to Deepgram; mono (`channels=1`) or stereo interleave mic/system (`channels=2&multichannel=true`) matching macOS.
- **Hot-path**: never filter unbounded transcript arrays on every finalize; reverse-iterate and break on window expiry (same rule as Apple apps).
- **Secrets**: never commit API keys; obfuscate shared `X-API-Key` like other clients; device UUID via libsecret with file fallback.
- **AUR**: disable Tauri updater pubkey for AUR/source builds; pacman owns Arch updates.
- **Changelog**: public-audience prose when shipping; no code refs in user-facing notes.

## Status

- Folder created 2026-09-05. **Phase 0 scaffold + core modules landed**: Tauri 2 + Rust + React/TypeScript, `identifier=com.miniti.linux`, binary `miniti`.
- Implemented Rust modules under `src-tauri/src/` (all unit-tested — 56 tests): `audio` (PCM convert/resample/interleave/RMS + `cpal` mic + `parec` system capture), `deepgram` (query/URL builder, message parsing, speaker-ID mapping, stabilization, backoff, live WS), `db` (SQLite meetings + transcript segments), `prefs`, `device_id`, `coaching` (local metrics), `api` (backend client with obfuscated key + `X-Platform: linux`), `insights`, `webhook`, `gates`, `call_sensor` (policy), and `state` (recording engine + Tauri commands).
- Frontend: multi-screen dark UI (`src/views/`): Home, Record (level meters + live transcript), Coaching, History (pin/search/delete), Settings.
- **Not yet runnable end-to-end without credentials/backend**: live Deepgram needs a key (BYOK Secret) or managed session; managed API/insights need the backend + app secret; real capture needs an audio device (headless VMs fall back gracefully). Multichannel mic+system interleave util exists but the live sync engine is still TODO. Design tokens are placeholders until `../miniti` `ColorPalette` is reachable (see `repositoryDependencies` in `.cursor/environment.json`).
- Dev environment is codified for Cloud Agents in `.cursor/environment.json` (bootstrap: `.cursor/install.sh` — installs WebKitGTK/GTK/PipeWire/libsecret/tray libs, sets Rust `stable` default, `pnpm install`). Run locally per [README.md](README.md) § Develop.
- Cursor-hosted repo: `ian/miniti-linux` (`https://origin.cursor.com/ian/miniti-linux.git`); page: https://cursor.com/codebase/ian/miniti-linux
- Next engineering step: remaining Phase 0 spike in PLAN.md §13 — Rust mic capture → PCM16 16 kHz → Deepgram (BYOK), PipeWire system/monitor capture, stereo `channels=2&multichannel=true` smoke test, on Arch + an Ubuntu-built binary.

## When in doubt

1. Read [PLAN.md](PLAN.md).
2. Prefer reimplementing contracts over copying Swift.
3. Ask before adding Flatpak/AppImage or changing the binary+AUR ship model.
