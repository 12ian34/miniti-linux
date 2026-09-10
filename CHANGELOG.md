# changelog

public changelog for miniti for Linux. the changelog for every platform lives at https://miniti.app/changelog; end-user documentation at https://miniti.app/docs.

## releases

### 2026-09-10 - v0.5.0

- new: (Linux) A `miniti` command line: start, stop, and toggle a meeting, see the live questions, list and export meetings, and answer a Smart-meeting prompt from a terminal, a keybind, or a script. `man miniti` and shell completions come with the packages
- new: (Linux) Bars and widgets can follow miniti live: a Waybar module (`miniti status --waybar`), a state file, a control socket, and a D-Bus interface, all documented in the repository. An Omarchy bar widget with a live panel is a separate plugin
- new: (Linux) Launch at login, hidden in the tray (Settings → General, or `miniti autostart on`)
- new: (Linux) The desktop entry offers Start, Stop, and Toggle actions from the app menu, and the app appears in software centres with a description, screenshots, and release notes
- new: (Linux) On Omarchy the app installs from the Omarchy package repository with `sudo pacman -S miniti-bin` once the package is listed; the same package builds anywhere with `makepkg`
- improvement: (Linux) The machine no longer goes to sleep or locks in the middle of a meeting, and logging out or shutting down while recording saves the meeting first
- improvement: (Linux) A keyring is no longer needed. Your account credentials are kept in your own files, as the device id already was, and only mirrored into GNOME Keyring or KWallet when one exists, so a locked, slow, or missing keyring can no longer make your computer look new
- fix: (Linux) Connecting Google Calendar or a CRM opened a second copy of miniti instead of returning to the running one
- fix: (Linux) A failed "create recovery key" attempt could leave the app signed in with a key the backend had never seen, so every meeting failed to start and the plan never loaded. If the backend already knows this computer but its credentials are gone, the enrollment screen now offers to restore with your recovery key or to start over as a new device
- fix: (Linux) Building the Arch package from the repository failed on 0.4.1 because the recipe expected files that release did not ship

### 2026-09-06 - v0.4.1

- new: miniti tells you on the home screen when a newer version is available, with a link to the install steps. It shows once per release and you can dismiss it
- fix: Installing from source opened to an error page instead of the app. Building from source works now; the prebuilt package was never affected

### 2026-09-06 - v0.4.0

- new: Templates: pick BANT, SPIN, an interview scorecard, a customer check-in, a stand-up, or a 1:1 from the insights menu and miniti fills it in as the meeting goes. Works on past meetings too, and the filled sections are included in exports and webhooks
- new: The floating recording window now shows the call you are on, how transcription and audio are doing, and the right buttons for the moment: keep recording or end now, take notes or not now. It opens itself when a decision is needed and never steals focus from your call
- new: Coaching shows a chart for each metric across your recent meetings, and real quotes from your own meetings that show each habit
- new: The home screen shows how many minutes you have left, and a clear page with your options when the monthly allowance runs out
- new: Choose a text size in Settings: compact, standard, or large
- new: Renaming a speaker opens a small panel with a "this is me" switch
- improvement: If your headphones or speakers switch mid-call, system audio recovers on its own instead of going silent
- improvement: Long meetings scroll more smoothly
- improvement: A debug log you can view, copy, and save from Settings when something goes wrong

### 2026-09-06 - v0.3.0

first public release of miniti for Linux, as a tarball, a `.deb`, and an Arch package.

- new: No account or email needed. On first launch miniti creates a recovery key that is your account; keep it safe and use it to sign in on another computer
- new: Settings shows the devices on your account and lets you reveal or change the recovery key, remove a device, sign out, or delete the account
- fix: Creating a recovery key could hang forever on some desktops

### 2026-09-06 - v0.2.0

Internal test build.

- new: Remote callers join the transcript through system audio, kept separate from your own voice
- new: Live insights while you talk: summary, questions, coaching, sales analysis, and playbook lookups
- new: Catch me up, investigate, automatic speaker naming, and a nudge to turn on sales analysis when a call sounds like one
- new: Edit past meetings: rename speakers, mark yourself, trim, delete lines, notes, Markdown export, webhooks, Granola import
- new: Tray icon with a timer, floating recording window, desktop notifications
- new: Smart meetings, Google Calendar, Attio, and Twenty
- new: The same layout as the Mac app

### 2026-09-05 - v0.1.0

Internal prototype: microphone transcription, live captions, coaching stats, and local meeting history.
