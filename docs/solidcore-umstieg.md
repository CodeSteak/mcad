# Umstieg auf solidcore (Plan 2.3, M8)

Stand: 2026-10-23 · solidcore `main` @ `c9de85d` (2026-10-23 19:10 UTC, tangency-labs/solidcore, Apache-2.0)

## Stand

- **Crate `kernel-solidcore`** implementiert `cad_kernel::Kernel` über die öffentliche API von
  solidcore. Die Abhängigkeit ist ein git-Eintrag mit festem Commit
  (`crates/kernel-solidcore/Cargo.toml`, Konstante `SOLIDCORE_REV`, Kernel-Id
  `solidcore-c9de85d`). Vorheriger Pin: `3a6a50b` (2026-10-22); dazwischen 538 Commits. Während der Arbeit kam
  `c9de85d` hinzu (12 Commits nach `169dbc4`, gleiche Zahlen); gepinnt ist `c9de85d`.
- **solidcore selbst** (`plan/PROGRESS.md` @ `c9de85d`): 238 von 261 Port-Einheiten portiert
  (116 grün = identisch zum C++, 122 teilweise), 89,8 % der Upstream-Zeilen. Das README sagt
  weiter „Nothing is usable end to end yet (no primitives, booleans or fillets)“; „etwa ein
  Drittel portiert“ im README ist veraltet.
- **Toolchain:** solidcore pinnt in seinem eigenen Repository Rust 1.98
  (`rust-toolchain.toml`); diese Datei gilt nicht für abhängige Crates. Mit unserem Stand
  (Rust 1.97) baut solidcore ohne Änderung. Debug-Build von `cad-cli --features solidcore` ab
  leerem Target-Verzeichnis: ca. 11 min mit 4 Kernen (einmalig; Abhängigkeiten mit
  `opt-level = 2`), danach Sekunden.
- **Feature `solidcore`** (opt-in in `kernel-solidcore` und `cad-cli`, Standard aus): ein
  normaler Build (`cargo build`, `cargo test --workspace`) kompiliert solidcore nicht; jede
  Operation von `kernel-solidcore` meldet dann
  `Unsupported("solidcore: … (built without feature `solidcore`)")`, und `cad diff-kernels`
  bricht mit „diff-kernels braucht solidcore: cad mit `--features solidcore` bauen“ ab
  (Exit-Code 2). Für den Vergleich: `cargo build -p cad-cli --features solidcore`, Tests mit
  `cargo test -p cad-cli --features solidcore` bzw. `cargo test -p kernel-solidcore
  --features solidcore -- --nocapture` (zeigt die Antwort jeder Operation). Der git-Eintrag
  steht weiter in `Cargo.lock` (Auflösung einmal mit Netz).

## Öffentliche Einstiege in solidcore (Recherche 2026-10-23)

| Bereich | Einstieg (Rust-Pfad in `solidcore`) | Upstream |
| --- | --- | --- |
| Primitive | `primitive::api_primitives::create_box`, `create_sphere`, `create_cylinder`, `create_cone`, `create_torus`, Teilprimitive, `create_pyramid` | `SmApiCreate*` |
| Kurven | `primitive::api_curves::create_line_segment`, `create_analytic_circle`, `create_circle`, `create_arc`, `create_helix`, … | `SmApiCreate*` |
| Booleans | `boolean::api_brep::boolean`, `boolean_union/_difference/_intersection`, `merge_breps`, … | `SmApiBoolean*` |
| Transformation | `topology::proto_topology::p1::api_transform::{api_transform, api_translate, api_rotate, api_scale, api_scale_by_pt, transform_independent_object}`: starre Platzierung (`Axis2Placement`) nach einer Skalierung entlang der Achsen; die 4×4-Matrix-Variante von `SmBrep::Transform` ist nicht portiert | `SmApiTransform` u. a. |
| Sweeps | `sweep::api_brep::create_linear_sweep`, `create_rotational_sweep`, `create_pipe_sweep`, `create_draft_sweep`, `create_taper_extrude`, `create_curve_sweep*`, `create_sweep_along_planar_path`; Skins: `sweep::api_surfaces::create_skin`, `create_ruled_surface`, `create_extrude`, `create_surface_revolution` | `SmApiCreate*Sweep`, `SmApiCreateSkin` |
| Offset/Shell | `offset::offset_executive::p2::api_offset_brep_full`, `api_shell_brep_full` | `SmApiOffsetBrepFull`, `SmApiShellBrepFull` |
| Klassifizierung | `query::brep::BrepClassifier::point_3d_classify` (bzw. `BrepQuery`), `query::api_queries::face_classify_uv`, `edge_classify_parameter`, `brep_relationship` | `SmBrep::Point3DClassify`, `SmApiFaceClassifyUV` |
| Abfragen | `query::api_queries::get_faces`, `get_edges`, `face_get_boundary_loops`, `face_internal_point`, `brep_bounding_box`, `edge_bounding_box`, `vertex_get_point`, `brep_is_manifold_solid`, Abstände/nächste Punkte | `SmApi*` |
| Masseneigenschaften | `mass::integrator::properties::brep_mass_properties`, `brep_area`, `brep_volume` (exakte Integration über die Flächen, ohne Tessellierung) | `SmApiBrepCompute*` |
| Fillet/Fase | **keine öffentliche API**: die Fillet-Module (`blend::*`) sind portiert, die Einstiege `SmApiFilletEdges`, `SmApiChamferFillet`, `SmApiVariableRadiusFillet` (Einheit `blend.fillet_executive_p1`) nicht | `SmApiFillet*` |
| Tessellierung, Polyeder | **ausdrücklich nicht im Umfang** von solidcore (`plan/closure.md`, „Out of scope“: `SmApiTessellate`, `SmApiTessellateBoundaries`, `SmApiPoly*`; `SmTess`/`NL_Tessellate` werden von keiner Operation im Umfang gebraucht) | – |

Weitere Upstream-Einstiege, die solidcore nicht portiert (Auswahl aus `plan/closure.md`):
`SmApiCreateCurveInterpPoints` (interpolierende Spline-Kurve, bei uns für `Loop::Spline`),
`SmApiCreatePlanarFaces`, `SmApiCreateSurface*`, `SmApiCut`, `SmApiIntersect*`.

## Was heute geht (Lauf mit `c9de85d`)

| Kernel-Operation | Weg in `kernel-solidcore` | Ergebnis @ `c9de85d` |
| --- | --- | --- |
| `make_box` | `create_box` | **geht**, Volumen, Flächen- und Kantenzahl wie SMLib |
| `make_cylinder` (Zylinder, Kegel, Kegel mit Spitze unten) | `create_cylinder`, `create_cone` (Spitze unten: gedreht wie in der SMLib-Fassade) | `Unsupported`: `TrimSrfCache::build_tree_with_face` (Einheit `surface.trim_srf_cache`) |
| `make_sphere`, `make_torus` | `create_sphere`, `create_torus` | `Unsupported`: `SmFace handle of a surface's face` (`topology.face_p2`) |
| `make_polyhedron` | – | `Unsupported`: Poly-B-Reps nicht im Umfang |
| `transform` | `transform_independent_object` mit `Axis2Placement` + Skalierung; Scherung → `Unsupported` | **Antwortet mit Erfolg, aber falsch**: nur die Ecken werden bewegt, Kurven und Flächen nicht (`SmBrep::Transform`, `topology.brep_p5`, findet unter den Schlüsseln seiner Bearbeitung keine Geometrie). `kernel-solidcore` prüft Volumen und Schwerpunkt des Ergebnisses und meldet dann `Unsupported` |
| `extrude` (ohne Verdrehung/Skalierung) | Polygon: Liniensegmente, `create_linear_sweep`, dann `transform` mit dem Profilrahmen | **geht** für Polygone mit Einheitsrahmen; mit anderem Rahmen scheitert die Transformation (s. o.); Kreisprofil: `Unsupported` (`SmEdgeuse::GetOrCreateUVTrimCurve`, `topology.edgeuse_p3`); Spline-Profil: `SmApiCreateCurveInterpPoints` nicht im Umfang |
| `extrude` mit Verdrehung/Skalierung, `loft` | bräuchte `create_skin` | `Unsupported` (nicht verdrahtet) |
| `revolve` | `create_rotational_sweep` | `Unsupported` (`surface.trim_srf_cache`) |
| `boolean` | `boolean::api_brep::boolean` | `Unsupported`: `SmEdge::GetTolerance` (`topology.edge`) bzw. `SmFace::Point3DClassify` (`query.face_p1`) |
| `faces` | `get_faces`, Randschleifen-Ecken; innerer Punkt (`face_internal_point`), sobald portiert | **geht** (innerer Punkt: `SmEdge::GetCurve`, `topology.edge`, noch nicht) |
| `edges` | `get_edges`, Anfangs-/Endecke, Kurvenpunkte | **geht** |
| `bounds`, `is_solid`, `is_empty`, `mass_props` | `query::api_queries`, `mass::integrator::properties` | **gehen** (Quader und Extrusion: Volumen, Fläche, Schwerpunkt exakt) |
| `classify` | `BrepClassifier::point_3d_classify` | **Antwortet, aber falsch**: Punkte im Quader und auf seinen Flächen landen in der unendlichen Region. `kernel-solidcore` prüft das einmal an einem Einheitswürfel und meldet bis dahin `Unsupported` |
| `fillet`, `chamfer` | – | `Unsupported`: keine öffentliche API |
| `sweep`, `thicken`, `shell`, `nurbs_surface` | – | `Unsupported` (nicht verdrahtet; `api_shell_brep_full` endet @ `c9de85d` an `SmBrep::SmBrep`, `topology.brep_p3`) |
| `tessellate`, `brep` | – | `Unsupported` (Tessellierung nicht im Umfang, s. u.) |

Fehlerabbildung: `SolidError::Unported` → `KernelError::Unsupported("solidcore: <op>: …")`
mit Upstream-Funktion und blockierender Einheit, `SolidError::InvalidInput` → `InvalidInput`,
sonst `Failed`. Flächen tragen vorerst den Tag der erzeugenden Operation (Attribut-Weitergabe
für Provenienz ist noch nicht verdrahtet). Die Prüfungen bei `transform` und `classify`
schalten sich von selbst frei, sobald solidcore richtig rechnet.

## Vergleich über die Beispiele (`cad diff-kernels`)

`examples/openscad-compat` und `examples/spec`:

| Stand | Dateien | Knoten | gleich | unsupported | blockiert | abweichend | gescheitert |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `3a6a50b` (Testbericht 5) | 46 | 855 | 101 | 267 | 487 | 0 | 0 |
| `169dbc4`, `kernel-solidcore` unverändert | 48 | 942 | 102 | 287 | 431 | 122 | 0 |
| `169dbc4` und `c9de85d`, `kernel-solidcore` angepasst | 48 | 942 | **241** | 288 | 413 | **0** | 0 |

- Der Quader geht jetzt, wurde mit dem alten `kernel-solidcore` aber **nicht richtig
  verglichen**: `faces` scheiterte (innerer Punkt nicht portiert), `diff-kernels` verwarf den
  Fehler still und zählte den Knoten wegen fehlender Flächen-/Kantenzahl als „abweichend“
  (106 Quader, dazu Folgeknoten). Jetzt liefert `faces` die Randecken als Abtastpunkte, und
  `diff-kernels` meldet eine nicht unterstützte Abfrage (Flächen/Kanten/Volumen) des
  solidcore-Ergebnisses als `unsupported` mit Meldung statt als Abweichung.
- Gleich sind jetzt 106 von 112 `box`, 17 von 32 `linear_extrude` und die 2D-Knoten. Die
  größten Blocker nach Knoten (unsupported): `make_cylinder` (`surface.trim_srf_cache`, 115),
  `transform` (`topology.brep_p5`, 103), Booleans (`topology.edge`, `query.face_p1`, 13),
  Kreisprofile (`topology.edgeuse_p3`, 11), Kugel/Torus (`topology.face_p2`, 10).

## Was fehlt (in solidcore bzw. hier)

1. Primitive außer dem Quader (`surface.trim_srf_cache`, `topology.face_p2`).
2. Eine Transformation, die Kurven und Flächen mitnimmt (`topology.brep_p5`), und die
   4×4-Matrix-Variante (Scherung, z. B. `multmatrix`).
3. Booleans (`topology.edge`, `query.face_p1`), Punktklassifikation (`query.brep`).
4. Öffentliche Fillet-/Fasen-API (`blend.fillet_executive_p1`).
5. Hier: `create_skin` (Loft, Extrusion mit Verdrehung), `create_pipe_sweep` (Sweep),
   Offset/Shell verdrahten; Attribute für Face-Tags (Provenienz); `Kernel::brep` für den
   STEP-Export; Wasm-Build im selben Modul (Stack 8 MiB, siehe solidcore-README).
6. **Eigener Tessellierer** (nicht in solidcore, s. u.).

Schätzung aus der Recherche (unsicher, hängt an den Einheiten oben und an den Leuten, die in
solidcore Einheiten übernehmen): alle Primitive end to end in 1–2 Wochen, Booleans in 2–4
Wochen, Fillets, Sweeps und Shell in 6–10 Wochen.

## Tessellierung: eigener Tessellierer für M8

Tessellierung und Polyeder sind ausdrücklich nicht im Umfang von solidcore
(`plan/closure.md`). Für M8 (Anzeige, STL/3MF, Schnittansicht, `compare.py`) braucht es also
einen eigenen Tessellierer in Rust über dem solidcore-B-Rep, oder SMLib (`NL_Tessellate`,
C++) bzw. OCCT (`BRepMesh`, über den STEP-Weg) bleibt allein dafür im Build; das widerspricht
dem Ziel von M8, das SMLib-Wasm zu entfernen. Skizze:

1. **Kanten zuerst, einmal je Kante:** jede Kante wird genau einmal diskretisiert und von
   beiden Nachbarflächen benutzt (wasserdicht, Kantenabgleich). Sehnenfehler-Steuerung:
   Gerade 1 Segment; Kreis/Ellipse Schrittwinkel `θ = 2·acos(1 − tol/r)`; NURBS adaptiv
   halbieren, bis die Abweichung des Mittelpunkts ≤ `tol` und der Tangentenwinkel unter einer
   Grenze liegt.
2. **Ebene Flächen:** Schleifen in die Ebene projizieren und als Polygon mit Löchern
   triangulieren (Ear-Clipping mit Lochbrücken wie earcut, oder eingeschränkte
   Delaunay-Triangulierung, CDT); Orientierung aus Flächennormale und Schleifenrichtung.
3. **Analytische und NURBS-Flächen:** im Parameterraum arbeiten. Die Kantenpunkte über die
   UV-Trimmkurven der Kantenverwendungen (bzw. Punktinversion auf die Fläche) nach (u, v)
   bringen; Nähte periodischer Flächen (Zylinder, Kugel, Torus) aufschneiden und Punkte
   doppeln, Pole (Kugel, Kegelspitze) als entartete Kanten behandeln. Ein UV-Gitter, dessen
   Schrittweite aus dem Sehnenfehler folgt (Zylinder/Kegel: aus dem Radius; Kugel/Torus: aus
   beiden Radien; NURBS: aus zweiten Ableitungen bzw. der Ebenheit des Kontrollnetzes,
   adaptiv als Quadtree), die Gitterpunkte gegen die UV-Schleifen trimmen
   (Punkt-in-Polygon), dann CDT in UV mit den Kantenpolylinien als Zwangskanten, sodass der
   Rand exakt aus den Kantenpunkten aus Schritt 1 besteht. Danach nachverfeinern, wo die
   3D-Abweichung in Dreiecks- bzw. Kantenmitten `tol` übersteigt (Steiner-Punkte), und
   Dreiecke mit schlechtem Seitenverhältnis in UV vermeiden (anisotrope Skalierung von u
   und v nach den Ableitungslängen).
4. **Normalen** exakt aus der Flächenauswertung je Ecke, Orientierung nach dem Sinn der
   Fläche; Face-Id je Dreieck für die Provenienz (`Mesh`), Kanten-Polylinien für die
   Anzeige; grob/fein nach Toleranz wie in Plan 7.2.

Abhängigkeiten: aus solidcore Flächen- und Kurvenauswertung (Punkt, Ableitungen, Normale:
`SmApiEvaluateSurface*`, Kurven-`evaluate_point`), Randschleifen und Kantenverwendungen
(`face_get_boundary_loops`), UV-Trimmkurven (`SmEdgeuse::GetOrCreateUVTrimCurve`, Einheit
`topology.edgeuse_p3`, heute teilweise), Punktinversion (`surface_closest_point`),
UV-Klassifikation (`face_classify_uv`), Flächentyp und Parameterbereich/Periodizität. Auf
unserer Seite: eine CDT (eigene oder z. B. das Crate `spade`, MIT/Apache) mit robusten
Prädikaten (`robust`), Ear-Clipping für den ebenen Fall. `make_polyhedron` bräuchte einen
eigenen Aufbau aus ebenen Flächen und Nähen (`SmApiStitchIntoSolid` ist im Umfang, aber
ungeprüft).

## Abnahme

`cad diff-kernels <dateien…> [--json]` (`crates/cad-cli/src/diff.rs`) wertet jede Datei mit
SMLib und mit solidcore aus und vergleicht jeden von SMLib ausgewerteten Knoten:

- Status, Volumen (relativ 1e-6), Anzahl Flächen und Kanten des Knotenergebnisses,
  ausgewählte Kanten (Fillet/Fase: Anzahl und Endpunkte, 1e-6 der Modellgröße);
- Urteil je Knoten: `same`, `unsupported` (solidcore meldet die Operation oder eine Abfrage
  ihres Ergebnisses als nicht unterstützt), `blocked` (weicht nur ab, weil ein Eingangsknoten
  unsupported ist; bei `zone` der Geschwisterkörper), `differ`, `failed` (solidcore scheitert,
  SMLib nicht), `skipped`;
- „Abdeckung“: je Operation (Knotenlabel) gleich / Knoten, unsupported, blockiert, abweichend.

Exit-Code 0 nur, wenn alle Knoten aller Dateien `same` sind. Umgestellt wird erst, wenn
`diff-kernels` über alle Beispiele (`examples/openscad-compat`, `examples/spec`) mit 0 endet;
zusätzlich müssen `tests/openscad-compat/compare.py` und `tests/step/verify.py` mit solidcore
weiter bestehen (dafür braucht `kernel-solidcore` noch `tessellate` und `brep`).

Der Test `diff_kernels_on_examples` (`crates/cad-cli/tests/cli.rs`, nur mit `--features
solidcore`) prüft, dass der Vergleich über alle Beispiele ohne Absturz durchläuft, dass mehr
als 200 Knoten gleich sind und keiner abweicht; ohne das Feature prüft
`diff_kernels_needs_the_feature` den Hinweis zum Bauen. `crates/kernel-solidcore/tests/coverage.rs`
prüft jede Operation einzeln (Quader: Volumen, Fläche, Schwerpunkt, Grenzen; Transformation
und Klassifikation richtig oder `Unsupported`).

Wöchentlich (montags, `.github/workflows/solidcore.yml`, auch von Hand startbar) läuft
`scripts/ci/solidcore.sh`: `cad` mit `--features solidcore` gegen den jeweils neuesten
solidcore-`main` statt des Pins gebaut, `diff-kernels` über dieselben Beispiele; die Zahlen
stehen in der Job-Zusammenfassung (gesamt und je Operation) und als Artefakt
`solidcore-coverage` (`diff-kernels.json`). Der Job wird nicht rot, weil Knoten unsupported,
blockiert oder abweichend sind, nur wenn der Build (API-Änderung in solidcore) oder
`diff-kernels` selbst scheitert. Lokal: `SMLIB_PREBUILT=smlib/build-native
scripts/ci/solidcore.sh` (ändert den Pin im Arbeitsbaum, danach `git checkout --
crates/kernel-solidcore Cargo.lock`).
