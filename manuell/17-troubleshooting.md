# Troubleshooting and glossary

| Symptom | Cause / fix |
| --- | --- |
| warning "0 Kanten" | zone selects no edges: check zone position with `mcad render --line N`; try `select = "touch"` |
| kernel error on fillet | radius too large for the adjacent faces; the body stays unfilleted |
| "Vereinigung/Differenz/Schnittmenge gescheitert: Zeile 12 (dom) und Zeile 30 (rippe) berühren sich tangential bei [x, y, z] (und n weitere Stellen): the result is not a closed solid …" | the two operands touch along a line or tangentially while overlapping elsewhere: no manifold result. Let them overlap a little or keep a gap at that place (`mcad render` marks it red, JSON `at`) |
| "unbekannte Variable `top`" inside `g[…] + …` | plane names are not variables; use `g.top` |
| "nur Aufrufe eigener Module sind benennbar" | name the module call, not the transform: `translate(…) n: m();` |
| "2D-Form in 3D-Kontext wird ignoriert" | extrude the 2D shape |
| `--show grids` shows nothing | grid lives in a module: add `--module NAME` |
| warning "-D x: keine Zuweisung `x` auf oberster Ebene der Hauptdatei (ignoriert)" | without a module `-D` sets top-level variables only; for a module parameter add `--module NAME` |
| `-m 'm(…)'` exit 2 "zu viele Positionsargumente" / "hat keinen Parameter" / "steht im Aufruf und in -D" | arguments bound like a call: check the parameter list (`mcad outline`); give each parameter once |
| `cad: command not found`, old scripts | the program is `mcad` since 0.3.0; `CAD_*` variables and `~/.config/controller-cad/config.toml` are still read (`mcad usage daemon`) |
| "mcad: x gibt es nicht (Datei .mcad/.scad oder Ordner erwartet)" | `mcad PATH` opens the UI; PATH missing. For a command: `mcad check PATH` |
| `mcad ui …` exit 3 | no desktop UI running; work headless |
| `mcad` exits 2 "keine grafische Sitzung" | no `DISPLAY`/`WAYLAND_DISPLAY`; use the commands |
| UI dies with "Error 71 (Protokollfehler) dispatching to Wayland display" | WebKitGTK DMA-BUF renderer; `mcad` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland unless the variable is set |
| "Engine reagiert seit 20 min nicht (Kernel hängt)", line red | one kernel call ran 20 min: node blocked until its line changes; simplify it (fewer `$fn`, separate text) |
| "Watchdog: `mcad …` läuft seit 30 min" | daemon ended after 30 min; the command reruns without it; raise `watchdog_secs` |
| slow kernel on text | text unioned onto a body; keep it separate or use `--no-kernel` while iterating |
| "hull: Kugeln mit verschiedenen Radien nicht auf einer Achse; Kinder werden unverändert gezeigt" | no exact case (`mcad usage language`): put them on one axis, use equal radii, or build the body from parts |
| "hull: ebene und runde Körper gemischt …" | cube + sphere and the like; use prisms of equal height (cube + cylinder) or `minkowski() { body; sphere(r); }` |
| "hull: gleich große Kugeln mit Mittelpunkten in einer Ebene" | flat arrangement (rounded plate): `minkowski() { cube([a, b, c]); sphere(r); }` with c > 0, or prisms (`cylinder`) |
| "hull: … weder auf einer Achse noch als parallele Zylinder gleicher Höhe" | cylinders of different heights or crossing axes: equal start plane and height, or one axis |
| "hull: verzerrte Kugel" / "verzerrter Zylinder" | non-uniform `scale` on a sphere/cylinder: scale the hull instead, or use `resize` outside |
| "hull: die Kinder haben keine Fläche (Punkte auf einer Linie)" / "kein Volumen" | all input points on a line (2D) or in a plane (3D) |
| "minkowski: exakt nur Körper + Kugel(n), zwei parallele Prismen … oder zwei konvexe ebene Körper" | other combination (three bodies, a cylinder not parallel to the extrusion, non-convex polyhedra) |
| "minkowski: Offset r: offset is not exact: trim curves … off their edges" | the body has corners whose faces do not meet at right angles; SMLib's rounding there is approximate, so nothing is built |
| "minkowski: zwei Profile nur exakt, wenn beide konvex sind" | 2D: use one profile + `circle(r)` (any profile) or two convex ones |
| "x.stl: Zeile N: …", "Binär-STL abgeschnitten", "keine STL-Datei" | broken file (truncated download, wrong format); re-export it |
| "x.stl: 0 Dreiecke in der Datei" / "nur entartete Dreiecke" | empty or collapsed mesh (e.g. wrong unit: `scale`) |
| "x.stl: Orientierung korrigiert: Netz war innen nach außen …" | inverted file; corrected, nothing to do (fix the exporter if you can) |
| "x.stl: Netz nicht geschlossen (n offene Kanten): als Netz ohne B-Rep importiert" | holes in the mesh: repair it (e.g. in the slicer/mesh tool) for Booleans; display, export and measuring work |
| "x.stl: Netz nicht mannigfaltig (n Kanten mit mehr als zwei Dreiecken) …" | touching or self-intersecting shells; repair the mesh |
| "x.stl: as = \"solid\" geht nicht, das Netz ist …" | an open/non-manifold mesh cannot be a B-rep; imported as a mesh |
| "union: ein Netz wird nicht vereinigt …", "difference: ein Netz kann nicht beschnitten/abgezogen werden …", "intersection: mit einem Netz nicht möglich …", "fillet: ein Netz kann nicht bearbeitet werden …" | the STL is a mesh (more than 500 faces or `as = "mesh"`): `import(…, as = "solid")` (slow above ~500 faces: `mcad usage import-stl`) |
| "B-Rep aus dem Netz gescheitert (…): als Netz weiter" | the kernel refused the polyhedron; the mesh is shown instead |
| "hull: Netz mit n Ecken (höchstens 2000 …)" | `hull()` of a large mesh; use a coarser STL |
| "assert_keepout: X ragt ins Sperrvolumen (Schnittvolumen V mm³)" | a body enters the keep-out volume; move it, or the volume (`mcad render` shows the red intersection) |
| "assert_keepout: X hält den Abstand c mm zum Sperrvolumen nicht ein: d mm …" | closer than `clearance`; foot points in the message; a board the part sits on counts too (start the volume above it) |
| "assert_disjoint: a und b überschneiden sich (…)" | the pair shares volume; touching is fine |
| "assert_intersect: a und b schneiden sich nicht (Abstand d mm)" | the children are not one connected piece; touching faces are no intersection – let them overlap or use `assert_touches` |
| "assert_touches: a berührt b nicht, Spalt X mm" | gap above `tol` (+ mesh accuracy); close it or raise `tol` |
| "assert_touches: a und b überschneiden sich (V mm³)" | overlap instead of contact |
| "assert_touches: a berührt b nur mit Linienkontakt (nicht-mannigfaltig, schlecht druckbar …)" | cylinder on a plane, edge on a face: the union is non-manifold; give it a flat, or `contact = "any"` |
| "… Überlappung nicht prüfbar (…)" (Hinweis) | the kernel boolean failed (faces lying on each other); result undecided, not a violation |
| "Kontaktart nur netzbasiert bestimmt" | freeform faces: contact found on the mesh only |
| "`assert_disjoint()` braucht mindestens zwei Kinder (eine for-Schleife zählt als ein Kind)" | put the bodies as separate statements |
| `mcad export` exit 4 "Export abgebrochen: n Assert-Verletzung(en)" | fix the model, or `--ignore-asserts` |
| STEP "Hinweis: Körper a, b überschneiden sich: als ein Körper „a + b“ geschrieben" | top-level objects/parts overlap and are fused into one solid; move them apart to keep them separate, or `--merge` for one body of everything |
| STEP "Überschneidung von a und b nicht prüfbar (…)" / "Vereinigung gescheitert (…): getrennt geschrieben" | the kernel boolean failed (faces lying on each other); the bodies stay separate solids |

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
| Langloch | slot (`slot()`) |
| Körper überschneiden sich, als ein Körper geschrieben, Vereinigung, verschmolzen, nicht prüfbar, getrennt geschrieben | bodies overlap, written as one body, union, fused, cannot be checked, written apart |
| berühren sich tangential, Stelle, weitere Stellen, Berührung, Fehlerstelle | touch tangentially, place, more places, contact, error place |
| Differenz, Schnittmenge gescheitert | difference, intersection failed |
| Abstand, Winkel, Länge | distance, angle, length |
| Kontext: Datei / Modul | context: file / module |
| oberste Ebene, Zuweisung, ignoriert | top level, assignment, ignored |
| Aufruf, Argument, Positionsargument, doppelt angegeben | call, argument, positional argument, given twice |
| abgezogen, übersprungen, offen | subtracted, skipped, pending |
| Farben, Standard | colours, default (uncoloured) |
| Kinder werden unverändert gezeigt | children shown unchanged |
| exakt: Polyeder / Rotationskörper / Prisma / Offset | exact: polyhedron / solid of revolution / prism / offset (special case taken) |
| Hülle der Mittelpunkte | hull of the centres |
| verzerrt, ebene / runde Körper | distorted, planar / round bodies |
| Netz, Dreiecke, verschweißt | mesh, triangles, welded |
| offene Kanten, nicht geschlossen, nicht mannigfaltig, entartet | open edges, not closed, non-manifold, degenerate |
| Orientierung korrigiert, gewendet, innen nach außen | orientation corrected, flipped, inside out |
| B-Rep aus dem Netz, als Netz weiter | B-rep from the mesh, continuing as a mesh |
| netzbasiert, Netze überschneiden sich | mesh-based, meshes overlap |
| Sperrvolumen | keep-out volume (`assert_keepout`) |
| ragt ins Sperrvolumen, hält den Abstand nicht ein | enters the keep-out volume, violates the clearance |
| Schnittvolumen, überschneiden sich, getrennt | intersection volume, intersect (overlap), apart |
| schneiden sich (nicht), keine Verbindung | (do not) intersect, not connected |
| berührt, Spalt | touches, gap |
| Flächenkontakt, Linienkontakt, Punktkontakt, Kontaktart | face contact, line contact, point contact, kind of contact |
| nicht-mannigfaltig, schlecht druckbar | non-manifold, hard to print |
| Überlappung nicht prüfbar | overlap could not be checked |
| Asserts: n von m geprüft, k verletzt; Assert-Verletzung | asserts: n of m checked, k violated; assert violation |
| Export abgebrochen, nichts geschrieben, trotzdem exportiert | export stopped, nothing written, exported anyway |
| beim Ziehen übersprungen, nicht geprüft | skipped while dragging, not checked |
