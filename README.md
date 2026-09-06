# Miniti for Linux

Miniti is an AI meeting assistant: live transcription with speaker separation,
insights while you talk (summary, open questions, coaching, sales analysis,
playbook lookups), and a searchable history of every meeting, stored on your
machine. This repository is the native Linux desktop app, built with Tauri 2
and Rust.

- Website: https://miniti.app
- Documentation: https://miniti.app/docs
- Changelog for all platforms: https://miniti.app/changelog · this app's own [CHANGELOG.md](CHANGELOG.md)
- Releases: https://github.com/12ian34/miniti-linux/releases
- Issues and feature requests: https://github.com/12ian34/miniti-linux/issues

## Install

Prebuilt x86_64 releases are built on Ubuntu 22.04 (glibc 2.35), so they run on
Ubuntu 22.04+, Debian 12+, Fedora, Arch and other rolling distributions.

**Arch Linux.** Build the package from the PKGBUILD in this repo. It downloads
the release tarball, verifies its checksum and installs through pacman. The
same package will be published on the AUR as `miniti-bin` once AUR
registration reopens.

```bash
git clone https://github.com/12ian34/miniti-linux.git
cd miniti-linux/packaging/aur/miniti-bin
makepkg -si
```

**Debian / Ubuntu.** Download `Miniti_<version>_amd64.deb` from the
[latest release](https://github.com/12ian34/miniti-linux/releases/latest), then:

```bash
sudo apt install ./Miniti_*_amd64.deb
```

**Any distribution.** Download the tarball from the latest release, then:

```bash
tar xzf miniti-*-x86_64-unknown-linux-gnu.tar.gz && cd miniti-*-x86_64-unknown-linux-gnu
install -Dm755 miniti ~/.local/bin/miniti
install -Dm644 miniti.desktop ~/.local/share/applications/miniti.desktop
cp -r icons/hicolor/* ~/.local/share/icons/hicolor/
miniti
```

Runtime libraries (Arch names; the tarball's own README lists the Debian names):
`webkit2gtk-4.1 gtk3 libsoup3 librsvg libappindicator-gtk3 openssl pipewire
pipewire-pulse libpulse libsecret libnotify`. A secret service such as
`gnome-keyring` is optional: without one, credentials are kept in
`~/.local/share/miniti/auth.json` with mode 0600.

### First launch

Choose how Miniti should run:

- **Managed** uses the Miniti backend. Your account is an anonymous recovery
  key created in the app, with no email or password. Free minutes every month;
  Pro is a Polar subscription. Keep the recovery key somewhere safe: it is the
  only way to restore your account on another computer.
- **BYOK** uses your own Deepgram and OpenAI API keys and never talks to the
  Miniti backend.

System audio for remote callers needs PipeWire with the pulse shim
(`pipewire-pulse`). Without it Miniti transcribes the microphone only.

Updates come through your package manager or the releases page. The app
checks the minimum supported version on launch but does not update itself.

## What is in the Linux app

Miniti for Linux follows the macOS app feature for feature wherever Linux
allows it. The same backend, the same insights, and the same meeting data model
mean a Linux user sees what a Mac user sees.

| Area | Linux | Notes |
| --- | --- | --- |
| Live transcription (Deepgram Nova-3, speaker separation, echo reconciliation) | Yes | Microphone plus system audio through PipeWire |
| Live insights: Summary, Questions, Coaching, Sales (MEDDPICC), Playbook (Docs MCP) | Yes | Same cadence and prompts as macOS |
| Catch me up, Investigate (web and codebase), automatic speaker naming | Yes | |
| History: search, pin, rename speakers, mark as you, trim, delete, notes | Yes | |
| Export: Markdown, copy transcript, webhooks, Granola CSV import | Yes | |
| Google Calendar, Attio, Twenty | Yes | OAuth completes in the browser and returns through a `miniti-*` URL scheme |
| Smart meetings (quiet prompts, calendar handoff, call detection) | Yes | Call detection is PipeWire-client based; browsers are a weaker signal than native apps |
| Tray icon with timer, floating recording surface, desktop notifications | Yes | Floating surface placement may be ignored on some Wayland compositors |
| Pro via Polar, license-key restore, BYOK | Yes | |
| Coaching charts and grounded examples | Partial | Focus, stats and history are present; charts are not yet |
| Templates view | Not yet | Arrives with the macOS 2.6 backend change |
| Silent in-app updates | No | Package manager or releases page instead |
| iOS companion, Apple subscriptions, Sparkle | No | Apple-only by nature |

Known limits and verification status are tracked in [AGENTS.md](AGENTS.md) § Status.

## Develop and run locally

Prerequisites (Debian/Ubuntu package names):

- Rust **stable** (≥ 1.85, enforced by `rust-version` in Cargo.toml)
- Node 22 + `pnpm`
- System libs: `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev
  libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev libssl-dev
  libpipewire-0.3-dev libpulse-dev libsecret-1-dev build-essential`

```bash
pnpm install            # frontend deps
pnpm tauri dev          # run the desktop app (starts Vite + the Rust shell)

pnpm build              # typecheck + build the web frontend only
cargo test --manifest-path src-tauri/Cargo.toml   # Rust unit tests
pnpm tauri build        # release binary + .deb bundle
```

Debug logging: `RUST_LOG=debug pnpm tauri dev`. Headless machines can run the
app under a virtual display:

```bash
Xvfb :99 -screen 0 1280x800x24 &
DISPLAY=:99 WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- pnpm tauri dev
```

### No secrets in the binary

Nothing is baked into a build. On the first managed-mode launch the app creates
(or restores) an anonymous recovery key and a per-device P-256 installation
key; the backend then issues short-lived, device-bound tokens (see the Miniti
API reference, "Device-bound client authorization"). Credentials live in the
desktop secret service (`com.miniti.linux` / `device-auth`), or in
`~/.local/share/miniti/auth.json` (0600) when no secret service is running. Any
build, from source or CI, can use managed mode.

## Release

Releases are cut by pushing a `v*` tag. CI builds on Ubuntu 22.04 (the glibc
2.35 baseline) and publishes the tarball, its checksum and the `.deb` to a
GitHub Release. Then update `pkgver` and `sha256sums` in `packaging/aur/*`, add
the entry to [CHANGELOG.md](CHANGELOG.md), and push the AUR package. Details
in [docs/distribution.md](docs/distribution.md).

```bash
pnpm tauri build                 # local equivalent: binary + .deb
packaging/make-tarball.sh        # -> dist-release/miniti-<ver>-x86_64-unknown-linux-gnu.tar.gz
```

## License

Elastic License 2.0 (see `LICENSE`): use, modify and redistribute freely, but you
may not offer Miniti as a hosted or managed service, remove the notices, or
circumvent the Pro entitlement checks.
