# Unabhängige Prüfung der Farbfunktionen (Commit d8fe983)

Durchgeführt von einem unabhängigen Test-Agenten am 2026-10-23 gegen die Anforderungen des
Nutzers (Mehrfarbigkeit mit Export für Bambu Studio/OrcaSlicer, kleine gedeckte Palette mit
englischen Namen, Warnung + Zufallsfarbe bei unbekannten Namen, dunkles Thema). Testfälle in
`cases/`, Bilder in `img/`, Hilfsskripte `check3mf.py` (3MF-Struktur, Netze, Überlappung,
lib3mf), `contrast.py` (WCAG-Kontrast der Token), `deltae.py` (Farbabstände).

## Ergebnis je Anforderung

| Anforderung | Ergebnis |
| --- | --- |
| Mehrfarbig, Export für Bambu/Orca | teilweise: 3MF formal sauber (lib3mf, trimesh, Teile je Farbe, `extruder` je Teil); Darstellung und UI-Slots fehlerhaft (F1–F4) |
| Palette | erfüllt |
| Unbekannte Namen | teilweise: Warnung + stabile Farbe ja, Kollisionen (F5) |
| Dunkles Thema | CLI erfüllt, Token-Kontraste nachgerechnet; UI nicht geprüft (kein Build) |

## Fehler (nach Schwere)

- **F1 (hoch)** Gleiche Geometrie mit verschiedenen Farben wird in Ansicht (CLI/UI) und STEP in
  der ersten Farbe gezeigt (geteilter gecachter Knoten → ein Tag → `tagColors` nimmt den ersten
  Knoten). `color("red") cube(5); translate([10,0,0]) color("blue") cube(5);` → zwei rote
  Würfel. Schleifen `for (i) color(names[i]) cube(…)` → alles rot. 3MF ist richtig.
  Fälle: `cases/pal2.scad`, Bilder `img/pal2.png` vs. `img/pal2_regions.png`, `img/pal_iso_light.png`.
- **F2 (hoch)** UI-Slotwahl (`SetFilament`) schreibt `filament = n` an die `color()`-Anweisung;
  ist die Farbe ein Parameter (`module bar(c) color(c) …`) oder kommt sie aus einer Schleife,
  bekommen alle Farben dieser Anweisung denselben Slot. Fall: `cases/slot_mod.scad`, `cases/palette2.scad`.
- **F3 (mittel)** Flächenfarbe der Ansicht weicht vom Druck ab: (a) Überlappung
  (`cases/r_overlap2.scad`, `r_overlap3.scad`, `img/r_overlap23.png`; auch Rahmen in
  `examples/spec/mehrfarbig.scad`), (b) Verrundung an Farbgrenze grau gezeigt, rot exportiert
  (`cases/r_fillet2.scad`, `img/r_fillet2*.png`), (c) `linear_extrude(5) color("red") square(10);`
  rot gezeigt, als Standard/Slot 1 exportiert, fehlt in der Farbliste, keine Warnung
  (`cases/r_extrude.scad`, `img/r_extrude_*.png`).
- **F4 (mittel)** Material außerhalb von Bauteilen: Farbliste zeigt 0 mm³, UI-Export lässt es
  ohne Hinweis weg; CLI-Export ohne `--part` legt Bauteile zu einem Objekt zusammen und meldet
  trotzdem „je Bauteil ein Objekt“ (`cases/inc/main_use.scad`).
- **F5 (mittel)** Zufallsfarben kollidieren (nur Farbton variiert): `baz`/`violet` und
  `x`/`rose` exakt gleich, mehrere fast gleich oder nah an Palettenfarben; gleiche Farbe →
  zusammengelegt, gleicher Slot (`cases/randnames.scad`).
- **F6 (mittel/niedrig)** Splitter in Farbregionen bei gleichfarbigen Nachbarn: dünne Flächen
  (0,00016 mm) überlappen den Nachbarn, Volumen 2000,032 statt 2000, ohne Warnung
  (`cases/r_touch*.scad`).
- **F7 (niedrig)** Alpha-Varianten werden eigene Einträge/Slots mit gleichem Namen; Konfliktprüfung
  sieht über Alpha nicht; `[255,0,0]`, negative Werte, `alpha = 2` still geklemmt;
  `color("transparent")` ohne Warnung, als Zufallsfarbe gelistet, exportiert #00000000;
  `color("")` leerer Name (`cases/values.scad`).
- **F8 (niedrig)** Automatische Slots über 16 ohne Warnung; zwei Farben auf demselben expliziten
  Slot bzw. Slot 1 neben „Standard“ ohne Hinweis (`cases/f1.scad`); einfarbiges Modell exportiert
  trotzdem eine `colorgroup` (löst in Bambu vermutlich den Farbdialog aus; `cases/plain.scad`).
- **F9 (niedrig)** `--per-color -o x.stl` schreibt ZIP unter `.stl`; `--per-color --part figur`
  druckt die Farbliste der ganzen Datei; Farbliste nennt bei `use`/`include` keine Datei.
- **Daemon:** ein langer `CAD_IPC_DIR` überschreitet die Unix-Socket-Grenze (107 Zeichen,
  „path must be shorter than SUN_LEN“); der Daemon startet nicht, ohne sichtbaren Hinweis.

## Hinweise zu Bambu Studio (aus dem Quellcode, nicht am Programm geprüft)

Bei 3MF fremder Programme mit `colorgroup` öffnet Bambu Studio einen Farbdialog; dessen
Standard „Anhängen“ hängt Farben als neue Filamente an, und bei OK überschreibt
`apply_volume_filament_ids` den `extruder` jedes Teils (also auch `filament = n`); bei
„Abbrechen“ bleiben die Slots erhalten (`Plater.cpp` ~8890, `ObjColorDialog.cpp` 647 ff.).

## Korrekt geprüft

Verschachteltes `color()`, Überlappungsregel (spätere gewinnt), `intersection`, `difference`,
Module/`children()`/Schleifen/`include`/`use`/`scale`/`resize`/`mirror`, Modifier,
Warnungen für ungültige Hex/Vektoren, `filament`-Konflikte mit Warnung, `--single-color`,
ZIP je Farbe, XML-Escaping, Umlaute, 40 Körper in 8 Farben ≈ 1 s.
