# changelog

public changelog for miniti for Linux. the changelog for every platform lives at https://miniti.app/changelog; end-user documentation at https://miniti.app/docs.

## releases

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
