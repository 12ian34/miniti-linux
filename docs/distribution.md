# Distribution — binary tarball, `.deb`, Omarchy package repository

miniti Linux ships as a **normal dynamically linked desktop binary**, not
Flatpak/AppImage/Snap. Omarchy and other Arch users install `miniti-bin` from
the **Omarchy package repository** ([omacom/omarchy-pkgs](https://github.com/omacom/omarchy-pkgs)).
Everyone else downloads a GitHub Release tarball or the `.deb`.

**The AUR is not used and never will be** (decision 2026-09-07). Do not add
AUR packages, `.SRCINFO` files, or install instructions that mention it.

## Artifacts

| Artifact | Contents | Audience |
|---|---|---|
| `miniti-VERSION-x86_64-unknown-linux-gnu.tar.gz` | `miniti` binary, `miniti.desktop`, AppStream metainfo, man page, completions, icons, LICENSE, README (runtime deps) | Direct install / non-Arch |
| `miniti_VERSION_amd64.deb` | Tauri bundler output with the same file set | Debian / Ubuntu |
| `miniti-bin` in the Omarchy repository | PKGBUILD re-wraps the release tarball; no compiling | Omarchy, Arch |

Same pattern for `aarch64` when we care (Pi / ARM laptops); the Omarchy
repository builds both architectures, so adding an `aarch64` release asset and
a `source_aarch64=` line is all it takes.

Build the tarball with `packaging/make-tarball.sh` after building the release
binary (it stages the binary, `.desktop`, metainfo, man page, completions
generated from the binary, hicolor icons and a runtime-deps README, and emits
`miniti-<version>-x86_64-unknown-linux-gnu.tar.gz` plus a `.sha256`).

### Tarball layout

```
miniti-1.0.0-x86_64-unknown-linux-gnu/
  miniti
  miniti.desktop
  com.miniti.linux.metainfo.xml
  miniti.1
  completions/{miniti.bash,_miniti,miniti.fish}   # generated from the binary
  icons/hicolor/.../apps/miniti.png, scalable/apps/miniti.svg, symbolic/apps/miniti-symbolic.svg
  LICENSE
  README.md          # depends + install hints
```

Install hint for tarball users: copy the binary to `~/.local/bin` or
`/usr/local/bin`, the desktop file, metainfo, man page, completions and icons
to the XDG data dirs (the README in the tarball has the exact commands).

## Runtime dependencies

Arch names (Debian equivalents are in the tarball README):

- `webkit2gtk-4.1`, `gtk3`, `libsoup3`, `librsvg`
- `alsa-lib` (cpal microphone capture), `pipewire`, `pipewire-pulse`, `libpulse` (system audio through `parec`)
- `libsecret` (device id and credentials), `libnotify`
- tray: `libappindicator-gtk3`
- optional: `gtk-layer-shell` (floating surface on wlr compositors), `gnome-keyring`, `xdg-desktop-portal` (idle inhibit fallback), `xdg-utils`

The PKGBUILD's `depends=()` and the tarball README are the two places these
are written down; keep them in step, and check with `ldd` / `namcap` when a
crate changes.

## Builder baseline (glibc)

Prebuilt binaries must be produced on an **old-enough** image that still has
WebKitGTK 4.1 — **Ubuntu 22.04** (glibc 2.35) is the default. Building on
bleeding-edge Arch would break older glibc hosts. The Omarchy repository
package re-wraps this same tarball, so Omarchy users get the CI binary, not a
local compile.

CI (`.github/workflows/release.yml`, triggered by a `v*` tag, no secrets):

1. `ubuntu-22.04` → build the release binary → generate completions →
   `pnpm tauri build` for the `.deb` → `packaging/make-tarball.sh`
2. Upload tarball, `.sha256` and `.deb` to the GitHub Release

## The Omarchy package repository

Every Omarchy machine has the `[omarchy]` pacman repository enabled, so a
listed package installs with `sudo pacman -S miniti-bin` and updates with the
system. Packages are PKGBUILDs in `pkgbuilds/<name>/` of
[omacom/omarchy-pkgs](https://github.com/omacom/omarchy-pkgs) with Omarchy
metadata in `.omarchy/package.json`; outside contributors add packages by pull
request (recent examples: `schist-bin`, `openclaw`, `omakade`).

Our copy lives in `packaging/omarchy-pkgs/miniti-bin/` and must stay
byte-identical to `pkgbuilds/miniti-bin/` in that repository:

- `PKGBUILD`: `source_x86_64=` points at the release tarball, `sha256sums_x86_64=`
  pins it, `package()` installs the tarball's file set.
- `.omarchy/package.json`: `source: local` (we own the PKGBUILD),
  `release_ring: fast` (builds for the edge, rc and stable channels),
  `min_release_age: 24h` (a release is quarantined for a day before the
  repository picks it up, so a bad tag can be pulled), and an `upstream`
  block that follows this repository's GitHub releases with `digests: true`
  (checksums come from GitHub's release API; nothing is downloaded to sync).

How it flows after listing: we tag `vX.Y.Z` → CI publishes the tarball → the
repository's scheduled `bin/sync-upstream` sees the release once it is 24 h
old, rewrites `pkgver` and the checksum, and the builder ships it to `edge`,
then on to `rc` and `stable` with the next Omarchy release train. No AUR, no
manual PKGBUILD bump on their side. A maintainer can pull a release in early
with `BYPASS_MIN_RELEASE_AGE=1 bin/sync-upstream miniti-bin`.

### Getting listed (one-time)

1. Fork `omacom/omarchy-pkgs`, copy `packaging/omarchy-pkgs/miniti-bin/` to
   `pkgbuilds/miniti-bin/`.
2. Verify in a clean `archlinux:base-devel` container: `makepkg -s`, `pacman -U`,
   `miniti --version`, `namcap` (expect only prebuilt-binary warnings), and
   `bin/sync-upstream miniti-bin` reporting no update at the pinned version.
3. Open the PR in the style of the `schist-bin` submission: what the package
   is, provenance (the pinned sum matches GitHub's digest and an independent
   download), and the validation you ran. The maintainers review PKGBUILDs
   closely (dependencies that pull unnecessary packages into the builder,
   `optdepends` vs `depends`).

### Local test on any Arch machine

```bash
cd packaging/omarchy-pkgs/miniti-bin && makepkg -si
```

Point `source_x86_64=` at a `file://` path to test an unreleased tarball.

## Local release (no CI)

If you drive `cargo` yourself instead of `pnpm tauri build`, pass
`--features custom-protocol` for any binary that will be run outside
`tauri dev`; without it the window looks for the Vite dev server.

```bash
pnpm build && cargo build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml
packaging/gen-completions.sh src-tauri/target/release/miniti packaging/completions   # the .deb bundles these
pnpm tauri build                       # .deb (reuses the release binary)
packaging/make-tarball.sh              # dist-release/miniti-<ver>-x86_64-unknown-linux-gnu.tar.gz + .sha256
```

## Release checklist

1. Add the version's entry at the top of `CHANGELOG.md` (public wording, no
   implementation details) and bump `version` in `package.json`,
   `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`. Do **not** touch
   the PKGBUILD yet: `pkgver` and `sha256sums_x86_64` change together in
   step 4, after the asset exists.
2. Push `main`, wait for `ci` to pass, then tag `vX.Y.Z` and push the tag; the
   `release` workflow publishes the tarball, `.sha256`, and `.deb`.
3. Paste the changelog entry into the GitHub Release notes.
4. In one commit: bump `pkgver` and put the tarball's sha256 into
   `packaging/omarchy-pkgs/miniti-bin/PKGBUILD` (the `.sha256` asset has it),
   reset `pkgrel=1`. The Omarchy repository syncs itself from the release; if
   its copy drifts from ours (a `depends` change, say), open a PR there.
5. Update `LINUX_LATEST_VERSION` (and `LINUX_MIN_VERSION` if the release is a
   forced floor) on the backend.

## Updates

| Channel | Mechanism |
|---|---|
| Omarchy / Arch | `pacman -Syu` from the Omarchy repository |
| Tarball / `.deb` users | GitHub Releases; the in-app "new version" notice links to the install steps |
| Critical break | `GET /api/version` → `min_version` force gate → `download_url` |

No Tauri updater: pacman and the release page own updates.

## Desktop entry and installed files

`packaging/miniti.desktop`, `packaging/com.miniti.linux.metainfo.xml`,
`packaging/miniti.1`, `packaging/icons/` and the generated completions are the
single source for every artifact; [docs/desktop-integration.md](desktop-integration.md)
lists where each lands.
