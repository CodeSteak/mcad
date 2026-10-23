# controller-cad

Parametrisches CAD im Browser und als Desktop-App (Tauri 2) für kleine Hobbyprojekte. Modelle werden als Text in einem
OpenSCAD-Dialekt geschrieben; dazu kommen Raster, Anker, benannte Instanzen und Verrundungen
über Zonen. Geometrie ist exakte B-Rep (Kernel SMLib), keine Netze. Code und 3D-Ansicht sind
gekoppelt: Cursor und Hover markieren die zugehörige Geometrie, Griffe in der Ansicht
schreiben Literale oder Rasterreferenzen in den Code zurück.

Plan und Entscheidungen: [`docs/technischer-plan.md`](docs/technischer-plan.md).

## Status

Meilensteine M0–M7 sind umgesetzt, dazu STEP-Export, `include`/`use`, mehrere Bauteile und
variabler Fillet-Radius, eine CLI, mit der Agenten Modelle ganz ohne UI entwickeln und
prüfen, eine Desktop-App (Tauri 2: nativer Kernel, echte Dateien, Dateibeobachter wie in
VS Code, Git-Markierungen am Rand, Steuerung durch `cad ui …`), eine 3D-Schnittansicht, ein
CLI-Daemon, der den Kernel-Cache zwischen CLI-Aufrufen hält, sowie Mehrfarbigkeit mit
Farbpalette, Farbregionen, Filament-Slots, mehrfarbigem 3MF für Bambu Studio/OrcaSlicer und ein
überarbeitetes dunkles Thema, nach einer unabhängigen Prüfung korrigiert (Testberichte 1–12,
[`docs/reports/`](docs/reports/README.md)). Offen ist der Umstieg auf den
Rust-Kernel solidcore (M8, wartet auf den Portierungsstand von solidcore, siehe
[`docs/solidcore-umstieg.md`](docs/solidcore-umstieg.md)). Browser: kein Backend, gespeichert
wird im Browser (IndexedDB) oder über die File System Access API; Desktop: Dateisystem.

## Architektur in Kürze

```
Browser:  React/TS (CodeMirror 6, three.js) ── EngineTransport ── Web Worker
Worker:   cad-wasm → cad-session → cad-engine ── trait Kernel ── kernel-smlib ── smlib.wasm (Emscripten)
CLI:      cad-cli  → cad-session → cad-engine ── trait Kernel ── kernel-smlib ── SMLib nativ (FFI)
          (rechnet im Daemon `cad daemon serve` über cad-ipc, Cache bleibt zwischen Aufrufen)
Desktop:  cad (Tauri 2: dieselbe React-App, TauriTransport) ── Kanal (binär) ── cad engine-host
          (Engine-Prozess: cad-session, SMLib nativ); Beobachter, Git, Steuer-Socket in cad-desktop/cad-ipc
Sprache:  cad-syntax (Parser) → cad-eval (Werte mit Herkunft, Raster, Anker, Griffe) → cad-ir (Baum)
```

- Dieselben Rust-Crates laufen im Worker (Wasm) und in der CLI (nativ). Die Engine hält
  Zustand und Cache (inhaltsbasierte Schlüssel), wertet schrittweise mit Zeitbudget aus und
  liefert pro Auswertung eine vorläufige Szene (ohne Kernel) und eine vollständige.
- Kernel: SMLib/SM_API (C++, Apache-2.0) aus dem Fork `tangency-labs/smlib` (Zweig
  `capi-tess` @ `7eba8aa`) plus `smlib/patches/0001-…`, hinter einer eigenen C-Fassade
  (`smlib/facade/`). Zwei Wasm-Module: `cad_wasm_bg.wasm` (Rust, wasm-bindgen) und
  `smlib.wasm` (Emscripten), verbunden über eine kleine JS-Brücke.
- Export: STL und 3MF aus der Tessellierung, STEP (AP242/AP214) mit eigenem Writer
  (`cad-step`) aus der exakten B-Rep. 3MF ist mehrfarbig: je Bauteil ein Objekt, darin ein Teil
  je Farbe (Farbregionen über Kernel-Booleans, gecacht) mit Filament-Slot in
  `Metadata/model_settings.config` (Bambu Studio, OrcaSlicer); dazu „ein STL je Farbe“ als ZIP.
- Keine COOP/COEP-Header nötig (keine Threads, kein `SharedArrayBuffer`).
- CLI-Daemon: `check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure`, `set`
  laufen in einem Hintergrundprozess mit einer Engine für alle Projekte (startet beim ersten
  Aufruf, endet nach 15 min ohne Anfrage; Grenzen 512 MiB Cache, 8 Projekte, Watchdog 60 s).
  Ausgabe wie ohne Daemon; aus mit `--no-daemon`, `CAD_NO_DAEMON=1` oder `[daemon] enabled =
  false` in `$XDG_CONFIG_HOME/controller-cad/config.toml`; `cad daemon status|stop|serve`.
- Desktop: ein Binary `cad` (Feature `gui`, Standard). `cad` ohne Befehl startet die UI, die
  Engine läuft in einem eigenen Prozess (Watchdog beendet ihn bei hängendem Kernel), Meshes
  kommen binär über einen Tauri-2-Kanal. Ohne grafische Sitzung: Meldung, Exit 2.

## Schnellstart

Voraussetzungen (getestete Versionen): Rust 1.97 mit Ziel `wasm32-unknown-unknown`, C++17-
Compiler (g++ 13), CMake ≥ 3.20 (Ninja optional), emsdk 6.0.13, Node 22.18 oder neuer, pnpm
10.28, wasm-bindgen-cli in der Version aus `Cargo.lock` (0.2.129; `web/scripts/build-wasm.sh`
installiert sie bei Bedarf). Für die Vergleichstests zusätzlich OpenSCAD 2021.01 und Python 3
mit `cadquery-ocp==8.0.1.1.0`.

```sh
# 1. SMLib-Quellen (Fork + Patches) nach third_party/smlib
smlib/fetch.sh

# 2. SMLib nativ bauen (~3 min mit 4 Kernen) und die CLI bauen
smlib/build-native.sh                       # -> smlib/build-native/lib
export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo build --release -p cad-cli            # -> target/release/cad
```

Ohne `SMLIB_PREBUILT` baut `crates/kernel-smlib/build.rs` SMLib selbst (einmal pro Profil).

CLI für Menschen und Agenten (Details: `cad --help`, `.claude/skills/cad-cli/SKILL.md`,
Bericht 8). Empfohlener Ablauf: `outline` → editieren → `check` →
`render --views all --show grids,anchors` → `inspect`/`measure` → `export`.

```sh
cad=target/release/cad
$cad outline examples/projekt-gehaeuse/gehaeuse.scad          # Module, Parameter, Raster, Anker, Bauteile
$cad check examples/spec/gehaeuse.scad                         # Diagnosen mit Zeile:Spalte
$cad eval examples/spec/gehaeuse.scad --json                   # Szene, Raster, Anker, echo
$cad inspect examples/spec/fillet_zonen.scad --line 12         # was Zeile 12 erzeugt
$cad render examples/spec/gehaeuse.scad -o x.png --module gehaeuse --views all --show grids,anchors,labels
$cad render examples/spec/gehaeuse.scad -o s.png --section z=10 --show dims   # Schnitt mit Kappe
$cad render examples/spec/fillet_zonen.scad -o f.png --line 12 --azimuth 120 --elevation 30 --xray
$cad measure examples/spec/gehaeuse.scad --between k.ecke k.oben --volume part:k
$cad measure examples/projekt-gehaeuse/gehaeuse.scad --interference --clearance gehaeuse deckel
$cad set examples/spec/gehaeuse.scad --param gehaeuse.w=50 --dry-run   # Standardwert, Diff
$cad watch teil.scad --json --render teil.png --views all      # neu auswerten bei jeder Änderung
$cad export examples/spec/flasche.scad -o flasche.step         # auch .stl, .3mf
$cad export examples/spec/mehrfarbig.scad -o druck.3mf          # mehrfarbig (Bambu/Orca), --single-color
$cad measure examples/spec/mehrfarbig.scad --colors            # Volumen/Masse je Farbe und Slot
$cad render examples/spec/mehrfarbig.scad -o m.png --theme dark --show regions
$cad export examples/projekt-gehaeuse/gehaeuse.scad --all-parts -o teile/ --format step
$cad daemon status                                             # Daemon: PID, Laufzeit, Cache, Trefferquote
```

Der erste rechnende Befehl startet den Daemon (`cad daemon serve`, abgekoppelt, Log
`daemon.log` im Socket-Ordner); weitere Aufrufe nutzen dessen Cache (Messwerte: Bericht 10).
Wäre der Socketpfad länger als die Unix-Grenze (107 Byte, z. B. ein tiefer `CAD_IPC_DIR`),
liegt der Socket unter `/tmp/cad-<uid>/<hash>-daemon.sock` (gleiche Rechteprüfung);
`cad daemon status` nennt ihn („Ersatzpfad“).
Einstellungen (Standardwerte; Umgebung vor Datei, Argumente von `cad daemon serve` vor beidem):

```toml
# $XDG_CONFIG_HOME/controller-cad/config.toml  (sonst ~/.config/…, Pfad auch über CAD_CONFIG)
[daemon]
enabled = true        # false oder CAD_NO_DAEMON=1: nie Daemon; je Aufruf: --no-daemon
cache_mb = 512        # Knotenergebnisse + Netze (CAD_DAEMON_CACHE_MB, serve --cache-mb)
max_projects = 8      # Projekte, deren Ergebnisse der Cache hält (CAD_DAEMON_MAX_PROJECTS)
idle_minutes = 15     # Ende ohne Anfrage (CAD_DAEMON_IDLE_MINUTES oder CAD_DAEMON_IDLE_SECS)
watchdog_secs = 60    # länger laufender Befehl: Daemon endet, Befehl läuft ohne ihn (CAD_DAEMON_WATCHDOG_SECS)
rss_factor = 1.5      # RSS über Start + Faktor × cache_mb: Cache leeren (CAD_DAEMON_RSS_FACTOR)
cache_entries = 16384 # Höchstzahl der Einträge (CAD_DAEMON_CACHE_ENTRIES)
```

Jeder Befehl hat `--json` mit festem, versioniertem Schema (`schema_version`, Kopf von
`crates/cad-cli/src/main.rs`) und Exit-Codes 0 (ok), 1 (Fehler im Modell), 2 (Aufruf),
3 (Kernel/intern).

Desktop-App (Linux: `libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev libxdo-dev`, siehe `scripts/ci/desktop.sh deps`):

```sh
(cd web && pnpm install && pnpm build:tauri)   # UI ohne Worker/Wasm -> web/dist-tauri (wird eingebettet)
cargo build --release -p cad-cli               # target/release/cad mit UI (ohne UI: --no-default-features)
cad                                            # Desktop-UI
cad examples/spec/gehaeuse.scad                # mit Datei (wird beobachtet, Git-Rand)
cad examples/projekt-gehaeuse/                 # Ordner als Projekt
cad ui status --json                           # was ist offen (Datei, Cursor, Kontext, Parameter, Diagnosen)
cad ui show examples/spec/gehaeuse.scad --line 11
cad ui screenshot -o ansicht.png               # 3D-Ansicht der laufenden UI
```

Ohne `web/dist-tauri` bettet `cargo build` eine Platzhalterseite ein (der Workspace baut
trotzdem). Ablauf „Nutzer in der UI, Agent editiert und prüft“: `.claude/skills/cad-cli/SKILL.md`.

Browser (Wasm-Module bauen, dann Vite):

```sh
EMSDK=/pfad/zu/emsdk smlib/build-wasm.sh    # -> web/public/smlib/smlib.{js,wasm} (~3 min)
web/scripts/build-wasm.sh                   # cad-wasm + wasm-bindgen + wasm-opt -> web/src/wasm-pkg/
cd web
pnpm install
pnpm dev                                    # http://localhost:5173
pnpm build                                  # -> web/dist (statisch, z. B. GitHub Pages)
```

## Sprache in Kürze

- OpenSCAD-Syntax: Variablen, Funktionen, Module, `for`/`if`/`let`, Primitive, Transformationen,
  Booleans, Extrusionen, `include`/`use`. Längen in mm, exakte Geometrie (`$fn` nur Anzeige,
  außer `$fn` = 3…12 am Aufruf).
- `grid g { x: …; y: …; z: … }` definiert benannte Ebenen; `g[x, y, z]` ist ein Punkt, ein
  Eintrag kann auch eine Zahl sein; `g.name` ist Zahl oder Ebene, `*` ergibt Linie/Ebene.
- `anchor name = punkt, dir = v;` im Modul, `k: modul();` benennt die Instanz, `k.name` und
  `dir(k.name)` greifen von außen darauf zu; `k[i]: …` in Schleifen.
- `fillet(r) { körper; zone(…); … }` verrundet die Kanten des Körpers, die in den Zonen
  liegen (`chamfer` und `shell` ebenso); `difference(fillet = r)` die neuen Kanten,
  `$fillet = r` alle Kanten eines Primitivs; `r` darf eine Funktion der Position sein.
- Benannte Instanzen auf oberster Ebene sind Bauteile (Export je Bauteil).
- `color("teal")` nimmt eine kleine Palette aus 16 gedeckten Farben (`red, orange, yellow, lime,
  green, teal, cyan, blue, purple, magenta, pink, brown, black, gray, silver, white`, Groß/Klein
  egal, `grey`/`aqua`/`fuchsia` als CSS-Schreibweisen); unbekannte Namen warnen und bekommen
  eine stabile Zufallsfarbe, Hex und Vektoren bleiben exakt. `color("red", filament = 3)` legt
  den Filament-Slot fest, sonst Slots in Reihenfolge des ersten Auftretens (ungefärbtes
  Material: Slot 1). Die UI zeigt „Farben & Filamente“ (Slot per Auswahl, schreibt
  `filament = n` in den Code, Volumen und Masse PLA) und die Farbregionen wie gedruckt.
  Kommt die Farbe aus einem Parameter oder einer Schleife, legt `filament("red", 4);` auf
  oberster Ebene den Slot fest (die UI schreibt das dann). `alpha` wirkt nur in der Ansicht,
  `transparent` färbt nicht. Das 3MF hat ein Objekt je Bauteil (Rest: „ohne Bauteil“) und
  keine Farbgruppe, damit Bambu Studio die Slots ohne Farbdialog übernimmt (Bericht 12).

- `sketch { start(p); line(to = …); arc(to = …, r = …); bezier(c1 = …, to = …); spline(…);
  close(); }` ist ein 2D-Profil aus exakten Segmenten (Geraden, Kreisbögen, Bézier, Splines) für
  `linear_extrude`, `rotate_extrude`, `loft` und `sweep` (STEP schreibt Kreise und Linien exakt).
  Jede Zeile ist ein Segment mit Darstellung, Griffen und eigenen Fehlern (nicht geschlossen,
  unmöglicher Bogen, Selbstüberschneidung); mehrere Konturen mit `start`, Löcher nach der
  Windungsregel oder in `hole { … }`, `for`/`if`/`let` und Raster in der Skizze;
  `sketch(closed = false)` ist ein `sweep`-Pfad. `module profil(b = 40) sketch { … }` ist eine
  benannte Skizze (Plan 17, Bericht 14).
- `corner(r = …)` / `corner(chamfer = …)` rundet bzw. fast die Ecke zwischen zwei Segmenten
  (exakt für Geraden und Bögen, numerisch für Bézier/Splines; Radius-Griff), `fillet2d(r)` /
  `chamfer2d(d)` alle Ecken eines 2D-Profils oder nur die in 2D-Zonen, `offset(r | delta,
  chamfer)` wie OpenSCAD (exakte Bögen statt Facetten), `stroke(w, cap, join)` macht aus
  einem offenen Pfad eine Fläche (Rippen, Langlöcher, Kabelkanäle; Plan 17.3, Bericht 15).
- Module sind Namensräume: `module teile { module schraube(d = 3) { … } }` (Klammern dürfen
  fehlen), Aufruf von außen `teile.schraube(d = 4);`, mehrstufig `a.b.c();`; Funktionen und
  Konstanten eines Moduls als `teile.kopf(d)`, `teile.M3`; `cad --module teile.schraube`.

```
linear_extrude(5) sketch {
    start([0, 0]);
    line(to = [40, 0]);
    arc(to = [50, 10], r = 10);          // auch: center + angle, through, tangential
    line(by = [0, 20]);
    bezier(c1 = [45, 40], to = [20, 35]);
    close();
    hole { start([12, 12]); arc(to = [22, 12], r = 5); arc(to = [12, 12], r = 5); }
}
```

```
module halter(w = 40, d = 20, h = 10) {
    grid g {
        x: links = 0, mitte = w / 2, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }
    fillet(2) {
        difference() {
            box(g[links, vorne, boden], g[rechts, hinten, deckel]);
            translate(g[mitte, 10, -1]) cylinder(h = h + 2, r = 4);   // Zahl statt Ebene: y = 10
        }
        zone(g.deckel, d = 1);              // alle Kanten in der Deckelebene
    }
    anchor oben = g[mitte, 10, deckel], dir = [0, 0, 1];
}

k: halter(w = 50);
translate(k.oben) rotate(to = dir(k.oben)) cylinder(h = 8, r = 3);
```

Weitere Beispiele: `examples/spec/` (Spracherweiterungen, Skizzen: `skizze_*.scad`), `examples/openscad-compat/`
(36 OpenSCAD-Modelle), `examples/projekt-gehaeuse/` (Projekt aus drei Dateien).

## Verzeichnisstruktur

```
crates/
  cad-syntax/        Lexer, Parser, AST mit Spans (Datei-IDs)
  cad-eval/          Auswertung: Werte mit Herkunft, Raster, Anker, Griffe, Rückschreiben, include/use
  cad-ir/            Geometriebaum, Cache-Schlüssel, Radiusgesetz (Law)
  cad-kernel/        trait Kernel, gemeinsame Typen, neutrale B-Rep
  kernel-smlib/      Kernel über die SMLib-Fassade (nativ per FFI, Wasm über JS-Brücke)
  kernel-solidcore/  Kernel über solidcore (opt-in, Feature `solidcore`)
  kernel-mock/       Testkernel in Rust
  cad-engine/        Auswertung mit Cache, Provenienz, Zonen, Bauteile, Export STL/3MF/STEP
  cad-step/          STEP-Writer (AP242/AP214)
  cad-render/        Software-Rasterizer für `cad render` (Schnitt mit Kappe, Beschriftung)
  cad-session/       Protokoll (Worker und CLI), TS-Typen
  cad-wasm/          wasm-bindgen-Einstieg für den Worker
  cad-ipc/           lokale IPC (Unix-Socket je Benutzer, versionierte Rahmen, Sperrdateien) – UI-Steuerung, CLI-Daemon
  cad-desktop/       Desktop-Backend ohne Tauri: Engine-Prozess, Watchdog, Dateien, Beobachter, Git-Hunks
  cad-app/           Tauri-2-App (eingebettete UI aus web/dist-tauri)
  cad-cli/           CLI `cad` (ohne Befehl: Desktop-UI)
  compat-tests/      OpenSCAD-Vergleich Stufe 1 (Rust-Tests)
smlib/               fetch.sh, Patches, CMakeLists, Fassade, Build-Skripte nativ/Wasm
third_party/smlib/   SMLib-Quellen (nicht eingecheckt, smlib/fetch.sh)
web/                 React-App (src/), Playwright-Tests (e2e/), Tauri-E2E (e2e-tauri/), Build-Skripte (scripts/)
examples/            Beispielmodelle (zugleich Testfälle)
tests/               openscad-compat/ (Referenz, compare.py), step/verify.py
tools/report/        Generatoren der Testberichte, 3MF-Prüfung (verify3mf.py)
tools/palette/       Prüfung der Farbpalette (ΔE in OKLab, Farbfehlsichtigkeit, Kontrast)
tools/font/          Generator der Bitmap-Schrift von cad-render
scripts/ci/          Schritte der GitHub-Actions-Workflows (lokal genauso aufrufbar)
docs/                Plan, solidcore-Umstieg, Testberichte
```

## Tests

```sh
export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                          # ~450 Tests (Daemon: tests/daemon.rs)
cargo check -p cad-wasm --target wasm32-unknown-unknown

node crates/kernel-smlib/js/test-node.mjs                       # smlib.wasm über die Brücke
node web/scripts/smoke-node.mjs                                 # Browser-Engine in Node
cd web && pnpm typecheck && pnpm test && pnpm e2e               # tsc, Vitest, Playwright (Chromium)
node web/e2e-tauri/desktop.e2e.mjs --bin target/release/cad    # Desktop: tauri-driver + WebKitWebDriver (Xvfb)

python3 tests/openscad-compat/make-reference.py                 # Referenz-STLs (OpenSCAD 2021.01)
python3 tests/openscad-compat/compare.py --self-test
CAD=target/release/cad python3 tests/openscad-compat/compare.py --eval   # 33/33
/pfad/zum/venv/bin/python tests/step/verify.py --cad target/release/cad  # STEP mit OpenCASCADE
python3 tools/palette/palette.py                                # Palette: ΔE, Farbfehlsichtigkeit, Kontrast
python3 tools/report/verify3mf.py teil.3mf                      # 3MF: lib3mf, trimesh, Bambu-Import-Modell
```

CI (`.github/workflows/`): `rust` (Tauri-Pakete, fmt, clippy, Tests, Wasm-Check), `web` (smlib.wasm,
cad-wasm, Typecheck, Vitest, Node-Tests, Build, Playwright), `desktop` (UI für Tauri, Release-Build,
Tauri-E2E unter Xvfb), `compat` (OpenSCAD-Referenz und
-Vergleich, include/use gegen OpenSCAD, STEP-Prüfung), `pages` (manuell: `web/dist` auf
GitHub Pages). Die Workflows rufen nur `scripts/ci/*.sh` auf; dieselben Aufrufe laufen lokal,
z. B. `scripts/ci/smlib-native.sh && SMLIB_PREBUILT=smlib/build-native scripts/ci/rust.sh all`.
SMLib (nativ und Wasm) und emsdk werden gecacht (Schlüssel aus Fork-Commit, Patches,
CMakeLists, Build-Skript und Compiler: `scripts/ci/smlib-key.sh`). Ist der SMLib-Fork nicht
öffentlich lesbar, braucht das Repository das Secret `SMLIB_CLONE_URL`
(`https://x-access-token:<token>@github.com/tangency-labs/smlib.git`).

## Testberichte

Übersicht mit Generator-Befehlen: [`docs/reports/README.md`](docs/reports/README.md).

- [Bericht 1](docs/reports/01-kernel-sprache-engine-cli/report.pdf): Kernel, Sprache, Engine, CLI; OpenSCAD-Vergleich 33/33.
- [Bericht 2](docs/reports/02-ui-phase-a/report.pdf): Browser-UI Phase A – Editor, Ansicht, Zuordnung Code ↔ Ansicht.
- [Bericht 3](docs/reports/03-ui-phase-b/report.pdf): UI Phase B – Griffe, Snap, Parameterleiste.
- [Bericht 4](docs/reports/04-fillet-profile/report.pdf): Verrundung, Fase, Shell, Profiloperationen, Entwurfsmodus, Watchdog-Sperre.
- [Bericht 5](docs/reports/05-step-export/report.pdf): STEP-Export und Kernel-Vergleich SMLib ↔ solidcore.
- [Bericht 6](docs/reports/06-projekte-bauteile/report.pdf): Projekte, Bauteile, `include`/`use`.
- [Bericht 7](docs/reports/07-kernel-variabler-radius/report.pdf): Kernel-Korrekturen und variabler Radius.
- [Bericht 8](docs/reports/08-cli-agenten/report.pdf): CLI für Agenten (outline, render-Ansichten/Schnitt, measure, watch, set).
- [Bericht 9](docs/reports/09-desktop-schnitt/report.pdf): Desktop-App, Dateibeobachter, Git, Schnittansicht, CLI↔UI.
- [Bericht 10](docs/reports/10-cli-daemon/report.pdf): CLI-Daemon (Cache zwischen Aufrufen, Grenzen, Messung).
- [Bericht 11](docs/reports/11-farben-filamente/report.pdf): Farben, Multi-Filament-3MF (Bambu Studio/OrcaSlicer), dunkles Thema.
- [Bericht 12](docs/reports/12-farben-korrekturen/report.pdf): Korrekturen Farben und Daemon (Befunde F1–F9 der unabhängigen Prüfung, Bambu-Farbdialog, Socketpfad).
- [Bericht 14](docs/reports/14-skizzen/report.pdf): Skizzen mit gemischten Konturen (Geraden, Bögen, Bézier, Splines) und Module als Namensräume.
- [Bericht 15](docs/reports/15-skizzen-ecken-offset/report.pdf): Skizzen – Ecken (`corner`, `fillet2d`), `offset` (OpenSCAD-Vergleich), `stroke`.

## Lizenzen

- Dieses Projekt: privat, nicht lizenziert (`license = "UNLICENSED"`, `publish = false`).
- SMLib/SM_API (NVIDIA usd-brep, Fork tangency-labs/smlib): Apache-2.0.
- Tauri 2 (Apache-2.0/MIT), `notify` (CC0/MIT/Apache-2.0), `similar` (Apache-2.0). Die Quellen sind
  nicht Teil des Repositorys; `smlib/patches/` enthält Änderungen daran.
- solidcore (tangency-labs/solidcore, optionale Abhängigkeit): Apache-2.0.
- Weitere Abhängigkeiten über Cargo und pnpm (u. a. React, three.js, CodeMirror) unter ihren
  jeweiligen Lizenzen (überwiegend MIT/Apache-2.0).
