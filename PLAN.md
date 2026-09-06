# Miniti Linux — Build Plan & Full Context

Self-contained context for `miniti-linux`, a **Tauri 2 + Rust** rewrite of the Miniti meeting assistant targeting **macOS feature parity** on Linux. Agents in this repo may not have the Swift tree open — every contract needed to build lives here (with pointers to sibling repos for prompts and fine detail).

**Tagline:** multi-dimensional meetings  
**Sibling Apple app:** `../miniti` (macOS + iOS)  
**Backend:** `../miniti-api` → `https://api.miniti.app`  
**Not this repo:** Flatpak / AppImage / Electron / Swift-on-Linux

---

## 1. What Miniti is

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
| Ship | GitHub Release **binary tarball** + **AUR** (`miniti-bin`, `miniti`/`miniti-git`) |
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
- In-app Sparkle-class silent update for AUR users (pacman updates instead)  
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
    aur/
      miniti-bin/PKGBUILD      # filled when first release exists
      miniti/PKGBUILD
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

Query params (update if `../miniti/docs/audio.md` moves):

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

Port Apple algorithms from `../miniti/docs/audio.md`:

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

Release tarball CI, AUR PKGBUILDs, CRM, Granola, markdown export, long-meeting transcript perf, supported-distro matrix doc.

Rough calendar: **9–15 months** to near-parity with 1–2 engineers after a green spike.

---

## 14. Distribution (summary)

See [docs/distribution.md](docs/distribution.md).

- Artifact: `miniti-VERSION-x86_64-unknown-linux-gnu.tar.gz` (binary + `.desktop` + icons + LICENSE + README deps)  
- Optional: `.deb` from Tauri bundler for Debian users / AUR `-bin` convenience  
- AUR: `miniti-bin` (prebuilt), `miniti` / `miniti-git` (source)  
- **No** Flatpak / AppImage  
- **No** Tauri updater plugin for AUR builds  
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
| Binary + AUR ship | near | medium |
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
3. Re-verify Deepgram query params and model pins against `../miniti/docs/audio.md` and `../miniti-api` at implementation time — this plan is a snapshot.  
4. Do not enable Tauri updater signing in a way that breaks AUR source builds.  
5. Do not commit secrets.  
6. Ask before `git init` / remote / publish if the user has not requested it.
