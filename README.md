# Miniti Linux

Tauri 2 + Rust desktop client for [Miniti](https://miniti.app).

**Status:** Phase 0 scaffold. A minimal Tauri 2 + React/TypeScript skeleton is in
place (`src-tauri/`, `src/`) with a dark "environment check" screen wired through
the Rust ↔ WebView bridge. Audio/Deepgram/insights are not implemented yet — see
[AGENTS.md](AGENTS.md) and [PLAN.md](PLAN.md) for the full plan.

Distribution target: GitHub Release binary tarball + AUR — not Flatpak/AppImage.

## Develop / run locally

Prerequisites (Debian/Ubuntu package names — the Cloud Agent env installs these
automatically via [`.cursor/install.sh`](.cursor/install.sh)):

- Rust **stable** (≥ 1.85; the `edition2024` Cargo feature is required)
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

## Package a release (tarball)

```bash
pnpm tauri build                 # builds src-tauri/target/release/miniti + .deb
packaging/make-tarball.sh        # -> dist-release/miniti-<ver>-x86_64-unknown-linux-gnu.tar.gz
```

The `.deb` (Debian/Ubuntu) is written to
`src-tauri/target/release/bundle/deb/`. The tarball is the primary,
distro-agnostic artifact and its bundled `README.md` lists the runtime deps.

### Install on Arch Linux

```bash
sudo pacman -S --needed webkit2gtk-4.1 gtk3 libsoup3 librsvg \
  libappindicator-gtk3 openssl pipewire libpulse libsecret
tar xzf miniti-*-x86_64-unknown-linux-gnu.tar.gz && cd miniti-*-x86_64-unknown-linux-gnu
install -Dm755 miniti ~/.local/bin/miniti
install -Dm644 miniti.desktop ~/.local/share/applications/miniti.desktop
cp -r icons/hicolor/* ~/.local/share/icons/hicolor/
miniti
```

Built on Ubuntu 24.04 (glibc 2.39); Arch's newer glibc runs it fine.

Headless machines (CI / Cloud Agents) can run the app under a virtual display:

```bash
Xvfb :99 -screen 0 1280x800x24 &
DISPLAY=:99 WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- pnpm tauri dev
```
