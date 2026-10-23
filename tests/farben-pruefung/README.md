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

## Status der Befunde (Bericht 12, `docs/reports/12-farben-korrekturen/report.pdf`)

Alle Fälle aus `cases/` laufen als Regressionstest `crates/cad-cli/tests/farben_pruefung.rs`
(jede Datei ohne Fehler, Regionsvolumen = Bauteilvolumen, Zufallsfarben mit Abstand zur
Palette), dazu je Befund ein eigener Test.

| Befund | Status | Korrektur | Tests |
| --- | --- | --- | --- |
| F1 | behoben | Ansicht zeigt die Farbregionen (UI-Standard, `cad render`); Farbe je Fläche nur aus Knoten unter dem gezeichneten Knoten, bei gleichem Inhalt der mit passender Hülle; STEP-Flächenfarben aus den Regionen (`face_region_colors`). Cache-Wiederverwendung bleibt. | `f1_equal_geometry_…`, Vitest „F1“, Playwright G01, Tauri T15 |
| F2 | behoben | `filament("red", 4);` auf oberster Ebene: Slot-Tabelle mit Vorrang; die UI schreibt sie, wenn die Farbe an einer Anweisung kein Literal ist | `set_filament_of_a_parameter_colour_writes_a_table`, `filament_table_rules`, `filament_declarations`, G02, T15 |
| F3 | behoben | (a, b) Regionen in der Ansicht; (c) einheitliche 2D-Farbe färbt die Extrusion, gemischte → Warnung, Standard | `f3_view_matches_the_print`, G03 |
| F4 | behoben | Objekt „ohne Bauteil“ in Liste und Export, CLI = UI, Meldung nennt die Objekte | `f4_material_outside_parts`, `material_outside_parts_is_exported`, G04, T15 |
| F5 | behoben | L, C, Farbton aus dem Hash; ΔE ≥ 0,07 zur Palette, ≥ 0,06 zu anderen Zufallsfarben im Modell | `f5_random_colours_keep_their_distance`, `random_colours_do_not_collide_in_a_model` |
| F6 | behoben | nur andersfarbiges späteres Material wird abgezogen | `f6_no_slivers_…`, alle Fälle (Summe auf 10⁻⁶) |
| F7 | behoben | Identität = RGB (alpha nur Ansicht), Klemmen mit Warnung, `transparent`/`""`/`color()` färben nicht (Warnung) | `f7_value_rules` |
| F8 | behoben | Warnung über 16 Slots, Hinweis bei geteiltem Slot und Slot 1 neben Standard; keine Farbgruppe (auch einfarbig) | `f8_slot_remarks_and_single_colour`, `no_color_group_by_default` |
| F9 | behoben | `--per-color` nur mit `.zip`, `--part` filtert die Farbliste, Zeilen anderer Dateien als `lib.scad:1` | `f9_per_colour_export_and_list`, Vitest „F9“ |
| Daemon | behoben | zu langer Socketpfad → `/tmp/cad-<uid>/<hash>-daemon.sock`, in `cad daemon status` | `long_ipc_dir_uses_short_socket_path` |
| Bambu-Dialog | vermieden | 3MF ohne `m:colorgroup` (Dialog öffnet nur mit Farbgruppe); `--color-group` für andere Programme, Meldung „Abbrechen“ | `no_color_group_by_default`, `export_multicolour_single_and_per_colour` |

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
