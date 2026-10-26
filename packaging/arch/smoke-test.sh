#!/usr/bin/env bash
# Checks the Arch package in a fresh archlinux container (packaging/arch/docker.sh test):
# installs $PKG with the dependencies from the repositories, checks files and libraries,
# the CLI headless (check, outline, render, export, measure, daemon) and the desktop UI under
# Xvfb (start, `mcad ui status`, `mcad ui screenshot`, WM_CLASS).
#
#   PKG=/pkg/mcad-….pkg.tar.zst packaging/arch/smoke-test.sh OUT_DIR
set -euo pipefail

OUT="${1:-/tmp/mcad-smoke}"
mkdir -p "$OUT"
: "${PKG:?PKG (package file) missing}"
step() { printf '\n== %s\n' "$*"; }

step "Install the package (pacman -U, dependencies from the repositories)"
# The archlinux Docker image excludes /usr/share/doc and others via NoExtract; for the test
# (examples under /usr/share/doc/mcad) extract as on a normal system
sed -i '/^NoExtract/d' /etc/pacman.conf
pacman -Sy --noconfirm >/dev/null
pacman -U --noconfirm "$PKG" 2>&1 | grep -E '^(Packages|Pakete|Total|Gesamt)|mcad' || true
pacman -Q mcad

step "pacman -Qi"
pacman -Qi mcad | grep -E '^(Name|Version|Description|Licenses|Depends On|Installed Size)'

step "pacman -Ql (without examples)"
pacman -Ql mcad | grep -v '/examples/.' | sed 's/^mcad //'
echo "(+ $(pacman -Ql mcad | grep -c '/examples/.*[^/]$') example files in /usr/share/doc/mcad/examples)"

step "pacman -Qkk (files as in the package)"
pacman -Qkk mcad

step "Libraries of /usr/bin/mcad"
if ldd /usr/bin/mcad | grep 'not found'; then echo "missing libraries" >&2; exit 1; fi
readelf -d /usr/bin/mcad | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' '; echo
ls -l /usr/bin/mcad

step "mcad --version"
mcad --version

work="$(mktemp -d)"
cp -r /usr/share/doc/mcad/examples "$work/"
cd "$work/examples"
export MCAD_NO_DAEMON=1

step "mcad check (diagnostics)"
for f in 0*.mcad; do mcad check "$f"; done   # tutorial, 06: embedded fonts
cp 01-plate.mcad plate.scad && mcad check plate.scad   # .scad still opens

step "mcad outline"
mcad outline 05-box-lid.mcad | head -n 15

step "mcad render (software rasterizer, headless)"
mcad render 08-macropad.mcad -o "$OUT/raster.png" --module plate --views all --show grids,anchors,labels
mcad render 03-tray.mcad -o "$OUT/schnitt.png" --section z=10 --show dims
mcad render 06-name-tag.mcad -o "$OUT/text.png"
file "$OUT"/*.png

step "mcad export STEP, STL, 3MF"
mcad export 04-hook.mcad -o "$OUT/haken.step"
mcad export 03-tray.mcad -o "$OUT/schale.stl"
mcad export 06-name-tag.mcad -o "$OUT/schild.3mf"
head -c 200 "$OUT/haken.step" | head -n 3
ls -l "$OUT"/haken.step "$OUT"/schale.stl "$OUT"/schild.3mf

step "mcad measure"
mcad measure 05-box-lid.mcad --between bin.rim bin.base --volume part:bin
mcad measure 06-name-tag.mcad --colors | head -n 12

step "Daemon (start on first call, status, stop)"
unset MCAD_NO_DAEMON
export MCAD_IPC_DIR="$work/ipc"
mkdir -p "$MCAD_IPC_DIR"
mcad check 03-tray.mcad
mcad check 03-tray.mcad
mcad daemon status
mcad daemon stop
export MCAD_NO_DAEMON=1

step "mcad without a graphical session: message, exit 2"
rc=0
env -u DISPLAY -u WAYLAND_DISPLAY mcad 2>"$OUT/no-display.txt" || rc=$?
head -n 2 "$OUT/no-display.txt"
echo "exit code: $rc"
[ "$rc" = 2 ]

step "Desktop UI under Xvfb (test tools: xorg-server-xvfb, xorg-xprop, xorg-xwininfo, mesa)"
pacman -S --noconfirm --needed xorg-server-xvfb xorg-xprop xorg-xwininfo mesa >/dev/null
Xvfb :99 -screen 0 1440x900x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
xvfb=$!
export DISPLAY=:99
for _ in $(seq 1 50); do xwininfo -root >/dev/null 2>&1 && break; sleep 0.2; done

ui_try() {
  # $1: description; rest: extra environment for mcad
  local what="$1"; shift
  echo "-- Start: mcad 03-tray.mcad ($what)"
  env "$@" mcad 03-tray.mcad >"$OUT/ui.log" 2>&1 &
  ui=$!
  local ok=1
  for _ in $(seq 1 90); do
    if mcad ui status --json >"$OUT/ui-status.json" 2>/dev/null &&
      grep -q '"ok": *true' "$OUT/ui-status.json"; then ok=0; break; fi
    kill -0 "$ui" 2>/dev/null || break
    sleep 1
  done
  return $ok
}
stop_ui() { kill "$ui" 2>/dev/null || true; wait "$ui" 2>/dev/null || true; }

sandbox=""
if ! ui_try "default"; then
  echo "UI does not respond; log:"; tail -n 20 "$OUT/ui.log" || true
  stop_ui
  # In containers without user namespaces the bubblewrap sandbox of WebKitGTK fails
  sandbox=" (WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1)"
  ui_try "without WebKit sandbox" WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 || {
    echo "UI does not start; log:" >&2; cat "$OUT/ui.log" >&2; exit 1; }
fi
echo "UI running$sandbox, PID $ui"
mcad ui status
sleep 5   # first evaluation and view
mcad ui screenshot -o "$OUT/ui-screenshot.png"
file "$OUT/ui-screenshot.png"
echo "-- Windows (xwininfo) and WM_CLASS (for StartupWMClass of the desktop file)"
xwininfo -root -tree | grep -E '"[^"]+": \("[^"]*" "[^"]*"\)' | head -n 5 || true
# App ID (Tauri identifier) = name of the desktop file = WM_CLASS (X11) or app_id (Wayland)
desktop=/usr/share/applications/dev.mcad.desktop.desktop
win="$(xwininfo -root -tree | sed -n 's/^ *\(0x[0-9a-f]*\) "\([^"]*\)": ("dev.mcad.desktop".*/\1/p' | head -n 1)"
[ -n "$win" ] || { echo "no window with WM_CLASS dev.mcad.desktop" >&2; exit 1; }
xprop -id "$win" WM_CLASS _NET_WM_NAME 2>/dev/null || true
grep -q '^StartupWMClass=dev.mcad.desktop$' "$desktop"
stop_ui
kill "$xvfb" 2>/dev/null || true

step "Desktop file"
pacman -S --noconfirm --needed desktop-file-utils >/dev/null
desktop-file-validate "$desktop" && echo "desktop-file-validate: ok"
icon="$(sed -n 's/^Icon=//p' "$desktop")"
ls /usr/share/icons/hicolor/*/apps/"$icon".* | sed 's|/usr/share/icons/hicolor/||'
[ -f "/usr/share/icons/hicolor/scalable/apps/$icon.svg" ] && [ -f "/usr/share/icons/hicolor/48x48/apps/$icon.png" ]

step "File types (.mcad, .scad)"
pacman -S --noconfirm --needed xdg-utils >/dev/null
grep -q '^MimeType=text/x-mcad;application/x-openscad;$' "$desktop"
touch "$work/probe.mcad" "$work/probe.scad"
for f in probe.mcad probe.scad; do echo "$f: $(xdg-mime query filetype "$work/$f")"; done
[ "$(xdg-mime query filetype "$work/probe.mcad")" = text/x-mcad ]

step "all ok"
