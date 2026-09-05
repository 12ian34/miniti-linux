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

Folder created 2026-09-05. Tauri 2 + Rust + React/TypeScript, `identifier=com.miniti.linux`, binary `miniti`. Last reviewed and reworked 2026-09-05 (second-pass review; see git log).

### Wired and runnable (end-to-end paths that exist today)

| Path | Notes |
|---|---|
| Mic → 16 kHz PCM16 → Deepgram → SQLite → UI | `cpal` capture with a phase-continuous resampler; per-word speaker segmentation and identity mapping ported from macOS (`SpeakerIdentityState` / `segmentBySpeaker`); interims replaced in place in the UI; `CloseStream` + drain on stop so trailing finals are kept; bounded reconnect with wall-clock timeline offsets |
| BYOK credential | Deepgram `Token` from prefs; recording refuses to start without a key instead of producing an empty meeting |
| Managed credential | `POST /api/session` → `Bearer` grant, refreshed before reconnects when near expiry; `POST /api/session/end` on stop. Requires a build with `MINITI_API_KEY` (see README); without it managed mode reports itself unavailable |
| Launch gates | `GET /api/version` min-version force gate → terms → onboarding → main. Backend failures never block launch |
| Webhook | `meeting.saved` POST on stop with the Apple payload shape (`meeting` envelope, `training` blob, resolved speaker labels) |
| Coaching | `TrainingMetrics` + `CoachingAdvisor` ported (fillers/min, wpm, words/turn, questions/30 min, talk ratio, monologue words); per-language filler defaults verbatim from `TranscriptionLanguage`; focus / stats / history tabs |
| History | list, search (LIKE-escaped), pin, delete, rename, speaker rename, mark-as-you |
| Pro (Polar) | subscribe / portal URLs opened in the browser, license-key restore, usage display |
| Device id | secret service via `keyring` with file mirror + fallback |

### Contract-only (typed, tested, not called from any runtime path)

- `insights` — `/api/insights` request/response shapes for every backend mode (`standard`, `meddpicc`, `questions`, `speaker_names`, `catchup`, `investigation`, `docs`, `docs_topics`) with mode-specific validation. The live insights loop (staggered incremental calls, apply-safety, UI) is Phase 2.
- `call_sensor` — known-app classifier + `pactl source-outputs` parser (corked streams and Miniti's own clients excluded). The Smart-meetings lifecycle engine is Phase 4.

### Not started

- System audio **into the transcript**: `parec` monitor capture is metered only. Stereo interleave utilities exist; the live mic+system sync engine, `channels=2&multichannel=true` sessions, and echo reconciliation are still the Phase 0 kill-criteria spike.
- Tray, floating presence, notifications, deep links (Phase 3). Prefs toggles exist but do nothing yet and are labelled as such in Settings.
- Insights UI, Playbook/Docs MCP, catch-up, investigation (Phase 2). Google Calendar, Attio/Twenty, Granola import, markdown export (Phase 5).
- Design tokens are placeholders until `../miniti` `ColorPalette` is ported.

### Verification

`cargo test --manifest-path src-tauri/Cargo.toml` (93 tests) and `pnpm build` both pass on macOS as of 2026-09-05. Nothing here has been run against a live Deepgram socket or the production backend yet — the next engineering step is exactly that, on Arch with a BYOK key, then the multichannel spike.

- Dev environment for Cloud Agents: `.cursor/environment.json` (bootstrap `.cursor/install.sh`). Run locally per [README.md](README.md) § Develop.
- Cursor-hosted repo: `ian/miniti-linux` (`https://origin.cursor.com/ian/miniti-linux.git`); page: https://cursor.com/codebase/ian/miniti-linux

## When in doubt

1. Read [PLAN.md](PLAN.md).
2. Prefer reimplementing contracts over copying Swift.
3. Ask before adding Flatpak/AppImage or changing the binary+AUR ship model.
