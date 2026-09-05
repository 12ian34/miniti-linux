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
pnpm tauri build        # produce a release binary / .deb bundle
```

Headless machines (CI / Cloud Agents) can run the app under a virtual display:

```bash
Xvfb :99 -screen 0 1280x800x24 &
DISPLAY=:99 WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- pnpm tauri dev
```
