#!/usr/bin/env bash
#
# Cloud Agent bootstrap for miniti-linux (Tauri 2 + Rust).
# Idempotent: safe to re-run against a warm or partially prepared VM.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Installing system libraries (Tauri 2 / audio / secrets / tray / headless)"
export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libsoup-3.0-dev \
  librsvg2-dev \
  libjavascriptcoregtk-4.1-dev \
  libayatana-appindicator3-dev \
  libssl-dev \
  libpipewire-0.3-dev \
  libpulse-dev \
  libasound2-dev \
  libsecret-1-dev \
  build-essential \
  curl wget file libglib2.0-dev \
  xvfb

# The base image pins the default Rust toolchain to 1.83, but current Tauri
# dependency graph needs the `edition2024` Cargo feature (Cargo >= 1.85).
if command -v rustup >/dev/null 2>&1; then
  echo "==> Ensuring Rust stable toolchain is the default"
  rustup default stable
fi

echo "==> Installing frontend dependencies (pnpm)"
# CI=true keeps pnpm non-interactive (no TTY purge prompt); the pinned
# packageManager field keeps the pnpm version deterministic across VMs.
export CI=true
pnpm install --frozen-lockfile

echo "==> Warming the Rust build cache"
( cd src-tauri && cargo fetch )

echo "==> Bootstrap complete"
