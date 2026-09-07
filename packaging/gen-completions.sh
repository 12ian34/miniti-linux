#!/usr/bin/env bash
# Generate shell completions from the built binary into a directory:
#   packaging/gen-completions.sh <miniti-binary> <out-dir>
# Writes miniti.bash, _miniti (zsh) and miniti.fish. Used by the tarball
# script, the Omarchy PKGBUILD (via the tarball) and the release workflow (for the .deb).
set -euo pipefail
bin="$1"
out="$2"
mkdir -p "$out"
"$bin" completions bash > "$out/miniti.bash"
"$bin" completions zsh  > "$out/_miniti"
"$bin" completions fish > "$out/miniti.fish"
