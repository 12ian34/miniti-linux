# Distribution — binary tarball + AUR

miniti Linux ships as a **normal dynamically linked desktop binary**, not Flatpak/AppImage/Snap. Arch users install from the **AUR**. Everyone else downloads a GitHub Release tarball (optional `.deb` later).

## Artifacts

| Artifact | Contents | Audience |
|---|---|---|
| `miniti-VERSION-x86_64-unknown-linux-gnu.tar.gz` | `miniti` binary, `miniti.desktop`, icons, LICENSE, README (runtime deps) | Direct install / non-Arch |
| `miniti-bin` (AUR) | PKGBUILD fetches the GitHub Release tarball or `.deb` | Arch, prebuilt |
| `miniti` / `miniti-git` (AUR) | Build from source (`cargo` + frontend toolchain) | Arch, source |
| Optional `.deb` | Tauri bundler output | Debian/Ubuntu; optional `-bin` source |

Same pattern for `aarch64` when we care (Pi / ARM laptops) — not required for v1.

Build the tarball with `packaging/make-tarball.sh` after `pnpm tauri build`
(it stages the binary + `.desktop` + hicolor icons + a runtime-deps README and
emits `miniti-<version>-x86_64-unknown-linux-gnu.tar.gz` plus a `.sha256`).

### Tarball layout (proposed)

```
miniti-1.0.0-x86_64-unknown-linux-gnu/
  miniti
  miniti.desktop
  icons/hicolor/.../apps/miniti.png
  LICENSE
  README.md          # depends + install hints
```

Install hint for tarball users: copy binary to `~/.local/bin` or `/usr/local/bin`, desktop file + icons to XDG data dirs — or use the AUR.

## Runtime dependencies

Typical Tauri 2 + this app (names are Arch/Debian-ish — adjust per distro):

- `webkit2gtk-4.1` (or distro equivalent)
- `gtk3`
- `libsoup3` / `libsoup`
- cairo, pango, gdk-pixbuf, glibc
- **pipewire** (+ pulse compat) for capture
- **libsecret** for device ID
- tray: `libappindicator-gtk3` / ayatana as required by Tauri tray on the target DE

Document these in the tarball README and AUR `depends=()`.

## Builder baseline (glibc)

Prebuilt binaries must be produced on an **old-enough** image that still has WebKitGTK 4.1 — **Ubuntu 22.04** is the default recommendation (Tauri docs). Building on bleeding-edge Arch for `-bin` will break older glibc hosts.

CI sketch:

1. Job on `ubuntu-22.04` → `pnpm tauri build` (no secrets: managed mode enrolls at runtime with a device-bound key) → `packaging/make-tarball.sh` (records build host + glibc, warns above the 2.35 baseline) → upload GitHub Release  
2. Optional: emit `.deb` in the same job  
3. Do **not** require Flatpak

Arch `miniti` / `miniti-git` source packages build natively on the user’s machine and ignore the glibc baseline.

## AUR notes

Tauri documents AUR packaging: https://v2.tauri.app/distribute/aur/

### `miniti-bin`

- `source=` GitHub Release asset  
- `depends=` WebKitGTK/GTK/PipeWire/libsecret/…  
- `package()` install binary + desktop + icons  
- `provides=(miniti)` `conflicts=(miniti miniti-git)`  

### `miniti` / `miniti-git`

- `makedepends=(rust cargo nodejs pnpm …)` + WebKitGTK **dev** packages  
- Build with Tauri; install from `target/release` / bundle data  
- **Disable Tauri updater pubkey / `createUpdaterArtifacts`** for source builds — missing private key breaks AUR builds, and pacman owns updates anyway
- Source and CI builds are identical in capability: managed mode needs no build-time secret (device-bound enrollment at runtime)  

PKGBUILDs live in `packaging/aur/miniti-bin/PKGBUILD` (prebuilt tarball) and `packaging/aur/miniti/PKGBUILD` (source). Update `pkgver` and `sha256sums` per release; `.github/workflows/release.yml` builds the tarball + `.deb` on `ubuntu-22.04` from a `v*` tag with no repository secrets.

### Local release (no CI)

```bash
pnpm tauri build                       # binary + .deb
packaging/make-tarball.sh              # dist-release/miniti-<ver>-x86_64-unknown-linux-gnu.tar.gz + .sha256
cd packaging/aur/miniti-bin && makepkg -si   # after pointing source= at the tarball (file:// works for local tests)
```

## Release checklist

1. Add the version's entry at the top of `CHANGELOG.md` (public wording, no implementation details) and bump `version` in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `pkgver` in both PKGBUILDs.
2. Push `main`, wait for `ci` to pass, then tag `vX.Y.Z` and push the tag; the `release` workflow publishes the tarball, `.sha256`, and `.deb`.
3. Paste the changelog entry into the GitHub Release notes.
4. Put the tarball's sha256 into `packaging/aur/miniti-bin/PKGBUILD`, regenerate `.SRCINFO` with `packaging/aur/srcinfo.py`, and push to AUR.
5. Update `LINUX_LATEST_VERSION` (and `LINUX_MIN_VERSION` if the release is a forced floor) on the backend.

## Updates

| Channel | Mechanism |
|---|---|
| AUR | `pacman` / AUR helper |
| Tarball users | GitHub Releases; optional in-app “new version” link (not silent Sparkle) |
| Critical break | `GET /api/version` → `min_version` force gate → `download_url` |

Do **not** rely on Tauri’s signed updater as the primary Arch path.

## Desktop entry

`packaging/miniti.desktop` (install into tarball / packages):

```ini
[Desktop Entry]
Name=miniti
Comment=AI meeting assistant
Exec=miniti %U
Icon=miniti
Terminal=false
Type=Application
Categories=Office;AudioVideo;
StartupWMClass=miniti
MimeType=x-scheme-handler/miniti-google;x-scheme-handler/miniti-attio;x-scheme-handler/miniti-twenty;
```

Register URL scheme handlers so Google / Attio / Twenty OAuth returns land in the app (match Apple callback schemes).

## Backend

`GET /api/version` for `X-Platform: linux` should return:

- `min_version` from `LINUX_MIN_VERSION`  
- `download_url` → GitHub Releases (latest or versioned asset page)  

See PLAN.md §15.

## Explicit non-goals

- Flatpak, AppImage, Snap as supported ship channels  
- Fully static musl binary with embedded WebKit (not practical)  
- Claiming Sparkle-equivalent silent update on every distro  
