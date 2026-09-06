# changelog

public changelog for miniti for Linux. the changelog for every platform lives at https://miniti.app/changelog; end-user documentation at https://miniti.app/docs.

## releases

### 2026-09-06 - v0.4.0

the parity release: everything the Mac app does that Linux can do, it now does.

- new: Templates — a specialist insights view that fills a structured template live from the conversation: BANT, SPIN discovery, an interview scorecard, a customer check-in, a stand-up, or a 1:1. Pick one from the More menu; it fills as the meeting goes, refreshes with the update button, completes when the meeting ends, and works on saved meetings. Filled sections are included in Markdown exports and webhook payloads
- new: The floating recording surface matches the Mac: it shows the timer with the call app, and when expanded the meeting title, call status, transcription status and audio status with open and end controls; call, quiet, calendar and ending decisions get their own wording and actions; question, monologue, filler and sales nudges come with dismiss, don't remind me, and enable sales analysis. It sizes to its content, opens itself for decisions, stays on its monitor, and never steals focus from your call
- new: Coaching shows a trend chart for every metric and grounded examples: real passages from your recent meetings behind each pattern, one click from the meeting they came from
- new: Home shows your plan and remaining minutes with a progress bar that turns amber under an hour and red under fifteen minutes; hitting the monthly cap shows a proper limit-reached view with the reset date, upgrade, and switch to BYOK
- new: Settings → Privacy & Support has a debug log: view, copy, save, clear, or open the logs folder. miniti keeps seven days of logs with no transcript text or keys
- new: Interface scale in Settings → General: compact, standard, or large
- new: Renaming a speaker opens a proper sheet with a "this is me" switch
- improvement: System audio recovers from Bluetooth and output-device changes: if the monitor goes quiet for four seconds miniti restarts capture against the current default output, and tells you on the floating surface when audio is recovering or degraded
- improvement: Long transcripts render faster: finished turns no longer re-render on every live update, and off-screen stretches skip layout entirely
- improvement: On Wayland compositors that support layer-shell (Hyprland, Sway, KDE) the floating surface pins itself to the top-right corner and stays out of the window stack when `gtk-layer-shell` is installed

### 2026-09-06 - v0.3.0

first public release of miniti for Linux. Available as a release tarball, a `.deb`, and an Arch package built from the PKGBUILD in this repository (AUR publication follows when registration reopens).

- new: Managed mode no longer needs anything baked into the app. On first launch you create an anonymous recovery key, with no email or password, and miniti enrolls this computer with a per-device key. Restore the same account on another computer with the recovery key
- new: Settings → Account & Plan shows the account, reveals or rotates the recovery key, lists the devices on the account, removes a device, signs this computer out, or deletes the account
- new: Managed mode is available in every build, including builds from source, because there is no secret to compile in

- improvement: The app is now distributed as a signed-checksum release tarball, a `.deb`, and an Arch package, all built on the Ubuntu 22.04 baseline so they run on Ubuntu 22.04+, Debian 12+, Fedora and rolling distributions
- improvement: A command that fails inside the app now reports an error instead of leaving a button spinning
- improvement: Credentials are stored in the desktop secret service when one is running, with a private file fallback when none is, and the app says which it used

- fix: Creating a recovery key could hang forever on some desktops; the keyring call now runs off the app's main runtime with a timeout

### 2026-09-06 - v0.2.0

Internal test build. Feature parity pass against the macOS app.

- new: System audio joins the microphone in the transcript through PipeWire, with remote speakers kept separate from you and echo of your own voice removed
- new: Live insights while you talk: Summary, Questions, Coaching, Sales (MEDDPICC), and Playbook lookups from a Docs MCP server, on the same schedule as macOS, plus a final pass when the meeting ends
- new: Catch me up, Investigate (web or a local codebase folder), automatic speaker naming, and suggestions to turn on Sales analysis when a conversation sounds commercial
- new: History editing: rename speakers, mark yourself, trim the start or end, delete lines, notes under the transcript, Markdown export, copy transcript, webhooks on save, and Granola CSV import
- new: Desktop shell: tray icon with a timer, floating recording surface with live nudges, desktop notifications, and browser handoff for OAuth
- new: Smart meetings: notices when a meeting may have ended or another is approaching, with calendar handoff
- new: Google Calendar, Attio and Twenty integrations; upcoming meetings on Home with prep notes
- new: The macOS layout: history sidebar with pinned and dated groups, transcript with speaker legend and resizable notes, an insights rail, searchable Settings destinations, keyboard shortcuts, and the miniti app icon

- fix: Confirmed transcript lines no longer appear twice
- fix: Live transcription connects reliably in managed mode

### 2026-09-05 - v0.1.0

Internal prototype. Microphone transcription with live captions, coaching statistics, local meeting history, managed and BYOK modes, terms and update gates, and a Deepgram query contract matching the macOS app.
