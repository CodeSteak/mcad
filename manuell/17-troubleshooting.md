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
| "hull: Kugeln mit verschiedenen Radien nicht auf einer Achse; Kinder werden unverändert gezeigt" | no exact case (`cad usage language`): put them on one axis, use equal radii, or build the body from parts |
| "hull: ebene und runde Körper gemischt …" | cube + sphere and the like; use prisms of equal height (cube + cylinder) or `minkowski() { body; sphere(r); }` |
| "hull: gleich große Kugeln mit Mittelpunkten in einer Ebene" | flat arrangement (rounded plate): `minkowski() { cube([a, b, c]); sphere(r); }` with c > 0, or prisms (`cylinder`) |
| "hull: … weder auf einer Achse noch als parallele Zylinder gleicher Höhe" | cylinders of different heights or crossing axes: equal start plane and height, or one axis |
| "hull: verzerrte Kugel" / "verzerrter Zylinder" | non-uniform `scale` on a sphere/cylinder: scale the hull instead, or use `resize` outside |
| "hull: die Kinder haben keine Fläche (Punkte auf einer Linie)" / "kein Volumen" | all input points on a line (2D) or in a plane (3D) |
| "minkowski: exakt nur Körper + Kugel(n), zwei parallele Prismen … oder zwei konvexe ebene Körper" | other combination (three bodies, a cylinder not parallel to the extrusion, non-convex polyhedra) |
| "minkowski: Offset r: offset is not exact: trim curves … off their edges" | the body has corners whose faces do not meet at right angles; SMLib's rounding there is approximate, so nothing is built |
| "minkowski: zwei Profile nur exakt, wenn beide konvex sind" | 2D: use one profile + `circle(r)` (any profile) or two convex ones |

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
| Kinder werden unverändert gezeigt | children shown unchanged |
| exakt: Polyeder / Rotationskörper / Prisma / Offset | exact: polyhedron / solid of revolution / prism / offset (special case taken) |
| Hülle der Mittelpunkte | hull of the centres |
| verzerrt, ebene / runde Körper | distorted, planar / round bodies |
