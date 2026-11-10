# miniti Linux

Apple publication, 2026-11-09: macOS 3.8.0 build 164 is public. The compatible backend is deployed, and iOS build 164 is on internal TestFlight and waiting for App Review. Linux 0.7.2 remains the independent public release; the Apple parity items below are tracking, not Linux adoption. Apple final validation and publication evidence live in the [3.8.0 release record](../miniti/docs/releases/3.8.0.md). No Linux runtime or version change accompanies this record.

Native Linux desktop client for [Miniti](https://miniti.app) — AI meeting assistant with live transcription, insights, and Smart meetings. Sibling of the Apple apps in `../miniti`, the Android port in `../miniti-android`, and the backend in `../miniti-api`.

This file is the canonical entry point for agents working here. Long-form delivery context lives in [PLAN.md](PLAN.md). Do not invent Apple/Swift APIs in this repo — reimplement product contracts in Rust + Tauri.

Local implementation (2026-11-03): the [test-foundation handoff](docs/test-foundation-followup.md) records the incremental Questions baseline correction and canonical wire-fixture follow-through. No model migration or release is implied.

Apple 3.8.0 local implementation handoff (2026-11-04): [PLAN.md](PLAN.md#apple-380-shared-contract-planning-2026-11-04) tracks additive v2 metering, account-choice/device-label parity and atomic link compatibility. The local implementation handoff and exact wire fixture are in PLAN.md and test-contracts/managed-accounting-v2.json. No Linux adoption or release is claimed; its version line remains independent.

Apple Home layout parity (2026-11-09): PLAN.md records bottom recording controls, the independent full-height Mac calendar sidebar, mirrored sidebar footer controls and labelled collapsed calendar rail, full-row folder disclosures and labelled Models/keyboard access. No Linux runtime change.

Apple folder journey parity (2026-11-08): PLAN.md tracks optional folder setup and separate preview/create/manage destinations. No Linux runtime change.

Apple setup visibility parity (2026-11-05): PLAN.md tracks progress, independent guide/task dismissal and General restoration; no Linux runtime change. Fifth- through seventh-review tracking there covers retryable account contention, stable Markdown checks, pending-usage recovery and Apple-specific old-client folder protection, without claiming Linux adoption.

Apple audio-route follow-up (2026-11-06): PLAN.md tracks the Mac-specific hardware timeout/recovery boundary. No Linux runtime, wire contract or release change is implied.

Apple recording-status parity (2026-11-09): PLAN.md tracks neutral startup copy versus actual recovery. No Linux runtime change.

Apple 3.9.0 reading metrics (2026-11-09): [PLAN.md](PLAN.md#apple-390-reading-metrics-follow-through-2026-11-09) tracks live/saved prose-size consistency and platform scaling parity; the adjacent contrast and target follow-through cover readable question/coaching/action text including aggregate stats values, live per-speaker filler counts and Questions/Playbook guidance, snippets and lookup indicators, placeholders, non-overlapping interaction bounds and named help/copy controls and live/saved Update actions with native keyboard eligibility, plus named Coaching history sort controls, narrow-table reachability and usable minimum text-pane sizes. The meeting-section parity entry tracks consistent live/saved navigation while retaining screen-local selection; list-presentation tracking covers selection across items, aligned wrapping and reading spacing, while distinguishing readable actions from persistent task completion. Standard/Sales-section tracking covers named field identity, order and consistent category colours without changing content; standard sections preserve each platform’s field coverage and empty-summary policy. Shared section-geometry tracking keeps platform density and help controls explicit. Insight Update and Copy tracking covers neutral shared chrome, canonical copied feedback, preserved state wording and native behavior without changing clipboard content or timing. The appearance follow-through tracks native window/control readability under the platform theme contract; Apple’s scene override is Mac-specific. No Linux runtime or version change.

## Locked decisions (do not reopen without asking)

| Decision | Choice |
|---|---|
| Stack | **Tauri 2 + Rust** backend, web UI in system WebKitGTK |
| Parity target | **macOS feature-complete** (not the thinner iOS/Android mic-only cut) |
| Distribution | **GitHub Release binary tarball + `.deb` + the Omarchy package repository** (`miniti-bin` in [omacom/omarchy-pkgs](https://github.com/omacom/omarchy-pkgs), following our GitHub releases). **Never the AUR** (user decision 2026-09-07) |
| Not shipping | Flatpak, AppImage, Snap, Swift-on-Linux, Electron |
| Monetization | Managed Free + **Polar Pro** (same web rail as macOS) + BYOK |
| Platform header | `X-Platform: linux` |
| Repo | Separate from `../miniti` — no shared Swift sources |
| App id | `com.miniti.linux` (D-Bus, AppStream, keyring); desktop file and `app_id` / `WM_CLASS` stay `miniti` (see docs/desktop-integration.md) |

Qt remains a **fallback only** if the PipeWire / long-transcript spike proves Tauri intractable.

## Sibling repos (local)

| Path | Role |
|---|---|
| `../miniti` | macOS + iOS Swift apps (source of truth for product behaviour) |
| `../miniti-api` | Vercel backend — API contract in that repo’s `AGENTS.md` |
| `../miniti-android` | Android port (mic-only; useful for API/schema patterns, not desktop audio) |
| `../miniti-omarchy` | Omarchy (Quickshell/QML) bar widget + panel; reads this app's `state.json` and drives it through the CLI |

Do not modify sibling repos from this workflow unless the user explicitly asks. Backend work needed for Linux (`X-Platform: linux`, `LINUX_MIN_VERSION`, download URL) is listed in [PLAN.md](PLAN.md) § backend.

## Where to find things

| I need to… | Read |
|---|---|
| Build / ship the product | [PLAN.md](PLAN.md) |
| Set up / run the app locally | [README.md](README.md) § Develop; Cloud Agent env in `.cursor/environment.json` |
| Understand feasibility & gaps | [docs/feasibility.md](docs/feasibility.md) |
| Package binary + Omarchy repo | [docs/distribution.md](docs/distribution.md) |
| CLI, control socket, state file, D-Bus, Waybar, autostart | [docs/desktop-integration.md](docs/desktop-integration.md) |
| What a gold-standard Linux app still needs (done / open / decided) | [docs/linux-roadmap.md](docs/linux-roadmap.md) |
| Backend endpoints / Polar | `../miniti-api/AGENTS.md` (and PLAN § API) |
| Current macOS audio / echo / multichannel | `../miniti/docs/reference/audio.md` (reference only — reimplement here) |
| Call lifecycle policy semantics | `../miniti/docs/archive/call-lifecycle-and-recording-presence-plan.md` |

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
- **Secrets**: never commit API keys or introduce a shared `X-API-Key`. Managed requests use device-bound installation credentials and request proofs; local credential files are owner-only, with desktop keyring integration when available.
- **Arch / Omarchy**: no Tauri updater; pacman owns updates through the Omarchy package repository. Nothing is ever published to the AUR.
- **Changelog**: public-audience prose when shipping; no code refs in user-facing notes.

## Release preparation, 2026-11-09

Ian authorized the coordinated release across repositories. Linux 0.7.2 includes the existing Granola import and BYOK/Questions corrections since 0.7.1, plus the 503 busy compatibility mapping. Apple feature parity remains deferred. Full local checks and Linux CI/package evidence are required before the new tag and availability update.

## Status

Tauri 2 + Rust + React/TypeScript, `identifier=com.miniti.linux`, binary `miniti`. Last reworked 2026-09-06 (macOS-parity build-out; see git log).

### Wired and runnable

| Area | What works |
|---|---|
| Capture | Mic (`cpal`, phase-continuous resampler) + system audio (`parec` monitor) → stereo interleave (ch0 mic, ch1 system) → Deepgram `channels=2&multichannel=true`; mono fallback when no monitor. Source-energy log for dominance. System-tap stall watchdog: 4 s without frames restarts `parec` against the current default sink (three failures → audio degraded, mic continues) |
| Transcription | Nova-3 with the macOS query contract, `SpeakerIdentityState` + `segmentBySpeaker` ports, echo reconciliation (350 ms ordering buffer, 4 s pending-mic window, LCS/contiguous-run suppression), `CloseStream` drain on stop, bounded reconnect with timeline offsets |
| Credentials | BYOK (`Token`) or managed (`POST /api/session` Bearer grant, refreshed before reconnects, `session/end` on stop). Backend auth is device-bound (`src-tauri/src/auth`): P-256 installation key + anonymous recovery key, HS256 access tokens bound to the key thumbprint, ES256 request proofs on every POST/DELETE, rotating refresh with challenge fallback. No shared secret in the binary |
| Layout | macOS three-pane: history sidebar (Pinned / Today / Yesterday / This week / Older, auto-collapses on record, Ctrl+[), transcript + notes, insights rail (Ctrl+]) with lowercase summary / questions / coaching and Sales / Templates / Playbook under More (the More label carries the active template). Same view live and saved. Transcript turns are memoized and grouped into `content-visibility` blocks for long meetings. Interface scale compact / standard / large |
| Insights | Templates view (BANT, SPIN, interview scorecard, customer check-in, stand-up, 1:1) filled live, on demand, on saved meetings, and at the final pass; in exports and webhooks. Live engine ported from `AppState` (cadence policies, staggering, incremental delta + rolling state, degraded/stale/out-of-order safety, auto title until rename). Managed via `/api/insights`; BYOK via OpenAI with the backend prompts verbatim. Background final pass on stop; regenerate; catch me up; investigate (web / codebase); speaker naming that never overwrites manual renames; sales suggestion + investigation-moment heuristics |
| Playbook | Streamable-HTTP MCP client (backend SSRF guard, tool discovery, chunk normalization), topic extraction every 20 s, per-topic lookup state, auto lookups for BYOK/Pro |
| Coaching | `TrainingMetrics` + `CoachingAdvisor` ports; focus / stats / history; per-metric trend charts (SVG, categorical meeting axis, ringed latest point, broad-range band); grounded examples (real passages from recent meetings per metric, clickable); verbatim per-language filler lists |
| History | search, pin, rename, delete, speaker rename, mark-as-you, trim (turn / before / after with regeneration), copy transcript, Markdown export (macOS section order), Granola CSV import with duplicate protection |
| Desktop integration | `miniti` CLI in the same binary (clap: status / start / stop / toggle / questions / meetings / export / decide / watch / autostart / paths / completions; exit 3 = not running); control socket `$XDG_RUNTIME_DIR/miniti/miniti.sock` (JSON lines, `subscribe` stream) doubling as the single-instance guard that forwards deep links; `state.json` snapshot rewritten atomically on change; D-Bus `com.miniti.linux` / `com.miniti.linux.Control` (zbus, same handler as the socket); idle inhibit while recording (ScreenSaver then portal); XDG autostart entry from a pref; logs under `$XDG_STATE_HOME`; desktop entry with actions, AppStream metainfo, SVG + symbolic icons, man page, completions, all validated in CI. The Omarchy bar plugin lives in `../miniti-omarchy` and consumes `state.json` + the CLI |
| Desktop shell | Tray with live timer + Start/Stop + decision rows, close-to-tray, floating recording surface at macOS parity (presence model pushed each second: timer, call app, meeting title, call/transcription/audio status, ending countdown; kind-specific prompt actions; nudges with dismiss / don't remind / enable sales; content-sized, auto-expand/collapse attention policy, monitor clamping, never takes focus; wlr-layer-shell overlay on Wayland when `libgtk-layer-shell` is present), desktop notifications with the surface-aware rule, deep links for OAuth returns, daily rolling log with an in-app viewer |
| Smart meetings | `CallLifecycleEngine` port over PipeWire capture clients; quiet-ended prompts (threshold table + :00/:30 boundary); calendar transition prompt with Remind-in-2-min, an End-meeting answer on every surface, and a gated 15 s handoff; expected-call adoption for join / calendar-link starts (5 min window); calendar auto-start countdown; silence auto-stop; 10 s ending grace; live guidance nudges. Non-meeting calendar entries (out of office, focus time, working location, birthdays, and titles that say as much) never raise a prompt; the refresh fetches `?include_filtered=true` and applies the user's own filters, adjustable in Settings → Calendar → Meeting filters with a seven-day preview. Only an all-day entry is non-negotiable (a bare date has no time to record against) |
| Integrations | Google Calendar (upcoming five, prep notes seeding live notes, auto title/attendees), Attio + Twenty send sheet (search, payload preview, per-task inclusion) — all via the backend. Webhooks are https-only (loopback http for a hook on this machine), validated in Settings and again at send, with the hook URL kept out of the log |
| Gates / Pro | force-update via `/api/version`, terms, onboarding, managed-mode enrollment (recovery key), moving the device to another account (`/api/auth/account/attach`); Polar subscribe / portal / restore, plus account-sponsored Pro (`entitlement_via_account`, no portal for a sponsored device); usage banner (amber < 60 min, red < 15) and limit-reached view with upgrade / switch to BYOK |
| Settings | macOS destinations (General … Privacy & Support) with sidebar + Ctrl+F search that scrolls to the control; Share diagnostics (opt-out reliability events to `/api/client-events`, on in managed mode, never in BYOK) under Version & diagnostics |

### Known Linux limits (documented, not faked)

- System audio needs PipeWire with the pulse shim (`pactl` / `parec`). Route changes recover through the stall watchdog (restart after 4 s of silence from the monitor), not through a device-change event; a switch mid-sentence loses up to 4 s of remote audio.
- Call detection is PipeWire-client based: native apps are strong signals, browsers weak; no per-process HAL.
- Floating surface on Wayland: pinned and focus-free only through wlr-layer-shell (Hyprland, Sway, KDE, with `libgtk-layer-shell` installed); on GNOME or without the library the compositor decides placement and the window cannot be dragged into place. Layer surfaces cannot be dragged at all (fixed top-right, 16 px margin). X11 gets the full behaviour (utility window, keep-above, never focused, clamped to the monitor).
- No in-app silent updater: pacman (Omarchy repository) or the release tarball; `/api/version` only hard-gates.
- Long transcripts use memoized turns in `content-visibility` blocks rather than a native text view; selection across blocks works, but very long meetings (thousands of turns) still render slower than the Mac's NSTextView.
- Microphone capture has no restart path yet: a disconnected USB/Bluetooth mic surfaces as a stream error rather than recovering.
- Coaching charts have no hover tooltips; VoiceOver-style per-chart summaries are exposed through `aria-label` only.
- Screenshots in the README are from the Mac app; Linux screenshots need a real desktop session.

### Verification

`cargo test --manifest-path src-tauri/Cargo.toml` (216 tests), `cargo clippy` clean, and `pnpm build` pass on macOS. On the Arch ThinkPad, the mono BYOK/managed path has been run live; the dual-source, Smart-meetings and integration paths were built against the documented contracts and still need a real-hardware pass.

- Dev environment for Cloud Agents: `.cursor/environment.json` (bootstrap `.cursor/install.sh`). Run locally per [README.md](README.md) § Develop.
- Cursor-hosted repo: `ian/miniti-linux` (`https://origin.cursor.com/ian/miniti-linux.git`); page: https://cursor.com/codebase/ian/miniti-linux

## Changelog

Every release adds an entry to `CHANGELOG.md`; the rules are in [docs/changelog-style.md](docs/changelog-style.md).

## When in doubt

3.7.0 cross-client review (2026-11-03): [Apple release checklist](../miniti/docs/releases/3.7.0.md) owns integration gates. PLAN.md records the separate managed-usage versus diagnostic-consent contract; Linux's existing Deepgram usage must not be double-reported. No new Linux engine/default or telemetry producer is selected.

Apple 3.7.0 recording/client AI diagnostic follow-up is tracked in [PLAN.md](PLAN.md#tracking-recording-and-client-ai-diagnostics-2026-11-03). New failure-stage/request correlation is planning only; existing Linux recovery remains unconfirmed until its own producer contract and tests are verified. No Linux source, BYOK routing or release change follows from the Apple observations.

Apple Mac local speaker reliability/device diagnostics (scope revised 2026-11-03) is tracked in PLAN.md and the [Apple plan](../miniti/docs/plans/local-speakers-default.md). Failure-handling fixes and diagnostics remain in 3.7.0; the Mac default stays off, with default-on deferred until M1 testing. No Linux engine/default or telemetry producer change is selected; future backend metadata stays additive. Planning only.

GPT-5 mini retirement (selected 2026-11-03): Apple's 3.7.0 [bounded migration](../miniti/docs/plans/gpt-5-mini-retirement.md) approves Luna none for summaries/lightweight tasks and pinned GPT-5.4 mini low for other AI tasks and backend fallbacks, including Linux BYOK coordination before 2026-12-11. PLAN.md owns Linux tracking; Linux keeps its own release version. Local implementation now selects Luna/none for standard and helpers, quality mini/low for specialist tasks, and sends explicit reasoning in direct chat bodies. Rust checks pass (220 tests, one live test ignored), frontend build passes; no binary or release has shipped.

1. Read [PLAN.md](PLAN.md).
2. Prefer reimplementing contracts over copying Swift.
3. Ask before adding Flatpak/AppImage or changing the binary + Omarchy-repository ship model. Do not suggest the AUR.

Apple O6 diagnostic timeline/support schema parity is tracked in `PLAN.md` (2026-10-29); not implemented or released on Linux. Existing support shapes remain accepted by the additive backend parser.

Apple 3.6.0 sync/delivery parity is tracked in [PLAN.md](PLAN.md#tracking-apple-360-sync-and-delivery-reliability-2026-10-30-local-implementation): optional webhook `processing`, durable stopped-meeting eligibility/count freshness, held completion handoff, immutable delivery IDs/current-data resend and saved-AI authority. Tracking only; Swift implementation does not establish Linux parity or change its release version. No backend sync schema change.

Recording-recovery telemetry has no verified Linux `recording_recovered`/`pcm_sent` producer yet. The backend keeps Linux recovery unconfirmed; implementing it needs an explicit producer contract and minimum-version tests, tracked in `PLAN.md` (2026-10-31). Do not infer capability from Apple or numeric version ordering.

Apple handoff follow-up (2026-10-31) is tracked in PLAN.md: held-row expiry, released-only saved-before-updated ordering, temporary destination fingerprints, terminal-marker recovery and honest blocked status. Tracking only; do not claim a Linux implementation from the Apple build.

Managed lightweight model follow-up (2026-10-31, deployed but disabled): backend Luna routing for four modes is tracked in [PLAN.md](PLAN.md#tracking-managed-lightweight-luna-2026-10-31-local-only). No Linux client or BYOK model changes. Luna defaults off in backend code; only explicit `INSIGHTS_LIGHTWEIGHT_LUNA=on` enables it. Initial rollout retains GPT-5 mini low; activation and managed-client checks remain pending. Do not infer shipped parity.

Managed standard-summary extension (2026-10-31, deployed but disabled): `INSIGHTS_STANDARD_LUNA=on` separately opts full and incremental `standard` requests into Luna none. Default off; the legacy incremental-quality rollback takes precedence. Tracked alongside lightweight Luna in PLAN.md. Linux BYOK/prompts/binary stay unchanged; activation and client verification remain pending.

Apple 3.6.0 publication (2026-10-31): Mac build 146 is released; iOS build 146 became available on 2026-11-01. Backend `031c692` deployed with both Luna flags unset, so managed model choices have not changed. This repo receives tracking documentation only; no Linux binary/version/BYOK change or Linux release is implied.

Managed Luna activation (2026-11-01): both backend opt-ins are now on, so existing managed Linux callers inherit Luna none for the four lightweight tasks and full/incremental standard summaries. Synthetic HTTP checks passed on the shared backend; this is not a physical Linux client pass. Questions and other quality modes, BYOK, Linux source/binary and release version remain unchanged. PLAN.md records follow-up and rollback. Earlier disabled-rollout entries above are historical.

Recording reporting v1 (2026-11-03, local only) is tracked in [the cross-client handoff](docs/recording-reporting-v1.md). Separate authenticated local-duration accounting and additive diagnostic fields do not create a Linux producer or duplicate its Deepgram usage. Account IDs remain case-sensitive; only installation IDs canonicalize. No deployment or Linux release.

Apple local-speaker failure handoff (2026-11-03): [tracking](docs/local-speaker-failures-370.md) records compatible Unknown speaker names and the optional speaker-state/failure producer. No Linux model, producer or release change.

Apple/backend 3.7.0 hardware/activity/outcome changes have a [local handoff](docs/hardware-activity-outcomes-370.md). No Linux hardware producer or release is implied; backend normalization keeps existing Linux contracts compatible.

3.7.0 recovery/AI/removal handoff (local, 2026-11-03): [tracking](docs/recovery-ai-removal-370.md) defines compatible additive backend fields and future Linux parity boundaries. Rust tests and frontend build pass locally; no native Linux recording or publication is implied.

Apple/backend 3.7.0 review handoff (2026-11-03): [PLAN.md](PLAN.md#tracking-370-review-corrections-2026-11-03) tracks full Questions prompt/fixture parity and Luna access-error behavior. Linux runtime implementation is still pending; do not infer parity from the incremental-only baseline fix.

Public documentation audit (2026-11-03, local only): README and the public Linux guide now distinguish released 0.7.1 from Apple features, Polar versus App Store account coverage, BYOK provider traffic versus integration/version requests, and the tarball icon-directory prerequisite. No implementation or release changed. Full evidence: [public docs audit](../miniti/docs/research/public-docs-audit-2026-11-03.md).

Expanded Apple 3.8.0 handoff: PLAN.md tracks optional folder catalog/membership compatibility and the exact `test-contracts/folders-v1.json` fixture. Linux has no current cloud-sync producer, so no Linux folder UI/sync implementation or round-trip claim follows from this Apple update. Its version/release line remains unchanged.

Apple SG1 Home setup UX parity is tracked in PLAN.md (2026-11-04). No Linux behavior or backend contract changes are implied.

Apple/backend second-review handoff (2026-11-05): PLAN tracks compatible raw-accounting, deletion and legacy-ID fixes plus Apple sync/export/folder parity. No Linux runtime change. Apple checklist redesign is now locally implemented after Ian’s approval; PLAN records parity, without Linux adoption.

Apple third-review follow-up (2026-11-05): PLAN.md tracks calendar/setup refinement and folder-only sync preservation; backend accounting compatibility changes require no Linux version bump. No runtime parity is claimed.

CRM parity decision (2026-11-05): automatic Attio/Twenty sync runs at the first completed recording; later edits or resumed content are resent manually. PLAN.md records the resolved Apple 3.8.0 contract.

Apple eighth review (2026-11-06): [PLAN.md](PLAN.md#apple-eighth-review-compatibility-2026-11-06) tracks migration case compatibility, folder refresh and account-repair parity. No Linux runtime or release change.

Apple build 155 compatibility follow-through (2026-11-06): the API mapper accepts 503 as well as 409 `busy`. PLAN.md owns the bounded change and validation; no Linux release is implied.

Apple Home presentation tracking (2026-11-08): PLAN.md records exclusive setup/calendar content and persistent calendar visibility controls. No Linux runtime or version change.

Linux 0.7.2 release execution (2026-11-09): Ian authorized the coordinated release. Local Rust tests pass (220 passed, one ignored), clippy and frontend build pass. Ubuntu CI tests and desktop paint smoke pass; packaging is in progress. The release includes committed BYOK/Granola/Questions work since 0.7.1 and the 503 busy mapper. Folder/setup parity remains tracked, not implemented. Public availability will be recorded after tagged assets exist.

Linux 0.7.2 is published (2026-11-09), tag `v0.7.2` at `c57cfd8`. Ubuntu tests, desktop paint smoke, package CI and release workflow pass. Tarball and amd64 Debian downloads match their GitHub SHA-256 digests; Debian metadata is 0.7.2/amd64. The PKGBUILD now pins the new tarball checksum. Omarchy’s external repository follows its own 24-hour quarantine and update schedule; no immediate pacman availability is claimed. Backend latest-version update and public docs are recorded in the Apple 3.8.0 execution record.
