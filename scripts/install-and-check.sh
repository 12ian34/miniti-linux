#!/usr/bin/env bash
#
# Build the current checkout, install it through pacman, and check the desktop
# integration. One command for trying an unreleased build on an Arch/Omarchy
# machine:
#
#   scripts/install-and-check.sh            # build + install + checks
#   scripts/install-and-check.sh --check    # checks only (already installed)
#   scripts/install-and-check.sh --no-build # reuse the last dogfood binary
#
# Builds with the `dogfood` cargo profile (no LTO, incremental): the first
# build takes as long as ever, later ones seconds to a minute. Installing
# `mold` makes linking faster still; the script uses it when present.
#
# Released versions never need this: pacman installs them from the Omarchy
# package repository. This exists because the checked-in package pins the last
# release, and a build from git is not that.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mode="${1:-all}"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
pass=0; fail=0
ok()   { pass=$((pass + 1)); printf '  \033[32mok\033[0m    %s\n' "$*"; }
bad()  { fail=$((fail + 1)); printf '  \033[31mFAIL\033[0m  %s\n' "$*"; }
check() { # check "<label>" <command...>
  local label=$1; shift
  if "$@" >/tmp/miniti-check.log 2>&1; then ok "$label"; else bad "$label"; sed 's/^/        /' /tmp/miniti-check.log | head -8; fi
}

version="$(sed -n 's/^\s*"version"\s*:\s*"\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -1)"
bin="$root/src-tauri/target/dogfood/miniti"

if [[ "$mode" != "--check" ]]; then
  if [[ "$mode" != "--no-build" || ! -x "$bin" ]]; then
    bold "building miniti $version (dogfood profile)"
    if command -v mold >/dev/null 2>&1; then
      export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"
    else
      echo "  (tip: sudo pacman -S mold makes the link step much faster)"
    fi
    pnpm install --frozen-lockfile --prefer-offline && pnpm build || exit 1
    cargo build --profile dogfood --features custom-protocol --manifest-path src-tauri/Cargo.toml || exit 1
  fi
  bold "packaging"
  packaging/gen-completions.sh "$bin" packaging/completions || exit 1
  MINITI_BIN="$bin" packaging/make-tarball.sh dist-release >/dev/null || exit 1
  tarball="$root/dist-release/miniti-$version-x86_64-unknown-linux-gnu.tar.gz"
  work="$(mktemp -d /tmp/miniti-pkg.XXXXXX)"
  sed -e "s|^pkgver=.*|pkgver=$version|" \
      -e "s|^source_x86_64=.*|source_x86_64=(\"file://$tarball\")|" \
      packaging/omarchy-pkgs/miniti-bin/PKGBUILD > "$work/PKGBUILD"
  bold "installing through pacman (sudo will ask once)"
  ( cd "$work" && updpkgsums && makepkg -si --noconfirm ) || exit 1
  miniti quit 2>/dev/null || true
  sleep 1
  bold "relaunching"
  setsid miniti --hidden >/dev/null 2>&1 < /dev/null &
  sleep 3
fi

bold "checks"
check "miniti $version is what pacman installed" bash -c "miniti --version | grep -q \"$version\""
check "control socket answers"                    miniti status
check "waybar output is JSON with a class"        bash -c 'miniti status --waybar | python3 -c "import json,sys; d=json.load(sys.stdin); assert d[\"class\"] in (\"idle\",\"recording\",\"recording degraded\")"'
check "state file exists"                         test -s "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/miniti/state.json"
check "d-bus name owned"                          busctl --user status com.miniti.linux
check "d-bus Status() returns a snapshot"         bash -c 'busctl --user call com.miniti.linux /com/miniti/linux com.miniti.linux.Control Status | grep -q is_recording'
check "second launch hands off instead of starting a copy" bash -c 'miniti --hidden; test "$(pgrep -cx miniti)" -eq 1'
check "deep link raises the running app"          bash -c 'xdg-open "miniti-google://oauth-callback?status=success"; sleep 2; test "$(pgrep -cx miniti)" -eq 1'
check "autostart entry written and removed"       bash -c 'miniti autostart on >/dev/null && test -f ~/.config/autostart/miniti.desktop && miniti autostart off >/dev/null && ! test -f ~/.config/autostart/miniti.desktop'
check "desktop entry valid"                       desktop-file-validate /usr/share/applications/miniti.desktop
check "appstream metainfo valid"                  appstreamcli validate --no-net /usr/share/metainfo/com.miniti.linux.metainfo.xml
check "man page installed"                        man -w miniti
check "completions installed"                     test -f /usr/share/bash-completion/completions/miniti -a -f /usr/share/zsh/site-functions/_miniti -a -f /usr/share/fish/vendor_completions.d/miniti.fish
check "icons installed"                           test -f /usr/share/icons/hicolor/scalable/apps/miniti.svg -a -f /usr/share/icons/hicolor/symbolic/apps/miniti-symbolic.svg -a -f /usr/share/icons/hicolor/48x48/apps/miniti.png

bold "recording (starts a real meeting for ~8 seconds)"
if miniti start "install check" >/dev/null 2>&1; then
  sleep 5
  check "status shows recording"                  bash -c 'miniti status | grep -q "recording"'
  check "idle inhibit held while recording"       bash -c 'systemd-inhibit --list 2>/dev/null | grep -qi miniti'
  check "questions command answers"               miniti questions
  sleep 3
  check "stop saves a meeting"                    bash -c 'miniti stop | grep -q "stopped"'
  check "export last produces markdown"           bash -c 'miniti export last | grep -q "#"'
else
  bad "start a meeting (needs a working mic / credentials; the rest still counts)"
fi

echo
if [[ $fail -eq 0 ]]; then bold "all $pass checks passed"; else bold "$pass passed, $fail failed"; fi
echo "left to check by hand: the tray timer, the floating surface, and the Omarchy widget (omarchy plugin add <miniti-omarchy clone> --enable)."
exit $((fail > 0))
