# miniti-bin (AUR) — placeholder

Fill this PKGBUILD when the first GitHub Release tarball exists.

See ../../docs/distribution.md and https://v2.tauri.app/distribute/aur/

Suggested shape:

- pkgname=miniti-bin
- provides=(miniti)
- conflicts=(miniti miniti-git)
- source= GitHub Release `miniti-${pkgver}-x86_64-unknown-linux-gnu.tar.gz`
- depends= webkit2gtk-4.1 gtk3 … pipewire libsecret (full list in distribution.md)
