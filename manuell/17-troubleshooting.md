# Troubleshooting and glossary

| Symptom | Cause / fix |
| --- | --- |
| warning "0 edges" | zone selects no edges: check zone position with `mcad render --line N`; try `select = "touch"` |
| kernel error on fillet | radius too large for the adjacent faces; the body stays unfilleted |
| "union/difference/intersection failed: line 12 (dom) and line 30 (rippe) touch tangentially at [x, y, z] (and n more places): the result is not a closed solid …" | the two operands touch along a line or tangentially while overlapping elsewhere: no manifold result. Let them overlap a little or keep a gap at that place (`mcad render` marks it red, JSON `at`) |
| "linear_extrude failed: extrude did not produce a closed solid (self-intersecting or open profile?)" | the 2D profile crosses itself (e.g. a bowtie polygon) or is not closed: fix the points |
| "rotate_extrude failed: invalid input: revolve: the profile crosses the axis (x < 0 after its frame)" | part of the profile lies at x < 0: move it to x ≥ 0 |
| "unknown variable `top`" inside `g[…] + …` | plane names are not variables; use `g.top` |
| "only calls of your own modules can be named" | name the module call, not the transform: `translate(…) n: m();` |
| "2D shape in 3D context is ignored" | extrude the 2D shape |
| `--show grids` shows nothing | grid lives in a module: add `--module NAME` |
| warning "-D x: no assignment `x` at the top level of the main file (ignored)" | without a module `-D` sets top-level variables only; for a module parameter add `--module NAME` |
| `-m 'm(…)'` exit 2 "too many positional arguments" / "has no parameter" / "is in the call and in -D" | arguments bound like a call: check the parameter list (`mcad outline`); give each parameter once |
| `cad: command not found`, old scripts | the program is `mcad` since 0.3.0; `CAD_*` variables and `~/.config/controller-cad/config.toml` are still read (`mcad usage daemon`) |
| "mcad: x does not exist (expected a .mcad/.scad file or a folder)" | `mcad PATH` opens the UI; PATH missing. For a command: `mcad check PATH` |
| `mcad ui …` exit 3 | no desktop UI running; work headless |
| exit 2 "--kernel solidcore needs solidcore: build mcad with `--features solidcore` …" (or `MCAD_KERNEL=solidcore …`) | this `mcad` was built without solidcore: `cargo build -p cad-cli --features solidcore`, or use SMLib (default; unset `MCAD_KERNEL`) |
| exit 2 "MCAD_KERNEL: unknown kernel `x` (smlib or solidcore)" | wrong value in the environment variable |
| "not supported by this kernel: solidcore: …" (kernel error, `render`/`measure`/`export` exit 3) | the operation is not wired to solidcore yet (shell, polyhedra, offset, tessellation for `render`, `measure`, STL/3MF and the assert checks, …) or solidcore cannot do it (shear in `multmatrix`; variable-radius fillet with different laws on several edges: "fillet_variable: different radius laws on several edges"): use the default kernel SMLib |
| `diff-kernels --brep`: "line N … [B-rep]: surfaces {\"plane\": 6} vs {\"bspline\": 6}" after a `mirror` | solidcore turns every surface into a B-spline under a mirror (negative scale); geometry and volume are the same |
| `diff-kernels --brep`: "N faces with other tags" after a `union` of coplanar faces | the kernels keep a different operand's tag on a merged face |
| "--ao only applies with --style studio" (also `--light`, `--shadows`, `--ground`, `--edges` …) | studio settings need `--style studio` |
| "--ground-color X: expected #rrggbb or a palette colour name" (also `--background`) | quote `#` in the shell (`'#e0e0e0'`) or drop it (`e0e0e0`); names: `mcad usage colors` |
| studio render: no ground / no ground shadow | the camera looks from below or horizontally (`front`, `right`, `bottom`): the ground is only drawn from above |
| studio render: no shadow of a part | transparent bodies (x-ray, dimmed rest with `--line`, alpha) cast none; only the first light casts shadows |
| "--material red=silver: unknown material silver (known: plastic, …)" | presets: `mcad usage render`; or give values: `red=metalness=1,roughness=0.3` |
| "--quality fast: expected draft, normal, high" / "--ground-reflection 2: allowed 0…1" | `mcad usage render` (studio options) |
| "--material x=chrome: x: expected a colour, default or part:NAME" | colour name (`mcad usage colors`), `#rrggbb`, `default` (uncoloured material) or `part:NAME` |
| warning "render: unknown setting `x` (known: …)" (also in `light`/`material`) | typo in the render block; names: `mcad usage render-block`; the value is ignored |
| warning "render: ambient = 3 is outside 0…1 – ignored" / "expects true or false" / "expects a colour" | value of the wrong type or range; the default applies |
| warning "render: material \"x\": unknown colour" / "material lid: no part `lid` (parts: …)" | colour as in `color()` (palette name, `#rrggbb`, `default`); part names: `mcad outline` |
| warning "render { … } applies only at the top level of the main file" / "only the first render block applies" | move the block to the top level of the main file; keep one |
| "--render x: expected key=value, light.NAME.key=value or material.SEL.key=value" / "--render: render: unknown setting `x`" | `mcad set --render` path or name wrong: `mcad usage render-block` |
| `mcad render` draws the studio look without `--style` | the file has a render block; `--style standard` for the CAD look |
| studio render: metal looks dark or flat | metals show their surroundings: keep `--environment studio`, raise `--ambient`; flat faces in an orthographic view reflect one direction (uniform colour): use `--perspective` or curved faces; `chrome` is dark by nature (dark walls), `aluminium`/`stainless` are brighter |
| studio render: grainy glossy reflections | `--quality high` (more rays); rough surfaces (≥ 0.6) use no rays |
| studio render: slow | `--quality draft` (no rays for surfaces), `--reflections off`, `--ground-reflection 0`, `--ao off` |
| `mcad` exits 2 "no graphical session" | no `DISPLAY`/`WAYLAND_DISPLAY`; use the commands |
| UI dies with "Error 71 (Protokollfehler) dispatching to Wayland display" | WebKitGTK DMA-BUF renderer; `mcad` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland unless the variable is set |
| "Engine not responding for 20 min (kernel hangs)", line red | one kernel call ran 20 min: node blocked until its line changes; simplify it (fewer `$fn`, separate text) |
| "watchdog: `mcad …` has been running for 30 min" | daemon ended after 30 min; the command reruns without it; raise `watchdog_secs` |
| slow kernel on text | text unioned onto a body; keep it separate or use `--no-kernel` while iterating |
| "hull: spheres of different radii not on one axis; children shown unchanged" | no exact case (`mcad usage language`): put them on one axis, use equal radii, or build the body from parts |
| "hull: planar and round bodies mixed …" | cube + sphere and the like; use prisms of equal height (cube + cylinder) or `minkowski() { body; sphere(r); }` |
| "hull: equal spheres with centres in one plane" | flat arrangement (rounded plate): `minkowski() { cube([a, b, c]); sphere(r); }` with c > 0, or prisms (`cylinder`) |
| "hull: … neither on one axis nor parallel cylinders of equal height" | cylinders of different heights or crossing axes: equal start plane and height, or one axis |
| "hull: distorted sphere" / "distorted cylinder" | non-uniform `scale` on a sphere/cylinder: scale the hull instead, or use `resize` outside |
| "hull: the children have no area (points on a line)" / "no volume" | all input points on a line (2D) or in a plane (3D) |
| "minkowski: exact only body + sphere(s), two parallel prisms … or two convex planar bodies" | other combination (three bodies, a cylinder not parallel to the extrusion, non-convex polyhedra) |
| "minkowski: offset r: offset is not exact: trim curves … off their edges" | the body has corners whose faces do not meet at right angles; SMLib's rounding there is approximate, so nothing is built |
| "minkowski: two profiles only exact if both are convex" | 2D: use one profile + `circle(r)` (any profile) or two convex ones |
| "x.stl: line N: …", "binary STL truncated", "not an STL file" | broken file (truncated download, wrong format); re-export it |
| "x.stl: 0 triangles in the file" / "only degenerate triangles" | empty or collapsed mesh (e.g. wrong unit: `scale`) |
| "x.stl: orientation corrected: mesh was inside out …" | inverted file; corrected, nothing to do (fix the exporter if you can) |
| "x.stl: mesh not closed (n open edges): imported as a mesh without B-rep" | holes in the mesh: repair it (e.g. in the slicer/mesh tool) for Booleans; display, export and measuring work |
| "x.stl: mesh non-manifold (n edges with more than two triangles) …" | touching or self-intersecting shells; repair the mesh |
| "x.stl: as = \"solid\" not possible, the mesh is …" | an open/non-manifold mesh cannot be a B-rep; imported as a mesh |
| "union: a mesh is not united …", "difference: a mesh cannot be cut/subtracted …", "intersection: not possible with a mesh …", "fillet: a mesh cannot be modified …" | the STL is a mesh (more than 500 faces or `as = "mesh"`): `import(…, as = "solid")` (slow above ~500 faces: `mcad usage import-stl`) |
| "B-rep from the mesh failed (…): continuing as a mesh" | the kernel refused the polyhedron; the mesh is shown instead |
| "hull: mesh with n vertices (at most 2000 …)" | `hull()` of a large mesh; use a coarser STL |
| "assert_keepout: X enters the keep-out volume (intersection volume V mm³)" | a body enters the keep-out volume; move it, or the volume (`mcad render` shows the red intersection) |
| "assert_keepout: X violates the clearance c mm to the keep-out volume: d mm …" | closer than `clearance`; foot points in the message; a board the part sits on counts too (start the volume above it) |
| "assert_disjoint: a and b overlap (…)" | the pair shares volume; touching is fine |
| "assert_intersect: a and b do not intersect (distance d mm)" | the children are not one connected piece; touching faces are no intersection – let them overlap or use `assert_touches` |
| "assert_touches: a does not touch b, gap X mm" | gap above `tol` (+ mesh accuracy); close it or raise `tol` |
| "assert_touches: a and b overlap (V mm³)" | overlap instead of contact |
| "assert_touches: a touches b only with line contact (non-manifold, hard to print …)" | cylinder on a plane, edge on a face: the union is non-manifold; give it a flat, or `contact = "any"` |
| "… overlap could not be checked (…)" (info) | the kernel boolean failed (faces lying on each other); result undecided, not a violation |
| "kind of contact determined on the mesh only" | freeform faces: contact found on the mesh only |
| "`assert_disjoint()` needs at least two children (a for loop counts as one child)" | put the bodies as separate statements |
| `mcad export` exit 4 "export stopped: n assert violation(s)" | fix the model, or `--ignore-asserts` |
| STEP "note: bodies a, b overlap: written as one body “a + b”" | top-level objects/parts overlap and are fused into one solid; move them apart to keep them separate, or `--merge` for one body of everything |
| STEP "overlap of a and b could not be checked (…)" / "union failed (…): written apart" | the kernel boolean failed (faces lying on each other); the bodies stay separate solids |

Messages above are the English texts (default). German output (`--lang de`, `MCAD_LANG=de`,
German locale) – glossary:

| German | English |
| --- | --- |
| Fehler / Warnung / Hinweis | error / warning / note (hint) |
| Syntax / Auswertung / Kernel | syntax / evaluation / kernel (diagnostic source) |
| Z.12, Zeile 12 | line 12 (render labels `l.12`) |
| Hülle | bounds |
| Bauteil, ohne Bauteil | part, outside any part (JSON part name and 3MF object name `no part` in every language) |
| Benannte Instanzen | named instances |
| Raster, Rasterebene, Rasterpunkt, Rasterlinie | grid, grid plane, grid point, grid line |
| Anker | anchor |
| Knoten, Knoten gesperrt | node, node blocked |
| Kante(n), ausgewählte Kanten, Kantenmitte | edge(s), selected edges, edge midpoint |
| Fläche, Körper, Volumen | face/area, body, volume |
| Schnitt, Bereich, Loch, Wandstärke | section (render label: intersection), region, hole, wall thickness |
| Langloch | slot (`slot()`) |
| Bogen, Kreismitte, Gerade, Kontur | arc, circle centre, line, contour |
| Körper überschneiden sich, als ein Körper geschrieben, Vereinigung, verschmolzen, nicht prüfbar, getrennt geschrieben | bodies overlap, written as one body, union, fused, could not be checked, written apart |
| berühren sich tangential, Stelle, weitere Stellen, Berührung, Fehlerstelle | touch tangentially, place, more places, contact, error place |
| Differenz, Schnittmenge gescheitert | difference, intersection failed |
| Abstand, Winkel, Länge | distance, angle, length |
| Kontext: Datei / Modul | context: file / module |
| oberste Ebene, Zuweisung, ignoriert | top level, assignment, ignored |
| Aufruf, Argument, Positionsargument, doppelt angegeben | call, argument, positional argument, given twice |
| abgezogen, übersprungen, offen | subtracted, skipped, pending |
| Wert, Literal, ziehbar, Griff, Referenz, gesperrt | value, literal, draggable, handle, reference, locked |
| Farben, Standard | colours, default (uncoloured; JSON/3MF label `default` in every language) |
| Röntgen | x-ray |
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
| Schnittvolumen, überschneiden sich, getrennt | intersection volume, overlap, apart |
| schneiden sich (nicht), keine Verbindung | (do not) intersect, not connected |
| berührt (nicht), Spalt | touches (does not touch), gap |
| Flächenkontakt, Linienkontakt, Punktkontakt, Kontaktart | face contact, line contact, point contact, kind of contact |
| nicht-mannigfaltig, schlecht druckbar | non-manifold, hard to print |
| Überlappung nicht prüfbar | overlap could not be checked |
| Asserts: n von m geprüft, k verletzt; Assert-Verletzung; VERLETZT | Asserts: n of m checked, k violated; assert violation; VIOLATED |
| Export abgebrochen, nichts geschrieben, trotzdem exportiert | export stopped, nothing written, exported despite … |
| beim Ziehen übersprungen, nicht geprüft | skipped while dragging, not checked |
