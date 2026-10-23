# Alltagsmodelle und typische CAD-Teile

19 parametrische Modelle (15 Einzeldateien, 4 Baugruppen-Ordner mit `include`/`use`), entwickelt
und geprüft nur mit der CLI (`cad outline`, `check`, `render --views all --show grids,anchors`,
`inspect`, `measure --interference/--clearance`, `export` STL/STEP). Jede Datei beginnt mit
`// Zweck: …`; alle Maße in mm; jedes Modell ist ein Modul mit Standardwerten, auf oberster
Ebene als benannte Instanz (Bauteil, Export je Teil mit `cad export … --all-parts`).

Regression: `tests/alltag/check.sh` (alle Modelle: `check --json`, Export STEP je Bauteil, STL
und STEP gesamt in ein temporäres Verzeichnis, Vergleich mit `tests/alltag/expected.json`;
`--update` schreibt die Datei neu, `--fehler` führt zusätzlich die Fehlerbeispiele aus).

## Modelle

Volumen: exakte B-Rep (Kernel), Summe der Bauteile. Kernel: `scene.stats.kernel_ms` von
`cad eval` ohne Cache; Auswertung: `timings.evaluate_ms` von `cad render` (Auswertung samt
Kantenabfragen und Tessellierung), beides einmalig gemessen auf der Entwicklungsmaschine
(Linux, ein Kern je Aufruf, ohne Daemon – siehe F14).

| Datei | Zweck | Sprachmittel | Bauteile | Volumen mm³ | Kernel / Auswertung | Vorschau |
| --- | --- | --- | --- | ---: | ---: | --- |
| [`handystaender.scad`](handystaender.scad) | Handyständer, Neigung 65°, Kabelloch mit Kanal | 2D-Raster für das Profil + 3D-Raster für Zonen, `polygon` aus Rasterpunkten, `linear_extrude`, Funktion mit `let` (Geradenschnitt), verschachtelte `fillet`-Zonen an Rasterlinien (r 3 / 1,5) | `staender` | 109 978 | 0,2 / 0,5 s | [img](img/handystaender.png) |
| [`becher.scad`](becher.scad) | Becher mit Henkel, ~370 ml | `rotate_extrude`, Zone an Rasterebene, **`shell`** (oben offen), `fillet` um `shell`, **`sweep`** (Spline-Pfad), `union(fillet = …)`, Funktion | `b` | 77 709 | 1,6 / 4,5 s | [img](img/becher.png) |
| [`kabelclip.scad`](kabelclip.scad) | Kabelhalter für die Tischkante (19 mm), 4 Kanäle | indizierte Rasterebenen aus Liste, `linear_extrude`, Zonen an Rasterlinien, `difference(fillet = …)` (40 Kanten), `cylinder(a, b, r)`, `for` | `clip` | 24 457 | 0,6 / 1,7 s | [img](img/kabelclip.png) |
| [`wandhaken.scad`](wandhaken.scad) | Wandhaken mit Kugelknauf, 2 Senkbohrungen | `fillet`-Zonen (Linien) in `chamfer`-Zone (Ebene) – Fase erfasst die Randkanten der Verrundung, **`sweep`**, `union(fillet)`, Kegel `cylinder(h, r1, r2)`, Anker mit `dir` | `haken` | 18 818 | 1,0 / 2,2 s | [img](img/wandhaken.png) |
| [`seifenschale.scad`](seifenschale.scad) | Seifenschale mit Ablaufschlitzen, Rippen, Füßen | verschachtelte Zonen (senkrechte Linien r 14, Bodenebene r 5), **`shell`**, `cylinder(a, b, r)` zwischen Rasterpunkten, `for` mit zwei Variablen, `difference(fillet)` | `schale` | 36 961 | 1,6 / 2,1 s | [img](img/seifenschale.png) |
| [`stiftebox.scad`](stiftebox.scad) | Stiftebox, 3 hohe Fächer + flaches Fach | rekursive Funktion, Ebenenlisten aus Ausdrücken, 4 verschachtelte Zonenblöcke, `zone` in `for`, `chamfer`-Zone | `box_` | 115 179 | 1,3 / 1,5 s | [img](img/stiftebox.png) |
| [`tuerstopper.scad`](tuerstopper.scad) | Türstopper-Keil mit Rillen, Griffmulde, Aufhängeloch | **`loft`** durch 3 Profile (Funktion), `rotate([90, 0, 90])`, `chamfer`-Zone an der Bodenebene, `for`, Kugel als Werkzeug | `stopper` | 64 237 | 0,2 / 0,6 s | [img](img/tuerstopper.png) |
| [`topfuntersetzer.scad`](topfuntersetzer.scad) | Topfuntersetzer Ø 180, Ringe + Speichen | `rotate_extrude` in `for`, Liste als Rasterebenen, `box` zwischen Punkten unter `rotate`, `chamfer`-Zone (103 Kanten) | `u` | 83 250 | 5,4 / 7,2 s | [img](img/topfuntersetzer.png) |
| [`flansch.scad`](flansch.scad) | Flanschplatte Ø 120, Bund, Lochkreis 6 × M10, Dichtnut | Querschnitt aus 2D-Raster, `rotate_extrude`, **eigenes Modul als Zone** (`ringzone`), `fillet`- und `chamfer`-Zonen, `difference(chamfer)`, Lochkreis per `for`/`rotate` | `f` | 150 300 | 1,2 / 2,4 s | [img](img/flansch.png) |
| [`lagerbock.scad`](lagerbock.scad) | Stehlagerbock für Kugellager 608, Langlöcher M6 | Zonen an Rasterlinien in x, y und z (Kehlen rund um das Auge), `chamfer`-Zone, `difference(chamfer)`, Hilfsmodul Langloch, `cylinder(a, b, r)` tangential an `box` | `bock` | 34 404 | 0,8 / 1,2 s | [img](img/lagerbock.png) |
| [`winkel_rippe.scad`](winkel_rippe.scad) | Montagewinkel 60 × 50 mit Rippe, M5 | indizierte Ebenen (Rippen), Zonen in `for`, Kehlen mit Eckübergang in einem Block, `linear_extrude`, `difference(chamfer)` | `w` | 22 246 | 0,6 / 1,1 s | [img](img/winkel_rippe.png) |
| [`kuehlkoerper.scad`](kuehlkoerper.scad) | Kühlkörper 60 × 50, 9 Rippen | indizierte Ebenen, Rippen per `for`, Zonen an Rasterlinien in `for` (Rippenfüße), Ebenen-Zone (Köpfe), `floor` | `kk` | 34 652 | 0,9 / 1,5 s | [img](img/kuehlkoerper.png) |
| [`zahnrad.scad`](zahnrad.scad) | Stirnradpaar m 2, z 20/12, Passfedernut, D-Bohrung | Evolvente aus Listenfunktionen (`let`, `each`, `concat`), `polygon`, `if`, `intersection`, `difference(chamfer)`, zwei Bauteile im Eingriff | `rad1`, `rad2` | 16 628 | 5,1 / 5,5 s | [img](img/zahnrad.png) |
| [`rohrschelle.scad`](rohrschelle.scad) | zweiteilige Rohrschelle Ø 32, M5/M8 | `rotate_extrude(angle = 180)`, `chamfer`-Zonen, `mirror`, Bauteile in einem gedrehten Block, Anker mit `?:` | `oben`, `unten`, `rohr` | 25 870 | 2,1 / 2,8 s | [img](img/rohrschelle.png) |
| [`pi_platte.scad`](pi_platte.scad) | Montageplatte Raspberry Pi (58 × 49, M2.5) + Platinen-Dummy | benannte Instanzen `dom[i]` in einer Schleife im Modul, Anker aus Instanz-Anker (`dom[0].oben`), `union(fillet)`, Zonen, `color`, `$fillet`, Platine am Anker | `platte`, `pi` | 46 048 | 1,6 / 2,1 s | [img](img/pi_platte.png) |
| [`gehaeuse/`](gehaeuse/gehaeuse.scad) | Elektronikgehäuse mit Deckel, 4 Schrauben M3 | **Baugruppe**: `include <masse.scad>` (Maße + Funktion `dom_pos`), `use <teile.scad>`, **`shell`** eines verrundeten Quaders, `union(fillet)`, Dome `dom[i]`, Deckel am Anker, Schrauben `s[i]`, `explosion` | `unterteil`, `deckel`, `s[0]`…`s[3]` | 42 068 | 4,0 / 3,5 s¹ | [img](img/gehaeuse.png) (explodiert) |
| [`scharnier/`](scharnier/scharnier.scad) | Scharnier 60 mm, 5 Augen, Spiel 0,4 | **Baugruppe**: `include`/`use`, Augen aus einer Funktion, `for` mit Schrittweite, Freiräume, Blatt B auf Öffnungswinkel gedreht | `a`, `b`, `stift` | 10 151 | 2,6 / 3,7 s | [img](img/scharnier.png) |
| [`pflanztopf/`](pflanztopf/pflanztopf.scad) | Pflanztopf mit Untersetzer | **Baugruppe**: Superellipse als `spline` (Funktion mit `pow`, `sign`), **`loft`** Quadrat → Kreis, Differenz zweier Lofts, `rotate_extrude` + **`shell`**, Volumenzonen, Anker auf Anker | `untersetzer`, `topf` | 107 178 | 1,1 / 1,8 s | [img](img/pflanztopf.png) |
| [`schluesselbrett/`](schluesselbrett/schluesselbrett.scad) | Schlüsselbrett mit 5 eingeklebten Haken | **Baugruppe**: `include`/`use`, Ebenen aus Funktion, Zonen + Fase, Schlüssellochaufhängung, **`sweep`**, `union(fillet)`, Haken `h[i]` in `for` | `brett`, `h[0]`…`h[4]` | 158 077 | 4,0 / 15,9 s² | [img](img/schluesselbrett.png) |

¹ Auswertung des explodierten Vorschaustands (`explosion = 22`). ² Siehe „Laufzeit“ unten.

Abdeckung der Vorgaben: Baugruppen mit mehreren Bauteilen: 4 Ordner + `zahnrad`, `rohrschelle`,
`pi_platte`; Verrundung/Fase über Zonen: 18 Modelle (alle außer `zahnrad`); `shell`: `becher`, `seifenschale`,
`gehaeuse`, `pflanztopf` (Untersetzer); `sweep`/`loft`: `becher`, `wandhaken`,
`schluesselbrett` / `tuerstopper`, `pflanztopf`.

## Passungen und Kollisionen (`cad measure`)

Alle Baugruppen: `--interference` ohne Kollision (Toleranz 0,001 mm³).

| Modell | Paar | Abstand | Bedeutung |
| --- | --- | ---: | --- |
| `zahnrad` | `rad1` ↔ `rad2` | 0,116 mm | Flankenspiel (Zahndicke um 0,1 mm verringert) |
| `rohrschelle` | `oben` ↔ `unten` / `oben` ↔ `rohr` | 2,0 / 0,035 mm | Klemmspalt an den Laschen / Rohr liegt an |
| `pi_platte` | `platte` ↔ `pi` | 0 mm | Platine liegt auf den Domen (Berührung, keine Kollision); Lochbild 58 × 49 per `--between dom[0].oben dom[3].oben` |
| `gehaeuse` | `unterteil` ↔ `deckel`, `s[0]` ↔ `deckel` / `s[0]` ↔ `unterteil` | 0 / 0,05 mm | Deckel liegt auf, Schraubenkopf liegt auf / Schaft (Kern 2,4) im Kernloch 2,5 |
| `scharnier` | `a` ↔ `b` / `stift` ↔ `b` / `stift` ↔ `a` | 0,397 / 0,223 / 0 mm | Spiel der Augen / Lagerspiel / Stiftkopf liegt an; bei `winkel` < ca. 40° kollidieren die Blätter |
| `pflanztopf` | `topf` ↔ `untersetzer` | 0 mm | Füße stehen auf dem Boden, Wand 4 mm vom Rand |
| `schluesselbrett` | `brett` ↔ `h[0]` | 0,081 mm | Zapfen 6 in Bohrung 6,3 (Kleberspalt), Engstelle an der Fase |

## Gefundene Fehler und Schwächen

Minimalbeispiele liegen in [`tests/alltag/fehler/`](../../tests/alltag/fehler/) (Kopfkommentar:
Erwartung, Befund, Umgehung; `tests/alltag/check.sh --fehler` meldet „besteht“/„behoben“).
CLI-Stand: `target/release/cad` vom 23.10.2026 14:31 (Merge `384d31b`).

### Falsches Ergebnis ohne Meldung (am schwersten)

- **F5 – Fase/Verrundung liefert offene Hülle, Status ok.** Kasten (Wand 3) mit senkrechten
  Ecken r 6, danach `chamfer(0.8)` über die Zone der Oberkante: 0 Fehler, Volumen 0 mm³, STEP mit
  26 Flächen. Ebenso in der Stiftebox (Ecken r 6, Wand 2,4) auch mit `fillet(0.8)`. Erwartet:
  Kernel-Fehler, wenn der Körper nicht geschlossen ist. [`f05_fase_volumen_null.scad`](../../tests/alltag/fehler/f05_fase_volumen_null.scad)
- **F8 – Booleans mit Torus.** `union() { cylinder(h = 2, r = 30); translate([0, 0, 2]) torus(20, 1); }`
  ergibt zwei sich überlappende Körper (6049,65 statt 5852,26 mm³); dasselbe mit
  `rotate_extrude() translate([20, 0]) circle(1)` verliert den Ring ganz (5654,87 mm³);
  `difference() { cylinder(h = 2, r = 30, center = true); torus(20, 1); }` (Torus berührt Deck- und
  Bodenfläche) hat Volumen 0. Alles ohne Meldung. `f08a…`, `f08b…`, `f08c…`
- **F10 – `union(fillet)` verliert einen Operanden.** Verrundete Schale (`shell`) plus Dom nahe
  der Innenecke: das Ergebnis ist nur der Dom samt Kehle, Volumen 0, Status ok (die
  `difference(chamfer)` darüber meldet dann „0 Kanten“). Mit dem Dom weiter innen korrekt.
  [`f10_union_fillet_schale_verloren.scad`](../../tests/alltag/fehler/f10_union_fillet_schale_verloren.scad)

### Absturz

- **F9 – Segmentation fault (Exit 139) der CLI** bei `union(fillet = 1)`, wenn sich zwei Körper
  tangential berühren: (a) Platte + Wand + Zylinder, der die Wand berührt
  (`union(fillet = 1) { cube([20, 20, 2]); cube([2, 20, 10]); translate([5.5, 10, 0]) cylinder(h = 10, r = 3.5); }`),
  (b) Zapfen d 6 + gleich dicker `sweep` ab seiner Stirnfläche. Ohne `fillet` kein Absturz. Kein
  Diagnosetext, keine Ausgabe – auch `measure`/`export` brechen ab. `f09a…`, `f09b…`

### Kernel-Fehler mit Meldung

- **F1 – `sweep` mit `spline`-Profil** auf gekrümmtem Pfad abgelehnt („profile that is not
  centred on its origin“), obwohl das Profil symmetrisch ist (Prüfung über die NURBS-Hülle mit
  Toleranz 1e-9 in `smlib/facade/sc_facade.cpp`). Damit gibt es keine glatten elliptischen
  Sweep-Profile; Umgehung `circle` bzw. Polygon. `f01…`
- **F2 – Zylinder ∪ gekrümmter `sweep`**, dessen Endkappen die Zylinderfläche schneiden:
  `SM_ERR_NULL_POINTER`. In `union() { … }` erscheint dieselbe Diagnose doppelt. `f02…`
- **F3 – `sweep` eines 16-Ecks** auf gekrümmtem Pfad: „did not produce a closed solid“ (32-Eck
  und `circle` gehen). `f03…`
- **F6 – Fase/Verrundung über eine Ebenen-Zone scheitert**, wenn konvexe Kanten an einer kurzen
  konkaven Kante enden (Stufe vor einer Wand, kurze Außenkanten zwischen Kühlrippen, vorderes
  Fach der Stiftebox). `render --line` markiert die Kante rot (hilfreich); Umgehung: Zonen an
  Rasterlinien statt an der Ebene. `f06…`
- **F11 – deckungsgleiche Zylinder** (Stift 3,0 in Bohrung 3,0 des Scharniers): Wurzel-Union
  `SM_ERR_NULL_POINTER`, `--interference` meldet 0,24 mm³. Nicht weiter reduziert (ein Auge mit
  Stift allein geht). `f11…`
- **F12 – `shell` eines Lofts Quadrat → Kreis** scheitert (`SmMerge.cpp:672`); Kreis → Kreis geht.
  Umgehung: Differenz zweier Lofts (Pflanztopf). `f12…`
- **F15 – `union(fillet)` sweep (d 6) + Kugel d 9** scheitert für jeden Radius, mit Kugel d 10
  nicht. `f15…`
- F4 (klein): `union(fillet)` an einem Sweep mit 32-Eck-Profil scheitert (viele Facettenkanten);
  mit `circle` geht es.
- F13 (Meldung): `shell(2)` eines Körpers mit Verrundung r 2 (Innenradius 0) meldet nur
  „SmApiShellBrepFull failed: SM_ERR (1001)“; ein Hinweis „Verrundungsradius ≤ Wandstärke“
  würde die Ursache nennen.
- Ohne Fehler im Programm, aber lehrreich: Kollisionen im eigenen Entwurf (Fuß 0,3 mm neben
  einem Schlitz, Langloch in der Rippe, Schlüsselloch in der Briefrille) zeigen sich nur als
  „SM_ERR (1001)“ der Verrundung/Fase – ein Hinweis auf die beteiligten Flächen fehlt.

### CLI, Darstellung, Sprache

- **F14 – Daemon still nicht verfügbar:** mit `CAD_IPC_DIR=/tmp/claude-0/…/scratchpad/alltag-ipc`
  (98 Zeichen, mit `/daemon.sock` 110 – über der Grenze von 107 für Unix-Sockets) gilt „path must
  be shorter than SUN_LEN“; jeder Befehl rechnet ohne
  Cache. Sichtbar nur in `--json` (`daemon.reason`) oder mit `--verbose`, die normale Ausgabe
  schweigt. Vorschlag: Hinweis auf stderr, oder Socket unter kurzem Pfad (Hash) anlegen.
- **Laufzeit:** `kernel_ms` deckt oft nur einen Teil der Auswertung: Schlüsselbrett 4,0 s Kernel,
  aber 15,9 s `evaluate_ms` und 16 s für `cad check` (ohne Bild); Becher 1,6 / 4,5 s. Der Rest ist
  in den Zählern nicht ausgewiesen (Kantenabfragen der Zonen/neuen Kanten, Tessellierung?).
- **S1 – Profilraster in der falschen Ebene:** ein 2D-Raster für ein `rotate_extrude`-Profil
  (x = Radius, y = Höhe) wird in der xy-Ebene des Moduls gezeichnet, das Profil aber in xz
  (Becher, Flansch, Handyständer: `p.rand` liegt im Bild 95 mm hinter dem Teil). Ein Raster vor
  `rotate` zu platzieren hilft nicht, weil `g[…]` dann Modulkoordinaten liefert, die nicht mehr in
  der Profilebene liegen – es fehlt ein Zugriff auf rasterlokale Koordinaten.
- **S2 – Ebenennamen sind keine Werte:** `g[i % 2 == 0 ? loch_l : loch_r, …]` ergibt „unbekannte
  Variable `loch_l`“; Umgehung über Zahlen `[i % 2 == 0 ? g.loch_l : g.loch_r, …]`. Anker lassen
  sich nicht als Liste deklarieren (`anchor loch[i] = …`), daher Positionen über Funktionen.
- **S3 – Sichtbarkeit:** `cad outline` führt Instanzen aus einem Modul (`dom[0]` … in
  `pi_platte`, `gehaeuse`) unter „Benannte Instanzen“ der Datei und ihre Anker als Datei-Anker;
  `measure --between dom[0].oben …` funktioniert im Dateikontext, obwohl der Name laut Spec nur im
  Modul gilt.
- **S4 – Darstellung:** Schnittkappen berührender Bauteile verschmelzen (keine Trennlinie,
  Gehäuse-Schnitt); bei vielen Rasterebenen überdecken sich die Beschriftungen von
  `--show grids` stark (Kühlkörper, Seifenschale).
- Fallstrick (OpenSCAD-kompatibel, aber teuer): `for (i = [0 : n - 1])` mit `n = 0` läuft über
  `[-1, 0]` (nur Warnung „Anfang > Ende … vertauscht“) – im Zahnrad entstanden so zwei riesige
  Erleichterungslöcher.

### Sprachmittel, die gefehlt haben

`offset`/`hull` und 2D-Booleans (abgerundete Rechtecke, Langlöcher, Bögen im Profil mussten
über 3D-Verrundung, Superellipsen-Splines oder Hilfsmodule gebaut werden); ein Bogen bzw.
abgerundete Ecke in `polygon`; ungleichmäßig skalierter Kreis (Ellipse) als Profil;
`text` (Beschriftung des Schlüsselbretts); Gewinde; Anker-Listen und Ebenennamen als Werte (S2);
rasterlokale Koordinaten für Profile in gedrehten Ebenen (S1); eine Zone „Kante zwischen Fläche
A und B“ (konkave Kehlen ohne die Nachbarkanten, vgl. F6); Parameter (`-D`) für Variablen der
Datei, nicht nur für Modulparameter (Öffnungswinkel, `explosion`).
