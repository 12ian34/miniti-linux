# Miniti Linux

Tauri 2 + Rust desktop client for [Miniti](https://miniti.app).

**Status:** feature build-out toward macOS parity is complete in code: mic +
system audio with echo reconciliation, live insights (Summary / Questions /
Coaching / Sales / Playbook), catch-up and investigation, history with trim /
export / Granola import, tray + floating recording surface, Smart meetings,
Google Calendar and Attio / Twenty. The mono transcription path is verified on
Arch; the rest needs its real-hardware pass. Details and known Linux limits in
[AGENTS.md](AGENTS.md) § Status.

Distribution target: GitHub Release binary tarball + AUR — not Flatpak/AppImage.

## Develop / run locally

Prerequisites (Debian/Ubuntu package names — the Cloud Agent env installs these
automatically via [`.cursor/install.sh`](.cursor/install.sh)):

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

### Managed mode: no secrets in the binary

Nothing is baked into the build. On the first managed-mode launch the app
creates (or restores) an anonymous **recovery key** and a per-device P-256
installation key; the backend then issues short-lived, device-bound tokens
(`../miniti-api/docs/agents/04-api-reference.md` § Device-bound client
authorization). Credentials live in the desktop secret service
(`com.miniti.linux` / `device-auth`), or in `~/.local/share/miniti/auth.json`
(0600) when no secret service is running. Any build, from source or CI, can use
managed mode. Debug logging: `RUST_LOG=debug pnpm tauri dev`.

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
  libappindicator-gtk3 openssl pipewire pipewire-pulse libpulse libsecret libnotify
tar xzf miniti-*-x86_64-unknown-linux-gnu.tar.gz && cd miniti-*-x86_64-unknown-linux-gnu
install -Dm755 miniti ~/.local/bin/miniti
install -Dm644 miniti.desktop ~/.local/share/applications/miniti.desktop
cp -r icons/hicolor/* ~/.local/share/icons/hicolor/
miniti
```

Release binaries must be built on the **Ubuntu 22.04** baseline (glibc 2.35) so they
run on Ubuntu 22.04+, Debian 12+ and rolling distros — see
[docs/distribution.md](docs/distribution.md). The Cloud Agent VM is Ubuntu 24.04
(glibc 2.39): binaries built there run on Arch but **not** on Ubuntu 22.04 or
Debian 12. `packaging/make-tarball.sh` records the actual build host and glibc
in the tarball README so nobody has to guess.

Headless machines (CI / Cloud Agents) can run the app under a virtual display:

```bash
Xvfb :99 -screen 0 1280x800x24 &
DISPLAY=:99 WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- pnpm tauri dev
```

## License

Elastic License 2.0 (see `LICENSE`): use, modify and redistribute freely, but you
may not offer Miniti as a hosted or managed service, remove the notices, or
circumvent the Pro entitlement checks. Source: https://github.com/12ian34/miniti-linux
