# miniti Linux

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

Tauri 2 + Rust + React/TypeScript, `identifier=com.miniti.linux`, binary `miniti`. Last reworked 2026-09-06 (macOS-parity build-out; see git log).

### Wired and runnable

| Area | What works |
|---|---|
| Capture | Mic (`cpal`, phase-continuous resampler) + system audio (`parec` monitor) → stereo interleave (ch0 mic, ch1 system) → Deepgram `channels=2&multichannel=true`; mono fallback when no monitor. Source-energy log for dominance |
| Transcription | Nova-3 with the macOS query contract, `SpeakerIdentityState` + `segmentBySpeaker` ports, echo reconciliation (350 ms ordering buffer, 4 s pending-mic window, LCS/contiguous-run suppression), `CloseStream` drain on stop, bounded reconnect with timeline offsets |
| Credentials | BYOK (`Token`) or managed (`POST /api/session` Bearer grant, refreshed before reconnects, `session/end` on stop). Backend auth is device-bound (`src-tauri/src/auth`): P-256 installation key + anonymous recovery key, HS256 access tokens bound to the key thumbprint, ES256 request proofs on every POST/DELETE, rotating refresh with challenge fallback. No shared secret in the binary |
| Layout | macOS three-pane: history sidebar (Pinned / Today / Yesterday / This week / Older, auto-collapses on record, Ctrl+[), transcript + notes, insights rail (Ctrl+]) with lowercase summary / questions / coaching and Sales / Playbook under More. Same view live and saved |
| Insights | Live engine ported from `AppState` (cadence policies, staggering, incremental delta + rolling state, degraded/stale/out-of-order safety, auto title until rename). Managed via `/api/insights`; BYOK via OpenAI with the backend prompts verbatim. Background final pass on stop; regenerate; catch me up; investigate (web / codebase); speaker naming that never overwrites manual renames; sales suggestion + investigation-moment heuristics |
| Playbook | Streamable-HTTP MCP client (backend SSRF guard, tool discovery, chunk normalization), topic extraction every 20 s, per-topic lookup state, auto lookups for BYOK/Pro |
| Coaching | `TrainingMetrics` + `CoachingAdvisor` ports; focus / stats / history; verbatim per-language filler lists |
| History | search, pin, rename, delete, speaker rename, mark-as-you, trim (turn / before / after with regeneration), copy transcript, Markdown export (macOS section order), Granola CSV import with duplicate protection |
| Desktop shell | Tray with live timer + Start/Stop + decision rows, close-to-tray, floating recording surface (always-on-top window with timer/stop/decisions/nudges), desktop notifications with the surface-aware rule, deep links for OAuth returns |
| Smart meetings | `CallLifecycleEngine` port over PipeWire capture clients; quiet-ended prompts (threshold table + :00/:30 boundary); calendar transition prompt with Remind-in-2-min and gated 15 s handoff; calendar auto-start countdown; silence auto-stop; 10 s ending grace; live guidance nudges |
| Integrations | Google Calendar (upcoming five, prep notes seeding live notes, auto title/attendees), Attio + Twenty send sheet (search, payload preview, per-task inclusion) — all via the backend |
| Gates / Pro | force-update via `/api/version`, terms, onboarding; Polar subscribe / portal / restore; usage pill |
| Settings | macOS destinations (General … Privacy & Support) with sidebar + Ctrl+F search that scrolls to the control |

### Known Linux limits (documented, not faked)

- System audio needs PipeWire with the pulse shim (`pactl` / `parec`); Bluetooth route changes are not yet auto-recovered (macOS has a stall watchdog).
- Call detection is PipeWire-client based: native apps are strong signals, browsers weak; no per-process HAL.
- Floating surface placement may be ignored on Wayland compositors.
- No in-app silent updater: AUR / pacman or the release tarball; `/api/version` only hard-gates.
- Long transcripts rely on `content-visibility: auto` rather than a native text view.

### Verification

`cargo test --manifest-path src-tauri/Cargo.toml` (150 tests), `cargo clippy` clean, and `pnpm build` pass on macOS. On the Arch ThinkPad, the mono BYOK/managed path has been run live; the dual-source, Smart-meetings and integration paths were built against the documented contracts and still need a real-hardware pass.

- Dev environment for Cloud Agents: `.cursor/environment.json` (bootstrap `.cursor/install.sh`). Run locally per [README.md](README.md) § Develop.
- Cursor-hosted repo: `ian/miniti-linux` (`https://origin.cursor.com/ian/miniti-linux.git`); page: https://cursor.com/codebase/ian/miniti-linux

## Changelog

Every release adds an entry to `CHANGELOG.md`; the rules are in [docs/changelog-style.md](docs/changelog-style.md).

## When in doubt

1. Read [PLAN.md](PLAN.md).
2. Prefer reimplementing contracts over copying Swift.
3. Ask before adding Flatpak/AppImage or changing the binary+AUR ship model.
