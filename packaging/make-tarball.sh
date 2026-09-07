#!/usr/bin/env bash
#
# Assemble the self-contained release tarball described in docs/distribution.md:
#
#   miniti-<version>-x86_64-unknown-linux-gnu/
#     miniti                       (dynamically linked release binary)
#     miniti.desktop
#     com.miniti.linux.metainfo.xml (AppStream)
#     miniti.1                     (man page)
#     completions/{miniti.bash,_miniti,miniti.fish}
#     icons/hicolor/<size>/apps/miniti.png
#     icons/hicolor/scalable/apps/miniti.svg
#     icons/hicolor/symbolic/apps/miniti-symbolic.svg
#     README.md                    (runtime deps + install hints)
#     LICENSE                      (Elastic License 2.0)
#
# Prereq: `pnpm tauri build` (or `cargo build --release`) has produced
# src-tauri/target/release/miniti.
#
# Usage: packaging/make-tarball.sh [output_dir]
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out_dir="${1:-$repo_root/dist-release}"
target="x86_64-unknown-linux-gnu"

bin="$repo_root/src-tauri/target/release/miniti"
if [[ ! -x "$bin" ]]; then
  echo "error: release binary not found at $bin — run 'pnpm tauri build' first" >&2
  exit 1
fi

version="$(sed -n 's/^\s*"version"\s*:\s*"\([^"]*\)".*/\1/p' \
  "$repo_root/src-tauri/tauri.conf.json" | head -1)"
version="${version:-0.0.0}"

pkg="miniti-${version}-${target}"

# Record what this binary was actually built on. The release baseline is Ubuntu
# 22.04 (glibc 2.35); a newer host produces a binary older distros cannot load.
build_host="$(. /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-unknown}" || echo unknown)"
build_glibc="$(ldd --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+$' || echo unknown)"
if [[ "$build_glibc" != "unknown" ]] && [[ "$(printf '%s\n' "$build_glibc" 2.35 | sort -V | tail -1)" != "2.35" ]]; then
  echo "warning: built against glibc $build_glibc (> 2.35 baseline); this tarball will not run on Ubuntu 22.04 / Debian 12" >&2
fi
stage="$out_dir/$pkg"
rm -rf "$stage"
mkdir -p "$stage/icons/hicolor/32x32/apps" \
         "$stage/icons/hicolor/128x128/apps" \
         "$stage/icons/hicolor/256x256@2/apps" \
         "$stage/icons/hicolor/512x512/apps" \
         "$stage/icons/hicolor/scalable/apps" \
         "$stage/icons/hicolor/symbolic/apps" \
         "$stage/completions"

install -m 0755 "$bin" "$stage/miniti"
install -m 0644 "$repo_root/packaging/miniti.desktop" "$stage/miniti.desktop"
install -m 0644 "$repo_root/packaging/com.miniti.linux.metainfo.xml" "$stage/com.miniti.linux.metainfo.xml"
install -m 0644 "$repo_root/packaging/miniti.1" "$stage/miniti.1"
install -m 0644 "$repo_root/packaging/icons/miniti.svg" "$stage/icons/hicolor/scalable/apps/miniti.svg"
install -m 0644 "$repo_root/packaging/icons/miniti-symbolic.svg" "$stage/icons/hicolor/symbolic/apps/miniti-symbolic.svg"
install -m 0644 "$repo_root/LICENSE" "$stage/LICENSE"
# Shell completions come from the binary itself, so they always match its flags.
"$repo_root/packaging/gen-completions.sh" "$bin" "$stage/completions"
install -m 0644 "$repo_root/src-tauri/icons/32x32.png"     "$stage/icons/hicolor/32x32/apps/miniti.png"
install -m 0644 "$repo_root/src-tauri/icons/128x128.png"   "$stage/icons/hicolor/128x128/apps/miniti.png"
install -m 0644 "$repo_root/src-tauri/icons/128x128@2x.png" "$stage/icons/hicolor/256x256@2/apps/miniti.png"
install -m 0644 "$repo_root/src-tauri/icons/icon.png"        "$stage/icons/hicolor/512x512/apps/miniti.png"

cat > "$stage/README.md" <<EOF
# miniti $version (Linux, x86_64)

Native desktop meeting assistant (Tauri 2 + Rust). Dynamically linked binary —
install the runtime libraries below, then run \`./miniti\`.

## Runtime dependencies

### Arch Linux
\`\`\`
sudo pacman -S --needed webkit2gtk-4.1 gtk3 libsoup3 librsvg \\
  libappindicator-gtk3 openssl pipewire libpulse libsecret
\`\`\`

### Debian / Ubuntu
\`\`\`
sudo apt-get install -y libwebkit2gtk-4.1-0 libgtk-3-0 libsoup-3.0-0 \\
  librsvg2-2 libayatana-appindicator3-1 libssl3 \\
  libpipewire-0.3-0 libpulse0 libsecret-1-0
\`\`\`

## Install

\`\`\`
install -Dm755 miniti           ~/.local/bin/miniti
install -Dm644 miniti.desktop   ~/.local/share/applications/miniti.desktop
install -Dm644 com.miniti.linux.metainfo.xml ~/.local/share/metainfo/com.miniti.linux.metainfo.xml
install -Dm644 miniti.1         ~/.local/share/man/man1/miniti.1
install -Dm644 completions/miniti.bash ~/.local/share/bash-completion/completions/miniti
install -Dm644 completions/_miniti     ~/.local/share/zsh/site-functions/_miniti
install -Dm644 completions/miniti.fish ~/.config/fish/completions/miniti.fish
cp -r icons/hicolor/*           ~/.local/share/icons/hicolor/
update-desktop-database ~/.local/share/applications 2>/dev/null || true
\`\`\`

Then launch from your app menu or run \`miniti\`. \`miniti --help\` lists the
command line (status, start, stop, questions, export, …); see \`man miniti\`.

Built on: $build_host (glibc $build_glibc). Requires glibc >= $build_glibc.
Official releases are built on the Ubuntu 22.04 baseline (glibc 2.35); a tarball
built on a newer host will not load on older distros.
EOF

tar -C "$out_dir" -czf "$out_dir/${pkg}.tar.gz" "$pkg"
echo "==> wrote $out_dir/${pkg}.tar.gz"
( cd "$out_dir" && sha256sum "${pkg}.tar.gz" | tee "${pkg}.tar.gz.sha256" )
