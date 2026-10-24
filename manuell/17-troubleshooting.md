# Troubleshooting and glossary

| Symptom | Cause / fix |
| --- | --- |
| warning "0 Kanten" | zone selects no edges: check zone position with `cad render --line N`; try `select = "touch"` |
| kernel error on fillet | radius too large for the adjacent faces; the body stays unfilleted |
| "unbekannte Variable `top`" inside `g[…] + …` | plane names are not variables; use `g.top` |
| "nur Aufrufe eigener Module sind benennbar" | name the module call, not the transform: `translate(…) n: m();` |
| "2D-Form in 3D-Kontext wird ignoriert" | extrude the 2D shape |
| `--show grids` shows nothing | grid lives in a module: add `--module NAME` |
| `-D` refused | needs `--module` (or `--line` inside a module) |
| `cad ui …` exit 3 | no desktop UI running; work headless |
| `cad` exits 2 "keine grafische Sitzung" | no `DISPLAY`/`WAYLAND_DISPLAY`; use the commands |
| UI dies with "Error 71 (Protokollfehler) dispatching to Wayland display" | WebKitGTK DMA-BUF renderer; `cad` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland unless the variable is set |
| "Engine reagiert seit 20 min nicht (Kernel hängt)", line red | one kernel call ran 20 min: node blocked until its line changes; simplify it (fewer `$fn`, separate text) |
| "Watchdog: `cad …` läuft seit 30 min" | daemon ended after 30 min; the command reruns without it; raise `watchdog_secs` |
| slow kernel on text | text unioned onto a body; keep it separate or use `--no-kernel` while iterating |

Glossary of the German text output:

| German | English |
| --- | --- |
| Fehler / Warnung / Hinweis | error / warning / hint |
| Syntax / Auswertung / Kernel | syntax / evaluation / kernel (diagnostic source) |
| Z.12, Zeile 12 | line 12 |
| Hülle | bounds |
| Bauteil, ohne Bauteil | part, outside any part |
| Benannte Instanzen | named instances |
| Raster, Rasterebene, Rasterpunkt, Rasterlinie | grid, grid plane, grid point, grid line |
| Anker | anchor |
| Knoten | node |
| Kante(n), ausgewählte Kanten | edge(s), selected edges |
| Fläche, Körper, Volumen | face/area, body, volume |
| Schnitt, Bereich, Loch, Wandstärke | section, region, hole, wall thickness |
| Abstand, Winkel, Länge | distance, angle, length |
| Kontext: Datei / Modul | context: file / module |
| abgezogen, übersprungen, offen | subtracted, skipped, pending |
| Farben, Standard | colours, default (uncoloured) |
