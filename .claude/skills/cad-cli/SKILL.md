---
name: cad-cli
description: Modelle der parametrischen CAD-Sprache (OpenSCAD-Stil mit Rastern, Ankern und Fillet-Zonen, Dateien *.scad) ohne UI entwickeln und prüfen – Übersicht, Diagnosen, Bilder (Kontaktbogen, Raster/Anker eingeblendet, Schnitt, Röntgen), Messungen an der exakten B-Rep (Abstand mit Fußpunkten, Radius/Durchmesser, Winkel, Länge, Fläche, Körper, Schnittkonturen und Wandstärke, Volumen, Kollisionen, Volumen je Farbe), Beobachten bei Dateiänderung, Parameter setzen und Export STL/3MF (mehrfarbig für Bambu Studio/OrcaSlicer)/STEP mit der CLI `cad` (crates/cad-cli). Verwenden, wenn ein Modell geschrieben, geändert, untersucht oder Fehler darin gesucht werden.
---

# CAD-CLI `cad`

Die CLI wertet eine Quelldatei mit denselben Crates aus wie die UI (Parser, Auswertung,
Engine, SMLib-Kernel nativ) und zeigt das Ergebnis als Text, JSON oder Bild. Damit lässt sich
ein Modell komplett headless bauen und kontrollieren – auch parallel zu einem Menschen, der
dieselben Dateien in der UI bearbeitet (`cad watch` bemerkt jedes Speichern).

## Bauen und aufrufen

```sh
# SMLib nativ: vorgebaute Bibliotheken (schnell) oder Build über CMake (~3 min beim ersten Mal)
export SMLIB_PREBUILT=/home/user/controller-cad/smlib/build-native   # falls vorhanden
smlib/fetch.sh                                                         # Quellen (tangency-labs/smlib @ 7eba8aa), falls fehlend
cargo build --release -p cad-cli
target/release/cad --help              # Hilfe, englisch (ohne Argumente: Desktop-UI)
target/release/cad usage               # Manual für Agenten (englisch, Themen: cad usage --list)
```

Datei `-` liest von stdin: `echo 'cube(5);' | cad check -`. Ohne Befehl startet `cad` die
Desktop-UI (`cad`, `cad datei.scad`, `cad ordner/`); ohne grafische Sitzung (kein
`DISPLAY`/`WAYLAND_DISPLAY`, z. B. in Containern und CI) endet das mit Exit 2 und einem Hinweis –
Agenten arbeiten immer mit den Befehlen unten, die UI ist nie nötig. Headless-Build ohne UI:
`cargo build --release -p cad-cli --no-default-features`.

## Empfohlener Ablauf für Agenten

1. **Verstehen:** `cad outline datei.scad` – Module mit Parametern (Standardwert, Typ), Raster
   und Anker je Modul, Funktionen, Variablen mit Werten, benannte Instanzen, Bauteile (Hülle,
   Volumen), `include`/`use`, Diagnosen. Ein Aufruf reicht für ein Projekt.
2. **Editieren:** Datei ändern (Edit-Werkzeug) oder `cad set` für Standardwerte/Literale.
3. **Prüfen:** `cad check datei.scad` – erst weiter, wenn keine Fehler. Warnungen lesen
   (z. B. „0 Kanten“ bei leerer Zonenauswahl).
4. **Ansehen:** `cad render datei.scad -o /tmp/x.png --views all --show grids,anchors` und
   **das Bild ansehen** (Read-Tool). Für ein Modul `--module NAME` (Raster sind nur im eigenen
   Modul sichtbar), für Details `--line N`, `--section z=…`, `--xray`, `--show labels,dims`.
5. **Nachmessen:** `cad inspect datei.scad --line N` (Knoten, Hülle, Volumen, Kanten,
   Kernel-Meldung) und `cad measure`: exakt an der B-Rep Abstand (`--distance face@P face@Q`),
   Radius (`--radius edge@P`), Winkel, Länge, Fläche, Körper; im Schnitt (`--section z=…`)
   Konturen, Bohrungskreise und Wandstärke (`--wall P`); `--at P` sagt, welche Elemente bei P
   liegen und wie man sie nennt. Dazu Abstände zwischen Ankern/Rasterpunkten, Hülle, Volumen,
   Kollisionen der Bauteile, Abstand zweier Bauteile.
6. **Ausgeben:** `cad export datei.scad -o teil.step` (oder `--all-parts -o ordner/`). Zum
   Mehrfarbdruck `cad export datei.scad -o teil.3mf` (Standard mehrfarbig: je Bauteil ein
   Objekt, Material außerhalb der Bauteile als Objekt „ohne Bauteil“, ein Teil je Farbe mit
   Filament-Slot; ohne Farbgruppe, daher übernehmen Bambu Studio und OrcaSlicer die Slots ohne
   Farbdialog; vorher `cad measure datei.scad --colors` für Slots, Volumen und Masse je Farbe).

Der Kernel-Cache bleibt zwischen den Aufrufen erhalten (**Daemon**, unten): ein zweiter Aufruf
auf derselben oder einer wenig geänderten Datei rechnet nur die geänderten Knoten neu. Für
viele kleine Schritte geht zusätzlich `cad watch datei.scad --json --render /tmp/x.png --views all`
in einem Hintergrundprozess – jede Dateiänderung ergibt eine JSON-Zeile und ein neues Bild.

## Daemon (Cache zwischen Aufrufen)

`check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure` und `set` rechnen in einem
Hintergrundprozess `cad daemon serve`, den der erste solche Aufruf startet (abgekoppelt, Log
`daemon.log` im Socket-Ordner) und der den Engine-Cache hält. Ausgabe, Exit-Codes, Dateien und
JSON sind dieselben wie ohne Daemon (nur `--json` hat zusätzlich `daemon: {used, reason}`, und
die Zähler `stats.cache_hits`/`kernel_calls`/`cached` sowie Zeitfelder zeigen, was tatsächlich
passiert ist). Der Cache ist inhaltsbasiert: geänderte Dateien (auch eingebundene) werden immer
neu gelesen, ein Ergebnis ist nie veraltet. Nichts zu tun für Agenten – einfach die Befehle
nutzen; `watch` hat seinen eigenen Cache und nutzt den Daemon nicht.

| Befehl / Einstellung | Wirkung |
| --- | --- |
| `cad daemon status [--json]` | PID, Version/Build, Laufzeit, Leerlauf, Anfragen, Projekte, Cache (Einträge, MiB, Grenze, Trefferquote, verdrängt), RSS |
| `cad daemon stop` | beenden (wartet auf den laufenden Befehl) |
| `cad daemon serve [--cache-mb N] [--idle-minutes M] [--max-projects N] [--watchdog-secs S]` | im Vordergrund (Fehlersuche) |
| `--no-daemon` (jeder Befehl), `CAD_NO_DAEMON=1`, `[daemon] enabled = false` | ohne Daemon rechnen |
| `--verbose` | auf stderr: Daemon verwendet (Zeit, Cache-Treffer) oder warum nicht |

Grenzen (Standard): 512 MiB Cache (Knotenergebnisse + Netze, im Daemon pro Knoten am Heap
gemessen), 16384 Einträge, Ergebnisse von 8 Projekten (Hauptdateien), Cache leeren bei RSS über
Start + 1,5 × Grenze, Ende nach 15 min ohne Anfrage, Watchdog 30 min (ein hängender Befehl beendet
den Daemon, der Befehl läuft dann ohne Daemon und meldet das auf stderr). Einstellbar in
`$XDG_CONFIG_HOME/controller-cad/config.toml` (bzw. `~/.config/…`, Pfad auch `CAD_CONFIG`):

```toml
[daemon]
enabled = true
cache_mb = 512
max_projects = 8
idle_minutes = 15
watchdog_secs = 1800
rss_factor = 1.5
```

oder über `CAD_DAEMON_CACHE_MB`, `CAD_DAEMON_MAX_PROJECTS`, `CAD_DAEMON_IDLE_SECS`/`_IDLE_MINUTES`,
`CAD_DAEMON_WATCHDOG_SECS`, `CAD_DAEMON_RSS_FACTOR` (Umgebung vor Datei, Argumente von `serve`
vor beidem). Gibt es keinen Daemon (Windows, Socket-Ordner unbrauchbar, Fehler), rechnet der
Befehl still selbst. Ein `cad` mit anderer Build-ID ersetzt den laufenden Daemon. Socket-Ordner
wie bei `cad ui`: `$CAD_IPC_DIR`, sonst `$XDG_RUNTIME_DIR/controller-cad`, sonst
`/tmp/controller-cad-<uid>` (0700, Socket 0600); ist der Socketpfad zu lang (> 107 Byte), liegt
der Socket unter `/tmp/cad-<uid>/<hash>-daemon.sock` (`cad daemon status` zeigt ihn).

## Zusammenarbeit: Nutzer in der Desktop-UI, Agent editiert und prüft

Der Nutzer hat die Datei in der UI offen (`cad datei.scad`), der Agent ändert dieselbe Datei
und prüft mit der CLI. Die UI beobachtet die Datei und alle eingebundenen Dateien:

- **Datei ohne ungespeicherte Änderungen in der UI:** jede Änderung des Agenten (Edit-Werkzeug,
  `cad set`) erscheint sofort im Editor und in der 3D-Ansicht (~0,2–0,3 s, Bericht 9); Cursor und
  Bildausschnitt des Nutzers bleiben, die Änderung ist ein eigener Rückgängig-Schritt.
- **Nutzer hat ungespeicherte Änderungen:** die UI überschreibt nichts, sondern zeigt die Leiste
  „Datei wurde extern geändert: Neu laden / Meine behalten / Vergleichen“. Deshalb vor dem
  Schreiben `cad ui status --json` prüfen: steht die Datei in `dirty`, den Nutzer fragen statt
  zu schreiben.
- Ablauf: `cad ui status` (was ist offen, Cursorzeile, ausgewählter Knoten, Kontextmodul, Werte
  der Parameterleiste, Diagnosen) → editieren → `cad check` → `cad render …` (eigene Bilder) →
  bei Bedarf `cad ui show datei.scad --line N [--module m]` (zeigt dem Nutzer die Stelle: Fenster
  nach vorn, Cursor dorthin) und `cad ui screenshot -o /tmp/ui.png` (die 3D-Ansicht, wie der
  Nutzer sie sieht; Bild ansehen).
- Ohne laufende UI enden `cad ui …` mit Exit 3 („keine laufende Desktop-UI“) – dann einfach
  headless weiterarbeiten. Alles andere funktioniert ohne UI unverändert.
- Schnitt aus der UI: „Als `cad render …` kopieren“ in der Schnittansicht liefert denselben
  Schnitt als CLI-Aufruf (`--section z=12 --azimuth … --elevation …`); `cad ui status` nennt ihn
  unter `section.cli`.

| Befehl | Ergebnis |
| --- | --- |
| `cad ui status [--json]` | je Fenster: `project {root, main, active, files, folder}`, `dirty` (ungespeicherte Dateien), `cursor {file, line, col}`, `selection {line, nodes}`, `context` (Modul), `params` (Parameterleiste), `diagnostics`, `scene {version, complete, nodes, parts}`, `section` |
| `cad ui show DATEI [--line N] [--module m]` | Datei öffnen bzw. zu ihr wechseln, Cursor setzen, Fenster nach vorn |
| `cad ui screenshot -o bild.png` | PNG der 3D-Ansicht |

## Befehle

| Befehl | Zweck |
| --- | --- |
| `cad check DATEI [--no-kernel]` | Diagnosen `datei:zeile:spalte: Fehler/Warnung (Syntax/Auswertung/Kernel): …` |
| `cad outline DATEI [--no-kernel]` | Übersicht (siehe oben); `--no-kernel`: ohne Hüllen/Volumen, schneller |
| `cad eval DATEI [--module m] [-D name=wert]…` | Baum (Knoten, Zeile, Status, Hülle), Werte der Variablen, Raster, Anker, `echo` |
| `cad inspect DATEI --line N` | Was Zeile N erzeugt: Knoten, Hülle, Volumen/Fläche, ausgewählte Kanten, Hinweise („4 Kanten“), Fehler, Griffe |
| `cad render DATEI -o bild.png [Optionen]` | Bild (Optionen unten) |
| `cad measure DATEI [Optionen]` | Messen (Optionen unten) |
| `cad watch DATEI [--json] [--render bild.png …]` | Beobachten und bei jeder Änderung neu auswerten (unten) |
| `cad set DATEI --param modul.param=wert \| --literal [datei:]zeile:spalte=wert [--dry-run]` | Code über die Engine ändern (unten) |
| `cad export DATEI -o teil.stl\|teil.3mf\|teil.step [--schema ap214]` | Exportdatei (STEP: exakte B-Rep, AP242, Flächenfarben aus den Farbregionen; 3MF: **mehrfarbig**, je Bauteil ein Objekt + „ohne Bauteil“, ein Teil je Farbe mit Filament-Slot für Bambu Studio/OrcaSlicer, ohne Farbgruppe; `--color-group` mit Farbgruppe für andere Programme – Bambu öffnet dann einen Farbdialog, „Abbrechen“ behält die Slots; einfarbig mit `--single-color`) |
| `cad export DATEI --per-color [--part NAME]… -o farben.zip` | ZIP mit einem STL je Farbe (und Bauteil): `F<slot>_<farbe>.stl`, alle im selben Koordinatensystem (Endung `.zip` Pflicht) |
| `cad export DATEI --part NAME [--part NAME2] -o …` | Nur diese Bauteile (benannte Instanzen auf oberster Ebene, z. B. `deckel`, `k[2]`) |
| `cad export DATEI --all-parts -o ordner/ [--format stl\|3mf\|step]` | Jedes Bauteil als eigene Datei (`deckel.stl`, `k_2.step`) |
| `cad diff-kernels DATEI…` | Vergleich SMLib ↔ solidcore (nur Entwicklung, `--features solidcore`) |

Gemeinsame Optionen: `--json`, `--module m` (Modul als Kontext, verschachtelt `aussen.innen`),
`-D name=wert` (Parameter des Kontextmoduls, Wert als JSON: `-D w=60`, `-D hohl=true`,
`-D p=[1,2,3]`). `-D` braucht einen Modulkontext.

**Kontext wie im Editor:** Liegt `--line` in einer Moduldefinition, zeigen `inspect` und
`render` nur dieses Modul mit seinen Standardwerten (und `-D`). `--whole-file` behält den
Dateikontext.

**Ziele** (`--fit`, `measure --bounds/--volume/--area`): `all` (Wurzel), `part:NAME` (oder nur
`NAME`), `line:N` (Knoten der Zeile), `node:I` (Index aus `cad eval`).

### `render`

| Option | Wirkung |
| --- | --- |
| `--view iso\|front\|back\|top\|bottom\|left\|right` | Standardansicht (Standard `iso`) |
| `--views iso,front,top,right` / `--views all` | Kontaktbogen: beschriftete Teilbilder mit **gleicher Skalierung** (`all` = iso, front, top, right); `--size` ist dann die Gesamtgröße (Standard 1600x1200) |
| `--azimuth A --elevation E` | Freie Kamera: Azimut 0° = von vorn (−y), 90° = von rechts (+x); Höhe 90° = von oben; `iso` = 45°/35,26° |
| `--zoom Z`, `--target x,y,z`, `--fit ZIEL` | Vergrößern, Blickpunkt, Einpassen auf ein Ziel (Standard: alles Gezeichnete) |
| `--ortho` (Standard) / `--perspective [--fov 35]` | Projektion |
| `--part NAME` (mehrfach) | Nur diese Bauteile |
| `--line N` (mehrfach), `--node I` (mehrfach), `--only` | Knoten voll, Rest gedimmt (mit `--only` weggelassen); Zonen blau, abgezogene Körper rot halbtransparent, ausgewählte Kanten orange, gescheiterte Kanten rot |
| `--show grids,anchors,handles,edges,labels,bbox,dims` (oder `all`) | Überlagerungen: Raster des Kontextmoduls (Ebenen als Rechtecke am Rand, Ausdrucksebenen gestrichelt, Schnittpunkte als Punkte, Ebenennamen `g.links`), Anker als gelbe Rauten mit Namen und Richtungspfeil, Griffe als orange Quadrate, verdeckte Kanten gestrichelt, `labels`: Rasterpunkte `g[a,b,c]`, Griffe, Bauteilnamen; Hüllquader, `dims`: Maße B/T/H in mm |
| `--section x=WERT\|y=WERT\|z=WERT` | Schnitt an einer Achsebene, Schnittfläche gefüllt (rot schraffiert) |
| `--section-plane px,py,pz,nx,ny,nz` | Schnitt an beliebiger Ebene (Punkt, Normale) |
| `--section-flip` | Die andere Hälfte behalten |
| `--xray` | Alles halbtransparent mit Kanten |
| `--theme dark` | Dunkles Farbschema (wie die UI; Palette-Farben in der dunklen Variante) |
| `--show regions` / `--show faces` | Farbregionen (Körper je Farbe wie exportiert; Standard bei Modellen mit Farben) bzw. Farbe je Fläche (schneller, bei Überlappung/Verrundung ungenau) |
| `--size BxH`, `--no-axes`, `--whole-file` | Bildgröße, ohne Achsenkreuz, Dateikontext |

**Schnitt-Konvention (gleich in der UI):** Ebene durch Punkt `p` mit Normale `n`; entfernt
wird die Hälfte, in die `n` zeigt, es bleibt `(q − p) · n ≤ 0`. `x=10` heißt `p = (10,0,0)`,
`n = +x` (entfernt x > 10), ebenso `y=`/`z=`; `--section-flip` kehrt `n` um. Verfahren:
Clipping im Renderer mit gefüllter Kappe (Stencil-Prinzip: Kappe dort, wo die Ebene im
Körper liegt), kein Kernel-Boolean – schnell und für jede Ebene gleich. In `iso` sieht man
die Kappe von `z=`/`x=`-Schnitten; für `y=` `--section-flip` oder `--view back` nehmen.

Raster sind nur im eigenen Modul sichtbar: `--show grids` im Dateikontext zeigt nur Raster
der obersten Ebene (Hinweis in `notes`); für Modulraster `--module NAME` oder `--line N` in
der Moduldefinition.

### `measure`

| Option | Ergebnis |
| --- | --- |
| `--between A B` | Abstand und Differenzvektor. A/B: Anker `k.ecke`, `k[2].ecke` (im Modulkontext auch `ecke`), Rasterpunkt im Kontext `g[rechts, hinten, deckel]` (Einträge auch Zahlen oder `_`), Koordinate `[x,y,z]` |
| `--bounds [ZIEL]` | Achsparallele Hülle (min, max, Größe) |
| `--volume [ZIEL]`, `--area [ZIEL]` | Volumen mm³ / Fläche mm² (Kernel, exakt) |
| `--colors` | Farbregionen: Volumen und Masse (PLA 1,24 g/cm³) je Farbe und Filament-Slot, je Bauteil (Summe = Bauteilvolumen) |
| `--interference [--tolerance 0.001]` | Paarweises Schnittvolumen aller Bauteile (Kernel-Schnitt der exakten B-Rep), Kollisionen über der Toleranz (mm³) |
| `--clearance A B [--mesh-tol 0.01]` | Kleinster Abstand zweier Bauteile über die Tessellierung (Dreieck-Dreieck-Abstand), Genauigkeit ± 2·mesh-tol; überschneiden sie sich: 0 und `collision: true` |
| `--distance A B` | Kleinster Abstand zweier **Elemente** mit Fußpunkten, exakt aus der B-Rep (Ebenen, Zylinder, Kegel, Kugeln, Tori, Linien, Kreise geschlossen; sonst numerisch mit Toleranz); zwei parallele Ebenen: Normalabstand (`details`: „parallele Ebenen“, dazu der kleinste Abstand der begrenzten Flächen); parallele Achsen: Achsabstand. A/B: Punkt wie `--between` oder Elementreferenz (unten). Mehrfach |
| `--radius REF` | Radius, Durchmesser, Mittelpunkt, Achse einer Kreiskante (`edge@P`) oder runden Fläche (`face@P`: Zylinder, Kegel mit Halbwinkel und Spitze, Kugel, Torus mit Ringradius; Verrundungen an Kreiskanten als Rotationsfläche erkannt; Freiform: Krümmungsradien, `exact: false`) |
| `--angle A B` | Winkel zweier Kanten oder Flächen: Ebenen 0–90° (`values.opening` = Öffnungswinkel an der Schnittgeraden, `normals`), Geraden 0–90° (an gemeinsamer Ecke 0–180°), Gerade–Ebene; Zylinder/Kegel über die Achse |
| `--length REF`, `--area face@P` | Länge einer Kante (exakt für Gerade/Bogen), Fläche einer Fläche (eben exakt) |
| `--body REF` | Bauteil bzw. Körper des Elements: Volumen, Oberfläche, Schwerpunkt, Hülle aus der B-Rep (`details[0]`: „Bauteil k (Zeile 21)“) |
| `--section z=W` (`--section-plane p,n`, `--section-flip`) | Schnitt wie `cad render`: Ebene ∩ B-Rep über den Kernel; allein: Bereiche mit Fläche und Löchern, Konturen mit Segmenten (`kind`, `center`, `radius`, `exact`, `source` = geschnittene Fläche), Kreise (Bohrungen), kleinste Wandstärke (Schätzung über 8 Stellen je Abschnitt). Mit `--distance`/`--radius`/`--angle`/`--length`/`--area region@P`/`--wall` misst man in der Ebene |
| `--wall P` | Wandstärke im Schnitt am Konturpunkt nächst P, senkrecht zur Kontur durchs Material (braucht `--section`) |
| `--at P` | Was bei P liegt: nächster Eckpunkt, Kantenmitte, Kreismitte, Kante, Fläche (im Schnitt: Kontur, Bereich) mit Referenz, Abstand und Herkunft – zum Finden der Referenzen |

**Elementreferenzen:** `vertex@P`, `mid@P` (Kantenmitte), `center@P` (Kreismitte), `edge@P`,
`face@P`, `body@P` nehmen das Element des Ergebnisses nächst P (P wie `--between`:
`[x,y,z]`, Anker, Rasterpunkt); mit `--section` gelten `vertex@`, `mid@`, `center@`, `edge@`
für die Schnittkontur und `region@P` für den Bereich, der P enthält (`face@`/`body@` dann
nicht). Reihenfolge der Ergebnisse in `measurements`: Abstände, Radien, Winkel, Längen,
Fläche, Körper, Wandstärken. Jede Messung hat `text` (Zeilen wie in der UI), `cli` (die
Argumente, die sie wiederholen) und `picks[].origin` (Herkunft: `Zeile 11: box (Instanz k:
gehaeuse)`, `Rasterebene g.innen_l`, `Rasterpunkt g[b, h, o]`, `Rasterlinie g[loch[0],
mitte, *]`, `Anker k.ecke`). Die UI („Messen“, „Als cad measure kopieren“) erzeugt dieselben
Aufrufe.

Beispiel (Spec `gehaeuse`): `cad measure gehaeuse.scad --between k.ecke k.oben` → 67,082 mm,
Δ [−60, −30, 0]; `cad measure gehaeuse.scad --module gehaeuse --between "g[links,vorne,boden]" "g[rechts,hinten,deckel]"` → 53,852 mm.
Wandstärke: `cad measure gehaeuse.scad --section z=10 --wall "[0,15,10]"` → 3 mm (zwei Bereiche
à 90 mm², kleinste Wand ≈ 3 mm); Innenmaß: `--distance "face@[3,15,10]" "face@[57,15,10]"` →
54 mm (parallele Ebenen); Lochplatte: `cad measure lochplatte.scad --radius "face@[8,6,2]"` →
R 2 · ⌀ 4 (Achse durch [6, 6, …], Rasterlinie g[loch[0], mitte, *]), `--section z=2` → vier
Kreise R 2, `--distance "center@[6,6,0]" "center@[18,6,0]"` → 12 mm.

### `watch`

`cad watch DATEI [--json] [--render bild.png RENDER-OPTIONEN] [--once-after-change] [--timeout S]`
beobachtet die Datei und alle eingebundenen Dateien (`include`/`use`, auch neu hinzugekommene),
wertet bei Änderung in derselben Sitzung neu aus (Engine-Cache bleibt: nur geänderte Knoten
gehen an den Kernel) und schreibt pro Lauf eine Zeile auf stdout – mit `--json` ein
JSON-Objekt pro Zeile. Ende mit Strg+C, `--once-after-change` (nach dem ersten durch eine
Änderung ausgelösten Lauf) oder `--timeout S` (S Sekunden ohne Änderung).

### `set`

- `cad set datei.scad --param gehaeuse.w=50` schreibt den Standardwert (wie „Als Standard
  übernehmen“ in der UI; Modul auch aus `use`-Dateien, verschachtelt `aussen::innen.p`).
- `cad set datei.scad --literal 21:17=70` ersetzt das editierbare Literal an Zeile:Spalte
  (Spalte irgendwo im Literal; die Literale, die die UI ändert: Variablenwerte, Griffe,
  Rasterebenen, Standardwerte); in Projektdateien `--literal lib/teile.scad:12:20=4`.
- Mehrere `--param`/`--literal` in einem Aufruf; `--dry-run` zeigt nur den Diff. Vor dem
  Schreiben wird das geänderte Projekt ausgewertet: erzeugt die Änderung neue Fehler, wird
  nichts geschrieben (Exit 1, `refused: true`), außer mit `--force`. Geschrieben wird atomar
  (temporäre Datei + Umbenennen).

Exit-Codes (alle Befehle): `0` ok · `1` Diagnosen mit Fehlern (Ausgabe trotzdem vollständig;
`set`: Änderung abgelehnt) · `2` Aufruffehler (Argumente, Datei, unbekannter Anker/Bauteil/
Ebene) · `3` Kernel-/interner Fehler.

## JSON (Kurzfassung)

Jede Antwort ist ein Objekt mit `schema_version` (1), `command`, `file`, `ok`, `exit_code`,
`context` (Modulname oder `null`), `diagnostics: Diagnostic[]`, bei Aufruffehlern `error`, bei
den Befehlen mit Daemon `daemon: {used, reason}` (`reason` z. B. `--no-daemon`, sonst `null`).
`Diagnostic = { severity: "error"|"warning"|"info", message, source: "syntax"|"eval"|"kernel",
node, range, file? }`, `range = { from, to, line, col, end_line, end_col }` (1-basiert,
Offsets UTF-16). `Bounds = { min, max, size }`. Vollständiges Schema: Kopf von
`crates/cad-cli/src/main.rs`.

- `check`: `+ { errors, warnings }`
- `outline`: `+ { files, includes: [{file, line, path, use, found}], modules: [{name, qualified,
  file, line, end_line, params: [{name, default, default_value, type}], grids: [{name, line,
  planes: [{axis, name, value, kind: literal|expression|reference|implicit, expr}]}], anchors:
  [{name, point, dir, line}]}], functions: [{name, params, file, line, body}], variables:
  [{name, expr, value, text, literal, file, line}], grids, anchors: [{ref, point, dir}],
  instances: [{name, module, line, part}], parts: [{name, module, bounds, volume}], counts }`
- `eval`: `+ { scene }` – vollständige Szene wie im Protokoll (`web/src/engine/protocol.ts`,
  Typ `Scene`): `nodes[]`, `grids[]`, `anchors[]` (`text` = `k.ecke`), `hints[]`, `handles[]`,
  `modules[]`, `echoes[]`, `context.params`, `stats`.
- `inspect`: `+ { line, nodes: [{ idx, label, instance, range, status, dim, flags, subtracted,
  world_bounds, volume, area, selected_edges, opened_faces, hints, messages, cached }], hints,
  handles, echoes }`
- `render`: `+ { output, width, height, view, views: [{name, x, y, width, height, camera}],
  camera: {projection, fov_deg, azimuth, elevation, dir, up, right, target, eye,
  scale_px_per_mm, width_mm, height_mm}, lines, highlighted, visible_nodes: [{idx, label,
  instance, role, line}], triangles, show, overlays: {grids, grid_points, anchors, handles,
  labels}, section: {point, normal, keeps, method}|null, xray, bounds, notes, timings }`
- `measure`: `+ { between?: {a: {ref, kind, point}, b, delta, distance}, bounds?: {target,
  bounds}, volume?: {target, volume}, area?: {target, area}, interference?: {parts, pairs,
  collisions: [{a, b, volume}]}, clearance?: {a, b, distance, point_a, point_b, collision,
  accuracy}, measurements?: [{kind: distance|radius|angle|length|area|body|wall, title, value,
  unit, label, values: [{key, label, value, unit}], lines, guides, points, at, details,
  picks: [{kind, reference, label, point, origin}], exact, tol?, section?, text, cli}],
  section?: {plane, exact, max_dev, area, regions: [{area, holes, bounds, reference,
  contours: [{hole, area, segments}]}], circles: [{center, radius, diameter, full}],
  min_wall}, at?: [{point, elements: [{kind, reference, label, point, distance, origin}]}] }`
  – Werte auf 12 Stellen gerundet; `values` z. B. `distance, dx, dy, dz, min_distance,
  axis_distance` · `radius, diameter, sweep, semi_angle, major` · `angle, opening, normals` ·
  `volume, area, size_x/y/z` · `wall`
- `watch` (eine Zeile je Lauf): `{ schema_version, command, run, trigger: start|change,
  changed, ok, errors, warnings, diagnostics, stats: {nodes, cache_hits, kernel_calls}, volume,
  bounds, parts: [{name, volume, bounds}], render, timings: {total_ms, eval_ms, kernel_ms,
  render_ms} }`
- `set`: `+ { changes: [{kind, spec, file, line, col, old, new}], dry_run, written, refused,
  errors_before, errors_after, diff }`
- `export`: `+ { output, format, bytes, triangles, volume, area, bounds, step?, multicolor,
  color_group?, notes?, colors?, color_regions? }` (`--per-color`: `format: "zip"`, `files`)
- `outline`/`measure --colors`: `colors: [{key, label, source: default|palette|random|exact,
  filament, explicit, lines, places, volume, mass_g}]` (`places`: `Z.3` oder `lib.scad:3`), `color_regions`/`colors.parts: [{part, node,
  total, warning, regions: [{color, label, filament, volume, mass_g}]}]`
- `daemon status`: `{ running, pid, version, build_id, uptime_s, idle_s, idle_limit_s, watchdog_s,
  busy, current, pending, requests, failed, projects, max_projects, project_evictions, cache:
  {entries, nodes, meshes, bytes, limit_bytes, max_entries, evictions, hits, misses, hit_rate},
  rss_bytes, rss_start_bytes, rss_limit_bytes, rss_clears, socket, log }` (`running: false` ohne Daemon)

## Beispiele

```sh
cad outline examples/projekt-gehaeuse/gehaeuse.scad
cad render examples/spec/gehaeuse.scad -o /tmp/g.png --module gehaeuse --views all --show grids,anchors,labels
cad render examples/spec/gehaeuse.scad -o /tmp/s.png --section z=10 --show dims
cad render examples/spec/fillet_zonen.scad -o /tmp/f.png --line 12 --azimuth 120 --elevation 30
cad render examples/projekt-gehaeuse/gehaeuse.scad -o /tmp/d.png --part deckel --xray
cad measure examples/spec/gehaeuse.scad --between k.ecke k.oben --volume part:k
cad measure examples/projekt-gehaeuse/gehaeuse.scad --interference --clearance gehaeuse deckel
cad measure examples/spec/lochplatte.scad --at "[8,6,2]"
cad measure examples/spec/lochplatte.scad --radius "face@[8,6,2]" --distance "center@[6,6,0]" "center@[18,6,0]" --json
cad measure examples/spec/gehaeuse.scad --section z=10 --wall "[0,15,10]" --distance "edge@[3,15,10]" "edge@[57,15,10]"
cad measure examples/spec/fase_chamfer.scad --angle "face@[39.5,12,9.5]" "face@[20,12,10]"
cad set examples/spec/gehaeuse.scad --param gehaeuse.w=50 --dry-run
cad watch teil.scad --json --render /tmp/teil.png --views all --show grids,anchors &
```

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
fillet(r = function (p) p.z < 10 ? 2 : 4) { … }          // variabler Radius (Position p)
difference(fillet = function (p) 1 + p.x / 40) { … }     // auch an Booleans
```

Variabler Radius: die Funktion wird an Stützpunkten jeder Kante ausgewertet (`p` im System
der Anweisung); Hinweis „r 2–4“. Nur reine Ausdrücke über `p` (Arithmetik, `?:`, `let`,
Mathe-Builtins, `lookup`, eingefangene Variablen, Benutzerfunktionen); nicht für `chamfer`
und `$fillet`. Naht- und Polkanten (Zylinder, Kugel) und tangentiale Kanten zählen nie.

Kanten zählen, wenn sie ganz in einer Zone liegen (`select = "inside"`). Leere Auswahl ist
eine Warnung („0 Kanten“), ein zu großer Radius ein Kernel-Fehler (Körper bleibt ohne
Verrundung). `chamfer` und `shell(t) { körper; zone; }` folgen demselben Muster.
`inspect --line` des Blocks zeigt die Kantenanzahl, `render --line` die Zonen (blau) und die
Kanten (orange).

**Farben und Filamente** (Bericht 11, Korrekturen Bericht 12): `color("name")` nimmt die
Palette (16 gedeckte Farben, Groß/Klein egal): `red, orange, yellow, lime, green, teal, cyan,
blue, purple, magenta, pink, brown, black, gray (grey), silver, white`; `aqua` = cyan,
`fuchsia` = magenta. Andere Namen (auch CSS wie `lightblue`, deutsche wie `rot`) geben eine
Warnung und eine stabile Zufallsfarbe (mit Abstand zur Palette und zu den anderen Zufallsfarben
des Modells). `#rgb`, `#rrggbb`, `#rrggbbaa`, `[r, g, b(, a)]` (Werte 0–1; anderes wird mit
Warnung geklemmt) bleiben exakt; `alpha` wirkt nur in der Ansicht (`red` mit alpha ist der
Eintrag `red`); `transparent` und `""` färben nicht (Warnung).
Slot: `color("teal", filament = 2)` (benannt) an einem `color()` mit festem Farbwert; kommt
die Farbe aus einem Parameter oder einer Schleife (`module bar(c) color(c) …`), auf oberster
Ebene der Hauptdatei `filament("red", 4);` – das gilt für das ganze Modell und vor
`filament = n`. Sonst Slots in Reihenfolge des ersten Auftretens, Material ohne `color()`
immer Slot 1; über 16 Slots warnt, geteilte Slots zeigt ein Hinweis. Volumen je Farbe: der
Körper eines gefärbten Teilbaums gehört dieser Farbe, bei Überlappung gewinnt die spätere
Anweisung (auch ein inneres `color()`), Abgezogenes gilt für alle; `color()` im 2D-Profil
färbt die Extrusion (gemischte 2D-Farben: Warnung). Prüfen mit `cad outline` (Abschnitt
„Farben“, Zeilen anderer Dateien als `lib.scad:3`) oder `cad measure --colors`, ansehen mit
`cad render` (ein Modell mit Farben zeigt die Farbregionen wie gedruckt; `--show faces` die
schnellere Farbe je Fläche).

```
color("white") cube([40, 20, 2]);                              // Platte: Slot 1
color("red", filament = 3) translate([5, 5, 2]) cube([30, 10, 1]);  // Schrift: Slot 3
module knopf(c) color(c) cylinder(h = 2, r = 3);
filament("teal", 4);                                           // Slot einer Parameterfarbe
translate([10, 10, 3]) knopf("teal");
```

**Modifier:** `#` rot halbtransparent zusätzlich, `%` Hilfsgeometrie (grau, nicht im
Ergebnis), `!` nur dieser Teilbaum, `*` ignoriert.

**Skizzen** (Plan 17, Bericht 14): `sketch { … }` ist ein 2D-Profil aus Segmenten, jede
Zeile ein Segment mit eigener Fehlermeldung (Zeile:Spalte in `cad check`):

```
linear_extrude(5) sketch {
    grid s { x: l = 0, r = 40; y: u = 0, o = 20; }  // Raster in der Skizze erlaubt
    start(s[l, u]);                       // neue Kontur
    line(to = s[r, u]);                   // auch: by = [dx, dy], length = l, angle = a
    arc(to = [40, 20], r = 10);           // + large = true / cw = true (wie SVG)
    arc(center = [20, 20], angle = 90);   // Grad, negativ = im Uhrzeigersinn; oder center + to
    arc(through = [5, 30], to = [0, 20]); // durch einen Punkt
    arc(to = [0, 10]);                    // ohne r: tangential an das vorige Segment
    bezier(c1 = [-5, 5], to = [0, 0]);    // kubisch: c1, c2, to
    // spline([[…], …], tangent = true, end_tangent = [1, 0]);
    close();                              // Gerade zum Start (endet die Kontur dort, unnötig)
    start([10, 5]); …                     // weitere Kontur: Loch nach Windungsregel (Gegenlauf)
    hole { start(…); …; }                 // Loch in jeder Laufrichtung
}
module profil(b = 40) sketch { … }        // benannte Skizze; for/if/let und Module mit Segmenten
sweep() { sketch(closed = false) { start([0, 0]); line(to = [30, 0]); arc(to = [45, 15]); } circle(2); }
```

Fehler: „Kontur nicht geschlossen“, „unmöglicher Bogen“ (Radius < halbe Sehne, …),
„Selbstüberschneidung … mit Segment in Zeile n“, „Kontur ohne Fläche“. `loft` verbindet
Skizzen gleicher Segmentzahl Segment für Segment (Start = `start`), sonst als je eine Kurve.
In `cad render --line N` einer Segmentzeile ist die Skizze hervorgehoben.

**Ecken, Offset, Stroke** (Plan 17.3, Bericht 15) – alles exakt aus Geraden und Bögen (STEP:
`CIRCLE`), B-Splines als Biarcs (1 µm):

```
sketch {
    start([0, 0]);
    corner(r = 2);                 // direkt nach start: die Startecke
    line(to = [30, 0]);
    corner(r = 4);                 // zwischen zwei Segmenten: rundet diese Ecke
    line(to = [30, 20]);
    corner(chamfer = 3);           // Fase: Schnittpunkte im Abstand 3 vom Eckpunkt
    line(to = [0, 20]);
    corner(r = 1);                 // vor close(): Ecke zur Schlusslinie
    close();
}
fillet2d(2) square([30, 20]);                       // alle Ecken, konvex und konkav
fillet2d(5) { profil(); translate([6, 6]) circle(1); }   // nur Ecken in 2D-Zonen (select = "touch")
chamfer2d(1) polygon(…);
offset(r = 2) …;  offset(-1) …;  offset(delta = 1, chamfer = true) …;   // wie OpenSCAD
stroke(2, cap = "round" | "flat" | "square", join = "round" | "miter" | "bevel")
    sketch(closed = false) { … };  // offener Pfad → Fläche; stroke(1) circle(5) = Ring
```

`corner` passt nicht → Fehler an der corner-Zeile mit „höchstens ≈ r“; tangentiale Ecke →
„keine Ecke“. `fillet2d` meldet „n Ecken“ (Inline-Hinweis), zu große Radien je Ecke
(„Ecke bei [x, y]: … passt nicht“). `offset` ohne Argument: `r = 1`; `r` gewinnt vor `delta`;
negative Werte schrumpfen (schmale Teile verschwinden, Löcher wachsen), positive schließen
Löcher. `cad render --line N` auf offset/stroke/fillet2d zeigt die Ausgangskontur, die
Offset-Kontur gestrichelt, die Mittellinie des Strichs und die neuen Eckbögen; `--show handles`
zeigt den Radius-Griff von `corner`.

**`hull` und `minkowski`** (Plan 21): nur exakte Sonderfälle, sonst Warnung
„hull: <Grund>; Kinder werden unverändert gezeigt“ (`cad usage language` hat die Tabelle).
`hull` in 2D immer (Geraden + Bögen); in 3D: nur ebene Körper (Polyeder), Kugeln/Zylinder/Kegel
auf einer Achse (Rotationskörper), parallele Prismen gleicher Höhe (Zylinder + Quader →
Platte mit runden Enden), ≥ 3 gleich große Kugeln nicht in einer Ebene (abgerundeter Quader).
`minkowski`: Körper + `sphere(r)` (Offset), 2D-Profil + `circle(r)` (`offset(r)`),
`linear_extrude` + `cylinder`, zwei konvexe ebene Körper. `cad inspect --line N` zeigt den
Fall („exakt: Prisma“). Offset ist nur an rechtwinkligen Ecken exakt (Tetraeder → Warnung).

```
hull() for (x = [0, 20], y = [0, 10], z = [0, 5]) translate([x, y, z]) sphere(2);   // abgerundeter Quader
linear_extrude(3) hull() { circle(3); translate([20, 0]) circle(3); }                // Langloch-Platte
minkowski() { cube([20, 10, 5]); sphere(2); }
```

**Module als Namensräume:** `module teile { M3 = 3.2; function kopf(d) = 1.8 * d;
module schraube(d = 3) { … } }` (Klammern dürfen fehlen) → `teile.schraube(d = 4);`,
`k: teile.schraube();`, `a.b.c();`, `teile.kopf(teile.M3)`. Das innere Modul sieht die Parameter
des äußeren mit Standardwerten, nicht dessen Raster/Instanzen. `cad --module teile.schraube`,
`cad set --param teile.schraube.d=4`; `cad outline` zeigt Module und Funktionen mit Pfad.

**Text und SVG** (Plan 17.4/17.5, Bericht 16):

```
linear_extrude(1.2) text("Werkstatt", size = 11, font = "Inter:style=Bold", halign = "center", valign = "center");
linear_extrude(1) text("Maß 20 mm", font = "fonts/meine.ttf");   // Schriftdatei relativ zur .scad-Datei
linear_extrude(2) import("logo.svg", center = true);             // auch id = "…", scale = …, dpi = 96
```

- `text()` wie OpenSCAD (`size` ≈ Versalhöhe, em = size/0,72; Laufweite/Ausrichtung gleich
  OpenSCAD 2021.01; `spacing`, `direction` ltr/rtl/ttb/btt, `language`, `script`).
  Standard: `"cad stencil mono"` (eigene monospaced Schablonenschrift, Regular/Bold, keine
  Löcher – Innenräume über Stege verbunden; `stroke_width`/`bridge_width` in mm, Standard
  0,14·size bzw. 0,1·size, Grenzen 0,2·size bzw. 0,03…0,2·size, `bridge_width = 0` ohne
  Stege). Mitgeliefert: JetBrains Mono (Regular/Bold), Inter (Regular/Bold), Allerta Stencil.
  Für OpenSCAD-gleiche Maße eine TTF-Schrift explizit angeben (`font = "Inter"`). Unbekannte
  Schrift → Warnung + cad stencil mono; fehlende Zeichen (z. B. `°` in Allerta Stencil) → Warnung, Zeichen
  aus JetBrains Mono. Umlaute, ß/ẞ und übliche Satzzeichen haben alle mitgelieferten Schriften
  (Ausnahmen Allerta Stencil: ẞ„‚{}_#°²³§~^·•…±µØø).
- `import("….svg")`: Pfade, Rechtecke (rx/ry), Kreise/Ellipsen exakt, Polygone, Gruppen mit
  `transform`, `viewBox`/mm/in/pt, `fill-rule`. Ursprung unten links (y nach oben) wie OpenSCAD.
  Warnungen nennen die SVG-Zeile (`logo.svg:4: \`text\`: Text nicht unterstützt …`): Text in
  Pfade umwandeln, Striche als Flächen gibt es nicht, offene Pfade werden nicht gefüllt.
- Dateinamen müssen als Text im Projekt stehen (auch als Parameter-Standardwert); `cad watch`
  beobachtet die SVG/TTF-Dateien mit.
- Kernel-Zeit: „Hello World“ extrudiert ≈ 0,7 s; viel Text **auf** einer Platte (Vereinigung)
  kostet Sekunden (28 Glyphen ≈ 9 s) – Text als eigenes Teil oder weniger Zeichen, beim
  Ausprobieren `--no-kernel`.

**Profile:** 2D-Formen (`square`, `rect`, `circle`, `polygon`, `spline`) in `linear_extrude`,
`rotate_extrude`, `loft`, `sweep`; Freiformflächen mit `surface(points)` + `thicken(t)`.
