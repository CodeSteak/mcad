# OpenSCAD-Vergleich

Vergleich unserer Sprache (OpenSCAD-Dialekt, B-Rep) mit OpenSCAD 2021.01 an typischen
Hobbymodellen. Umfang: nur Features aus „Voll“ in `docs/technischer-plan.md`, Abschnitt 5.

| Ort | Inhalt |
| --- | --- |
| `examples/openscad-compat/*.scad` | 36 Modelle, je mit Kopfkommentar `// Zweck: …` (34–36: `offset`) |
| `examples/spec/*.mcad` | 8 Modelle mit den neuen Sprachfeatures (Raster, Anker, Fillets, …) |
| `tests/openscad-compat/make-reference.py` | erzeugt `reference.json` mit OpenSCAD |
| `tests/openscad-compat/reference.json` | Referenz (eingecheckt): Volumen, Fläche, Hülle, Dreiecke, echo |
| `tests/openscad-compat/compare.py` | Stufe 2: unsere CLI gegen die Referenz |
| `tests/openscad-compat/mock_cad.py` | Attrappe der CLI zum Testen von `compare.py` |
| `tests/openscad-compat/stl_stats.py` | gemeinsame Hilfen (STL lesen, Kennzahlen, Toleranzklasse) |
| `crates/compat-tests` | Stufe 1 als Rust-Integrationstests (nur Tests, `publish = false`) |

STL- und echo-Dateien liegen unter `target/openscad-compat/` und werden nicht eingecheckt.

## Ablauf

1. **Referenz erzeugen** (nur nach Änderung eines Modells nötig; braucht `openscad`):

   ```sh
   python3 tests/openscad-compat/make-reference.py            # alle Modelle (~2,5 min)
   python3 tests/openscad-compat/make-reference.py examples/openscad-compat/19_hex_nut.scad
   ```

   Pro Datei: `openscad -o target/openscad-compat/<name>.stl` und `-o <name>.echo`, dann
   Volumen (Divergenzsatz), Oberfläche, Bounding Box, Dreiecksanzahl und die `ECHO:`-Zeilen.
   Jede Datei bekommt einen Quell-Hash (FNV-1a 64); Tests und `compare.py` melden eine
   veraltete Referenz.

   **`$fn`-Regel:** `-D '$fn=128'` wird gesetzt, wenn die Datei Rundungen hat (sphere,
   cylinder, circle ohne `$fn` = 3…12 am Aufruf, rotate_extrude, linear_extrude mit twist)
   und `$fn`/`$fa`/`$fs` **nicht auf oberster Ebene** zuweist. Ein `$fn` am Aufruf
   (`cylinder(d = e, $fn = 6)`) gilt in OpenSCAD vor dem globalen Wert, die Sechskantmutter
   bleibt also ein Sechseck und nur die runde Bohrung wird fein. Setzt eine Datei `$fn`
   selbst global (`33_global_fn.scad`), wird sie unverändert exportiert, weil `-D` ihre
   eigene Zuweisung überschreiben würde.

2. **Stufe 1** (jetzt; braucht kein OpenSCAD, nur `reference.json`):

   ```sh
   cargo test -p compat-tests                  # alles
   cargo test -p compat-tests --test openscad_compat -- --nocapture   # mit Tabelle
   ```

   - `openscad_compat`: jede Datei in `examples/openscad-compat/` → `cad_eval::eval_source`
     ohne Fehler-Diagnosen, kein Abbruch, Baum mit 3D-Geometrie, echo-Zeilen gleich wie
     OpenSCAD. Bekannte Fehler stehen mit Grund in `KNOWN_FAILURES` (der Test schlägt auch
     fehl, wenn ein bekannter Fehler behoben ist, damit der Eintrag verschwindet).
   - `tree_bounds`: Hülle des ausgewerteten Baums (ohne Kernel geschätzt: Union = Vereinigung,
     Schnitt = Schnitt der Hüllen, Differenz = Hülle des ersten Kinds) gegen OpenSCADs
     Hülle, Toleranz 0,01 mm. Prüft Transformationen, `center`, `r`/`d`, `$fn`-Polygone,
     Modifier und `children()`.
   - `spec_models`: jede Datei in `examples/spec/` ohne Fehler und Warnungen; erwartete
     Raster (Name, Ebenenzahl), Instanzen mit Ankern, Anzahl Griffe, Knoten (fillet, zone, …)
     und einzelne Werte (Ankerlage nach Instanztransformation, Zonen, Sweep-Pfad).

3. **Stufe 2** (sobald die CLI existiert):

   ```sh
   python3 tests/openscad-compat/compare.py                       # CLI `mcad` im PATH
   MCAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval
   CAD_EXPORT_CMD='cargo run -q -p cad-cli -- export {src} -o {out}' python3 tests/openscad-compat/compare.py
   python3 tests/openscad-compat/compare.py --eval --metrics eval   # Kennzahlen aus `eval --json`
   python3 tests/openscad-compat/compare.py 05_nested_booleans.scad # einzelne Dateien
   ```

   Befehlsvorlagen mit Platzhaltern `{cad} {src} {out} {name} {stem}`:
   `CAD_EXPORT_CMD` (Standard `{cad} export {src} -o {out}`) und `CAD_EVAL_CMD`
   (Standard `{cad} eval {src} --json`, nur mit `--eval`). Aus `eval --json` werden
   `echoes` (Liste von Strings oder `{"text": …}`) und mit `--metrics eval` das erste
   Objekt mit `volume`, `area`, `bounds` gelesen.

   Ausgabe: Tabelle auf stdout, JSON-Bericht in `target/openscad-compat/compare-report.json`
   (`--report`). Exit-Code 0 = alles innerhalb der Toleranz, 1 = Abweichung oder
   Befehlsfehler, 2 = Einrichtungsfehler (Referenz fehlt oder veraltet).

   **Selbsttest** mit der Attrappe (kopiert OpenSCADs eigene STL, optional verzerrt):

   ```sh
   python3 tests/openscad-compat/compare.py --self-test
   ```

   Prüft: identische STL → Exit 0; Skalierung 1,005 → Exit 1; eine Datei um 0,02 mm
   verschoben → Exit 1; falsches echo → Exit 1; Export schlägt fehl → Exit 1; Kennzahlen
   aus `eval --json` → Exit 0. Braucht die STLs aus Schritt 1.

## Toleranzen (Stufe 2)

| Größe | Toleranz |
| --- | --- |
| Bounding Box | 0,01 mm je Koordinate |
| Volumen, Fläche (relativ), Klasse `polygonal` | 1 % |
| Volumen, Fläche (relativ), Klasse `rounded` | 2 % |

Die Klasse steht pro Datei in `reference.json` (`stl_stats.classify`, überschreibbar mit
einem Kommentar `// compat-class: polygonal|rounded`). `polygonal`: keine Rundungen oder
nur `$fn` = 3…12 am Aufruf; beide Seiten erzeugen dieselben Polygone (Plan, Abschnitt 12,
Punkt 3), Unterschiede sind nur numerisch. `rounded`: OpenSCAD facettiert, wir rechnen
exakt.

Begründung: Bei `$fn = 128` ist der Facettierungsfehler klein. Ein einbeschriebenes
128-Eck hat `sin(2π/n)/(2π/n) ≈ 1 − 4·10⁻⁴` der Kreisfläche; gemessen an den Modellen
gegen die analytischen Volumen: Zylinder (`03`) −0,040 %, Torus und Scheibe (`13`)
−0,064 %, Kugeln (`02`) −0,100 % (die Ringe erreichen weder Pole noch Äquator). Der Fehler
wächst, wo eine Differenz den Großteil des Volumens abzieht, und unser STL-Export ist
ebenfalls tesselliert. 1 % bzw. 2 % lassen mehr als den zehnfachen Abstand, ein falsches
Maß eines Hauptmerkmals fällt trotzdem auf. Die Hülle verschiebt sich durch Facetten um
höchstens `r·(1 − cos(π/128)) ≈ 3·10⁻⁴·r` (0,006 mm bei r = 20 mm), Kugeln mit `$fn = 64`
(`33`) um 0,006 mm bei r = 5 mm. Unser Export muss dafür fein genug tessellieren
(Sehnenfehler deutlich unter 0,01 mm) oder die Hülle exakt per `eval --json` liefern.

Dreiecksanzahlen werden nur berichtet, nicht verglichen.
