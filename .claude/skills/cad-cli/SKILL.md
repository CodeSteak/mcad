---
name: cad-cli
description: Modelle der parametrischen CAD-Sprache (OpenSCAD-Stil mit Rastern, Ankern und Fillet-Zonen, Dateien *.scad) prüfen, auswerten, als PNG ansehen und als STL/3MF/STEP exportieren – mit der CLI `cad` (crates/cad-cli). Verwenden, wenn ein Modell geschrieben, geändert, untersucht oder Fehler darin gesucht werden.
---

# CAD-CLI `cad`

Die CLI wertet eine Quelldatei mit denselben Crates aus wie der Browser (Parser, Auswertung,
Engine, SMLib-Kernel nativ) und zeigt das Ergebnis als Text, JSON oder Bild. Damit lässt sich
ein Modell ohne Browser bauen und kontrollieren.

## Bauen und aufrufen

```sh
# SMLib nativ: vorgebaute Bibliotheken (schnell) oder Build über CMake (~3 min beim ersten Mal)
export SMLIB_PREBUILT=/home/user/controller-cad/smlib/build-native   # falls vorhanden
smlib/fetch.sh                                                         # Quellen (tangency-labs/smlib @ 7eba8aa), falls fehlend
cargo build --release -p cad-cli
target/release/cad --help
```

Datei `-` liest von stdin: `echo 'cube(5);' | cad check -`.

## Befehle

| Befehl | Zweck |
| --- | --- |
| `cad check DATEI [--no-kernel]` | Diagnosen `datei:zeile:spalte: Fehler/Warnung (Syntax/Auswertung/Kernel): …` |
| `cad eval DATEI [--module m] [-D name=wert]…` | Baum (Knoten, Zeile, Status, Hülle), Werte der Variablen, Raster, Anker, `echo` |
| `cad inspect DATEI --line N` | Was Zeile N erzeugt: Knoten, Hülle, Volumen/Fläche, ausgewählte Kanten, Hinweise („4 Kanten“), Fehler, Griffe |
| `cad render DATEI -o bild.png [--view iso\|front\|back\|top\|bottom\|left\|right] [--line N] [--size 1024x768]` | Bild; mit `--line` wie im Editor: Knoten der Zeile voll, Rest gedimmt, Zonen blau, abgezogene Körper rot halbtransparent, ausgewählte Kanten orange, gescheiterte Kanten rot |
| `cad export DATEI -o teil.stl\|teil.3mf\|teil.step [--schema ap214]` | Exportdatei der Wurzel (STEP: exakte B-Rep, AP242) |
| `cad export DATEI --part NAME [--part NAME2] -o teil.stl\|.3mf\|.step` | Nur diese Bauteile (benannte Instanzen auf oberster Ebene, z. B. `deckel`, `k[2]`); 3MF: ein Objekt je Bauteil, STEP: ein Produkt mit ihren Körpern |
| `cad export DATEI --all-parts -o ordner/ [--format stl\|3mf\|step]` | Jedes Bauteil als eigene Datei (`deckel.stl`, `k_2.step`) |
| `cad diff-kernels DATEI…` | Vergleich SMLib ↔ solidcore je Knoten (nur Entwicklung; nur mit `cargo build -p cad-cli --features solidcore`, sonst Exit 2 mit Hinweis) |

Gemeinsame Optionen: `--json`, `--module m` (Modul als Kontext, verschachtelt `aussen::innen`),
`-D name=wert` (Parameter des Kontextmoduls, Wert als JSON: `-D w=60`, `-D hohl=true`,
`-D p=[1,2,3]`). `-D` braucht einen Modulkontext.

**Kontext wie im Editor:** Liegt `--line` in einer Moduldefinition, zeigen `inspect` und
`render` nur dieses Modul mit seinen Standardwerten (und `-D`). `--whole-file` behält den
Dateikontext.

**Projekte aus mehreren Dateien:** `include <datei>` fügt den Inhalt ein, `use <datei>` macht
nur Module und Funktionen sichtbar (wie OpenSCAD); Pfade relativ zur einbindenden Datei.
Diagnosen anderer Dateien stehen mit deren Pfad da (`lib/teile.scad:3:5: …`, JSON: `file`,
`range` lokal in dieser Datei); `eval` listet `Dateien:` und `Bauteile:` (Hülle, Volumen).

Exit-Codes: `0` ok · `1` Diagnosen mit Fehlern (Syntax, Auswertung oder gescheiterte
Kernel-Operation; Ausgabe trotzdem vollständig) · `2` Aufruffehler (Argumente, Datei) ·
`3` Kernel-/interner Fehler (Tessellierung, Export).

## Vorgehen

1. Datei ändern.
2. `cad check datei.scad` – erst weiter, wenn keine Fehler. Warnungen lesen (z. B. „0 Kanten“
   bei leerer Zonenauswahl, ungleichmäßiges `scale`).
3. `cad render datei.scad -o /tmp/x.png` und **das Bild ansehen** (Read-Tool). Für Details
   andere Ansichten (`--view top`, `front`) oder eine Zeile hervorheben (`--line 12`).
4. Bei Zweifeln `cad inspect datei.scad --line N` (Volumen, Hülle in Weltkoordinaten,
   Kantenanzahl, Fehlermeldung des Kernels) oder `cad eval` (Werte, Raster, Anker).
5. Zum Schluss ggf. `cad export datei.scad -o teil.stl`.

## JSON (Kurzfassung)

Jede Antwort ist ein Objekt mit
`schema_version` (1), `command`, `file`, `ok`, `exit_code`, `context` (Modulname oder `null`),
`diagnostics: Diagnostic[]`, bei Aufruffehlern `error`.
`Diagnostic = { severity: "error"|"warning"|"info", message, source: "syntax"|"eval"|"kernel",
node, range }`, `range = { from, to, line, col, end_line, end_col }` (Zeilen/Spalten
1-basiert, Offsets in UTF-16).

- `check`: `+ { errors, warnings }`
- `eval`: `+ { scene }` – vollständige Szene wie im Browser-Protokoll
  (`web/src/engine/protocol.ts`, Typ `Scene`): `nodes[]` (idx, parent, children, label, range,
  status, bounds im Knotensystem, `display_world` 4×4 spaltenweise, hints, selected_edges,
  messages), `grids[]` (Ebenen mit Wert, Literal oder Ausdruck), `anchors[]`
  (Kontextkoordinaten, `text` = `k.ecke`), `hints[]` (Variablenwerte), `handles[]`, `modules[]`,
  `echoes[]`, `context.params`, `stats`.
- `inspect`: `+ { line, nodes: [{ idx, label, instance, range, status, dim, flags, subtracted,
  world_bounds, volume, area, selected_edges, opened_faces, hints, messages, cached }], hints,
  handles, echoes }`
- `render`: `+ { output, width, height, view, line, highlighted: [idx], triangles }`
- `export`: `+ { output, format, bytes, triangles, volume, area, bounds, step? }` (`step`: Körper, Flächen, Kanten, Flächen-/Kurventypen)

## Sprache: Besonderheiten gegenüber OpenSCAD

Längen in mm, Winkel in Grad. Geometrie ist kein Wert; Variablen halten Zahlen, Vektoren,
Listen. B-Rep-Kernel: exakte Zylinder/Kugeln, `$fn` ist nur Darstellung.

**Primitive zwischen Punkten:** `box(a, b)`, `cylinder(a, b, r)`, `rect(a, b)` neben
`cube`, `cylinder(h, r)`, `sphere(r)`, `torus(R, r)`.

**Raster** (lokal im Modul, Ebenen pro Achse, Zugriff `g[x, y, z]`, `_` für eine fehlende
Achse, `*` für Linie/Ebene, `g.name` als Zahl oder Ebene):

```
module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }
    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }
    anchor ecke = g[rechts, hinten, deckel];
    anchor oben = g[links, vorne, deckel], dir = [0, 0, 1];
}
```

Listen erzeugen indizierte Ebenen: `x: loch = [for (i = [0:3]) 6 + i * 12];` → `g[loch[2], …]`.
Transformationen vor `grid` platzieren das Raster: `translate(g[rechts, vorne, boden]) grid l { … }`.

**Anker und benannte Instanzen:** `k: gehaeuse(w = 60);` benennt die Instanz; `k.ecke` ist ein
Punkt im aktuellen System, `dir(k.oben)` die Richtung, `rotate(to = v)` dreht z auf `v`. In
Schleifen mit Index: `for (i = [0:3]) k[i]: halter();` → `k[2].oben` (Index ganzzahlig).
Benannte Instanzen auf oberster Ebene der Hauptdatei sind Bauteile (Exportnamen):

```
k: gehaeuse(w = 60);
translate(k.ecke) cube([10, 10, 2]);
translate(k.oben) rotate(to = dir(k.oben)) cylinder(h = 5, r = 1);
```

**Verrundung/Fase** – drei Formen:

```
difference(fillet = 1) { cube(20); translate([10, 10, -1]) cylinder(h = 22, r = 4); } // neue Kanten
cube(20, $fillet = 1);                                   // eigene Kanten jedes Primitivs
fillet(2) {                                              // Zonenblock: 1. Kind Körper, Rest Zonen
    box(g[links, vorne, boden], g[rechts, hinten, deckel]);
    zone(g[rechts, *, deckel], d = 2);                   // Zylinder um eine Rasterlinie
    zone(g.boden, d = 1);                                // Platte um eine Rasterebene
    translate([15, 10, 15]) sphere(6);                   // beliebiges Volumen als Zone
}
fillet(1.5, select = "touch") { … }                      // auch berührende Kanten
```

Kanten zählen, wenn sie ganz in einer Zone liegen (`select = "inside"`). Leere Auswahl ist
eine Warnung („0 Kanten“), ein zu großer Radius ein Kernel-Fehler (Körper bleibt ohne
Verrundung). `chamfer` und `shell(t) { körper; zone; }` folgen demselben Muster.
`inspect --line` des Blocks zeigt die Kantenanzahl, `render --line` die Zonen (blau) und die
Kanten (orange).

**Modifier:** `#` rot halbtransparent zusätzlich, `%` Hilfsgeometrie (grau, nicht im
Ergebnis), `!` nur dieser Teilbaum, `*` ignoriert.

**Profile:** 2D-Formen (`square`, `rect`, `circle`, `polygon`, `spline`) in `linear_extrude`,
`rotate_extrude`, `loft`, `sweep`; Freiformflächen mit `surface(points)` + `thicken(t)`.
