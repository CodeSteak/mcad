#!/usr/bin/env bash
# Prüft das Arch-Paket in einem frischen archlinux-Container (packaging/arch/docker.sh test):
# installiert $PKG mit den Abhängigkeiten aus den Repositorys, prüft Dateien und Bibliotheken,
# die CLI headless (check, outline, render, export, measure, Daemon) und die Desktop-UI unter
# Xvfb (Start, `cad ui status`, `cad ui screenshot`, WM_CLASS).
#
#   PKG=/pkg/controller-cad-….pkg.tar.zst packaging/arch/smoke-test.sh OUT_DIR
set -euo pipefail

OUT="${1:-/tmp/cad-smoke}"
mkdir -p "$OUT"
: "${PKG:?PKG (Paketdatei) fehlt}"
step() { printf '\n== %s\n' "$*"; }

step "Paket installieren (pacman -U, Abhängigkeiten aus den Repositorys)"
pacman -Sy --noconfirm >/dev/null
pacman -U --noconfirm "$PKG" 2>&1 | grep -E '^(Packages|Pakete|Total|Gesamt)|controller-cad' || true
pacman -Q controller-cad

step "pacman -Qi"
pacman -Qi controller-cad | grep -E '^(Name|Version|Description|Licenses|Depends On|Installed Size)'

step "pacman -Ql (ohne Beispiele)"
pacman -Ql controller-cad | grep -v '/examples/.' | sed 's/^controller-cad //'
echo "(+ $(pacman -Ql controller-cad | grep -c '/examples/.*[^/]$') Beispieldateien in /usr/share/doc/controller-cad/examples)"

step "pacman -Qkk (Dateien wie im Paket)"
pacman -Qkk controller-cad

step "Bibliotheken von /usr/bin/cad"
if ldd /usr/bin/cad | grep 'not found'; then echo "fehlende Bibliotheken" >&2; exit 1; fi
readelf -d /usr/bin/cad | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' '; echo
ls -l /usr/bin/cad

step "cad --version"
cad --version

work="$(mktemp -d)"
cp -r /usr/share/doc/controller-cad/examples "$work/"
cd "$work/examples"
export CAD_NO_DAEMON=1

step "cad check (Diagnosen)"
cad check spec/gehaeuse.scad
cad check spec/text_schild.scad        # eingebettete Schriften
cad check spec/svg_logo.scad           # SVG-Import

step "cad outline"
cad outline projekt-gehaeuse/gehaeuse.scad | head -n 15

step "cad render (Software-Rasterizer, headless)"
cad render spec/gehaeuse.scad -o "$OUT/gehaeuse.png" --module gehaeuse --views all --show grids,anchors,labels
cad render spec/gehaeuse.scad -o "$OUT/schnitt.png" --section z=10 --show dims
cad render spec/text_schild.scad -o "$OUT/text.png"
file "$OUT"/*.png

step "cad export STEP, STL, 3MF"
cad export spec/flasche.scad -o "$OUT/flasche.step"
cad export spec/gehaeuse.scad -o "$OUT/gehaeuse.stl"
cad export spec/mehrfarbig.scad -o "$OUT/mehrfarbig.3mf"
head -c 200 "$OUT/flasche.step" | head -n 3
ls -l "$OUT"/flasche.step "$OUT"/gehaeuse.stl "$OUT"/mehrfarbig.3mf

step "cad measure"
cad measure spec/gehaeuse.scad --between k.ecke k.oben --volume part:k
cad measure spec/mehrfarbig.scad --colors | head -n 12

step "Daemon (Start beim ersten Aufruf, Status, Ende)"
unset CAD_NO_DAEMON
export CAD_IPC_DIR="$work/ipc"
mkdir -p "$CAD_IPC_DIR"
cad check spec/gehaeuse.scad
cad check spec/gehaeuse.scad
cad daemon status
cad daemon stop
export CAD_NO_DAEMON=1

step "cad ohne grafische Sitzung: Meldung, Exit 2"
rc=0
env -u DISPLAY -u WAYLAND_DISPLAY cad 2>"$OUT/no-display.txt" || rc=$?
head -n 2 "$OUT/no-display.txt"
echo "exit code: $rc"
[ "$rc" = 2 ]

step "Desktop-UI unter Xvfb (Testwerkzeuge: xorg-server-xvfb, xorg-xprop, xorg-xwininfo, mesa)"
pacman -S --noconfirm --needed xorg-server-xvfb xorg-xprop xorg-xwininfo mesa >/dev/null
Xvfb :99 -screen 0 1440x900x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
xvfb=$!
export DISPLAY=:99
for _ in $(seq 1 50); do xwininfo -root >/dev/null 2>&1 && break; sleep 0.2; done

ui_try() {
  # $1: Beschreibung; Rest: zusätzliche Umgebung für cad
  local what="$1"; shift
  echo "-- Start: cad spec/gehaeuse.scad ($what)"
  env "$@" cad spec/gehaeuse.scad >"$OUT/ui.log" 2>&1 &
  ui=$!
  local ok=1
  for _ in $(seq 1 90); do
    if cad ui status --json >"$OUT/ui-status.json" 2>/dev/null &&
      grep -q '"ok": *true' "$OUT/ui-status.json"; then ok=0; break; fi
    kill -0 "$ui" 2>/dev/null || break
    sleep 1
  done
  return $ok
}
stop_ui() { kill "$ui" 2>/dev/null || true; wait "$ui" 2>/dev/null || true; }

sandbox=""
if ! ui_try "Standard"; then
  echo "UI meldet sich nicht; Log:"; tail -n 20 "$OUT/ui.log" || true
  stop_ui
  # In Containern ohne User-Namespaces scheitert die bubblewrap-Sandbox von WebKitGTK
  sandbox=" (WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1)"
  ui_try "ohne WebKit-Sandbox" WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 || {
    echo "UI startet nicht; Log:" >&2; cat "$OUT/ui.log" >&2; exit 1; }
fi
echo "UI läuft$sandbox, PID $ui"
cad ui status
sleep 5   # erste Auswertung und Ansicht
cad ui screenshot -o "$OUT/ui-screenshot.png"
file "$OUT/ui-screenshot.png"
echo "-- Fenster (xwininfo) und WM_CLASS (für StartupWMClass der Desktop-Datei)"
xwininfo -root -tree | grep -E '"[^"]+": \("[^"]*" "[^"]*"\)' | head -n 5 || true
win="$(xwininfo -root -tree | sed -n 's/^ *\(0x[0-9a-f]*\) "\([^"]*\)": ("cad".*/\1/p' | head -n 1)"
[ -z "$win" ] || xprop -id "$win" WM_CLASS _NET_WM_NAME 2>/dev/null || true
grep -q '^StartupWMClass=cad$' /usr/share/applications/controller-cad.desktop
stop_ui
kill "$xvfb" 2>/dev/null || true

step "Desktop-Datei"
pacman -S --noconfirm --needed desktop-file-utils >/dev/null
desktop-file-validate /usr/share/applications/controller-cad.desktop && echo "desktop-file-validate: ok"

step "alles ok"
