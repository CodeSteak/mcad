# controller-cad

Parametrisches CAD im Browser für kleine Hobbyprojekte. Modelle werden als Text in einem
OpenSCAD-Dialekt geschrieben; dazu kommen Raster, Anker, benannte Instanzen und Verrundungen
über Zonen. Geometrie ist exakte B-Rep (Kernel SMLib), keine Netze. Code und 3D-Ansicht sind
gekoppelt: Cursor und Hover markieren die zugehörige Geometrie, Griffe in der Ansicht
schreiben Literale oder Rasterreferenzen in den Code zurück.

Plan und Entscheidungen: [`docs/technischer-plan.md`](docs/technischer-plan.md).

## Status

Meilensteine M0–M7 sind umgesetzt, dazu STEP-Export, `include`/`use`, mehrere Bauteile und
variabler Fillet-Radius (Testberichte 1–7, [`docs/reports/`](docs/reports/README.md)). Offen
sind der Umstieg auf den Rust-Kernel solidcore (M8, wartet auf den Portierungsstand von
solidcore, siehe [`docs/solidcore-umstieg.md`](docs/solidcore-umstieg.md)) und eine
Tauri-Version (optional). Nur Desktop-Browser, kein Backend; gespeichert wird im Browser
(IndexedDB) oder über die File System Access API.

## Architektur in Kürze

```
Browser:  React/TS (CodeMirror 6, three.js) ── EngineTransport ── Web Worker
Worker:   cad-wasm → cad-session → cad-engine ── trait Kernel ── kernel-smlib ── smlib.wasm (Emscripten)
CLI:      cad-cli  → cad-session → cad-engine ── trait Kernel ── kernel-smlib ── SMLib nativ (FFI)
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
  (`cad-step`) aus der exakten B-Rep.
- Keine COOP/COEP-Header nötig (keine Threads, kein `SharedArrayBuffer`).

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

CLI-Beispiele (Details: `cad --help` und `.claude/skills/cad-cli/SKILL.md`):

```sh
cad=target/release/cad
$cad check examples/spec/gehaeuse.scad                      # Diagnosen mit Zeile:Spalte
$cad eval examples/spec/gehaeuse.scad --json                 # Szene, Raster, Anker, echo
$cad inspect examples/spec/fillet_zonen.scad --line 12       # was Zeile 12 erzeugt
$cad render examples/spec/fillet_zonen.scad -o x.png --line 12 --view iso
$cad export examples/spec/flasche.scad -o flasche.step       # auch .stl, .3mf
$cad export examples/projekt-gehaeuse/gehaeuse.scad --all-parts -o teile/ --format step
```

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

Weitere Beispiele: `examples/spec/` (Spracherweiterungen), `examples/openscad-compat/`
(33 OpenSCAD-Modelle), `examples/projekt-gehaeuse/` (Projekt aus drei Dateien).

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
  cad-render/        Software-Rasterizer für `cad render`
  cad-session/       Protokoll (Worker und CLI), TS-Typen
  cad-wasm/          wasm-bindgen-Einstieg für den Worker
  cad-cli/           CLI `cad`
  compat-tests/      OpenSCAD-Vergleich Stufe 1 (Rust-Tests)
smlib/               fetch.sh, Patches, CMakeLists, Fassade, Build-Skripte nativ/Wasm
third_party/smlib/   SMLib-Quellen (nicht eingecheckt, smlib/fetch.sh)
web/                 React-App (src/), Playwright-Tests (e2e/), Build-Skripte (scripts/)
examples/            Beispielmodelle (zugleich Testfälle)
tests/               openscad-compat/ (Referenz, compare.py), step/verify.py
tools/report/        Generatoren der Testberichte
scripts/ci/          Schritte der GitHub-Actions-Workflows (lokal genauso aufrufbar)
docs/                Plan, solidcore-Umstieg, Testberichte
```

## Tests

```sh
export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                          # ~300 Tests
cargo check -p cad-wasm --target wasm32-unknown-unknown

node crates/kernel-smlib/js/test-node.mjs                       # smlib.wasm über die Brücke
node web/scripts/smoke-node.mjs                                 # Browser-Engine in Node
cd web && pnpm typecheck && pnpm test && pnpm e2e               # tsc, Vitest, Playwright (Chromium)

python3 tests/openscad-compat/make-reference.py                 # Referenz-STLs (OpenSCAD 2021.01)
python3 tests/openscad-compat/compare.py --self-test
CAD=target/release/cad python3 tests/openscad-compat/compare.py --eval   # 33/33
/pfad/zum/venv/bin/python tests/step/verify.py --cad target/release/cad  # STEP mit OpenCASCADE
```

CI (`.github/workflows/`): `rust` (fmt, clippy, Tests, Wasm-Check), `web` (smlib.wasm,
cad-wasm, Typecheck, Vitest, Node-Tests, Build, Playwright), `compat` (OpenSCAD-Referenz und
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

## Lizenzen

- Dieses Projekt: privat, nicht lizenziert (`license = "UNLICENSED"`, `publish = false`).
- SMLib/SM_API (NVIDIA usd-brep, Fork tangency-labs/smlib): Apache-2.0. Die Quellen sind
  nicht Teil des Repositorys; `smlib/patches/` enthält Änderungen daran.
- solidcore (tangency-labs/solidcore, optionale Abhängigkeit): Apache-2.0.
- Weitere Abhängigkeiten über Cargo und pnpm (u. a. React, three.js, CodeMirror) unter ihren
  jeweiligen Lizenzen (überwiegend MIT/Apache-2.0).
