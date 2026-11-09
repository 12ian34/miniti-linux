# miniti Linux — Build Plan & Full Context

Apple publication, 2026-11-09: macOS 3.8.0 build 164 is public. The compatible backend is deployed, and iOS build 164 is on internal TestFlight and waiting for App Review. Linux 0.7.2 remains the independent public release; the Apple parity items below are tracking, not Linux adoption. Apple final validation and publication evidence live in the [3.8.0 release record](../miniti/docs/releases/3.8.0.md). No Linux runtime or version change accompanies this record.

## Apple 3.9.0 meeting-section parity, 2026-11-09

Apple iOS live and saved meetings now use one transcript/insights/notes picker with matching neutral chrome, minimum targets and selected semantics. Selection and content remain local to each screen. Track equivalent Linux live/saved navigation consistency while preserving desktop layout and shortcuts; do not duplicate the iOS tab layout automatically. No Linux runtime, backend contract or version change is included. Apple source evidence remains in the [3.9.0 checklist](../miniti/docs/releases/3.9.0.md).

## Apple 3.9.0 appearance follow-through, 2026-11-09

Apple explicitly keeps main, Settings and Debug Log windows dark under a Light ambient appearance, without overriding system-owned menu-bar appearance. The SwiftUI scene fix is Mac-specific. Audit the equivalent Linux native controls and window chrome against the Linux theme contract; do not change the desktop-wide theme. No Linux runtime, backend contract or version change is included. Apple native evidence belongs in the [3.9.0 checklist](../miniti/docs/releases/3.9.0.md).

## Apple 3.9.0 target follow-through, 2026-11-09

Apple also names metric help/close and question/docs copy actions for accessibility, gives them the shared target allocation, and removes copy-button focus opt-outs. Track Linux keyboard eligibility and recognizable labels for equivalent native controls. Mac live/saved transcript, notes and insight header Copy controls also use named, readable actions with minimum targets; include the equivalent Linux section-copy controls in this audit. Apple native keyboard/pointer acceptance is still open. The live and saved insight Update controls also adopt the shared minimum target and retain native focus eligibility; include equivalent Linux update/regenerate actions in this audit while preserving their generation and disabled behavior.

Apple Coaching history sort headers now have minimum targets, full accessible sort names, direction and selection. iOS widens its metric columns and scrolls the table at narrow widths to avoid overlap; that touch-specific geometry is not a Linux requirement. Include equivalent Linux sort controls, keyboard focus, metric help and narrow-table reachability in this audit. No Linux runtime or backend change is included.

Apple Mac live notes now retain a usable editor viewport when the pane is resized to its minimum. Include equivalent Linux resizable text panes in the narrow-layout audit; preserve their own desktop sizing rather than copying the Mac bound. No Linux runtime or wire change is included.

Apple UI1 separates compact control chrome from its allocated interactive bounds. Audit Linux shared tabs, compact controls and disclosures for appropriate desktop targets, non-overlapping adjacent frames and native action/disabled/focus behavior. Do not copy iOS's 44-point minimum into desktop controls blindly. Apple coverage includes a native iPhone target fixture and existing large-text journeys; Mac keyboard and constrained parent layouts remain separate gates. No Linux runtime, wire or version change is included.

## Apple 3.9.0 contrast follow-through, 2026-11-09

Apple UI1 improves question context/copy labels, coaching units/column headings, readable placeholders and foreground on solid recording red. Audit matching Linux surfaces against the 4.5:1 ordinary-text target, including available controls and empty-field hints; do not blindly mirror Apple token names. The Mac menu-bar badge is platform-specific. No wire, data, Linux runtime or version change is included. Evidence remains in the Apple [3.9.0 checklist](../miniti/docs/releases/3.9.0.md).

## Apple 3.9.0 reading metrics follow-through, 2026-11-09

Apple UI1 is implementing shared reading metrics for live/saved transcripts, insights, Sales/templates and notes. Track Linux parity for consistent readable sizes across those surfaces and the app text-size preference; audit the Linux system/font scaling mechanism before choosing its implementation. Apple uses InterfaceScale alone on Mac and combines it with unshifted Dynamic Type once on iOS; UIKit/SwiftUI APIs are Apple-specific. Preserve selection, editing and incremental transcript updates. This introduces no backend, persistence, prompt or wire change and assigns no Linux version. Apple validation/review remains in [3.9.0](../miniti/docs/releases/3.9.0.md); no Linux adoption is claimed.

## Apple 3.8.0 shared-contract planning, 2026-11-04

Apple assigned 3.8.0 to reconnect metering, provider speaker restoration, explicit account choices/device labels and Watch/link reliability. The [release checklist](../miniti/docs/releases/3.8.0.md) and [metering contract](../miniti/docs/plans/managed-reconnect-metering.md) own scope. This is a planning handoff, not Linux implementation or a Linux version assignment.

- Track `accounting_version: 2`, `recording_id`, `renew_session_id`, original `session_started_at`, `duration_ms` and `accepted_duration_ms` across `/api/session` and `/api/session/end`. Stable session identity survives grant renewal; v2 counts recordings beyond four hours and finalizes once. Preserve legacy request compatibility until a separately tested Linux adoption; never infer support from an old response missing the echoed version.
- Pending charges are frozen to the enrolled owner, with bounded retention/retry and terminal owner/expiry/conflict errors. BYOK remains excluded. Legacy Linux accounting remains limited until v2 adoption; do not advertise the Apple fix as delivered on Linux.
- Account parity: first use offers Sign in or Continue without signing in; sign-out/revocation persist until an explicit choice. Preserve healthy accounts, existing auth proofs, non-destructive credential handling and server Retry-After. Apple approval is the product contract, not evidence of a Linux implementation.
- Track additive PATCH `/api/auth/account/devices/{installationId}` with a bounded label or null reset. Manual labels survive future automatic metadata refreshes. Personal device names never enter diagnostics. Existing list/revoke endpoints stay compatible.
- Generic web sign-in/confirmation claims become atomic, with conditional approval/refusal behavior. Preserve current device-enrollment inspect/bind/consume and lost-response recovery; update affected Linux integration tests against the backend fixtures before adoption.
- Speaker restoration and Watch importer changes are Apple-specific in this delivery. Retain Linux's existing source/recovery contract; do not invent support for the Apple local model or Watch companion.

Before a Linux release adopting these contracts, implement Rust wire fixtures and real lifecycle/owner/long-session tests, update public docs and choose the Linux release version independently. No Linux code or release metadata changes are part of this planning handoff.


## Apple follow-ups in implementation, 2026-10-28

Track Linux parity for [logging and monitoring](../miniti/docs/plans/logging-and-monitoring.md), [selective cloud privacy](../miniti/docs/plans/selective-cloud-privacy.md) and [purchase return](../miniti/docs/plans/purchase-return.md). Apple implementation is built locally and tested, not released. Preserve legacy diagnostics ingestion while adding stable event IDs and acknowledgements; Linux must adopt the same content-free event contract. Add the live/saved meeting Cloud/Local toolbar choice (no Home checkbox, per Ian’s 2026-10-29 UX review) and local calendar-title rules, including exclusion during an outstanding upload and preservation of excluded local meetings during pulls/deletes. `syncExcluded` stays local, without a sync-document schema change. Polar's HTTPS confirmation page must retain a usable Linux fallback until Linux registers and tests its own billing callback. Do not send every buyer to a Mac-only callback. Wire additions: diagnostics `event_id` and `accepted_ids`/`rejected_ids`; explicit signed upload-envelope `restore_deleted: true` with exact base revision, used only for the user’s Put on the cloud action. Preserve a local pending-restore intent across pull/reconcile until acknowledged; ordinary writes cannot resurrect tombstones. Purchase status requires the original installation and captured account. See `../miniti-api/docs/agents/09-followups.md`. No Linux implementation or release is claimed here.

Self-contained context for `miniti-linux`, a **Tauri 2 + Rust** rewrite of the miniti meeting assistant targeting **macOS feature parity** on Linux. Agents in this repo may not have the Swift tree open — every contract needed to build lives here (with pointers to sibling repos for prompts and fine detail).

**Tagline:** multi-dimensional meetings  
**Sibling Apple app:** `../miniti` (macOS + iOS)  
**Backend:** `../miniti-api` → `https://api.miniti.app`  
**Not this repo:** Flatpak / AppImage / Electron / Swift-on-Linux

---

## 1. What miniti is

Records mic (and on desktop, system audio), streams to Deepgram for live transcription with speaker diarization, generates AI insights via OpenAI (managed proxy or BYOK), and adds Smart meetings, calendar, CRM, Coaching, and Playbook.

| Platform today | Capture | Purchase rail |
|---|---|---|
| macOS | Mic + Core Audio Process Tap | Polar.sh |
| iOS | Mic only | StoreKit |
| Android (port) | Mic only | None in v1 |
| **Linux (this repo)** | Mic + PipeWire system audio | **Polar.sh** (same as macOS) |

---

## 2. Locked product decisions

| Topic | Decision |
|---|---|
| Parity | **macOS-complete intent** (~70–85% realistic year-one; document hard gaps) |
| Stack | Tauri 2 + Rust + web UI (WebKitGTK) |
| Ship | GitHub Release **binary tarball** + `.deb` + **Omarchy package repository** (`miniti-bin`); never the AUR |
| Pro | Polar web checkout / portal / license restore |
| Platform id | `X-Platform: linux` |
| Qt | Fallback only if spike fails |
| Versioning | Marketing version lockstep with Apple apps when possible; independent build numbers OK |

### In scope (parity target)

- Mic + system audio, dual waveforms, independent source toggles  
- Deepgram Nova-3 live transcript, 11 languages, diarization, speaker naming + manual rename / mark-as-you  
- Dual-channel mic/system attribution + echo reconciliation (port macOS algorithms once both streams work)  
- Live + saved insights: Summary, Questions, Coaching (local metrics), Sales/MEDDPICC, Playbook/Docs MCP  
- Catch-up, investigation (web + codebase; never auto-run)  
- Google Calendar, Smart meetings (call sensor + quiet/calendar), tray + floating presence + notifications  
- History (pin/search/trim), Granola CSV import, markdown export, webhooks  
- Gates: force-update, terms, onboarding  
- Managed Free / Pro / BYOK, usage banner, device-disabled handling  
- Dark terminal-style UI  

### Explicit Linux-limited (ship with caveats, don’t block v1 on perfection)

- Process-Tap reliability on every distro / every BT headset route  
- Core Audio–grade “call app is using mic” detection (approximate via PipeWire clients + known binaries)  
- In-app Sparkle-class silent update for pacman users (the Omarchy repository updates instead)  
- TextKit-identical multi-hour selectable transcript without dedicated perf work  

---

## 3. Tech stack

| Concern | Choice |
|---|---|
| Shell | Tauri 2 |
| Native | Rust (audio, call sensor, keyring, FS, deep links) |
| UI | Web — **React + TypeScript + Vite** (chosen at scaffold); dark tokens from Apple `ColorPalette` |
| State | Rust-owned domain + UI store; isolate high-frequency levels/transcript |
| DB | SQLite (`sqlx` or `rusqlite`) — Meeting schema compatible with Apple fields |
| Prefs | JSON/TOML in XDG config, or `tauri-plugin-store` |
| HTTP / WS | `reqwest` + `tokio-tungstenite` (or equivalent) |
| Audio | PipeWire (primary); Pulse monitor fallback; `cpal` only where it helps |
| Secrets | libsecret via `keyring` crate; file UUID fallback |
| Tray / notify | Tauri tray + `notify-rust` / portal notifications |
| Deep links | `miniti-google://`, `miniti-attio://`, `miniti-twenty://` (desktop file MimeType/URL schemes) |
| Bundle id / app id | `com.miniti.linux` / binary name `miniti` |
| Builder baseline | **Ubuntu 22.04** (or older glibc still shipping WebKitGTK 4.1) for release binaries |

---

## 4. Proposed layout

```
miniti-linux/
  AGENTS.md
  PLAN.md
  docs/
    feasibility.md
    distribution.md
  packaging/
    omarchy-pkgs/
      miniti-bin/PKGBUILD      # mirror of pkgbuilds/miniti-bin in omacom/omarchy-pkgs
      miniti-bin/.omarchy/package.json
    miniti.desktop
  src-tauri/                   # Rust
    src/
      main.rs
      audio/                   # mic, system, mix, recovery
      call_sensor/
      deepgram/
      api/
      insights/
      coaching/
      db/
      device_id/
      webhook/
      mcp/
  src/                         # frontend
  .gitignore
```

Scaffold Tauri when Phase 0 starts — do not invent a second architecture.

### Gate routing (launch)

1. Force update if `min_version > current`  
2. Terms if accepted version stale  
3. Onboarding if first launch  
4. Main app (Home / Recording / Coaching / History / Settings)  

---

## 5. Backend API

Base URL: `https://api.miniti.app`  
Full contract: `../miniti-api/AGENTS.md`. Summary for this client:

### Auth headers (most routes)

- `X-API-Key` — shared app secret (XOR-obfuscate in binary; not real security)  
- `X-Device-ID` — stable UUID (libsecret)  
- `X-App-Version` — semver  
- `X-Platform` — **`linux`** (backend must accept this — see §16)  
- `X-App-Mode` — `byok` | `managed` (recommended)  

`403` + `device_disabled` → block recording.  
`402` → managed limit reached.  
`429` → rate limited.

### Endpoints Linux needs

| Method | Path | Notes |
|---|---|---|
| GET | `/api/version` | Force-update only for normal UX; `download_url` → GitHub Releases |
| GET | `/api/usage` | Managed minutes / tier |
| POST | `/api/session` | Managed Deepgram grant JWT |
| POST | `/api/session/end` | Durable; queue on failure |
| POST | `/api/insights` | Managed proxy; Coaching is local |
| GET | `/api/subscribe` | Polar checkout |
| GET | `/api/portal` | Polar portal |
| POST | `/api/restore` | Polar license key |
| POST/GET | `/api/google/*` | Calendar OAuth + events |
| POST/GET | `/api/attio/*`, `/api/twenty/*` | CRM |

**To mirror (2026-10-21, Apple 3.3.0):** the cloud switch is per account. `GET /api/usage` now returns `cloud_version` and, once set, `cloud_enabled`; `PUT /api/account/cloud { enabled }` (device auth + proof) writes it. Linux should follow a version it has not applied and write its own flips. Contract: `../miniti-api/docs/agents/08-cloud.md` § "One switch per account"; rules: `../miniti/docs/archive/cloud-sync-plan.md`.

**To mirror (2026-10-23, Apple 3.3.1, Granola import):** Granola's bulk export now has the columns `document_id, user_email, document_title, workspace_name, document_created, summary, notes, transcript`; the old alias lists matched none of the three that matter, so every row was skipped as "missing a date". The header fix (prefix-stripped aliases, `canonical_header`) is in `src-tauri/src/import/granola.rs` as of this commit. Still to port from `../miniti/Miniti/Services/GranolaCSVImporter.swift`: unlabelled transcripts (one fragment per line, no speakers) joined per sentence instead of merged into one block; meeting length estimated at 150 words a minute when the export has no end time; turn timestamps placed by word position; the markdown summary flattened to plain text (leading title heading dropped, `\&` unescaped, `**` removed); the zip Granola emails accepted directly (stored or deflated CSV entries, several `-00N.csv` parts, in name order). Plan and tests: `../miniti/docs/archive/granola-import-plan.md`, `../miniti/MinitiTests/MeetingModelTests.swift` Granola section.

**To mirror (2026-10-27, Apple 3.4.0, account email and UX):** `GET/POST/DELETE /api/account/email` now answer any enrolled managed account, free or Pro, on or off the cloud rollout allowlist, with the cloud switch on or off (the server-wide switch and store still apply); `GET` is bearer-only, `POST` and `DELETE` need proof, delivery configuration gates `POST` only. The Apple apps moved email to Settings, Account ("Your account", "Ways to sign in", "Devices", "Plan and billing"), show the verified address in full with a pending replacement beside it, call the key **Show recovery key** / **Replace recovery key**, offer **Switch account** and, on a broken installation key, **Sign in again**; a Home card "Keep access to your account" appears after the first saved meeting until an email is verified or the key acknowledged, dismissed per account. The consent screen keeps version 2 with shorter copy (live audio, title, attendees to Deepgram; transcript, notes, title, attendees to OpenAI; managed text through miniti, audio straight). Settings, Sending became three groups with one status word per row. Specs: `../miniti/docs/archive/account-sending-consent-ux-plan.md`.

### Session JWT

Prefer `access_token` + `expires_at` / `expires_in`. Connect Deepgram with `Authorization: Bearer <token>`. Refresh before reconnect if within ~60s of expiry. Persist `session_id` on the meeting for orphan end reporting.

Deprecated alias `temp_api_key` may still appear — prefer `access_token`.

### Managed flow

```
Launch:  GET /api/version (+ GET /api/usage if managed)
Start:   POST /api/session → Deepgram WS Bearer
Live:    periodic POST /api/insights (stagger modes)
Stop:    POST /api/session/end → final insights
```

### BYOK

No `/api/session`. Deepgram `Authorization: Token <user key>`. OpenAI direct with pinned models (see §7). Still call `/api/version`.

### Polar (Pro)

Same as macOS: open checkout/portal URLs in the system browser. Entitlement is server-side; app cannot fake Pro. Cross-platform policy: Polar does not unlock Apple IAP and vice versa — Linux Polar is its own rail (device-linked like macOS).

---

## 6. Deepgram (current macOS-aligned contract)

Query params (update if `../miniti/docs/reference/audio.md` moves):

- `model=nova-3`  
- `smart_format=true` (do **not** also send `punctuate`)  
- `filler_words=true`  
- `diarize_model=latest`  
- `interim_results=true`  
- `utterance_end_ms=1000`  
- `vad_events=true`  
- `endpointing=300`  
- `encoding=linear16`  
- `sample_rate=16000`  
- `channels=1` **or** `channels=2&multichannel=true` when mic+system  
- `language=…`, `keyterm` list (capped)  

**KeepAlive:** `{"type":"KeepAlive"}` every 5s while connected.  

**Audio:** raw PCM16 LE binary frames. Dual source: ch0 = mic, ch1 = system (stereo interleaved), matching macOS.  

**Auth:** managed `Bearer`, BYOK `Token`.  

**Speaker IDs:** system speakers low IDs; mic speakers `1000 + n` when multichannel (see Apple `SpeakerIdentityState`). Mono mic-only: mic range only.  

**Stabilization (port):** min words/duration for speaker change; confidence gating; new-speaker promotion; don’t promote speakers from interims alone.  

**Ignored frames:** `SpeechStarted`, `UtteranceEnd` (still may be enabled on the query string). Promote finals on `speech_final || is_final`.  

**Reconnect:** bounded backoff; generation counters discard stale results; transcript-starvation watchdog when levels show speech but no Results.  

### Languages (11)

`en es fr de pt it nl sv el pl ru` — filler defaults per language; store overrides per lang. Exact filler lists: pull from Apple `InsightsService` / `TranscriptionLanguage` when implementing.

---

## 7. Insights & Coaching

### Modes

| Mode | API `mode` | Notes |
|---|---|---|
| Summary | `standard` | Incremental + final |
| Sales | `meddpicc` | Opt-in specialist |
| Questions | `questions` | Types: deeper, challenge, reframe, clarify, explore, follow_up |
| Coaching | local only | Never call `/api/insights` for coaching metrics |
| Playbook | docs topic + ground | Docs MCP URL; managed free may meter |
| Catch-up | dedicated | Explicit user action |
| Investigation | `investigation` | Explicit only; web citations or local codebase excerpts |

### Models (pinned — verify against `miniti-api` before ship)

- Managed incremental / speaker naming / catch-up / Playbook topics: `gpt-5-mini-2025-08-07`  
- Managed non-incremental summary / MEDDPICC / questions / grounded Playbook / investigations: `gpt-5.4-mini-2026-03-17`  
- BYOK: `gpt-5-mini-2025-08-07` except investigations → `gpt-5.4-mini-2026-03-17`  

Prompts for BYOK must match `miniti-api` `app/api/insights/route.ts`. Managed mode: client sends transcript + mode + language (+ attendees); server owns prompts.

### Coaching (local)

Port `TrainingMetrics` / `CoachingAdvisor`: fillers, talk ratio, pace, monologue, questions, clarity. Overview: focus / stats / history. Do not reuse CTA colors for metric taxonomy.

### Apply safety

- Meeting/request identity guards discard stale insight results  
- `meta.degraded == true` → do not advance incremental ack cursor  
- Stop: finalize transcript save before slower insight work; allow home while insights finish  

---

## 8. Audio (Linux-specific)

### Mic

PipeWire / ALSA capture → resample to 16 kHz mono PCM16. Publish RMS for waveforms (~20 Hz coalesce).

### System

Primary: PipeWire monitor of default sink, or per-app nodes when selecting call apps.  
Fallback: Pulse `*.monitor` source via `pactl` / `PULSE_SOURCE` patterns used by other Tauri apps.  

Require PipeWire for “system audio supported” in UI; degrade to mic-only with a clear message otherwise.

### Dual-channel + echo

Port Apple algorithms from `../miniti/docs/reference/audio.md`:

- Ring-buffer interleave; pad system underruns with silence  
- ~350 ms ordering buffer; echo suppress ambiguous mic finals that match overlapping system text  
- Reconnect-aware meeting timeline using Deepgram word times  

### Recovery

Port concepts: silent-stall, callback-stall, default-output change debounce, post-recovery health check, ending-grace `suspendSending` (drop samples before Deepgram clock advances; keep metering).

### Call sensor

No Core Audio process objects. v1 approach:

1. Pure policy: port `CallLifecycleEngine` / known-app directory from Apple `CallLifecyclePolicy`  
2. Sensor: PipeWire clients with capture streams + known binary/desktop-id map (Zoom, Teams, Slack, browsers-as-weak, etc.)  
3. If sensor weak: quiet + calendar handoffs (iOS-proven subset)  

---

## 9. Desktop shell

- **Tray:** elapsed REC timer + Smart meeting actions  
- **Floating presence:** always-on-top window; expand for decisions / nudges; Wayland will be imperfect vs `NSPanel`  
- **Notifications:** when another app focused or indicator disabled  
- **Shortcuts:** in-app first (match macOS local monitors); portal global shortcuts optional later  
- **Deep links:** register URL schemes for OAuth returns  
- **Export / investigate:** normal POSIX folder pickers (no security-scoped bookmarks)  

---

## 10. Data model (SQLite)

Match Apple field semantics for webhooks and mental model:

**Meeting:** id, title, start/end, notes, language, summary, action items, key decisions, topics, discussion flow, MEDDPICC fields, suggested questions JSON, speaker names/overrides JSON, self speaker IDs, managedSessionId, calendarEventId, attendees JSON, pin, insightsUpdatedAt, import provenance, docs/playbook JSON as needed.

**TranscriptSegment:** id, meetingId, speaker, text, start/end (seconds from meeting start), timestamp, optional source (`microphone`/`system`).

`displayTitle`: strip legacy timestamp prefixes; never show empty.

Prefs: app mode, keys (BYOK), webhook URL, language, fillers, Smart meetings toggles, indicator/tray, export folder, Docs MCP URL, etc. (XDG `~/.config/miniti/`).

---

## 11. Webhook payload

Fire-and-forget POST on `meeting.saved` / `meeting.updated`, 10s timeout. Shape must match Apple (title, times, duration, language, insights, `training` coaching blob, suggested_questions, speaker_names, transcript array, optional calendar/attendees). See `../miniti` `WebhookService.swift` or Android PLAN §11 when implementing.

---

## 12. Design

- Dark only  
- Port `ColorPalette` + spacing/radius/control roles from Apple design system  
- Monospaced identity for brand lockup / timers; readable system UI font for prose  
- Respect prefers-reduced-motion  

---

## 13. Build order

### Phase 0 — Spike (2–3 weeks) — **do this first**

Kill criteria: dual capture → Deepgram multichannel (or mono×2 proof) on **Arch** and a binary built on **Ubuntu 22.04** running on Arch.

1. `pnpm create tauri-app` (or equivalent) skeleton — **done** (React+TS; `com.miniti.linux`; dev env in `.cursor/`)  
2. Rust mic capture → PCM16 16 kHz → Deepgram BYOK — **code done and wired** (`audio::capture` cpal + `deepgram::live` with CloseStream drain, reconnect, per-word speaker segmentation); **not yet run against a live socket** — needs a key + audio device on real hardware  
3. PipeWire system/monitor capture — **code done** (`parec` monitor reader, frame-aligned reads); metering wired, not sent to Deepgram yet  
4. Stereo interleave + `channels=2&multichannel=true` smoke test — interleave util + multichannel query + channel routing **done** and unit-tested; live mic+system sync engine **TODO** (this is the kill criterion)  
5. Minimal UI: levels + live transcript text — **done** (Record view: meters, connection status, interim-replacing live transcript)  
6. Document distro failures — pending real-hardware runs (Arch ThinkPad)  

Phase 1 core is wired (not just scaffolded): SQLite persistence, prefs, keyring device id, managed session start/end, launch gates via `/api/version`, webhook on save, coaching port, history editing. `insights` and `call_sensor` are contract/policy only. See AGENTS.md § Status for the exact wired vs contract-only split.

**Stop and reassess (Qt?) if system audio cannot be made reliable on Arch + one other distro.**

### Status 2026-09-06

Backend authentication is device-bound as of 0.3.0 (roadmap P0.1 Phase 4, implemented first on Linux): no `X-API-Key` in the binary; `src-tauri/src/auth` + `/api/auth/*`. Flip `CLIENT_AUTH_MODE_LINUX=require` and `LINUX_MIN_VERSION=0.3.0` on the backend once every Linux install runs 0.3.0+.

Phases 1–5 are implemented in code (see AGENTS.md § Status for the wired table). Mono transcription is verified on Arch; the dual-source spike (step 4 above) and the Smart-meetings / integration paths need their real-hardware pass. Remaining engineering after that pass: Bluetooth/route-change recovery for the system tap, release CI on the Ubuntu 22.04 baseline, and the AUR PKGBUILDs.

### Phase 1 — Core meeting (8–12 weeks)

SQLite meetings, save/resume, BYOK + managed session, history, gates, settings shell, debug log.

### Phase 2 — Insights parity (6–8 weeks)

Summary / Questions / Coaching / Sales / Playbook, catch-up, investigation web+codebase.

### Phase 3 — Desktop shell (6–10 weeks)

Tray, floating presence, notifications, Polar Pro, force-update download URL, OAuth deep links.

### Phase 4 — Smart meetings (6–10 weeks)

Call sensor v1 + quiet/calendar; ending grace.

### Phase 5 — Ship hardening (4–6 weeks)

Release tarball CI, the Omarchy repository package, CRM, Granola, markdown export, long-meeting transcript perf, supported-distro matrix doc.

Rough calendar: **9–15 months** to near-parity with 1–2 engineers after a green spike.

---

## 14. Distribution (summary)

See [docs/distribution.md](docs/distribution.md).

- Artifact: `miniti-VERSION-x86_64-unknown-linux-gnu.tar.gz` (binary + `.desktop` + icons + LICENSE + README deps)  
- `.deb` from the Tauri bundler for Debian users  
- Omarchy package repository: `miniti-bin` (prebuilt, follows GitHub releases). Never the AUR  
- **No** Flatpak / AppImage  
- **No** Tauri updater plugin  
- Force-update: `/api/version` → GitHub Releases URL  

Runtime depends (typical): `webkit2gtk-4.1`, `gtk3`, `libsoup`, cairo/pango stack, `pipewire` / `libpulse`, `libsecret`, tray indicator libs as needed.

---

## 15. Required backend changes (`miniti-api`)

Do in the API repo when Linux approaches a testable client (not blockers for Phase 0 BYOK):

1. Accept `X-Platform: linux` everywhere platform is validated/logged  
2. `LINUX_MIN_VERSION` env (default `0.0.0` or `1.0.0`)  
3. `LINUX_DOWNLOAD_URL` → GitHub Releases latest (or versioned)  
4. Admin/device listing shows `linux`  
5. Polar already device-scoped — confirm Linux devices can subscribe/restore like macOS  

No StoreKit / Apple webhook work for Linux.

---

## 16. Feasibility snapshot

Full write-up: [docs/feasibility.md](docs/feasibility.md).

| Area | Parity | Difficulty |
|---|---|---|
| Mic → Deepgram | full | easy |
| System audio | partial | hard |
| Dual-channel + echo | near | hard |
| Insights / Coaching / API | full | medium |
| Calendar / CRM / Polar | full | medium |
| Smart call detection | partial | hard |
| Tray / floating UI | near | medium |
| Binary + Omarchy repo ship | near | medium |
| Long selectable transcript | near | hard |

---

## 17. Non-goals

- Flatpak / AppImage / Snap as primary (or any required) channel  
- Electron  
- Compiling the Swift app on Linux  
- Claiming 100% macOS audio/call parity on day one  
- Auto-running investigations  
- Light mode  
- Sharing a git repo with `../miniti`  

---

## 18. Agent sanity checks

1. Read `AGENTS.md` + this plan before scaffolding.  
2. Phase 0 BYOK mic→Deepgram before UI chrome.  
3. Re-verify Deepgram query params and model pins against `../miniti/docs/reference/audio.md` and `../miniti-api` at implementation time — this plan is a snapshot.
4. Do not enable Tauri updater signing; pacman owns updates.  
5. Do not commit secrets.  
6. Ask before `git init` / remote / publish if the user has not requested it.

## Tracking: on-device speaker separation (macOS/iOS v2.10.0, 2026-09-25)

The Apple apps gained an optional (default off, beta) on-device diarizer: NVIDIA Nemotron-3-Diarization through FluidAudio (Core ML). When active they open the Deepgram socket without `diarize_model` and assign each word's speaker from a local 10 ms speaker-activity timeline. Backend contract unchanged (diarization is client-side). For parity here: use the ONNX build (`onnx-community/Nemotron-3-Diarization-ONNX`, ONNX Runtime) with the same streaming preset (1.04 s), the same word-assignment rule (majority dominant speaker over the word's timestamps, borrowed neighbour at low confidence past the horizon), and the same fallback to Deepgram diarization. Until then Linux keeps `diarize_model=latest`. Design and evidence: `../miniti/docs/archive/on-device-diarization-plan.md`, `../miniti/scripts/diarization-poc/RESULTS.md`.

2.12.0 (macOS, 2026-10-09, default off): on-device transcription with NVIDIA Nemotron 3.5 ASR Streaming Multilingual 0.6B (Core ML via FluidAudio, 1120 ms chunks) replaces the Deepgram socket for the whole session when the setting is on and the model is loaded; en/es/fr/de/pt/it tested, nl/ru experimental, el/pl/sv stay on Deepgram. Backend contract: such meetings open no `/api/session` and report no minutes; `/api/insights` is unchanged. For parity here: NVIDIA publishes ONNX builds (`onnx-community/nemotron-3.5-asr-streaming-0.6b-onnx-int4`); reuse the same utterance rules (final on sentence punctuation, 1 s silence at the decoded horizon, 12 s cap) and the same no-silent-fallback rule. Design and evidence: `../miniti/docs/archive/local-transcription-plan.md`, `../miniti/scripts/transcription-poc/RESULTS.md`.

Also 2.12.0: `/api/insights` gains `mode: "template_draft"` with `description` (3 to 1,000 chars; `transcript` carries the same text for the shared length rule) returning `{ template: { name, summary, sections: [{ title, guidance }] }, meta }`, used by the template editor's "Draft it" button; BYOK clients call OpenAI with the same prompt (see `miniti-api/lib/insights.ts` `generateTemplateDraft`). The insights More menu is now flat (Sales with the templates, then Playbook) and remembers the specialist view per meeting.

## Tracking: calendar filter rule, quiet prompt speech signal, inferred call association (macOS/iOS v2.10.0, 2026-09-25)

Three client-side behaviour changes in the Apple apps; no backend or Deepgram query change. For parity here:

1. **Calendar meeting filters**: a new default-on rule `skipWithoutGuestsOrLink` in the local filter preferences skips timed events with no conference link, no attendee other than self, and no organizer other than self (a self-made block such as "kids pickup" was auto-starting recordings). Skip reason label `no guests or meeting link`. The backend's `include_filtered=true` default filter is unchanged. Stored preferences decode tolerantly so an older blob keeps its custom prefixes.
2. **Quiet-ended prompt**: the "speech-like audio" gap now comes from Deepgram's `SpeechStarted` event (`vad_events=true`, already sent) while transcription is healthy, and from the raw capture level only while reconnecting/degraded. Only transcript activity dismisses the quiet or call-ended ask and clears the "Keep recording" suppression; a level burst still cancels an automatic calendar handoff countdown. Threshold drops to 2 minutes once the meeting's calendar event has ended, independent of the calendar auto-stop setting. Hard silence auto-stop keeps the raw level.
3. **Call association** (`CallLifecycleEngine` port): a plain start with no call active and no join link arms a 120 s window; the first recognized call to appear (after the 2 s start debounce) becomes an *inferred* association whose end only asks (never automatic ending grace) and which does not count as remote-meeting evidence. Two or more calls active at start arm nothing.
4. Unrelated to Linux: the macOS floating panel drag was rebuilt on an explicit AppKit drag handle (macOS 27), and the Google Calendar launch status check now retries instead of treating a network failure as disconnected. The retry ladder (5s/15s/45s/120s, then 5 min) and the activation/wake recheck are worth mirroring if the Linux client has the same launch-before-network gap.

Details: `../miniti/docs/archive/smart-meetings-plan.md` §3–4, `../miniti/docs/archive/call-lifecycle-and-recording-presence-plan.md` § Call association, `../miniti/docs/reference/patterns.md` (Google Calendar integration).

## Tracking: Jev classifier for speaker naming and the sales nudge (macOS/iOS v2.10.0, 2026-09-25)

New backend route `POST /api/classify` (device auth with proof, `classify` rate bucket). The Apple apps, behind a default-off managed-only setting, send `kind: speaker_names` (transcript of prefiltered `[SpeakerID:N]` turns, attendees, unresolved speaker IDs) and get one attendee name or `unknown` per ID, and `kind: sales_nudge` (last 40 finals) every 8 finals and fire the sales suggestion after two consecutive `isSales >= 0.7`. Any failure or a 503 `classifier_unavailable` hands back to GPT / the phrase list. For parity here: same request shapes and thresholds; see `../miniti/docs/archive/jev-system-one-plan.md` (Implementation status) and `../miniti-api/lib/classify.ts`.


## parity note 2026-10-02 (from the Apple apps, 2.10.0)

- calendar-linked meetings keep the event title: the Apple apps now ignore `suggested_title` from `/api/insights` (live and final) when the meeting has a `calendarEventId`. Mirror this in the Linux client. Also 2.10.0: on-device Nemotron diarization (default off), Jev classifier via `/api/classify` (default off), a macOS microphone picker, and a macOS fix for the mic engine stopping after an output-device switch. Backend minimum version for macOS/iOS is now 2.6.0 and their client-auth mode is `require`; Linux is unchanged.

## parity note 2026-10-10 (from the Apple apps, 2.12.1)

- A real device got `invalid_proof` on every challenge grant because the backend held a different public key for its device UUID (same account). The Apple clients now recover on their own: when `/api/auth/token` with `grant_type: challenge` answers 401 `invalid_proof`, call `/api/auth/account/restore` with the stored recovery key and the current key (the backend re-binds the same account to the new key without a device slot, `client_auth_rebind`), then continue with the issued tokens. Sign out also clears local credentials on any 401 or on a refused re-bind (`device_already_enrolled`, `recovery_key_unknown`), so a device never needs a token to sign out. Mirror both in `src-tauri/src/auth/manager.rs` (`grant_challenge` and the sign-out path).
- The actual cause of that device's lockout was a draft: the Apple client wrote key + recovery key to the store before `/api/auth/account/create`, the server answered 409 `device_already_enrolled` (the phone had been restored from a backup, so the device id survived but the device-only credentials had not), and the stored draft then counted as "enrolled", so every token grant was signed with a key the server never saw. Fixed on the Apple side by (1) treating a draft (no account id, no token) as not enrolled, and (2) on `device_already_enrolled` from create, rotating the device id to a fresh UUID and creating the account under it (the old id's usage, calendar and CRM records are left behind; a saved recovery key moves the device back with `/api/auth/account/attach`). Check whether the Linux manager has the same draft semantics and the same 409 dead end after a config-dir restore.


## Cloud sync parity (2026-10-15, from the Apple apps' 3.0.0)

The Apple apps and `miniti-api` gained an optional, Pro-only cloud: finished meetings synced across every device on the account, an optional email for web sign-in and new-device sign-in, personal access tokens for a read-only REST API and an MCP server, and OAuth for hosted connectors. The contract this client must follow is `../miniti-api/docs/agents/08-cloud.md` (sync document schema_version 1, `/api/sync/*`, the client protocol with manifest reconcile, revision CAS, tombstones, the trim guard, last-writer-wins on `updated_at`). Differences to absorb here: `import_source: "granola:<id>"` splits into `external_source` + `external_id`; attendee `name` maps to the wire `name`; epoch seconds become ISO 8601 with fractional seconds; `created_at`, `investigations`, `join_url`, `sales_enabled`, `title_auto` ride as extra fields the Apple apps echo untouched. SQLite needs the same bookkeeping columns (`updated_at`, `synced_at`, `cloud_revision`, `sync_fingerprint`, `local_fingerprint`, `sync_excluded`, `sync_state`, `sync_extra`) and a `meeting_tombstones` table. Linux has no private mode, so `sync_excluded` is only user-set. Settings gets a Cloud group. Not started.

## Tracking: Apple Watch recording (iOS v3.1.0, built 2026-10-18)

iOS gains a watchOS companion that records AAC on the wrist and hands the files to the phone, which transcribes them with Deepgram prerecorded (`diarize`, `utterances`) and imports one meeting with `externalSource: "watch"`. No Linux equivalent (hardware-bound). The only backend contract change is additive and reusable here if Linux ever imports saved audio: `POST /api/session/prerecorded` (body `{ language?, model?, recording_id? }`, same auth, limit and rate gates as `/api/session`) returns a Deepgram grant whose session is metered by the `duration_minutes` the client reports on `/api/session/end` (capped at 4 hours) instead of wall-clock elapsed time. Design: `../miniti/docs/archive/apple-watch-plan.md`.

Also 3.1.0: the sync document gains `recorded_on` (`mac`, `iphone`, `ipad`, `watch`, `linux`, or null), set when a recording starts; Linux should write `linux` on its own recordings and keep the key on round trips. History search gains `from:<device>` (and `from:granola`) with a device menu in the filter row listing only devices present. Fixture `meeting-v1.json` carries `"recorded_on": "mac"` in both repos; mirror it.

## Tracking: speaker emails and voice prints (3.2.0, in progress 2026-10-18)

The sync document gains `speaker_emails` (speaker id to calendar attendee email, lowercased; hand picks plus clean name matches, see `SpeakerAttendeeMatcher` in the Swift tree: full name, unique first name, or the email local part). Webhook payloads gain `speaker_emails` and `attendees[].speaker`; the REST and MCP meeting too. Voice prints (CAM++ embeddings per person, on device, opt-in) are being measured; design in `../miniti/docs/archive/voice-prints-plan.md`. Prints never enter the sync document.

Apple Settings parity follow-up (2026-10-29, unshipped): fold the Sending overview into Privacy and add Models under Setup for supported local models, with Configure links to existing feature controls. Preserve old Sending/search links. Do not list Apple-only models as available on Linux.

Apple Help parity follow-up (2026-10-29, unshipped): Help replaces Account’s support disclosure with expanded metadata and Copy/Preview/Send report. Public `/api/support/report` uses fixed server recipient, strict limits and only code/category diagnostics by default; optional reply email/description are user-entered. Details in sibling API `docs/agents/09-followups.md`. About acknowledgements move last with shared disclosure style.

### Apple local diagnostic timeline parity (2026-10-29, not implemented on Linux)

Apple O6 adds a local bounded sanitized timeline for auth/Keychain, recording, cloud and purchase return, exported only through explicit Help reporting. Backend `/api/support/report` additively accepts fixed `fields`, `count`, `last_time`, info milestones and auth/keychain/cloud/purchase categories; old report shapes remain supported. Mirror the server allowlists if implementing Linux Help reporting, never send raw debug strings. Apple retention: 300 events, 96 KiB, 48 hours, bounded pending admission, repeated-event folding and batched writes. This is tracking only, no Linux implementation or release claim. See `../miniti/docs/plans/logging-and-monitoring.md` O6.

Home Feedback parity (Apple local, 2026-10-29): bottom-right Home message/optional-reply sheet, no automatic diagnostics, shared Help route with optional `kind: feedback`. Shared server limits: one/30-second fixed network window, three/hour/network, 100/day global; respect Retry-After. Apple API mode UI is now two stacked choices. Linux tracking only, not implemented here.


## Tracking: Apple 3.6.0 sync and delivery reliability (2026-10-30, local implementation)

Tracking only: no Linux code or release change in this entry. Apple implementation and acceptance live in `../miniti/docs/plans/sync-and-delivery-reliability.md`; mechanics are in `../miniti/docs/reference/patterns.md` and `cloud-sync.md`. Reimplement in Rust/Tauri against Linux's existing persistence/outbox lifecycle, not Swift APIs. The earlier cloud parity work remains separately unimplemented.

- **S1/S2, cloud:** distinguish outbound upload eligibility from protection against incoming changes. After the stopped transcript is durable, a meeting may upload while selected and while final AI runs; later AI commits dirty it again. Selected/finalizing rows remain protected from inbound replacement/deletion/conflict. Protection release reconciles skipped server changes. Report pass time and count-fetch time separately; refresh dirty/missing counts or after five minutes, show stale counts/pending work honestly, and do not undo accepted uploads on a failed status read. No backend sync schema change.
- **S3, webhook completion:** Going Home must not emit a live snapshot. Deliver the saved transcript and available summary after requested final work settles, regardless of the open screen; skipped/failed work still yields a transcript with honest outcomes. No cloud dependency or global AI deadline. Apple uses a local optional completion marker and a held outbox row: persist ready → durable held row → persist handedOff → release. Relaunch resolves interrupted/ready work without replaying historical meetings or recreating a delivered row.
- **Additive payload:** optional top-level `processing` has `input_transcript_revision`, `state` (`complete`, `partial`, `failed`, `skipped`, `interrupted`) and a per-mode `outcomes` map (`complete`, `failed`, `skipped`, `interrupted`, `stale`). Transcript and available summary stay in the payload. Receivers need no cloud fetch. Ignore unknown additive fields; the backend does not transform configured webhook payloads.
- **R5/R6, delivery:** retries retain the exact body and `X-Miniti-Delivery-Id`. New content gets a new ID; an in-flight row cannot remove its newer successor. Persist the attempt before sending, revalidate queued work after awaits, and keep saved before updated for the same meeting. Failed disk writes must not acknowledge enqueue/replacement. Terminal webhook resend reconstructs current saved data under a new ID/current configured destination and atomically replaces failed rows for that meeting, omitting stale generated results; deleted, processing or unsaved meetings must not resend old content. CRM retry policy need not change.
- **R7, saved AI:** per-meeting/request authority covers transcript revision and prompt inputs, template/language/manual-speaker changes, account/credential generation and newer requests. Recheck after awaits before model/UI/export writes; old tasks clear only their own progress. Save failure restores displayed and stored state. Include Docs topic/lookup completions, undo, resume and deletion tests.

Apple queue forward compatibility does not imply safe downgrade: 3.5.0 ignores the new held flag and can dispatch before marker acknowledgment. Linux should design its own explicit queue-version/downgrade policy when porting. Test with held fake senders and isolated stores, including lost-response reload, old completion/new successor, stop while selected, count-refresh failure, History edits during AI and disk-write failure. Keep Linux version constants and release schedule independent.


### Recording-recovery evidence gap (2026-10-31)

Tracking only, no Linux implementation or release change. The backend incident evaluator accepts `recording_recovered` with `pcm_sent=true` only from verified Apple 3.5+ producers. No Linux emitter for this event/evidence was found in `src` or `src-tauri`; version numbers alone must not establish recovery. Linux recording failures remain recovery-unconfirmed until a Linux producer contract, minimum supporting version and tests prove audio resumed after failure. A connected socket or a later absence of errors is insufficient; resumed PCM still does not prove transcript completeness. Coordinate the capability predicate change in `../miniti-api/lib/clientIncidents.ts` with that future implementation.


### 3.6.0 review clarifications (2026-10-31)

Tracking only; no Linux code or release change. Configured completion exports remain independent of the managed account, while old-account AI answers remain fenced. Apple's local completion marker captures a temporary SHA-256 webhook URL fingerprint; a changed/cleared URL pauses ready/held handoff with a visible action, and explicit Try again accepts the current destination under a fresh ID. Already released queue rows retain the existing current-URL sender policy. Preserve that distinction when porting.

Template inputs belong only to the template job's authority; changing a template must not discard unrelated final Summary/Sales/Questions. The stopped-meeting picker persists the new selection before an old template child can commit. Stopped and History AI Update actions are disabled as finishing during final work (also checked inside the action), while Coaching still recomputes locally. Marker-only persistence does not notify cloud unless the same save also commits already-dirty content. Reconcile only marked meetings and fetch/index current meetings once per resend operation. The recording-recovery evidence note above is unchanged.

### Apple cloud QA corrections (2026-10-31)

Tracking only, no Linux implementation or release change. A save debounce must only cancel its pending wait, not an already-running upload. Later commits queue another pass; cancelled transport/status requests retain pending work and prior successful-check/count timestamps without reporting a server failure. Pending cloud-copy removal must distinguish automatic retry from cloud-off, private-mode, plan and sign-in gates; do not change consent or turn cloud on implicitly. Port the behavior to Linux's own sync lifecycle with held-request tests when implementing cloud parity. Apple iOS Settings sheet/navigation correction is platform-specific.


### Apple handoff retention and destination corrections (2026-10-31)

Tracking only, no Linux implementation or release change. Apple's replacement 3.6.0 candidate applies the 24-hour original-age limit to held rows as well as released rows, schedules expiry even with no sender, and discards expired bodies while retaining current-data resend. Only released pending saved events block updates. A ready handoff refused by a destination guard still allows ordinary meeting.updated to the current URL. The initial completion stays pending for an explicit destination decision.

Use a temporary destination fingerprint instead of persisting the full URL in meeting metadata; clear it only after durable release. Legacy plaintext markers confer unknown destination authority and need explicit retry. Terminal-row reconciliation and resend must resolve the initial marker so recovery cannot reconstruct expired delivery bodies. Preserve immutable IDs and original age across migration, display the actual block reason, and retain the forward-patch downgrade policy. No webhook event or payload schema changes. Port and test against Linux's own stores before claiming parity.

## Tracking: managed lightweight Luna (2026-10-31, local only)

Apple 3.6.0 includes a bounded backend migration: `speaker_names`, `catchup`, `docs_topics` and `template_draft` use `gpt-6-luna` with reasoning `none`. The backend routing was deployed on 2026-10-31 with both Luna opt-ins unset, retaining the previous models. Luna now defaults off in code: unset, blank, `off` or unrecognized values retain GPT-5 mini low. Only explicit `INSIGHTS_LIGHTWEIGHT_LUNA=on` enables it, followed by a separate authorized redeploy after review. Verify no explicit opt-in for the initial backend rollout; an unset variable needs no change. Existing Linux managed callers inherit the server-selected routing when enabled; client `model` remains ignored and response shapes are unchanged. Template drafting creates a definition, while filling a meeting template stays on the quality model.

- [x] Record shared API routing, rollback and unchanged Linux BYOK. `src-tauri/src/insights/mod.rs` model constants remain GPT-5 mini/GPT-5.4 mini; no Linux code or binary changed.
- [ ] After authorized backend deployment, verify available Linux managed speaker naming, catch-up and Playbook topic extraction through the normal client. Template-draft UI parity remains the separately tracked 2.12.0 follow-up above; this backend change does not implement it.
- [ ] Check telemetry/errors after rollout, without sending customer transcripts to an evaluation. Rollback is backend `INSIGHTS_LIGHTWEIGHT_LUNA=off` plus redeploy. This restores only the four modes to GPT-5 mini / low; `INSIGHTS_INCREMENTAL_QUALITY_MODEL=off` remains independent.

The subsequent 2026-10-31 standard-summary extension adds independent `INSIGHTS_STANDARD_LUNA=on` for managed full and incremental `standard` requests. It defaults off; off/unset/invalid retains GPT-5.4 mini low. `INSIGHTS_INCREMENTAL_QUALITY_MODEL=off` takes precedence and restores GPT-5 mini low for incremental quality requests even with standard Luna on. Questions, MEDDPICC, meeting-template filling, grounded Docs and investigations retain their existing routing/reasoning. No prompt, cadence, allowance, price, private-mode or account boundary changes. Scope/evidence: [bounded plan](../miniti/docs/plans/lightweight-luna.md), [3.6.0 release record](../miniti/docs/releases/3.6.0.md). No Linux release claim.

### Standard summaries extension, 2026-10-31

- [x] Track server-owned full/incremental `standard` Luna selection, independent from the four lightweight tasks. No Linux source, BYOK, prompt or response-shape changes.
- [ ] After separately authorized activation, verify Linux managed live, final and saved-meeting standard summaries, selected-model telemetry, latency and owner/date preservation. The initial backend rollout keeps both Luna opt-ins off.
- [ ] Verify rollback with `INSIGHTS_STANDARD_LUNA=off` plus redeploy; it leaves lightweight selection alone. The existing incremental-quality switch retains precedence. No release or device-verification claim follows from this tracking entry.

### Tracking: Apple iOS 3.6.0 availability, 2026-11-01

Apple’s public App Store lookup confirms iOS 3.6.0 is available. The backend iOS latest-version marker advances to 3.6.0; Linux versions, minimums, download URL, BYOK and binary are unchanged. Both production Luna opt-ins remain unset/off. This is release metadata tracking, not Linux feature parity or a Linux release.

### Both managed Luna groups activated, 2026-11-01

Ian explicitly authorized both production opt-ins after baseline observation. The shared backend now uses Luna none for speaker names, catch-up, Playbook topics, template drafting and full/incremental standard summaries. Synthetic live API checks passed; selected-model telemetry verifies Luna and unchanged Questions routing. Existing Linux managed requests inherit the routing; no Linux client source, BYOK or binary changed. Physical Linux client checks above remain open. Roll back either group with its `INSIGHTS_LIGHTWEIGHT_LUNA=off` or `INSIGHTS_STANDARD_LUNA=off` setting plus redeploy; legacy incremental-quality rollback keeps precedence.

## Tracking: recording and client AI diagnostics (2026-11-03)

Apple 3.7.0's [recording/client follow-up](../miniti/docs/plans/recording-reliability-followup.md) owns bounded final drain, truthful capture/connection/transcript evidence, activity-ID normalization and the newly explicit investigation of live/final AI failures despite successful backend responses. The observed incidents are Apple evidence, not proof of identical Linux defects. Linux keeps its own release line; this is parity tracking only.

- [ ] Review Linux stop-before-first-result and reconnect generation ownership against the bounded acceptance cases; preserve local-engine privacy and exclude automatic cloud fallback/audio replay.
- [ ] Define and test Linux attempt/engine/recovery evidence before enabling its backend recovery capability. Connection-open alone cannot establish words resumed.
- [ ] When the additive diagnostic contract is implemented, track AI mode/stage, fixed failure category and bounded request correlation through Linux's own transport/decode/apply lifecycle. Distinguish expected cancellation/stale authority from genuine failure; never include transcript, generated content, credentials or raw exception text.
- [ ] Coordinate ingest/Help allowlists and old-client compatibility with the backend before client delivery. Preserve independent BYOK routing, saved-result authority and bounded retry behavior. No Linux implementation or device validation is claimed here.

## Tracking: GPT-5 mini retirement (2026-11-03)

Selected alongside Apple 3.7.0; Linux ships on its own version line. Implemented locally, no release claim. The [bounded migration](../miniti/docs/plans/gpt-5-mini-retirement.md) owns the approved model split and cross-client acceptance: `gpt-6-luna` with reasoning `none` for summaries/lightweight tasks, `gpt-5.4-mini-2026-03-17` with `low` for other AI tasks and backend fallbacks. Both `gpt-5-mini` and the pinned `gpt-5-mini-2025-08-07` shut down on 2026-12-11. Backend migration cannot fix older Linux binaries making direct BYOK calls.

- [ ] Implement and validate the approved BYOK split in `src-tauri/src/insights/mod.rs` and direct request builders: Luna none for standard summaries, speaker naming, catch-up, Docs topics and template drafting wherever implemented; GPT-5.4 mini low for MEDDPICC, Questions, template filling and grounded Docs answers. Investigations keep GPT-5.4 mini low. Do not implement missing feature parity implicitly or inherit the old low-reasoning default for Luna.
- [x] Update direct request construction, parsing tests and Settings model descriptions. Preserve prompt/response contracts, bounded retries, account fences and cadence; record any necessary compatibility changes.
- [ ] Verify inherited managed behavior after backend fallback/probe migration, including supported rollback configurations. Current managed Luna/quality routing stays unchanged.
- [ ] Release the Linux update before 2026-12-11, record client evidence and provide older-BYOK-binary upgrade guidance. Keep Linux version/minimum changes in its own release process; no automatic force-update policy is selected here.

## Tracking: Apple Mac local speaker reliability and device diagnostics (2026-11-03)

Apple 3.7.0 selected [Mac local speaker reliability/device diagnostics](../miniti/docs/plans/local-speakers-default.md): fix inference failure handling, preserve choices/iOS/voice flags, and add bounded consent-gated hardware/actual-engine metadata. Ian revised the scope on 2026-11-03 to keep Mac local separation off by default, preserving existing enabled users. Default-on is deferred until M1 testing, with no later version assigned. Linux keeps its current engine/default and independent release line; this does not port Core ML or add Linux hardware telemetry. Backend fields will be additive and older clients remain unknown, not unsupported. Record final field allowlists/coverage and assess any Linux producer separately when the contract is implemented. Planning only, no schema or runtime change.

3.7.0 integration review: the [Apple release checklist](../miniti/docs/releases/3.7.0.md) owns cross-plan gates. All managed recording minutes, including Apple local/private duration, are accounted independently of Share diagnostics; BYOK reporting remains off. Existing Linux Deepgram reporting is unchanged and must not be doubled by a new route. This does not add Linux local transcription/telemetry. Keep shared wire fixtures and the model-retirement deadline aligned; implement any future Linux producer only after its contract is tracked here.

### Test foundation implementation handoff (2026-11-03)

[Tracking record](docs/test-foundation-followup.md): backend/Apple T06 exposed omitted prior questions in the incremental prompt; Linux BYOK now includes the baseline and has a regression test. Shared synthetic fixture versioning is tracked there. No provider routing or release version changed.

Retirement local evidence (2026-11-03): actual BYOK chat-body fixtures verify supported literal model IDs and explicit reasoning across full/incremental tasks. Full Rust suite: 220 passed, 1 ignored live-enrollment test. `pnpm build` passed. Checks ran on macOS, not Linux desktop/audio; provider availability/quality/cost and distribution remain open. No push or release.

## Tracking: recording reporting v1 (2026-11-03)

[Contract handoff](docs/recording-reporting-v1.md) records the locally implemented shared usage endpoint, exact owner semantics, atomic deduplication, coverage, and additive fixed diagnostic schema. Existing Linux Deepgram reporting remains unchanged. No new Linux engine, telemetry producer or release is selected.

Apple local-speaker failure handoff (2026-11-03): [tracking](docs/local-speaker-failures-370.md) records compatible Unknown speaker names and the optional speaker-state/failure producer. No Linux model, producer or release change.

Apple/backend 3.7.0 hardware/activity/outcome changes have a [local handoff](docs/hardware-activity-outcomes-370.md). No Linux hardware producer or release is implied; backend normalization keeps existing Linux contracts compatible.

3.7.0 same-day contract follow-through (2026-11-03): [recovery, AI and removal handoff](docs/recovery-ai-removal-370.md) tracks additive recovery milestones/counters, optional AI request correlation and atomic local-duration removal/ownership fences. Existing Linux protocols remain compatible; no new Linux telemetry producer is implemented. Local test evidence is recorded there; release stays separate.

## Tracking: 3.7.0 review corrections (2026-11-03)

Apple/backend review fixes are local and unreleased. Full Questions BYOK prompts in Swift now mirror the managed prompt verbatim, including previous-summary/start context and the existing question-stability rules. The shared `../miniti/test-contracts/v1/review-370-v1.json` covers absent, empty and populated previous summaries. Linux must mirror these full-prompt bytes and import this additive fixture in its next parity change; the earlier incremental baseline fix alone does not establish full-prompt parity. Also track the plain-language Luna model-access refusal and fixed `model_access` category, without automatic cross-model retries or BYOK telemetry. No Linux runtime or release changes are included in this review task.


## Apple 3.8.0 local contract handoff (2026-11-04)

Apple/backend implementation is local and uncommitted, not deployed. Linux remains on its independent release line and legacy request format. No Linux metering/UX change is claimed. The new fixture at `test-contracts/managed-accounting-v2.json` is an exact copy of Apple's `test-contracts/v1/managed-accounting-v2.json` and backend `test/fixtures/managed-accounting-v2.json`.

Track adoption as a bounded Linux task: stable recording UUID/version, monotonic active-time checkpoint, owner-bound durable report, v2 start/renew/end and six-hour accounting, positive legacy negotiation only before capture and no downgrade of an active v2 journal; an uncaptured failed request retains its UUID but can accept a successful legacy response on retry. Until then the old reconnect undercount and four-hour legacy cap remain. Legacy requests and response fields remain supported by the additive backend. Separately track explicit first-use/sign-out choices, unreadable keyring classification, persisted Retry-After, device label PATCH and manual-label preservation. Generic links gain atomic consumption server-side; device-enrollment proof/binding retries retain the existing protocol. No prompts, auth vectors or model choices change in 3.8.0.

### 3.8.0 Apple folder compatibility (2026-11-04, local contract)

Apple FO1 adds optional `folder_id` (UUID/null) and `folder_manual` (boolean) to schema-v1 meeting documents. Preserve these unknown fields verbatim in read/edit/write cycles. Empty-folder catalogs use separate account-scoped `/api/sync/folders` GET/PUT with revision CAS and deletion tombstones; Linux does not need to call it to remain compatible. Shared folder fixtures and round-trip verification are required before rollout. Linux folder UI is not implemented by the Apple work. Backend migration/route first, no production change authorized.

Folder handoff verification: `test-contracts/folders-v1.json` is an exact copy of the Apple canonical catalog fixture. The released Linux client has no cloud sync producer, so there is no current Linux folder round-trip path to run. Preserve the additive fields and tombstones when its future cloud sync is implemented; do not claim Linux folder UI or sync support from this tracking update.

### Apple 3.8.0 Home setup parity, 2026-11-04

Track SG1: optional staged Home account/calendar guide, direct working email action, preserved existing preferences/dismissals and truthful reviewed/skipped/pending progress. Apple local-transcription review is Mac-specific, not a Linux promise. No API/schema/prompt change or Linux runtime implementation in this handoff. See [Apple SG1](../miniti/docs/plans/home-setup-guide.md).

### 3.8.0 review correction tracking, 2026-11-04

The Apple/backend local review now persists normalized Polar period-end spelling under the device CAS, keeping legacy/Linux session-end accounting compatible. Shared meeting uploads without `folder_id` preserve server folder membership; explicit null removes it. Preserve unknown folder fields where available and omit them for any future device-only override. Apple also makes setup controls direct, requires explicit confirmation before rotating an already-registered device, and retains one CRM completion across Resume/Stop. These are Linux parity follow-ups, not Linux implementation or release claims. See the Apple 3.8.0 runtime/review correction record for validation and remaining device gates.

### Apple 3.8.0 UX parity follow-up, 2026-11-05

Track broad purpose folders instead of per-title generation, preview grouped by category with counts/descriptions, native visible form fields and Reduce Motion-aware setup hiding. Matching remains local and the catalog/wire format is unchanged. Existing folder IDs/custom rules/manual moves survive. Default recommendations remain a discussion, not an Apple or Linux preference migration. No Linux runtime change.

### Apple 3.8.0 file setup parity, 2026-11-05

Track visible local Markdown destination and AGENTS index choices in first-use/Home, retaining explicit export defaults and keeping app history distinct from file copies. Apple bookmarks now preserve the previous destination on failure; its index writer preserves existing instructions outside a miniti-owned comment block. Calendar auto-start is a recommendation after connection, not a preference migration. No wire or Linux runtime change is included.

### 3.8.0 second-review compatibility, 2026-11-05

Backend fixes raw Redis comparison/volatile read contention, negative legacy elapsed time/global aggregate compatibility and v2-prefix collisions without requiring a Linux update. Account deletion now propagates purge failure and tolerates a missing folder table. Apple preserves old cloud fingerprints, avoids export-enable backfills, completes crash recovery, keeps keychain retries safe and reduces folder matching/poll cost. Track applicable Linux parity; no Linux runtime implementation is claimed. The revised Apple checklist is a proposal only. Ian resolved CRM behavior on 2026-11-05: first-completion-only automatic sync, with later changes resent manually. Continuing-call calendar transitions and automatic shared-folder creation for upgraders remain product decisions.

### Apple checklist and release-note parity, 2026-11-05

Ian approved the vertical Home checklist with real completion states, one expanded task and direct preference controls. Mac adds separate transcription/speaker choices. No Linux runtime change. Release notes must compare final behavior with the previous public release, omitting candidate-only regressions and describing newly introduced features as new.


Apple setup visibility follow-up (2026-11-05): track a bottom-left setup control with configured/eligible progress, independent guide collapse/permanent dismissal, individual task hiding and General restoration. Hiding does not complete tasks or change preferences; account dismissal stays account-scoped. Contextual info links open specific docs. This is Apple-local UX parity tracking, not Linux implementation.


### Apple third-review parity, 2026-11-05

Track calendar-first Home with collapsible Today/Tomorrow/Later, subdued completed setup tasks, row-level hide/docs actions and model preparation/status. Backend accepts historical signed/exponent usage counters and retries legacy CAS races; no Linux client change is needed for that compatibility fix. Cloud writes now serialize with account deletion. Apple promotes newly known folder extras and protects remote content during folder-only sync; preserve those properties when adopting folders on Linux. Bulk uploads remain full documents. Applicable identity-recovery and CRM readiness fixes remain tracked; Ian chose first-completion-only automatic CRM sync with manual resend for later changes; automatic update-in-place is outside the Apple 3.8.0 scope. No Linux implementation/version change is claimed.


### Apple 3.8.0 fourth-review parity, 2026-11-05

Backend account-write contention is now retryable 503 rather than meeting-conflict 409; Linux must retain unsent edits and the last known revision on this transient response. Account deletion shares its owner-token lock with cloud writes, including token/grant creation. Both accounting paths reject oversized corrupt counters before charging. Apple folder migration rebases clean fingerprints, retains dirty edits and merges remote content for folder-only changes across all sync paths. Ambiguous start retries keep the recording UUID; recovered drafts do not automatically export before completion. CRM stays once on first completion, with manual resend. Track these contracts before Linux adopts the corresponding features; this is not a Linux implementation or release claim. Apple evidence: [fourth-review disposition](../miniti/docs/releases/3.8.0-evidence.md#fourth-review-corrections-2026-11-05).


3.8.0 fifth-review parity tracking (2026-11-05): the compatible backend uses 503 `busy` plus `Retry-After` for account contention, with short bounded acquisition waits; never interpret this as a meeting revision conflict. Apple retries twice quietly and retains dirty work for the next poll. Track equivalent Linux transient UX when adopting cloud sync. OAuth/email contention preserves unconsumed credentials. Automatic Markdown parity should compare locale-free content identity, excluding pin/folder state, and avoid rendering full history during launch checks. Tracking only, no Linux implementation or release claim.


### Apple 3.8.0 sixth-review parity, 2026-11-06

Track independent content/folder sync baselines across push, pull and reconciliation; retain both sides of concurrent note/folder edits and ignore stale Undo markers. Apple blocks account moves until recording and owned usage/import work settles, and prevents a late resume grant from starting capture after Discard. Automatic Markdown compares only rendered input, excluding session and unrelated sync metadata. Compatible backend fixes preserve committed results after lock-release failure, fence browser sign-in with account deletion, reject inactive web sessions and restore automatic device naming after clearing a label. Apple offline setup and compact Home/calendar UI remain platform parity work. No Linux runtime implementation, version bump or release is claimed.


### Apple 3.8.0 seventh-review parity, 2026-11-06

Tracking only: Apple now preserves complete Watch audio, permits credential repair while retaining pending usage under its original account, and retries existing managed reports after a BYOK switch. Evaluate equivalent recovery/owner-retention behavior in Linux's own bounded work; no Linux runtime change is claimed. Backend meeting PUT ignores echoed folder membership only for known pre-3.8 Apple versions. `X-Platform: linux` retains its own version line and behavior, covered by the route regression. See [Apple disposition](../miniti/docs/releases/3.8.0-evidence.md#seventh-review-corrections-2026-11-06).


## Apple audio-route hang follow-up, 2026-11-06

Apple 3.8.0 now isolates Mac Core Audio/AVAudioEngine lifecycle operations on a serial worker with route settling, bounded timeout/quarantine and stale-callback rejection. Stop preserves received transcript and freezes/enqueues usage before queued teardown. This is a Mac-specific fix, with no Linux API/PCM/default change or Linux runtime adoption. Retain the corresponding responsiveness/late-callback acceptance criteria for future PipeWire recovery work; do not copy Apple APIs. The [Apple release record](../miniti/docs/releases/3.8.0-evidence.md#audio-route-hang-follow-up-2026-11-06) owns automated evidence and outstanding physical AirPods checks.

### Apple eighth-review compatibility, 2026-11-06

Backend legacy migration now checks both UUID spellings during preflight and final eligibility, retaining proof/key binding. No wire or Linux version change. Track Apple's same-revision folder refresh before uploading promoted legacy extras, preservation of explicit restore intent, and temporary-key Sign out protection without blocking authenticated repair. Mac hardware deadlines now exclude queue wait and remain quarantined after a genuine timeout. Watch paid-response checkpoints are iOS-specific. See the [Apple disposition](../miniti/docs/releases/3.8.0-evidence.md#eighth-review-corrections-and-legacy-migration-2026-11-06). No Linux runtime implementation or production backend deployment is claimed.


### Confirmed Apple defaults and release handoff, 2026-11-06

Ian selected keeping a continuing call in one recording across calendar boundaries, with explicit next-meeting handoff. Automatic folders apply to newly completed meetings, including upgraders; historical regrouping requires Preview and Apply. Track these for Linux parity without changing Linux defaults in this Apple task. The [compact Apple release checklist](../miniti/docs/releases/3.8.0.md) now separates current gates from dated development evidence. No Linux runtime or release change.


### Apple build 155 review follow-through, 2026-11-06

Apple fixes legacy folder refresh without cached membership, receipt-only Watch retry billing, idle audio quarantine actions and migration negative-cache retry. Linux has no Watch or Mac audio owner. The API error mapper now treats both 503 account-lock contention and historical/attach-specific 409 busy as retryable Busy, with the existing mapping test extended. No Linux version or release change. Apple release evidence owns validation; backend rollout remains separate.

Apple 3.8.0 tenth-review tracking (2026-11-06): per-meeting folder-refresh retries/export callbacks, local migration query, hardware quarantine prompts and Watch checkpoint retention are Apple-only corrections. No backend wire/status change or new Linux runtime requirement; the existing 409/503 busy fix remains. Apple build 156 owns validation and no Linux release is implied.

Apple setup feedback (2026-11-06): info buttons now show a short native explanation and an explicit docs link; setup disclosure/info/hide controls align consistently. Microphone authorization is cached and queried off the UI thread. Track the help interaction and nonblocking readiness policy for future Linux setup parity, without adopting Apple APIs or changing Linux runtime, defaults or release metadata. Calendar task restoration remains visibility-only, separate from connection and completion.

### Apple transcript presentation follow-up, 2026-11-07

Apple 3.8.0 now retains provisional words during source ordering, guards newer provisional phrases against delayed finals and batches the visible handoff. Mac gently brightens only newly committed words and skips that animation with Reduce Motion. Track the equivalent no-blank handoff in Linux; no Linux runtime, backend, transcription wire format or preference change is included here.


### Apple setup remaining-choice follow-up, 2026-11-07

Track explicit audio-choice review separate from model preferences/readiness, verified-email account completion with recovery key retained as backup, and omission of completed microphone/calendar prerequisites from Home while retaining progress. Existing dismissals/preferences remain intact; adding email does not enable optional Pro cloud storage/sync. Apple Home also gets an opaque fixed header. No Linux runtime, wire contract, defaults or release change is included.


### Apple cloud-exclusion undo correction, 2026-11-07

Apple 3.8.0 cancels a pending cloud-only removal atomically when restoring upload eligibility and rejects stale removal records for a local non-excluded meeting. Track this invariant when Linux implements selective cloud privacy; local user deletion still propagates normally. Apple also isolates setup state/permission reads and uses targeted local ID fetches during folder reconciliation. No backend or wire change, Linux runtime change or release is implied.

Apple setup copy follow-up (2026-11-07): compare online processing and local processing benefits separately for transcription and speaker separation. Describe the optional AGENTS.md index as helping folder-aware AI assistants find saved meeting notes, with technical details in help. No preference, backend, Linux runtime or release change.

Apple post-158 review follow-up (2026-11-07): Mac display-only paragraph breaks bound long-speaker TextKit edits while preserving words and stored transcript. Selection restores after the edit batch; Reduce Motion and real presentation-order regressions are covered. Pending microphone checks remain visible without claiming denial. Track equivalent presentation invariants for Linux as appropriate; no backend, wire, Linux runtime or default change.

### Apple Home presentation parity, 2026-11-08

Track the selected Apple 3.8.0 Home layout: remove the large brand lockup/tagline, show either setup or calendar, and keep setup/calendar controls together in the footer. Opening setup hides calendar; collapse/removal returns to it. Calendar hide persists locally without changing connection or automatic recording. Reuse Linux control conventions when parity is selected. No Linux implementation, release or backend contract change is included.

Apple account-entry follow-up, 2026-11-08 (tracking only): explicit anonymous Continue now shows pending enrollment, selects managed mode before enrollment, and advances setup without waiting for usage. Preserve this behavior when adopting the planned explicit account-choice flow on Linux; temporary device identity failures must be visible, not silently reset. No Linux runtime change or release in this pass.

Apple Home visual parity, 2026-11-08 (tracking only): account choices now use a centered cardless prompt and neutral controls aligned with the recording-action width. Preserve clear progress/error/recovery states when adapting this entry flow. The centered Feedback row is iOS-specific; Mac retains its right-aligned footer action. No Linux implementation in this pass.


Apple folder journey parity, 2026-11-08: post-161 Apple source adds an optional folder setup task with explicit review confirmation, and separate preview/create/manage destinations. Preserve manual assignments by default, Preview → Apply for old history, optional matching/local-only editor controls, and independent hide/restore. No backend contract, Linux code or release change.


### Apple Home layout parity, 2026-11-09

Apple 3.8.0 source now places language above Start in a fixed bottom area. iOS setup/calendar/feedback share one equal-cell footer row. Mac retains its footer positions and moves calendar into an independent collapsible right sidebar, using an overlay at narrow widths. This is presentation-only tracking; no wire/data/auth/default change or Linux runtime implementation. Linux 0.7.2 remains the released baseline.

Apple Mac control parity, 2026-11-09: folder disclosures gain full-row targets and matching dark surfaces. Models becomes a labelled Home footer control; Cmd-] toggles the Home calendar or meeting insights by context. Track equivalent discoverability and hit targets when Linux parity is selected. No Linux implementation or backend contract change.

Apple sidebar follow-up, 2026-11-09: Mac Home groups setup, models and feedback at bottom left. Calendar occupies a matching full-height right sidebar with its toggle/keyboard hint in the bottom-right footer and a 52pt collapsed rail labelled calendar. Track the shared sidebar styling and preserve narrow-width overlay behavior when adopting Linux parity. No Linux runtime or wire change.

Apple recording-status copy, 2026-11-09: distinguish starting audio and initial transcription connection (neutral) from restarting capture and reconnecting transcription (real interruption). Keep this presentation separate from recovery thresholds and retry ownership; silent buffers count as capture. Track for Linux parity; no backend/wire change or Linux implementation in this pass.
