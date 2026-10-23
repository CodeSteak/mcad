# Testberichte

Jeder Bericht liegt als `report.html` (mit Bildern in `img/` bzw. im Ordner) und als
`report.pdf` vor. Die Berichte sind Momentaufnahmen: Zahlen und Grenzen gelten für den
genannten Commit. Die Generatoren liegen in `tools/report/`; Zwischenergebnisse der Testläufe
landen in `reports/tmp/` (nicht eingecheckt). Das PDF druckt
`node tools/report/print-pdf.mjs <report.html> <report.pdf>` mit Playwrights Chromium
(`cd tools/report && npm i playwright@1.56.1 --no-save`).

| Nr. | Ordner | Datum, Commit | Umfang | Erzeugt mit |
| --- | --- | --- | --- | --- |
| 1 | [`01-kernel-sprache-engine-cli`](01-kernel-sprache-engine-cli/report.pdf) | 23.10.2026, `fd8dda6` | Kernel (SMLib-Fassade nativ und Wasm), Sprache, Engine, CLI; 202 Rust-Tests, OpenSCAD-Vergleich 33/33, Wasm-Größen, Ladezeiten; Korrekturen Union dreier Zylinder, `twist`, Determinismus, Transparenz | `python3 tools/report/report1.py` (`--repeat 30` für Wiederholungsläufe, `--reuse` nur HTML/PDF) |
| 2 | [`02-ui-phase-a`](02-ui-phase-a/report.pdf) | 23.10.2026, `15e78ec` | Browser-UI Phase A: Editor, 3D-Ansicht, Zuordnung Code ↔ Ansicht, Modifier, Raster und Anker lesend, Dateien, Watchdog; 22 Playwright-Szenarien, 87 Vitest | `cd web && pnpm test:report` (`node tools/report/report2-ui.mjs [--run]`) |
| 3 | [`03-ui-phase-b`](03-ui-phase-b/report.pdf) | 23.10.2026, `af6edd6` | UI Phase B: Griffe, Ziehen, Snap auf Raster/Anker, Alt-Ziehen, Parameterleiste, Inline-Regler, Strg+Klick; 14 neue Szenarien | `cd web && pnpm test:report3` |
| 4 | [`04-fillet-profile`](04-fillet-profile/report.pdf) | 23.10.2026, `36fb562` | UI Phase C: Verrundung, Fase, Shell, Profiloperationen, achsgebundenes Ziehen, Entwurfsmodus beim Ziehen, Watchdog-Sperre; gefundene Kernel-Fehler F1–F3 | `cd web && pnpm test:report4` |
| 5 | [`05-step-export`](05-step-export/report.pdf) | 23.10.2026, `ff21522` | STEP-Export (AP242/AP214, eigener Writer `cad-step`), Prüfung mit OpenCASCADE 46/46, Kernel-Vergleich SMLib ↔ solidcore (`cad diff-kernels`) | `python3 tools/report/report5.py --run` (ohne `--run`: nur HTML/PDF) |
| 6 | [`06-projekte-bauteile`](06-projekte-bauteile/report.pdf) | 23.10.2026, `e4ee224` | `include`/`use` (gegen OpenSCAD), Projekte mit Tabs, Datei-IDs in Spans, Bauteile mit Export je Bauteil, `k[i]`; 9 neue Szenarien, 274 Rust-Tests | `cd web && pnpm test:report6` |
| 7 | [`07-kernel-variabler-radius`](07-kernel-variabler-radius/report.pdf) | 23.10.2026, `19f2e47` | Kernel-Korrekturen F1–F4 (shell verrundeter Körper, `surface` 2×2/3×3, Naht- und Polkanten, `union(chamfer)`), SMLib-Patch 0001, variabler Radius (`cad_ir::Law`), nativ ↔ Wasm; 298 Rust-Tests, 70 Playwright, STEP 47/47 | Testläufe nach `reports/tmp/report7/` (Kopf von `tools/report/report7.mjs`), dann `node tools/report/report7.mjs` und `print-pdf.mjs` |
| 8 | [`08-cli-agenten`](08-cli-agenten/report.pdf) | 23.10.2026, `8c9876a` | CLI für Agenten: `outline`, `render` (Kontaktbogen, freie Kamera, Auswahl, Überlagerungen, Schnitt mit Kappe, Röntgen), `measure` (Abstände, Volumen, Kollisionen, Bauteilabstand), `watch`, `set`, reservierter UI-Einstieg; Laufzeiten, Fallstudie „Agent entwickelt ein Teil nur mit der CLI“; 318 Rust-Tests | Testläufe nach `reports/tmp/report8/` (Kopf von `tools/report/report8.mjs`), dann `node tools/report/report8.mjs` und `print-pdf.mjs` |
| 9 | [`09-desktop-schnitt`](09-desktop-schnitt/report.pdf) | 23.10.2026, `COMMIT9` | Desktop-App (Tauri 2, ein Binary `cad`, Engine-Prozess mit Watchdog, binärer Kanal), Dateibeobachter (still neu laden, Hinweisleiste, Vergleichen, Autospeichern), Git-Rand gegen HEAD, `cad ui status/show/screenshot` über `cad-ipc`, 3D-Schnittansicht (Stencil-Kappe, Rasten, CLI-Aufruf); Tauri-E2E mit tauri-driver unter Xvfb, Messwerte | `node web/e2e-tauri/desktop.e2e.mjs --bin target/release/cad`, `cd web && pnpm e2e`, Rust-Läufe nach `reports/tmp/report9/` (Kopf von `tools/report/report9.mjs`), dann `node tools/report/report9.mjs` und `print-pdf.mjs` |

Voraussetzungen der Generatoren (lokal): SMLib-Quellen (`smlib/fetch.sh`), nativ gebaute
Bibliotheken (`SMLIB_PREBUILT`), emsdk 6.0.13, wasm-bindgen-cli in der Version aus
`Cargo.lock`, OpenSCAD 2021.01 (Berichte 1, 6), ein venv mit `cadquery-ocp==8.0.1.1.0`
(Berichte 5–7), Python 3 mit Pillow (Bericht 8, Pixelzählung), Chromium aus Playwright 1.56.1, für Bericht 9 die Tauri-Pakete, Xvfb, WebKitWebDriver (`webkit2gtk-driver`), `tauri-driver` 2.1.0 und ImageMagick (`scripts/ci/desktop.sh deps e2e-deps`). Die Berichte laufen nicht in CI; die
Prüfungen, auf denen sie beruhen, schon (`.github/workflows/`).
