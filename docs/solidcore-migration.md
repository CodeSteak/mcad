# Migration to solidcore (plan 2.3, M8)

Work plan up to the switch: [`solidcore-plan.md`](solidcore-plan.md). Handover (state, how to build
and test, open points, next steps): [`solidcore-handover.md`](solidcore-handover.md).

As of: 2026-11-04 (phase 0 of the plan done; phase 1 done except a variable fillet with several radius laws; phase 2, the own tessellator `cad-tess`, done; see the plan) · solidcore `main` @ `f4f05b1` (2026-11-04 05:18 UTC, crate version 1.0.1, tangency-labs/solidcore, Apache-2.0); the operation table "What works today" comes from the run with `c9de85d`, the figures for `d4940b6`, `bba8d87` and `f4f05b1` are in the comparison over the examples and in "State at `bba8d87`".

## Status

- **Crate `kernel-solidcore`** implements `cad_kernel::Kernel` via the public API of
  solidcore. The dependency is a git entry with a fixed commit
  (`crates/kernel-solidcore/Cargo.toml`, constant `SOLIDCORE_REV`, kernel ID
  `solidcore-f4f05b1`). Previous pins: `3a6a50b` (2026-10-22), `c9de85d` (2026-10-23),
  `d4940b6` (2026-10-25), `bba8d87` (2026-10-30). From `bba8d87` to `f4f05b1` (2969 commits,
  crate version 1.0.1) no API break for this crate: it builds unchanged (named public paths
  such as `topology::brep::faces` were added next to the numbered `p1`, `p2` modules, which stay
  public), and the corpus gives the same verdicts node by node; `FilletSession` still takes one
  radius law for all edges (`prepare_variable_radius_edges`). From `d4940b6` to `bba8d87` (3101
  commits) `face_internal_point` takes `&mut Brep`. From
  `c9de85d` to `d4940b6` (2026-10-25, a little over 1200 commits) the API of the
  mass properties changed (`brep_mass_properties`, `brep_area` take `&mut Brep`); the weekly
  run against `main` would have failed at the build.
- **solidcore itself** (`plan/PROGRESS.md` @ `c9de85d`): 238 of 261 port units ported
  (116 green = identical to the C++, 122 partial), 89.8 % of the upstream lines. The README still
  says "Nothing is usable end to end yet (no primitives, booleans or fillets)"; "about one
  third ported" in the README is outdated.
- **Toolchain:** solidcore pins Rust 1.98 in its own repository
  (`rust-toolchain.toml`); this file does not apply to dependent crates. With our setup
  (Rust 1.97) solidcore builds without changes. Debug build of `cad-cli --features solidcore` from
  an empty target directory: about 11 min with 4 cores (once; dependencies with
  `opt-level = 2`), seconds afterwards.
- **Kernel selection at run time:** `mcad <command> --kernel solidcore` or `MCAD_KERNEL=solidcore`
  evaluates with solidcore (`crates/cad-cli/src/kernel.rs`: `AnyKernel` forwards to SMLib or
  solidcore; JSON field `kernel_id`). Without the feature: exit 2 with the build hint. The daemon
  keeps one SMLib engine; a command with solidcore runs locally (`daemon.reason`), so the two
  kernels never share a cache. The desktop app (`mcad engine-host`, `cad-desktop`) stays on SMLib.
- **Feature `solidcore`** (opt-in in `kernel-solidcore` and `cad-cli`, off by default): a
  normal build (`cargo build`, `cargo test --workspace`) does not compile solidcore; every
  operation of `kernel-solidcore` then reports
  `Unsupported("solidcore: … (built without feature `solidcore`)")`, and `mcad diff-kernels`
  aborts with "diff-kernels braucht solidcore: mcad mit `--features solidcore` bauen" (diff-kernels needs solidcore: build mcad with `--features solidcore`)
  (exit code 2). For the comparison: `cargo build -p cad-cli --features solidcore`, tests with
  `cargo test -p cad-cli --features solidcore` or `cargo test -p kernel-solidcore
  --features solidcore -- --nocapture` (shows the response of every operation). The git entry
  remains in `Cargo.lock` (resolved once with network access).

## Public entry points in solidcore (research 2026-10-23)

| Area | Entry point (Rust path in `solidcore`) | Upstream |
| --- | --- | --- |
| Primitives | `primitive::api_primitives::create_box`, `create_sphere`, `create_cylinder`, `create_cone`, `create_torus`, partial primitives, `create_pyramid` | `SmApiCreate*` |
| Curves | `primitive::api_curves::create_line_segment`, `create_analytic_circle`, `create_circle`, `create_arc`, `create_helix`, … | `SmApiCreate*` |
| Booleans | `boolean::api_brep::boolean`, `boolean_union/_difference/_intersection`, `merge_breps`, … | `SmApiBoolean*` |
| Transformation | `topology::proto_topology::p1::api_transform::{api_transform, api_translate, api_rotate, api_scale, api_scale_by_pt, transform_independent_object}`: rigid placement (`Axis2Placement`) after scaling along the axes; the 4×4 matrix variant of `SmBrep::Transform` is not ported | `SmApiTransform` and others |
| Sweeps | `sweep::api_brep::create_linear_sweep`, `create_rotational_sweep`, `create_pipe_sweep`, `create_draft_sweep`, `create_taper_extrude`, `create_curve_sweep*`, `create_sweep_along_planar_path`; skins: `sweep::api_surfaces::create_skin`, `create_ruled_surface`, `create_extrude`, `create_surface_revolution` | `SmApiCreate*Sweep`, `SmApiCreateSkin` |
| Offset/shell | `offset::offset_executive::p2::api_offset_brep_full`, `api_shell_brep_full` | `SmApiOffsetBrepFull`, `SmApiShellBrepFull` |
| Classification | `query::brep::BrepClassifier::point_3d_classify` (or `BrepQuery`), `query::api_queries::face_classify_uv`, `edge_classify_parameter`, `brep_relationship` | `SmBrep::Point3DClassify`, `SmApiFaceClassifyUV` |
| Queries | `query::api_queries::get_faces`, `get_edges`, `face_get_boundary_loops`, `face_internal_point`, `brep_bounding_box`, `edge_bounding_box`, `vertex_get_point`, `brep_is_manifold_solid`, distances/closest points | `SmApi*` |
| Mass properties | `mass::integrator::properties::brep_mass_properties`, `brep_area`, `brep_volume` (exact integration over the faces, without tessellation) | `SmApiBrepCompute*` |
| Fillet/chamfer | **no public API**: the fillet modules (`blend::*`) are ported, the entry points `SmApiFilletEdges`, `SmApiChamferFillet`, `SmApiVariableRadiusFillet` (unit `blend.fillet_executive_p1`) are not | `SmApiFillet*` |
| Tessellation, polyhedra | **explicitly out of scope** of solidcore (`plan/closure.md`, "Out of scope": `SmApiTessellate`, `SmApiTessellateBoundaries`, `SmApiPoly*`; `SmTess`/`NL_Tessellate` are not needed by any in-scope operation) | – |

Further upstream entry points that solidcore does not port (selection from `plan/closure.md`):
`SmApiCreateCurveInterpPoints` (interpolating spline curve, used by the SMLib facade for
`Loop::Spline`; the `SmBSplineCurve::CreateInterpolatingCurve` it calls is ported and gives the
same curve, see "Profiles and sweeps"),
`SmApiCreatePlanarFaces`, `SmApiCreateSurface*`, `SmApiCut`, `SmApiIntersect*`.

## What works today (run with `c9de85d`)

| Kernel operation | Route in `kernel-solidcore` | Result @ `c9de85d` |
| --- | --- | --- |
| `make_box` | `create_box` | **works**, volume, face and edge count as in SMLib |
| `make_cylinder` (cylinder, cone, cone with apex at the bottom) | `create_cylinder`, `create_cone` (apex at the bottom: rotated as in the SMLib facade) | `Unsupported`: `TrimSrfCache::build_tree_with_face` (unit `surface.trim_srf_cache`) |
| `make_sphere`, `make_torus` | `create_sphere`, `create_torus` | `Unsupported`: `SmFace handle of a surface's face` (`topology.face_p2`) |
| `make_polyhedron` | – | `Unsupported`: poly B-Reps out of scope |
| `transform` | `transform_independent_object` with `Axis2Placement` + scaling; shear → `Unsupported` | **Reports success, but wrong**: only the vertices are moved, curves and faces are not (`SmBrep::Transform`, `topology.brep_p5`, finds no geometry under the keys of its edit). `kernel-solidcore` checks volume and centroid of the result and then reports `Unsupported` |
| `extrude` (without twist/scale) | polygon: line segments, `create_linear_sweep`, then `transform` with the profile frame | **works** for polygons with unit frame; with another frame the transformation fails (see above); circular profile: `Unsupported` (`SmEdgeuse::GetOrCreateUVTrimCurve`, `topology.edgeuse_p3`); spline profile: `SmApiCreateCurveInterpPoints` out of scope |
| `extrude` with twist/scale, `loft` | would need `create_skin` | `Unsupported` (not wired up) |
| `revolve` | `create_rotational_sweep` | `Unsupported` (`surface.trim_srf_cache`) |
| `boolean` | `boolean::api_brep::boolean` | `Unsupported`: `SmEdge::GetTolerance` (`topology.edge`) or `SmFace::Point3DClassify` (`query.face_p1`) |
| `faces` | `get_faces`, boundary loop vertices; interior point (`face_internal_point`) once ported | **works** (interior point: `SmEdge::GetCurve`, `topology.edge`, not yet) |
| `edges` | `get_edges`, start/end vertex, curve points | **works** |
| `bounds`, `is_solid`, `is_empty`, `mass_props` | `query::api_queries`, `mass::integrator::properties` | **work** (box and extrusion: volume, area, centroid exact) |
| `classify` | `BrepClassifier::point_3d_classify` | **Responds, but wrong**: points inside the box and on its faces end up in the infinite region. `kernel-solidcore` checks this once on a unit cube and reports `Unsupported` until then |
| `fillet`, `chamfer` | – | `Unsupported`: no public API |
| `sweep`, `thicken`, `shell`, `nurbs_surface` | – | `Unsupported` (not wired up; `api_shell_brep_full` @ `c9de85d` ends at `SmBrep::SmBrep`, `topology.brep_p3`) |
| `tessellate`, `brep` | – | `Unsupported` (tessellation out of scope, see below) |

Error mapping: `SolidError::Unported` → `KernelError::Unsupported("solidcore: <op>: …")`
with upstream function and blocking unit, `SolidError::InvalidInput` → `InvalidInput`,
otherwise `Failed`. (At `c9de85d` faces carried the tag of the generating operation only; face
tags are attributes since plan 1.10/1.9.) The checks for `transform` and `classify`
unlock themselves as soon as solidcore computes correctly.

## Comparison over the examples (`mcad diff-kernels`)

`tests/models/openscad-compat` and `tests/models/spec`; from 0.2 of the plan also
`tests/models/everyday` (15 files and the main file of each of the 4 assembly folders),
`tests/models/project-housing/housing.mcad` and `tests/everyday/bugs` (17), 101 files:

| State | Files | Nodes | same | unsupported | blocked | differ | failed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `3a6a50b` (test report 5) | 46 | 855 | 101 | 267 | 487 | 0 | 0 |
| `169dbc4`, `kernel-solidcore` unchanged | 48 | 942 | 102 | 287 | 431 | 122 | 0 |
| `169dbc4` and `c9de85d`, `kernel-solidcore` adapted | 48 | 942 | **241** | 288 | 413 | **0** | 0 |
| `3688a8b` (2026-10-24) | 61 | 1339 | 482 | 332 | 336 | 189 | 0 |
| `d4940b6` (2026-10-25) | 64 | 1436 | **516** | 354 | 360 | 206 | 0 |
| `bba8d87`, `kernel-solidcore` only builds | 64 | 1438 | 369 | 352 | 362 | 355 | 0 |
| `bba8d87` (2026-10-30, pin), `kernel-solidcore` adapted | 64 | 1438 | **1187** | 84 | 116 | 51 | 0 |
| `bba8d87`, `include`/`use`/`import` resolved, unsupported warnings count (64 files as before) | 64 | 1462 | 1190 | 95 | 136 | 41 | 0 |
| `bba8d87`, corpus extended (plan 0.2) | 101 | 2604 | 1927 | 180 | 295 | 202 | 0 |
| `bba8d87`, volume against the exact value (plan 0.4), 64 files | 64 | 1462 | 1217 | 95 | 136 | 14 | 0 |
| `bba8d87`, volume against the exact value (plan 0.4) | 101 | 2604 | 2110 | 180 | 287 | 27 | 0 |
| `bba8d87`, end of phase 0 (queries in place, plan 0.6) | 101 | 2604 | **2110** | 180 | 287 | 27 | 0 |
| `bba8d87`, sketch profiles (plan 1.1) | 101 | 2604 | 2231 | 129 | 216 | 28 | 0 |
| `bba8d87`, spline profiles (plan 1.2) | 101 | 2604 | 2233 | 128 | 211 | 32 | 0 |
| `bba8d87`, sweeps (plan 1.4) | 101 | 2604 | 2262 | 108 | 184 | 50 | 0 |
| `bba8d87`, twist/scale and loft (plan 1.5) | 101 | 2604 | **2280** | 102 | 172 | 50 | 0 |
| `bba8d87`, cheap `transform` check | 101 | 2604 | **2299** | 92 | 160 | 53 | 0 |
| `bba8d87`, face tags and edge information (plan 1.10) | 101 | 2604 | 2299 | 128 | 171 | 6 | 0 |
| `bba8d87`, fillets and chamfers (plan 1.3) | 101 | 2604 | **2523** | 26 | 47 | 8 | 0 |
| `bba8d87`, tags resolved after Booleans (plan 1.9), tester findings of task B | 101 | 2604 | **2523** | 26 | 47 | 8 | 0 |
| `bba8d87`, neutral B-rep (plan 1.11), tester findings of task C | 101 | 2604 | **2523** | 26 | 47 | 8 | 0 |
| `bba8d87`, polyhedra (plan 1.7) | 101 | 2604 | **2545** | 17 | 34 | 8 | 0 |
| `bba8d87`, offset, shell, thicken, NURBS surface (plan 1.6) | 101 | 2604 | **2581** | 5 | 6 | 12 | 0 |
| `bba8d87`, shear (plan 1.8), set_draft/shape_bytes/export (1.12), rule `blend` with offsets: end of phase 1 | 101 | 2604 | **2589** | 3 | 3 | 9 | 0 |
| `bba8d87`, re-test of task C: checked Booleans, rule `blend` from the changed volume | 101 | 2604 | **2596** | 3 | 3 | 2 | 0 |
| `f4f05b1` (2026-11-04, pin), `kernel-solidcore` unchanged | 101 | 2604 | **2596** | 3 | 3 | 2 | 0 |

- `f4f05b1` (pin bump from `bba8d87`, `scripts/ci/solidcore.sh`): every node has the verdict it
  had; the 2 `differ` are the same (`sketch_cable_duct`: 63 against 47 faces, 180 against 132
  edges; `f11`: SMLib fails), the 3 `unsupported` and 3 `blocked` are `variable_radius`; volume
  rules relative 2293, exact 129, operands 67, curved 25, blend 70; meshes at 0.05: 100 `ok`,
  1 without a B-rep result, solidcore watertight 100, chord max 0.0500 mm, 175 k triangles (as
  at `bba8d87`). Still failing where SMLib succeeds (item 7 of "What is missing"): the
  polyhedron of two cubes sharing an edge, `shell(1) cube(10, $fillet = 2)`.
- Corpus (plan 0.2): up to then `diff-kernels` evaluated each file without a resolver, so
  `include`/`use` and `import("x.stl")` found nothing (an assembly gave 3 nodes instead of 60);
  it now resolves them relative to the file like the other commands. A node whose warning
  names an unsupported operation counts as `unsupported` (STL import: solidcore has no
  polyhedron, the mesh stays a mesh and the following `difference` failed; `hull`/`minkowski`
  that fall back to their children because an operation is unsupported). New per corpus
  (same / nodes, before 0.4): `everyday` 621 / 947, `project-housing` 20 / 33,
  `everyday/bugs` 96 / 162.
- Volume rule (plan 0.4, see "Acceptance"): the 27 volume-only differences of the 64 files are
  `same` now (16 by the exact value, 4 by the operands, 7 curved within 1e-4, the tolerance then; 3e-5 since phase 1); over the 101 files
  183 nodes (119 exact, 53 operands, 11 curved). Left `differ` (27): 24 with other faces/edges
  (Booleans that lose faces: `hole_plate`, `vent_grille`, `chamfers`, `grip_shell`,
  `variable_radius`, `gear`, `project-housing`, `hull`/`minkowski` inputs; phase 3), and 3
  explained ones: `angle_anchor` (the implicit union of touching bodies at the top is not a
  manifold solid for solidcore, its volume query fails) and `f08c`/`f09a` in `tests/everyday/bugs`
  (SMLib fails there – the documented SMLib bugs – solidcore does not).

- @ `d4940b6`: cylinders, cones, spheres, tori and `revolve` deliver results with the face and
  edge counts of SMLib, but the volumes deviate by about 1e-5 (sphere r 5: 523.6048 instead of
  523.5988 mm³; the integration accuracy, 1e-6 or 1e-8, changes nothing) – these are most
  of the "differ" (cylinder 126, sphere 20). The biggest blocker remains `transform`
  (`topology.brep_p5`: only the vertices are moved; 258 nodes unsupported, most of the
  blocked ones depend on it), plus composite sketch profiles (not wired up here, 42),
  fillet/chamfer (no public API) and polyhedra (out of scope). Booleans work partially
  (`difference` 17 of 64 same).

- The box works now, but with the old `kernel-solidcore` it was **not compared
  correctly**: `faces` failed (interior point not ported), `diff-kernels` silently discarded the
  error and counted the node as "differ" because of the missing face/edge count
  (106 boxes, plus dependent nodes). Now `faces` delivers the boundary vertices as sample points, and
  `diff-kernels` reports an unsupported query (faces/edges/volume) of the
  solidcore result as `unsupported` with a message instead of as a difference.
- Same are now 106 of 112 `box`, 17 of 32 `linear_extrude` and the 2D nodes. The
  biggest blockers by node (unsupported): `make_cylinder` (`surface.trim_srf_cache`, 115),
  `transform` (`topology.brep_p5`, 103), Booleans (`topology.edge`, `query.face_p1`, 13),
  circular profiles (`topology.edgeuse_p3`, 11), sphere/torus (`topology.face_p2`, 10).

## State at `bba8d87` (2026-10-30)

solidcore: all 279 units `green`. New since `d4940b6`: public fillet/chamfer API
(`blend::fillet_executive::api::{fillet_edges, fillet_edges_per_edge, variable_radius_fillet,
chamfer_fillet}`), crates `step-import`, `step-export`, `step-brep` (STEP in both directions on the
solidcore B-rep) and `nurbs-grid-mesh` (grid mesher for trimmed NURBS faces). Still out of scope:
`SmApiTessellate`, `SmApiPoly*`, `SmApiCreateCurveInterpPoints`.

Changes in `kernel-solidcore`:

| Problem | Fix |
| --- | --- |
| `Brep::clone` gives curves and surfaces without owner: `transform` of the clone moves only the vertices, `face_internal_point` fails (status 1001) | copies with `SmApiBrepCopy` (`api_brep_copy`, caller keys per copy, `Inner::copy`) |
| face ids of a copy differ from the original (`face N not found`) | `faces` queried the copy only; since plan 0.6 every query (`faces`, `edges`, `classify`, `mass_props`, `bounds`) works on the shape's own body (`RefCell<Brep>` in `ShapeData`), so ids agree between all queries |
| `BrepClassifier` with `BrepIdentity::default()` puts every point outside | `BrepQuery::of_brep(..).point_3d_classify` (on the shape's own body since plan 0.6); the self-test passes |

Remaining nodes:

| Kind | Nodes | Cause |
| --- | ---: | --- |
| differ: volume only (≤ 5e-6 relative, same faces/edges) | 27 | curved solids, up to 4.3e-6 apart; `same` since plan 0.4 (solidcore is the more accurate one, see "Acceptance") |
| differ: faces/edges and volume (0.1 % to 95 %) | 24 | booleans that lose faces (`hole_plate`, `vent_grille`, `chamfers`, `grip_shell`), `hull`/`minkowski` |
| unsupported: sketch profile in `extrude` | 42 | not wired here |
| unsupported: fillet/chamfer | 9 | API exists, not wired here |
| unsupported: polyhedron | 8 | out of scope of solidcore |
| unsupported: sweep, skin (twist/scale), shell, spline revolve, shear | 17 | not wired here or not ported |

The sketch, spline, sweep and skin rows are done since plan 1.1–1.5 (see "Profiles and sweeps").

## Profiles and sweeps (plan 1.1, 1.2, 1.4, 1.5)

`kernel-solidcore` builds every profile, section and path as the SMLib facade does
(`smlib/facade/sc_facade.cpp`, `sc_sketch.cpp`), step by step, so faces and edges come out as
SMLib's (checked node by node with `diff-kernels` and in `crates/kernel-solidcore/tests/profiles.rs`
against exact volumes):

| Kernel operation | Route | Notes |
| --- | --- | --- |
| `Loop::Composite` (sketch) | lines: `create_line_segment`; arcs: rational quadratic B-spline of the circular-arc form (pieces of at most 90°; elliptic-arc form under a non-uniform frame); B-splines as given (`create_canonical_curve`); joints welded (gap ≤ 1e-6 of the extent) | as `sketch_segments`/`weld`/`make_curve` |
| `Loop::Spline` | `BSplineCurve::create_interpolating_curve` (chord length, cubic) through the points and the first again | the ported core of `SmApiCreateCurveInterpPoints`; control points and knots equal `cad_kernel::curve2::interpolate`'s within 1e-12 (unit test) and SMLib's within 3.6e-15 (a one-off comparison through kernel-smlib's `Kernel::brep`) |
| `revolve` | spline points that follow each other on the axis: a line along the axis, open runs between (3 points: two more on the parabola); a curve reaching x < 0 (257 samples) is `InvalidInput` | as `revolve_profile_curves` |
| `extrude` with twist/scale | skin (`create_skin_primitive`) through one section per 7.5° of twist (at least 2; bicubic), ruled for a pure scale; section = turn, then scale; scale 0 and an elliptic circle are `Unsupported` as in SMLib | as `sc_extrude` |
| `loft` | segment by segment when all sections are polygons with the same point count or sketches with the same segment count, else one closed curve per section (polygon: degree-1 B-spline, circle: B-spline arc); degree 1 (ruled) or min(3, n − 1); sketches of different segment counts joined as in kernel-smlib | as `sc_loft` |
| `sweep`, `sweep_path` | profile frame at the path start (x = global X projected into the normal plane); curve sweep (`create_curve_sweep`) on straight and spline paths and tangent-continuous sketch paths; mitered `create_sweep_along_planar_path` on planar polylines and sketch paths with corners; non-planar polyline and off-centre profile along corners `Unsupported` as in SMLib | SMLib patch 0003 (`bUseProfileRefPoint` with the frame origin) is `SweepOptions::profile_at_path_start` (solidcore#1450): the frame origin lies at the path start |

Results over the 101 files (from the end of phase 0): same 2110 → 2280, unsupported 180 → 102,
blocked 287 → 172, differ 27 → 50. All `linear_extrude`, `rotate_extrude`, `loft` and `sweep`
nodes are `same` (or unsupported for other reasons, e.g. a transform check). The new `differ`
nodes are explained:

| Nodes | Cause |
| ---: | --- |
| 20 | `union`/`difference(fillet = …)`: the engine selects the new edges of the Boolean by face tags; solidcore's faces carry only the tag of the operation (no provenance, plan 1.9), so it finds none ("0 edges: no new edges found") and the fillet is skipped (`key_rack` 16, `knob_mirror` 4) |
| 2 | the same in `tests/everyday/bugs/f04`, `f15`, where SMLib's fillet fails (`SMLib fails, solidcore not`) |
| 1 | `sketch_cable_duct`: the union of three extrusions loses faces (63 vs 47; Booleans, phase 3) |

Other findings: the adapter's `transform` check rejected a few rigid transforms of curved solids
whose centroid moved by 1e-6 (`wall_hook`, `f09b`, `knob_mirror`'s mirror): the tolerance of the
check was tighter than the volume integration (fixed since, see "Transform check"). SMLib's skins are less accurate than solidcore's
(twisted square bar 8 × 8 × 20, exact 1280: SMLib +1.5e-5, solidcore +6e-7 relative).

## Face tags and edge information (plan 1.10)

Face tags are solidcore attributes as in the SMLib facade: a long attribute (id
`SM_AI_USER_1 + 77`, the facade's `SC_TAG_ATTR_ID`) with copy behaviour on every face of a new
body (primitives, extrusions, revolutions, sweeps, skins). solidcore carries it as SMLib's default
handlers do (solidcore#1462): `SmApiBrepCopy` copies it, a face that moves into a Boolean's result
keeps it, both pieces of a split face get it, a face merged with a coplanar one keeps its own.
`faces` reports the attribute, `edges` the tags of an edge's faces (twice the face's for a seam,
as `sc_edges`) and `smooth` where `SmEdge::GetBlendEdgeuse` finds nothing to blend (seams, poles,
faces tangent within 2°, open edges), as SMLib. Unit tests: `crates/kernel-solidcore/tests/provenance.rs`.

With tags, `union`/`difference(fillet = …, chamfer = …)` find the new edges of a Boolean instead
of skipping the blend: the 22 `differ` of the fillet-on-Boolean nodes and 25 more that were
counted as Booleans losing faces (`hole_plate`, `vent_grille`, `gear`, `grip_shell`, `chamfers`,
`variable_radius`, `f09a`: the missing faces were SMLib's chamfers and fillets) are `unsupported`
or `blocked` now (fillet not wired yet, plan 1.3); nodes: unsupported 92 → 128, blocked 160 →
171, differ 53 → 6. Left `differ`: `sketch_cable_duct` (the union loses faces), `angle_anchor`,
`project-housing` (2) and `f08c` (2), all from before.

Provenance (plan 1.9): after a Boolean, a face without a tag gets the tag of the operand face it
lies on (an interior point on the face's surface, within its box), else the Boolean's, as the
facade's `resolve_tags`; solidcore leaves none untagged in the corpus, so nothing changed
(2523 same). After a fillet or chamfer every face is verified that way (plan 1.3). Checked: the
fillet-on-Boolean nodes, `mcad measure --colors` (38 cases, same volumes per colour; it splits the
colours with kernel Booleans, not by face tags, so it checks the Booleans, not the provenance), zones and
asserts (`asserts`, `fillet_zones`, `multicolor`, `f07_torus_zone`: all nodes `same`); a union of
a plate and a bar keeps 10 split plate faces with the plate's tag, `union(fillet = 0.5)` of it
matches SMLib (`tests/provenance.rs`). Not checkable yet: display colours, 3MF and STEP colours
(`tessellate` and `brep`, plan 1.11 and phase 2).

## Neutral B-rep (plan 1.11)

`Kernel::brep` (`crates/kernel-solidcore/src/brep.rs`) maps the solidcore B-rep to
`cad_kernel::brep::Brep` step by step as the SMLib facade's `sc_brep` stream and kernel-smlib's
`brep.rs` do, so both kernels hand the same B-rep to STEP export, measuring and the tessellator:

| Part | solidcore |
| --- | --- |
| order | vertices of the vertex list, edges and faces as `edges`/`faces` (`Face::id`), tags as `faces` |
| edge curves | the curve's NURB (B-spline, line, ellipse, parabola, hyperbola), trimmed to the edge interval, reversed if needed, lines and circles recognised (`cad_kernel::brep::edge_curve`, shared with kernel-smlib); other curves (intersection curves, curves on surfaces) `approximate_curve` to half the largest zone tolerance, flagged |
| surfaces | plane, cylinder, cone, sphere, torus, revolution, extrusion from their canonical data, accepted if 9 × 9 points of the face's UV domain lie on them (1e-6 of the size), else the NURB; B-splines as they are; others `approximate_surface`, flagged |
| bodies | per solid region (not infinite, not void) its face-use shells, inner shells as voids (`is_inner_shell`); the written face uses are the mates (outside the material); no solid region: one sheet of the upward face uses |
| loops | the face use's loop uses: vertex loops as `Loop::Vertex`, edge uses in solidcore's CCW order, `forward` = edge use `Same` |
| `same_sense` | as the facade's `face_sample`: the grid point (7 × 7, else 23 × 23) nearest the UV centre that lies in the face (`face_classify_uv`), else the interior point; solidcore's normal there (`evaluate_surface_normal`, flipped for an opposite face use) against the STEP normal of the neutral surface. (The interior point alone landed on the pole of a hemisphere, where the STEP normal is undefined: a wrong orientation, found by the comparison below.) |

Checks: `Brep::convention_problems` (kernel-independent: `validate`, curve ends at their
vertices, edges on their faces' surfaces, planar loops counter-clockwise about the face normal),
unit tests `crates/kernel-solidcore/tests/brep.rs` (kinds per primitive, seams, holes, voids,
every face normal points out: a point along it is outside, against it inside), and
`mcad diff-kernels --brep`, which compares the B-reps of both kernels per `same` node (counts
per surface and curve kind, loops, bodies; edges matched by kind and the point at half their
length; faces by kind and those points, with the oriented normals there and the tags).

Result over the 101 files (since task D's tester findings positions and orientation are
compared also where the kinds differ, see `diff/brep.rs`): 2095 of the 2149 compared nodes
agree. The 54 others are differences of the kernels, not of the mapping:

| Nodes | Cause |
| ---: | --- |
| 25 | only tags: faces merged by a union of coplanar faces (`multicolor` letters, `rib_bracket`) and a face after a fillet or shell (`rib_bracket`, `mug`, `grip_shell`, `planter`): the kernels keep another operand's tag |
| 25 | other kinds at the same positions and orientation: `mirror` (`09_rotate_mirror`, `21_angle_bracket`, `knob_mirror`, `pipe_clamp`): solidcore's `SmBrep::Transform` turns every surface into a B-spline under a negative scale (`turn_to_nurbs`), SMLib's 4 × 4 overload keeps planes and cylinders; shear (`10_scale_multmatrix`) likewise; `shell` (`soap_dish`, `housing`, `f10`): B-spline edges where SMLib has lines |
| 4 | `phone_stand`: solidcore's fillet of an edge around a hole makes a B-spline surface (SMLib a surface of revolution) and B-spline edges instead of circles at the same positions; the fillet face's loop has another number of edges (one face "at other positions") |

STEP export goes through the neutral B-rep and works with solidcore: `tests/step/verify.py`
(OCCT reads, checks validity, closed shells, solids, faces, volume, area, bounds) passes 61 of
64 models with `MCAD_KERNEL=solidcore` (SMLib 64): `17_polyhedron` and `19_hex_nut` need
polyhedra (1.7), `asserts` stops at its asserts, which need the tessellation (phase 2).
`mcad measure` needs the tessellation as well (`MeasurePart.mesh`).

## Draft mode, cache estimate, export (plan 1.12)

| Kernel operation | SMLib (facade, kernel-smlib) | solidcore |
| --- | --- | --- |
| `set_draft` | `g_draft`: fillets skip `invalid_faces` and the volume check, Booleans take SMLib's result without the plausibility check (if it succeeds and is closed) | the same for fillets; the adapter's Booleans have no plausibility check to skip |
| `shape_bytes` | 8 KiB + 32 KiB per face + 1 KiB per edge (measured over the examples) | 8 KiB + 20 KiB per face + 600 B per edge: the same form, scaled by the heap solidcore holds against SMLib (0.6 on the largest example, plan 0.5); not measured per shape |
| `export` | not implemented (`Unsupported("export")`); STEP goes through `brep` and `cad-step` | the same |

## Offset, shell, thicken, NURBS surface (plan 1.6)

At `bba8d87` solidcore's `SmApiShellBrepFull` and `SmApiOffsetBrepFull` work (at `c9de85d` the
shell stopped in `topology.brep_p3`). `crates/kernel-solidcore/src/offset.rs` follows the facade:

| Operation | solidcore | Checks (as the facade) |
| --- | --- | --- |
| `nurbs_surface` | `BSplineSurface::approximate_points` (one control point per point, degree min(3, n − 1) per direction), `create_face_from_surface` over its natural domain | at least 2 × 2 points |
| `thicken` | `shell_brep_simple` (`SmApiShellBrepFull(t, extend, merge, solid)`) | input open; no self-overlap; tags verified |
| `shell` | `shell_brep_simple(−t, extend, merge)`, open faces by `faces` index | closed result ("leaves no cavity"), no self-overlap, 0 < volume < body; outer faces keep their tags, inner ones get the shell's |
| `offset` | `offset_brep_simple(d, rounded, self-intersections)` (mode 2) | input closed; closed result larger than the body; no self-overlap; every edge point (9 per edge) and face sample at distance d from the body within 1e-6 of the size ("offset is not exact") |
| `mass_props` of an open body (no solid region) | area, centre of the box, volume 0, as `sc_mass_props` | a solid that is no manifold keeps the error |

Not ported: the facade's shell from an inward offset and Booleans (`shell_by_booleans`, its
fallback when SMLib's shell fails) and its pre-check of opposite walls closer than 2 t; the
offset's trim-curve check (`trim_gap`: SMLib's tessellation follows the trim curves, the
neutral B-rep does not). Unit tests `crates/kernel-solidcore/tests/offset.rs` (box and sphere
offset against the Minkowski volume, shells of a box with and without an open face and of a
sphere, a flat surface thickened). All shell, offset (`hull`/`minkowski` of spheres) and
`nurbs_surface` nodes are `same`; new `differ` (volumes only): `freeform_lid`'s `thicken` of a
B-spline sheet (SMLib 5237.594, solidcore 5237.413, 3.5e-5; SMLib approximates the offset
surface, which kernel is closer is not known) and the lid above it, and `mug`'s fillet after the
shell (70568.87 against 70576.65, 1.1e-4).

## Polyhedra (plan 1.7)

`make_polyhedron` (`polyhedron`, `hull`, `minkowski` of convex bodies, STL as B-rep) builds one
planar face per polygon as the facade's `sc_make_polyhedron` (`SmPolyBrep::CreatePoly` per
face, `SmBrep::CreateFromPolyBrep`; coplanar faces are not merged). solidcore has no poly B-reps,
so `crates/kernel-solidcore/src/polyhedron.rs` writes the faces as a planar
`step_import::model::Body` (line edges shared by their two faces, a plane per face from its
Newell normal) and `step_brep::build` makes the faces and stitches them
(`SmApiStitchIntoSolid`); the body is then copied into the adapter's kernel. As SMLib: points
within `max(1e-9, 1e-9 · diagonal)` are one vertex, every edge must belong to exactly two faces
(else `Failed` "the faces do not form a closed manifold solid"), the faces are oriented
consistently (neighbours run their edge opposite ways) and outward (positive signed volume,
SMLib's `OrientTrimmedSurfaces`). A polygon off its plane by more than 1e-6 of the diagonal is
`Unsupported` (the engine splits non-planar hull faces into triangles beforehand). Unit tests
`crates/kernel-solidcore/tests/polyhedron.rs`. All polyhedron, STL and hull/minkowski nodes are
`same` except the two that also need `offset` (1.6).

## Fillets and chamfers (plan 1.3)

`fillet`, `chamfer`, `law_stations` and `fillet_variable` follow the facade's `do_fillet_like`
and `blend_core` step by step:

| Step | solidcore |
| --- | --- |
| selection: edge ids are indices of `edges` on the shape's own body; edges without a blend edge use (`GetBlendEdgeuse`: seams, poles, tangent, open) are skipped, none left → `Failed` with those edges | `topology::edge::blend_edgeuse` |
| fillet: constant radius, circular cross-section, G1 | `blend::fillet_executive::api::fillet_edges` (`SmApiFilletEdges`) |
| chamfer: constant-radius solver with a linear cross-section, every corner a bevel, `DoFilleting`, `StitchAndOrient` | `FilletSession`: `prepare_constant_radius_edges` (cross-section 0), `create_corners`, `set_corner_bevel`, `do_filleting`; `stitch_and_orient` |
| variable radius: per edge the values at the Greville stations of a clamped B-spline law (degree min(3, m − 1)), law curve (r, t, 0); constant values → constant solver | `fillet_edges_per_edge` where every edge is constant, else one law for all edges (`prepare_variable_radius_edges` with a `FilletLawKind::BSpline` law); different laws on several edges → `Unsupported` (the session takes one law; the executive's per-edge laws are not public) |
| checks: no faces left, open result of a solid, volume change beyond what a blend of that size can make (20 r² per length + 10 r³ per edge + 2 %), no new face ("unchanged, size too large?") → `Failed` with the edges | as the facade |
| tags: every face of the result checked against the input (`resolve_tags` with verification): a face on an input face keeps that face's tag, the others are new and get the operation's tag | `face_internal_point`, `surface_closest_point` |

Not ported: the facade's retry chain by chain when the blend of all edges fails. The check for
self-overlapping faces (`invalid_faces`) is ported since the tester findings on task C. Unit tests: `crates/kernel-solidcore/tests/fillets.rs` (box edge: fillet
box − (1 − π/4) r² l, chamfer box − d²/2 l; all edges of a box with the corner balls; a linear
radius law; new edges of a union; seams skipped; bad input).

Results (101 files): same 2299 → 2523, unsupported 128 → 26, blocked 171 → 47, differ 6 → 8.
All fillet and chamfer nodes are `same` except three variable fillets with different laws on
several edges (all three in `variable_radius`), which are `unsupported`. Two
changes to `diff-kernels`:

- Selected edges match in any order (solidcore lists a Boolean's edges in another order than
  SMLib; 12 nodes differed only by that).
- Rule `blend` (1e-4) for volumes with a fillet or chamfer at or below the node: both kernels
  fit the blend surfaces, SMLib less closely. `pillow_block`'s `difference(chamfer = 0.5)` (19
  edges: 3 bore circles, 4 slot loops) removes 41.40 mm³ by Pappus, solidcore 41.39, SMLib 39.15
  (6.5e-5 of the part); `pipe_clamp`'s `chamfer(0.6)` of a half ring: about 43.02 mm³ estimated,
  solidcore 43.03, SMLib 42.91 (2.3e-5, the union above it 3.5e-5). Elsewhere both are close:
  cylinder rim chamfer SMLib −1.2e-6, solidcore −5.7e-6 of the exact volume; tube rim chamfer
  SMLib +8.2e-6, solidcore −1.2e-6; cylinder rim fillet both 3e-7. 8 nodes use the rule; the
  blended nodes within 3e-5 count as `curved` (24 → 72).

Left `differ` (8): `sketch_cable_duct` (the union loses faces), `angle_anchor`, `project-housing`
(2), `f09a` (results that are no manifold solid, Booleans), `f08c` (2) and `f11` (SMLib fails,
solidcore not). Of the "Booleans that lose faces" of phase 0, the faces were SMLib's chamfers
and fillets in 25 nodes (plan 1.10). Colour regions: `mcad measure --colors` gives the same
volume per colour with both kernels for all 37 cases of `tests/color-review/cases` and
`tests/models/spec/multicolor.mcad`. Evaluation of all 101 files: SMLib 112 s, solidcore 109 s
(debug build, dependencies `opt-level = 2`); solidcore is slower on `key_rack` (12.8 s vs 5.8 s),
`mug`, `pen_box` and `wall_hook` (2×), faster on `text_sign` and `f11`.

## Tester findings on task H (e66d726)

The tester passed task H (`e66d726`, the fixes for task G) and found:

| Finding | Fix |
| --- | --- |
| edge flips of the fold repair broke the chord tolerance, silently: a flip was taken if the new triangles faced the surface and had better angles, without measuring their distance from it, and `Stats` kept the deviation measured before the flips. `f12_shell_loft_square_circle` at 0.01: chord 0.0136 mm (0.0100 with flips off), `diff-kernels --mesh` `differ`; a twisted bar (`linear_extrude(height = 30, twist = 360) square([60, 2], center = true)`) at 0.05: 0.0496 → 0.0521 | a flip is rejected where the new triangles (centroid, and the three inner points near the tolerance) or the new edge's midpoint leave the surface by more than `tol` (or than the old pair did); after any flip the face's deviation is measured again on the final mesh. `f12` 0.0100, the twisted bar 0.0496 (test from both kernels' B-reps, chord sampled independently by `surface_distance`) |
| false face contact of planes at an angle meeting at a rim (from `cc6f651`): `common_gap` sampled mesh vertices only, and for planar faces only the shared edge's vertices lie over the other face, so the gap was 0 and the projected overlap counted: a cube tilted 30° on a plate's rim "face contact 20 mm²", ok, both kernels (also 45°, 60°, 80°; 10° right) | the gap along the normal from one plane to the other is measured over the whole intersection of the outlines `plane_contact` computes (linear in the plane: its outline points suffice); beyond `tol` anywhere there it is line contact. Tests for 10° to 80° (SMLib) and 30° with both kernels; the plate tilted 0.005°, the cone in its hole and the ball in its seat stay face contact |
| `diff-kernels --mesh` false `differ` on long free-form edges: `edge_vertex_gap` against 512 segments per free-form edge, 0.0608 mm on the 762 mm helical seam of `linear_extrude(height = 100, twist = 3600, scale = 0.2) circle(20)` at 0.05 (0.0000 against the exact curve) | ellipses and B-splines sampled as the tessellator's edges to a chord sag of 1e-4 mm (circles were already): 1.0e-4 mm, `ok` (test) |
| `check().inside_out` quadratic in the pieces: `cube(60)` minus 125 balls 3.6 s on every solidcore tessellation | only pieces whose bounding box holds the test point are summed (a closed piece has winding number 0 outside its box): 4.4 → 0.56 s for that mesh (373 k triangles) |
| documentation: the forced-fold rule "height < 2 · sag" is not limited to coarse tolerances; `--mesh-tol` range missing in `diff-kernels --help` | manual 17 says what the code does; the help text gives 0.0001 to 10 |

Left open (see "Known points for phase 5"): the inner-vertex excuse near an edge, staircase line
contacts, `f02`'s Hausdorff excess at 0.01 and the SMLib-only items.

## Tester findings on task G (d943031)

The tester passed task G (`d943031`) and found:

| Finding | Fix |
| --- | --- |
| folded triangles on ruled bands with many holes at the export tolerance, exported (exit 0): a cone between two radii with 7 rings of 18 radial holes (`band_holes7`) at 0.01: 24 triangles facing against the surface at the holes' rims, 36 bad vertex normals; also at 0.005, 0.002, 0.001; `4f150fa` had none | rows of a straight direction were stepped 16 rows at a time where no run across the grid was near, but a run's margin was one row: the far step skipped most rings (3 rows instead of 18 for the cone's 7 rings), and the CDT's coordinates, half a step per band, squashed 50 mm into half a step; not affine there, they turned triangles over (counter-clockwise in the CDT, clockwise in (u, v)). A far step now ends where the next run's rows begin: 0 folds, 36.7 k triangles (was 49.6 k; SMLib's B-rep 57 k before). Independently of the cause, every triangle is now checked after the refinement against the surface normal at its centroid; a fold, or a triangle more than 60° off a corner's normal, is repaired by flipping an inner edge; folds left are an error ("face N: K triangles facing against the surface at tolerance T mm", exit 3), never exported. Not checked where the surface turns over under a triangle (`f02`'s folding sweep). Tests: the band at 0.01 and 0.005 from both B-reps (no folds, every triangle within 60° of its corners' normals) |
| triangles more than 60° off a vertex normal at 0.01: 427 → 932 (`soap_dish` 0 → 144, max 86°; `pillow_block` 41° → 74°; `bottle` 38° → 72°; `f02` +490) | corner blends tapering to a needle at a pole: a grid ring nearer the pole than the boundary's first point on a meridian edge left triangles from the pole to points of two heights (nearly containing the axis). No ring is nearer the pole than that point now; the first ring goes through it. 0.01: 692 (`f02` 688, `f12` 4; was `f02` 413, `f12` 8, `mug` 6), `soap_dish` / `pillow_block` / `bottle` 0, max 44° / 33° / 33°; 0.05: 212 (`f02` 208, `f12` 4; was 152 + 4). `f02`'s are its sweep, whose surface folds over itself (the triangles there follow it) |
| the closed-solid check ignored non-manifold edges (`kernel-solidcore::tessellate`, `diff-kernels --mesh`): `graze2` (a cylinder minus one shifted by 0.02, a crescent whose arcs touch) exported with an edge used 4 times | both use `MeshCheck::watertight`; `check` counts `duplicate_triangles` (either orientation; not watertight). The fin itself: both faces at the crescent's cusp meshed the same sliver (two triangles facing each other); triangles that coincide at the output positions are dropped, a facing pair both (every edge keeps its balance of uses): `graze2` watertight at 0.5, 0.05, 0.01, 0.002 from both B-reps (test) |
| `diff-kernels --mesh` missed displaced meshes where the root is not `same` (boundary-vertex and Hausdorff checks off, no inner vertices: shift 0.3 and shrink 1 % passed `sketch_cable_duct`, `f11`, `variable_radius`); `f02`'s allowance and the Hausdorff limit were file-wide (SMLib's 0.568 mm let any solidcore vertex up to 0.618 pass anywhere) | the vertices are checked against solidcore's own B-rep whatever the root's verdict: inside a face within `tol` of the surface, on a face's boundary within `tol` of that face's edges in the B-rep (`edge_vertex_gap`; a box corner moved along a neighbour's edge is off), no allowance from SMLib (a local one excused a vertex moved 0.1 mm where SMLib's chord error is 0.24 mm); the Hausdorff check subtracts SMLib's own distance to its surfaces where each sample is (`hausdorff_excess`). `f02`'s gaps were inversion artefacts on its folding sweep (the unseeded inversion landed on another branch): corners off the surface are inverted again from their neighbours' parameters, 0.46 / 0.57 → 4e-6 mm. With the tester's `perturb.patch` over the corpus (tol 0.05; 100 files `ok` unperturbed, no false `differ`): all positions shifted by 0.3 mm, one vertex moved 0.3 mm or 0.1 mm along its normal, the mesh shrunk by 1 %: `differ` in 100 of 100 files each (at `d943031`: 97 / 97 / 96 / 97: `sketch_cable_duct`, `f11`, `variable_radius` and with 0.1 mm `f02` passed); one inner vertex moved 0.3 mm: all 45 files that have an inner vertex |
| asserts' "cannot coincide" ignored the tolerance: a plate tilted 0.005° (gap ≤ 0.0017 over 20 mm, tol 0.01) and a cone r 5 → 5.002 in a hole r 5.002 gave line contact | surfaces that cannot coincide exactly touch over an area where their exact gap stays within `tol` over all of their common extent (each face's mesh vertices over the other face, their distance from its surface): both face contact, a torus on a plate stays a line |
| a sphere in a spherical seat was never face contact (closed seat: point; open: line; seat r 5.005: none) | concentric spheres with radii within `tol` are face contact (mesh estimate) with both kernels (`cli` test) |
| a torus' line contact was drawn as a straight diagonal in `render --show asserts` | the near vertices are averaged in cells and chained into the contact curve (broken where a jump is far longer than the usual spacing and does not lie on both faces): the circle of radius 20 as one closed polyline (test) |
| "mcad diff-kernels: diff-kernels: --mesh-tol …"; no upper bound for `--mesh-tol`; the budget message printed all digits of the raised tolerance | the prefix once; at most 10 mm; tolerances in messages with four significant digits |
| `fillet_like`'s doc comment merged into `unsupported_corner`'s; "`scale(2) mug()` at 0.01: 84 k" | restored; the tester measured 79.6 k at `d943031` (84.7 k after the rows over every run) |
| `cad_kernel::brep::minimise` and `cad_measure::num::minimise` missed the minimum next to the seam of a closed profile (a tie keeps t = a, only [a, a + h] searched): `Surface::distance` on `14_pipe_bend`'s revolution face 0.1027 at a point 0.0357 off; `check().inside_out` used the net volume only | both ends are searched where they tie with the best sample (tests); `inside_out` per connected piece: a piece with negative volume that lies outside the others (winding number) is inside out, a void inside its body is not (test) |
| documentation only: the twisted extrusion at `--mesh-tol 0.001` fails loudly ("tolerance 0.001 not reached (4.107e-3)") where SMLib meshes with 0.0091 mm chord error; tolerance 0.5 with features smaller than it gave flipped triangles (`torus(100, 0.2)`: 92; `graze2`: 8) | the twisted extrusion now ends at 2.6e-3 mm, still an error (exit 3: a finer tolerance than the refinement reaches on that twisted B-spline; 0.002 and coarser work). At a coarse tolerance a fold on the boundary whose chord sags more than half the triangle's height above it is forced by the edges (a rim or a thin tube narrower than the tolerance; no point inside changes that chord): it is kept and counted (`Stats::forced_folds`, 0 in the corpus at 0.05 and 0.01, at 0.5 `planter` 24 on its rim), not an error. Triangles spanning a feature narrower than the tolerance whose corners' normals point more than 90° apart are not checked at all (`graze2` at 0.5: 8 across its 0.02 mm crescent, `mug` 19); the tester's 92 on the thin torus are triangles across the 0.2 mm tube whose centroids project onto its far side (a measuring effect at a tolerance above the radius). At 0.05 and 0.01 none of these occur in the corpus or the tester's models |

## Tester findings on task F (4f150fa)

The tester confirmed phase 2 (render, export, section, asserts, `measure` with solidcore) and
found:

| Finding | Fix |
| --- | --- |
| `mug` scaled by 2 (also 2.5, 3, 4; unscaled at 0.005) failed STL export: "face 9: tolerance 0.01 not reached (5.655e-2 mm after the refinement)" after 29 s, SMLib 100 k triangles in 5 s; also on SMLib's B-rep | face 9 is the conical wall (a revolved straight line) with the handle's holes: rows of the straight direction near the holes, none for 38 mm below them. A refinement point in that band was joined by Delaunay to row points several steps away across the curved direction, so each split made the next long edge. All rows of a straight direction are now half a step apart in the CDT's coordinates (`Warp`), however far apart by arc length. `scale(2) mug()` at 0.01: 79.6 k triangles (measured by the tester; 84.7 k after task G's rows over every run), 1.6 s for the mesh (SMLib 100 k); tests: ruled bands (revolved line, cone, cylinder) with stubs and a hole at scales 1, 2, 4 and tolerances 0.01, 0.005, and the corpus mug scaled by 2, from both kernels' B-reps |
| tori and revolved fillets meshed isotropically, 3 to 14 × SMLib's triangles, linear in the major radius: `torus(50, 5)` at 0.05 21 k (SMLib 8.2 k), at 0.01 100 k (33 k); the hoop `rotate_extrude() translate([300, 0]) circle(5)` at 0.01 "too fine" (513 k; SMLib 65 k); `torus(2000, 5)` at 0.05 failed render, section and measure; `sphere(300)` at 0.01 failed (SMLib 259 k); the 300 mm tray with `$fillet = 1` 250 k (SMLib 28.7 k) | the CDT works in grid steps instead of arc length (a step of either direction is one unit: Delaunay triangles round for the chord error, long along the circle of a torus, short across its tube); doubly curved surfaces get grid steps for half the tolerance in each direction (a cell's diagonal sags by the sum of both). Now: `torus(50, 5)` 7.1 k at 0.05, 33.6 k at 0.01 (SMLib 8.2 k, 32.8 k); hoop 79 k (65 k); `torus(2000, 5)` at 0.05 42 k (52 k); `sphere(300)` at 0.01 297 k (259 k: the grid keeps every meridian up to the poles); tray 23 k (28.7 k), the 240 mm tray with a foot ring 28 k (35 k). Budget (chosen: a fixed guard, not a quality limit): 2 000 000 triangles per face and in all, a 100 MB STL, where a realistic part at 0.01 needs a few hundred thousand; beyond it the error stays (exit 3), SMLib would mesh on for minutes |
| `f13_shell_fillet`, `f08` tori, `heat_sink` far above SMLib where SMLib holds the tolerance | rows of a straight direction one row beyond a run across the grid (was four) and 32 steps apart elsewhere (was 16): at 0.05 / 0.01 `f13` 316 / 772 → 132 / 344 (SMLib 92 / 164), `heat_sink` 3.9 k / 5.9 k → 2.1 k / 3.4 k (1.7 k / 2.2 k), `f08a` 4.9 k / 20.8 k → 1.5 k / 5.6 k (2.6 k / 5.4 k) |
| `diff-kernels --mesh`: all positions shifted by 50 mm was `ok` (Hausdorff 50, chord 0); one vertex moved 0.3 mm was `ok` | vertices are checked (within `tol` of their face plus SMLib's gap where the bodies are the same, separately inside faces and on their boundaries); the Hausdorff limit no longer adds solidcore's own surface distance (which contains the displacement) and makes the verdict `differ`; test with displaced meshes |
| `mesh_distance` far from the mesh took 75 s (the grid was searched in growing shells) | a bounding-volume hierarchy (nearer child first, boxes beyond the best skipped) |
| assert outcomes depended on the tessellator: two cylinders 0.005 mm apart side by side with `assert_touches(tol = 0.01)`: SMLib "face contact 0.0022 mm² (mesh-based)", solidcore "only line contact" (export exit 0 vs 4); `torus(20, 2)` on a plate: face contact 91.8 vs 22.3 mm² | solidcore's answer was right (the contacts are a line and a circle): analytic surfaces of different kinds, planes at an angle and cylinders that are not coaxial with equal radius cannot lie on each other over an area, so their mesh-based contact is a line or a point, independent of the mesher; free-form surfaces stay mesh-based (their face-contact area still depends on the mesh) |
| the budget message named the per-face limit where the remaining total had cut it | "at most L left of 2000000 in all" |
| manual: a `--tol` option that does not exist; `$tol` described as reaching export; the export example's numbers | the tolerances are the commands' (`export` 0.01, `render`/section 0.05, `measure --clearance --mesh-tol`); `$tol` is a node's display tolerance in the UI only (render and export ignore it by design: their tolerances are fixed per command); numbers rerun |
| `diff-kernels --mesh`: "solidcore: solidcore: tessellate: …"; `--mesh-tol 1e-9` hung SMLib's tessellator (3.7 GB), `inf` said "must be > 0" | the prefix once; at least 0.0001 mm (as `measure`), "is not a finite number" |
| `check`: an inside-out mesh and NaN positions counted as watertight, so the closed-solid check of `kernel-solidcore` accepted both | `inside_out` (closed, consistent, negative volume) and `non_finite`, neither watertight; `tessellate` fails on both |
| `f09b` and `f13` with solidcore gave raw upstream statuses | a failed fillet re-creates the corners with solidcore's opt-in `fillet_vertex_error_diagnostics`: an unsupported corner gives "cannot blend the corner where these edges meet"; a failed shell checks the smallest convex rounding (the facade's `min_convex_radius`): "wall thickness T is not smaller than the radius of a convex rounding …". Both bug models now end with the message they expect |
| SMLib's B-rep of `f02` at 0.01 (found while measuring these fixes): two triangles whose corners coincide at `f32` resolution (a crossing of loops split at a boundary point) used an edge twice | such triangles are dropped at the output; their neighbours close the surface |

## Tester findings on task E (d896e16)

The tester confirmed 101 / 101 watertight at 0.05 from both kernels' B-reps, face ids and tags as
SMLib's, `diff-kernels` unchanged, and found:

| Finding | Fix |
| --- | --- |
| a tiny tolerance hung or ran out of memory (`sphere(10)` at 1e-7: > 60 s, 4.4 GB), 1e-20 panicked (integer overflow in the refinement's point grid); `$tol` reaches the kernel unfiltered | the tolerance is at least `MIN_REL_TOL` = 1e-6 of the model size (the `f32` resolution of mesh positions; SMLib's facade passes the tolerance through unchanged and stops its own refinement at a budget of 300 k triangles); budgets of 300 k triangles per face and 500 k in all, checked on the grid before the CDT, after the CDT and while inserting refinement points: "tolerance T mm too fine: more than N triangles" (an error at once, the other faces are not meshed). `sphere(10)` at 1e-7 fails in 0.03 s, the twisted extrusion at 1e-7 in 30 s (debug); the overflow is gone (saturating cell keys, the floor) |
| a face stopped by the budget before the first pass kept `max_deviation` 0 (nothing measured); passes used up still gave `Ok` | each pass measures the mesh as it is, the last one only measures, so the deviation is that of the final mesh; a face above the tolerance is listed in `Stats::unconverged`; `tessellate` (and `kernel-solidcore`) fail if one is above `MAX_MISS` = 2 × the tolerance (SMLib keeps whatever its 3 passes reached) |
| "largest deviation 0.050" was measured at centroids and inner edge midpoints only; dense sampling found 0.0504 to 0.0552 | a triangle whose centroid is off by more than a third of the tolerance is also measured at three inner points towards its corners; `Stats::max_deviation` documents the points; `diff-kernels --mesh` and `cad_tess::surface_distance` sample 7 points per triangle independently (corpus at 0.05: chord error ≤ 0.050) |
| vertex normals at revolution poles and collapsed B-spline corners pointed against their triangles (`soap_dish` 104, `pillow_block` 5, `variable_radius` 22) | `Geom::normal` takes the limit from inside the face at a known pole (round-off `σu` passed the relative test); a vertex normal still against the mean of its triangles becomes that mean (collapsed corners of variable fillets): 0 in all three |
| `check`: a T-junction closed by a zero-area triangle passed, an empty mesh was watertight, an index out of range panicked | empty is not watertight; bad indices are counted (`bad_indices`) and not watertight; a zero-area triangle closing a crack leaves no gap and still counts as closed (it is reported in `degenerate`; at tolerances near the floor the `f32` positions make slivers of that kind); self-intersections and fins are not detected (documented) |
| an edge curve of degree ≥ 10 panicked | edge curves and revolution/extrusion profiles are checked like B-spline surfaces: "degree above 9" |
| more triangles than SMLib where SMLib holds the tolerance (`f13_shell_fillet`, `heat_sink`, `f08`), slower on some sweeps; "fewer in 78 / 101" counted equal ones | partly: the fine-tolerance work (task F) cut `heat_sink` at 0.05 to 3.9 k (SMLib 1.7 k), the counts in "Tessellator" are strictly fewer; the rest is open |
| `f32` mesh positions limit the accuracy far from the origin (a model at 10000 mm: 0.0515 at tol 0.05) | documented (the `Mesh` type; the floor `MIN_REL_TOL` is relative to the model size, not to its distance from the origin) |
| `Asserts: 6 of 6 checked, 0 violated` counted pairs that could not be checked | the summary says `Asserts: n of m checked, k violated, j could not be checked` (manual 17, 19) |

## Tester findings on task D (ae50f1f)

The tester reproduced the numbers (101 files, 2604 nodes: 2596 same, 3 unsupported, 3 blocked,
2 differ, 0 failed; no node worse) and found:

| Finding | Fix |
| --- | --- |
| rule `blend` missed wrong radii: `cube(100, $fillet = 1)` caught only at × 1.05, a filleted hole r 10 at × 1.2, r 3 with `fillet = 0.5` at × 2 (the 4e-6 · V part dominated) | blended nodes compare the neutral B-reps of both kernels (always, not only with `--brep`): with the same kinds the edge and face positions and orientation must agree (else `differ`, "blend: B-reps at other positions"), × 1.02 is caught on all three; with other kinds 5 % of the change; the 4e-6 · V part only for curved results. OCCT: the blend geometry of both kernels agrees within 1.8e-12, SMLib's integration is off up to 7.7e-5, solidcore's up to 2.7e-6 |
| Boolean checks sampled a face only if its box met a face box of the other operand: `cube(100, center = true)` with `sphere(5)` inside got no sample (only the 3 % volume bound); beyond 24 near faces every k-th | the facade's rule (a face whose box meets the other operand's box), every face of the smaller operand, at least one face per shell; every k-th only beyond 64 faces. Cost: point classification skips points outside the body's box (a cached box per shape): the evaluation of all files took 216 s instead of 240 s, with more samples |
| an operand smaller than the checks' offset (1e-4 of the pair) or the retries' shift (up to 1e-4 of the pair) could be moved or lost silently (a sphere r 0.005 on a cube of 10000 moved by 0.017 mm, a pocket r 0.002 lost, a cube of 0.01 kept whole) | in the solidcore adapter the offset is at most 1e-3 of the smaller operand (at least ten zone tolerances, so that the classification does not say "on"), the shift at most 1e-3 of it: such cases now fail loudly ("solidcore returned a wrong result") or come out right; unit tests `operand_inside_the_other`, `operand_smaller_than_the_offset`. SMLib's facade keeps its behaviour (a known SMLib weakness, see "What is missing") |
| `thicken` returned the offset sheet alone with status ok (volume 0, STEP one sheet) where SMLib fails | the result must be a closed solid, else `Failed` "thicken: the result is not a closed solid (the offset of the surface folds over itself …)"; unit test in `tests/offset.rs` |
| `diff/brep.rs` returned early on count or kind mismatches: mirrored, sheared and shelled bodies and fillets around holes were never compared by position | positions and orientation also across kinds (edges within 1e-4 of the size there, faces by the points of their loop edges); numbers in "Neutral B-rep" (2095 / 54) |
| manual 17 lacked solidcore's Boolean and thicken failures; a touching operand had no `at [x, y, z]` | entries added; the failure of a Boolean whose result is open carries the result's open edges as places (the engine names the place and marks it, as with SMLib) |
| solidcore fails loudly where SMLib succeeds (polyhedron of two cubes sharing an edge, `shell(1) cube(10, $fillet = 2)`, a root union of many slivers); shell failures said "upstream status 1007" | upstream status codes now carry their meaning ("the kernel refused its input (degenerate or self-overlapping geometry) (… upstream status 1007)"); the rest in "What is missing" |
| a projective matrix with shear failed as "shear (the matrix could not be split …)" | the projective check runs before the shear split; unit test |
| rule `operands` allowed far too much for small pieces of large curved faces (`intersection() { sphere(1000); translate([0, 0, 1000]) cube(1, center = true); }`: OCCT 0.49992, solidcore 0.791, SMLib −2.64, `same`) | the budget is capped at 1e-4 of the node's volume (largest need in the corpus 1.3e-5, `soap_dish`); such a node is `differ` now. Both kernels integrate a small piece of a large sphere poorly (the face's divergence integral cancels: solidcore +58 % there, SMLib negative) |

Also: `sketch_cable_duct`'s `differ` is an SMLib error (exact 9898.11 = solidcore, SMLib +0.54).

Result over the corpus: no verdict and no volume rule changed (2596 same, 3 unsupported, 3
blocked, 2 differ, 0 failed; rules relative 2293, exact 129, operands 67, blend 70, curved 25);
the 70 blended nodes pass with positions or within 5 %; copies 2420.

## Re-test findings on task C

| Finding | Fix |
| --- | --- |
| Booleans unchecked: a box minus a cylinder that touches it tangentially gave an open body (STEP export wrote it, the volume query failed); box ∩ a sphere of 1e-4 of its size gave the whole box (1e6 instead of 2.6e-7); Booleans of `scale(1e3)` and `scale(1e-3)` bodies no manifold solid, status ok | the facade's `checked_boolean` ported (`crates/kernel-solidcore/src/boolean.rs`): per step of closed solids the result must be closed (or, where the interiors do not overlap, `a − b = a`, `a ∩ b` empty), its volume within the bounds of the operation (3 %), points just outside and inside one face of each operand near a face of the other inside or outside the result as the operation says (confirmed at a second point; one point on at most 24 faces, the facade takes 5 on every face near the other operand: solidcore's point classification made that 2.5 times as slow as SMLib over the corpus, `text_sign` 200 s); a union of bodies that only touch is one solid with a shell per body (`SmBrep::MergeBrep`, the facade's `touching_union`: `angle_anchor`, `project-housing` are `same` now); else the swapped operands, then the second moved by 1e-6, 1e-5, 1e-4 of the size; else `Failed` with the facade's wording ("the result is not a closed solid: the operands touch tangentially …", "solidcore returned a wrong result (…)"). Volumes coarse (1e-2, as the facade), the accurate ones before a refusal (`rib_bracket`: a coarse 1.3 % error refused a right union). The tester's tiny-sphere and scaled cases now give SMLib's volumes after a retry, the tangential difference fails as in SMLib. Not ported: `disjoint_union` (solidcore unites bodies apart correctly), the reordering of a failed chain. Unit tests `tests/booleans.rs` |
| rule `blend` accepted a radius 5–10 % too large (`cube(100, $fillet = 1)`: via `curved`, via `blend` with 1e-4 of the node) | `blend`: 8 % of the volume the blends and offsets in the node's history changed (|V(result) − V(input)| per fillet, chamfer, thicken, shell, offset, carried by `diff/exact.rs` through transforms and Booleans) plus 4e-6 of the node; `curved` no longer applies to blended results. A 5 % radius error (10 % of the change) fails, `pillow_block`'s 5.4 % passes (unit test `blend_tolerance_follows_the_blended_volume`) |
| selected closed edges matched within twice the chordal sag | documented as the limit of the samples (18 per edge: 0.19 mm for a circle r 10); face and edge counts and volumes are still compared exactly |
| result of the round (101 files) | same 2589 → 2596, unsupported 3, blocked 3, differ 9 → 2, failed 0: `angle_anchor`, `project-housing` (touching union), `f08c`, `f09a` (tangential Booleans now fail as in SMLib or come out right), `freeform_lid`, `mug` (rule `blend`) are `same`; 58 `curved` and 4 `none` nodes use rule `blend`, no node became `differ` by it. Evaluation of all files: solidcore 252 s, SMLib 149 s (156 s before the checks; debug build) |
| `invalid_faces` (1e-4 of the diagonal) rejects valid tiny fillets (`cube(0.08, $fillet = 0.005)`) | left as the facade has it (parity); a larger `eps` from the zone tolerance would be the fix in both kernels |
| `transform` reset the tolerances under a mirror (scale −1) | only under a scale ≠ ±1, as the facade (rigid motions and mirrors keep them) |
| unused area in `tests/transforms.rs` | removed |

Known SMLib crashes (no fix, SMLib is the reference until the switch): `rotate_extrude() polygon([[x, 0], [5, 0], [5, 5], [x, 5]])` for x from about ±1e-10 to ±2e-9 (also +1e-10, a valid profile) ends `mcad` with a segmentation fault (exit 139); `diff-kernels` on the tester's `m_cone.mcad` crashes inside SMLib (`SmTopologyIntersector::CloseIntersectionLoop`, called from the facade's `checked_boolean`).

## Tester findings on task C (fillets, tags, transforms)

| Finding | Fix |
| --- | --- |
| oversize fillet accepted (`cube(10, $fillet = 6)`, `5.5`: SMLib "the result overlaps itself", solidcore a body whose volume grew with the radius; `union(fillet = 0.8)` with a twisted extrusion) | the facade's `invalid_faces` ported: a closed result whose face samples (as `face_sample`) are not a boundary between inside and outside (a point 1e-4 of the size along the outward normal outside, against it inside) is `Failed` "the result overlaps itself", with the selected edges within 3 r of those faces (`edges_near_faces`); only faces whose box meets the boxes of the blended edges grown by 4 r are checked (the facade checks all: two point classifications per face, `gear` 13.2 → 8.8 s); unit test `oversize_fillet_overlaps_itself` |
| `faces` failed on the tiny faces left by a fillet of almost half the box (`face_internal_point`: status 1001) | a sample as the facade's `face_sample` instead |
| transformed tori and revolved circles: Booleans returned the whole body or nothing after a mirror, a point reflection or a uniform scale with a rotation (`difference` of a scaled, turned torus and a half-space: 2327.49 instead of 1163.74; a top-level union of five bodies with one: 484 instead of 4915 mm³), while vertices, curves and volume looked right | the cause: `transform_independent_object` (`SmApiTransform`) rescales the tolerances from the box of the vertices whenever there is a scale (also −1 of a mirror); a torus has a single vertex, so every tolerance became 0 (and the context's model size changed). The adapter calls `SmBrep::Transform` without that, as the facade's `apply_matrix`, and gives a scaled body tolerances from its full box. The transform check also follows face normals now (outward normal at a point of each face against the inverse transpose of the matrix). Regression test `tests/transforms.rs`: 7 primitives × 8 transforms (rotation, mirrors in x, z and a skew plane after a rotation, uniform scale before and after a rotation, point reflection, scale 0.3), each cut by a half-space: the halves add up to the exact volume (12 failures before, none after) |
| empty Boolean results: `faces`, `edges`, `mass_props` failed (`SmApiGetFaces` refuses an empty body) | empty lists and zero mass properties |
| rule `blend` scaled with the whole node, up to the top-level group | 1e-4 of the volume of the nearest blend nodes at or below the node (summed), not of the node |
| closed selected edges matched by their first point only | all sample points, either direction, a closed edge from any start point |
| leftover `println!` in tests; "curved 73" (72); `measure --colors` described as a test of the tags (it splits colours with kernel Booleans) | removed, corrected |

Not changed: a revolve profile 1e-10 across the axis passes the pre-check in both kernels (the
facade's tolerance, 1e-9 of the frame's scale) and solidcore then fails loudly ("did not produce
a closed solid"); `mcad check` of that model with SMLib ends in a segmentation fault.
In the tester's matrix the remaining `differ` nodes are edge counts after a mirror (solidcore's
B-spline surfaces after `turn_to_nurbs` split differently: 3 edges instead of 2) and volumes of
groups where solidcore is the exact one; no volume is wrong.

## Tester findings on task B (profiles and sweeps)

| Finding | Fix |
| --- | --- |
| `union(fillet = …)` of a Boolean skipped the fillet ("0 edges"): no face tags | plan 1.10 (tags through Booleans) and 1.3 (fillets): `union(fillet = 1) { cube([20,20,4]); translate([10,10,0]) cylinder(h=10, r=4); }` is the same in both kernels (1 edge) |
| no exact volume for twist/scale extrusions (SMLib off by > 3e-5 outside the corpus: twist 360°, bar 20 × 8 × 4: SMLib +1.1e-4, solidcore −4.6e-7) | `diff/exact.rs`: A·\|h\|·∫₀¹ sx·sy dt. The `curved` tolerance stays 3e-5: a loft in `planter` (no exact volume) differs by 1.9e-5. `volume_rules` counts `same` nodes only: 24 `curved` at `3284a36` (27 nodes counting all verdicts) |
| `mcad measure … --at bogus --kernel solidcore` exit 3 | references and points are checked before the kernel model: exit 2 |
| `kernel_id` named a kernel when the kernel was wrong (`MCAD_KERNEL=foo`, solidcore without the feature) | `kernel_id` is missing then |
| revolve: no pre-check of polygon/spline points and circles | as `sc_revolve`: x ≥ −1e-9 · scale after the frame, else `InvalidInput` ("crosses the axis") |
| plain extrude and revolve without `require_solid` (a bowtie polygon gave a body that failed later) | `require_solid` as `sc_extrude`/`sc_revolve`: "did not produce a closed solid" |
| `loft_sections` duplicated; `similar_in_plane` checked x and y only | `cad_kernel::loft_sections` for both kernels; arcs and circles use the facade's `similarity` (all three columns) |

## Performance (plan 0.5)

Release build (`cargo build --release -p cad-cli --no-default-features --features solidcore`),
4 cores, 101 files of the `diff-kernels` corpus, solidcore `bba8d87`. Two measurements:

- `mcad diff-kernels --json`: per file and kernel `eval_ms` (evaluation: language and kernel
  operations), `query_ms` (faces, edges and volume of every node: the queries of `diff-kernels`,
  which go through the same paths as zones, measuring and `inspect`), `heap_bytes` (heap held
  after both, glibc `mallinfo2`), `copies`/`copy_ms` (`Inner::copy`, `SmApiBrepCopy`).
- `scripts/ci/kernel-perf.py target/release/mcad`: one `mcad eval` process per file and
  kernel, wall time and peak RSS (`wait4`).

Totals over all files say little: where solidcore stops at an unsupported operation (fillets,
sketch profiles, sweeps) it skips the most expensive work. The fair comparison is the 38 files
where every node is `same` or `differ` (nothing unsupported or blocked). "Before" is the adapter
with a copy per query (plan 0.5), "after" queries the shape's own body and caches its mass
properties (plan 0.6); the verdicts of all 2604 nodes are the same in both.

| Measure | SMLib | solidcore before 0.6 | solidcore after 0.6 | after / SMLib |
| --- | ---: | ---: | ---: | ---: |
| evaluation, 38 comparable files (s) | 12.3–12.6 | 16.0 | 13.7 | 1.12 |
| queries, 38 comparable files (s) | 9.2–9.4 | 23.0 | 10.7 | 1.17 |
| `Inner::copy`, 38 comparable files | – | 2329 copies, 0.93 s (2.4 %) | 552 copies, 0.20 s (0.8 %) | |
| evaluation, all 101 files (s) | 103.6–104.0 | 57.2 | 48.9 | 0.47 (solidcore skips unsupported work) |
| queries, all 101 files (s) | 49.7–50.4 | 59.4 | 23.2 | 0.46 |
| `Inner::copy`, all 101 files | – | 6429 copies, 1.78 s | 1549 copies, 0.39 s | |
| heap after evaluation and queries, largest file (MB) | 140 | 56 | 84 | 0.60 |
| `mcad eval` per process, wall time, sum over 101 files (s) | 105.6–106.5 | 59.8 | 50.7 | 0.48 |
| peak RSS per process, largest file (MB) | 165–167 | 82 | 85 | 0.51 |
| peak RSS per process, median file (MB) | 24–25 | 25 | 27 | 1.09 |

Twisted extrusions are expensive in solidcore (debug build): `linear_extrude(height = 10, twist =
720) square([6, 2], center = true)` 1.1 s against 78 ms in SMLib (97 sections, bicubic skin), the
tester's file of 8 twist/scale extrusions 8.1 s against 0.38 s.

Slowest files (solidcore evaluation in `diff-kernels`, ms, before → after 0.6):

| File | SMLib eval | solidcore eval | SMLib queries | solidcore queries | copies |
| --- | ---: | ---: | ---: | ---: | ---: |
| `spec/hull_minkowski.mcad` (8 unsupported) | 709 | 6441 → 4474 | 1231 | 6219 → 1110 | 140 → 37 |
| `spec/knob_mirror.mcad` (2 unsupported) | 596 | 5912 → 4422 | 1260 | 5418 → 395 | 46 → 8 |
| `everyday/gear.mcad` | 5019 | 3485 → 3124 | 1834 | 1778 → 784 | 126 → 28 |
| `everyday/key_rack/key_rack.mcad` (8 unsupported) | 5829 | 3198 → 2758 | 5420 | 2334 → 1290 | 190 → 52 |
| `openscad-compat/02_sphere_variants.scad` | 17 | 1187 → 972 | 394 | 1778 → 581 | 28 → 7 |
| `openscad-compat/26_if_else_variants.scad` | 81 | 674 → 522 | 453 | 1515 → 220 | 91 → 15 |

The "after" figures of the two `openscad-compat` rows were measured later (release, at the
end of plan 1.5, `0e18322`; neither file uses an operation of phase 1), the others at the end of
phase 0.

Findings:

- Copies themselves were cheap (1.5–2.4 % of solidcore's time; most in `gear`, big B-reps,
  8 %). What 0.6 saved is mostly repeated work: each query ran on a fresh copy, so the caches
  solidcore keeps in the body (and the mass properties computed for the `transform` check) were
  lost every time. Querying in place halves the query time (23.0 → 10.7 s on the comparable
  files) and takes 14 % off the evaluation; the bodies keep their caches, so the heap grows
  (56 → 84 MB on the largest file).
- `transform` checked its result with two volume integrations (`mass_props` before and after,
  see the adapter): `sphere(5)` takes 6 ms in both kernels, `translate(…) sphere(5)` 7 ms in SMLib
  and 460 ms in solidcore (one volume integration of a sphere: about 230 ms). With many
  transforms of curved solids (`knob_mirror`, `hull_minkowski`, `02_sphere_variants`) that
  dominated; replaced by a cheap check (next section).

### Transform check

`transform` (`transform_independent_object`) is correct at `bba8d87` on bodies copied with
`SmApiBrepCopy`; the adapter still checks each result, because solidcore moved only the vertices
up to `c9de85d` (and still does on a `Brep::clone`). The check follows probes through the
transform: every vertex, three points of every edge curve (a quarter, half and three quarters of
its interval; same normalised parameter, else the closest point on the curve) and the middle point
of each edge on the surfaces of its faces (`surface_closest_point`), all within 1e-7 of the body's
size. Only where a probe cannot decide (a closest-point solve fails) volume and centroid are
compared, within the accuracy of the integration (1e-5); over the corpus that never happens. The
mass properties of a similarity transform follow from its source's (one integration per chain of
transforms instead of one per node).

| Measure (debug build, dependencies `opt-level = 2`) | before | after |
| --- | ---: | ---: |
| `translate([1, 2, 3]) sphere(5)`, solidcore evaluation (ms) | 488 | 7 |
| 8 files with many transforms of curved solids (`knob_mirror`, `hull_minkowski`, `02_sphere_variants`, `wall_hook`, `f09b`, `pipe_clamp`, `26_if_else_variants`, the sphere), evaluation (s) | 17.4 | 4.0 |
| all 101 files, solidcore evaluation (s) | 95.4 | 65.8 |
| all 101 files, solidcore queries (s) | 40.9 | 57.9 |

The queries take longer because more nodes are evaluated now (and their volumes are integrated
when queried instead of during the transform). Nodes: same 2280 → 2299, unsupported 102 → 92,
blocked 172 → 160, differ 50 → 53: the 10 rejected transforms pass (`wall_hook`, `door_stop`,
`pillow_block`, `pipe_clamp`, `05_nested_booleans`, `28_cable_clip`, `knob_mirror`, `f09b`); the
new `differ` are `knob_mirror`'s mirror and the group above it (the mirrored knob with the
`difference(fillet = 0.5)` that finds no new edges, plan 1.9) and `f09b`'s union (SMLib fails,
solidcore not).

## What is missing (in solidcore or here)

1. ~~Primitives other than the box~~ (works at `bba8d87`).
2. ~~A transformation that carries curves and faces along~~ (works at `bba8d87`); the 4×4 matrix
   variant (shear, e.g. `multmatrix`) is not ported, the adapter splits such a matrix into a
   rotation, a scale along the axes and a rotation (plan 1.8).
3. Booleans that keep all faces (see "State at `bba8d87`"); ~~point classification~~ (works).
4. ~~Public fillet/chamfer API~~ (exists at `bba8d87`; wired here, plan 1.3); still missing: a
   public per-edge radius law in `FilletSession` (the executive has `var_rad_laws` per edge).
5. Here: ~~sketch profiles for `extrude`; wire up `create_skin` (loft, extrusion with twist),
   sweeps~~ (plan 1.1–1.5, see "Profiles and sweeps"); offset/shell; ~~attributes for face tags (provenance)~~ (plan 1.9/1.10);
   `Kernel::brep` and STEP export via `step-brep`/`step-export`; Wasm build in the same module
   (stack 8 MiB, see solidcore README).
6. ~~**Own tessellator**~~ (not in solidcore; `cad-tess`, plan 2.1–2.8, see below).
7. Where SMLib succeeds and solidcore fails loudly (task D's tester): a polyhedron of two cubes
   sharing an edge (SMLib 16 mm³; solidcore "every edge must belong to exactly two faces");
   `shell(1) cube(10, $fillet = 2)` (SMLib 414.97; solidcore's offset reports self-overlap,
   status 1007, now with its meaning); a root union of many slivers (the facade reorders a
   chain's operands when a step fails, not ported); a Boolean of an operand smaller than about
   1e-4 of the pair on a face of the other (solidcore's result is wrong; the adapter's retries
   move the operand by at most 1e-3 of its size, so it fails; SMLib's facade moves it by up to
   1e-4 of the pair or loses it, e.g. a sphere r 0.005 on a cube of 10000 moved by 0.017 mm, a
   pocket r 0.002 in a cube of 1000 lost: a known SMLib weakness, the facade stays as it is).

Estimate from the research (uncertain, depends on the units above and on the people who take
over units in solidcore): all primitives end to end in 1–2 weeks, Booleans in 2–4
weeks, fillets, sweeps and shell in 6–10 weeks.

## Tessellation: own tessellator for M8

The outline below was the plan; what was built is in "Tessellator `cad-tess`".


Tessellation and polyhedra are explicitly out of scope of solidcore
(`plan/closure.md`). For M8 (display, STL/3MF, section view, `compare.py`) we therefore need
our own tessellator in Rust on top of the solidcore B-Rep, or SMLib (`NL_Tessellate`,
C++) or OCCT (`BRepMesh`, via the STEP route) stays in the build just for this; that contradicts
the goal of M8 to remove the SMLib Wasm. Outline:

1. **Edges first, once per edge:** each edge is discretised exactly once and used by
   both neighbouring faces (watertight, edge matching). Chord error control:
   line 1 segment; circle/ellipse step angle `θ = 2·acos(1 − tol/r)`; NURBS adaptive
   bisection until the deviation of the midpoint is ≤ `tol` and the tangent angle is below a
   limit.
2. **Planar faces:** project the loops into the plane and triangulate as a polygon with holes
   (ear clipping with hole bridges like earcut, or constrained
   Delaunay triangulation, CDT); orientation from face normal and loop direction.
3. **Analytic and NURBS faces:** work in parameter space. Bring the edge points to (u, v) via the
   UV trim curves of the edge uses (or point inversion onto the surface);
   cut open the seams of periodic surfaces (cylinder, sphere, torus) and duplicate points,
   treat poles (sphere, cone apex) as degenerate edges. A UV grid whose
   step size follows from the chord error (cylinder/cone: from the radius; sphere/torus: from
   both radii; NURBS: from second derivatives or the flatness of the control net,
   adaptively as a quadtree), trim the grid points against the UV loops
   (point in polygon), then CDT in UV with the edge polylines as constrained edges, so that the
   boundary consists exactly of the edge points from step 1. Afterwards refine where the
   3D deviation at triangle or edge midpoints exceeds `tol` (Steiner points), and
   avoid triangles with a poor aspect ratio in UV (anisotropic scaling of u
   and v by the derivative lengths).
4. **Normals** exactly from the surface evaluation per vertex, orientation according to the sense of the
   face; face ID per triangle for provenance (`Mesh`), edge polylines for the
   display; coarse/fine by tolerance as in plan 7.2.

Dependencies: from solidcore surface and curve evaluation (point, derivatives, normal:
`SmApiEvaluateSurface*`, curve `evaluate_point`), boundary loops and edge uses
(`face_get_boundary_loops`), UV trim curves (`SmEdgeuse::GetOrCreateUVTrimCurve`, unit
`topology.edgeuse_p3`, partial today), point inversion (`surface_closest_point`),
UV classification (`face_classify_uv`), surface type and parameter range/periodicity. On
our side: a CDT (own or e.g. the crate `spade`, MIT/Apache) with robust
predicates (`robust`), ear clipping for the planar case. `make_polyhedron` would need its
own construction from planar faces and stitching (`SmApiStitchIntoSolid` is in scope, but
untested).

## Tessellator `cad-tess` (plan 2.1–2.8)

`crates/cad-tess` meshes the neutral B-rep (`cad_kernel::brep::Brep`) into a `cad_kernel::Mesh`:
pure Rust (dependency `spade` 2.15, MIT/Apache-2.0, pure Rust without OS dependencies, built
with `default-features = false`; Wasm size not measured here, no wasm32 target: spade's code is
about 100–200 KB of a release module by its size), no kernel code. `kernel-solidcore::tessellate`
(2.8) is `brep()` + `cad_tess::tessellate_with`: a face that cannot be meshed, or a mesh of
closed solids that is not watertight, is `KernelError::Failed` ("solidcore: tessellate: …",
exit 3), never a mesh with holes. So render, STL/3MF, section view, asserts and `measure` work
with `--kernel solidcore`. A switch to mesh SMLib's B-rep with `cad-tess` in the app was not
built (the daemon and the cache keys would need a kernel id per tessellator); the corpus example
and `diff-kernels --mesh` compare it instead.

| Step | How |
| --- | --- |
| 2.1 edges | each edge once, shared by both faces: line 1 segment (more where a face's normal turns along it by more than the angle step: points at equal turns, a twisted loft's rulings); circle uniform, `θ = min(2·acos(1 − tol/r), 20°)`; ellipse and B-spline: bisection at half the tolerance from the knot spans (chord at ¼, ½, ¾ and tangent turn), then thinned while the chord stays within the tolerance of the dense points (dense knots of a fitted sweep need no points) |
| 2.2 surfaces | point, first and second derivatives, unit normal for plane, cylinder, cone, sphere, torus (closed form, STEP parameterisation), revolution, extrusion, B-spline (Piegl & Tiller with rational derivatives); inversion closed form or Newton (Gauss-Newton where the Hessian is not positive; a parameter held at the end of its domain leaves the step to the other one alone, where the coupled step stopped short on a twisted surface's end), seeds from the previous point of a loop, else local minima of dense curve samples refined by golden section (both branches at a seam corner) or a surface grid; periods (analytic, closed curves, coinciding boundary rows) and poles (sphere poles, cone apex, revolution profile on the axis, collapsed B-spline rows) |
| UV loops | boundary points inverted, periodic parameters unwrapped along the loop (a seam used twice lands on both sides), a pole traversed on the face's side (the free parameter from the arriving to the leaving value, the direction from the face's orientation), inner loops shifted into the widest loop's period; a loop that wraps a period without a seam is an error (neither kernel writes one) |
| 2.3 planar faces | the loops projected into the plane, constrained Delaunay (spade) without inner points |
| 2.4 curved faces | grid lines by chord error and angle (analytic: from the radii; else from the normal part of the second derivative sampled per knot span (at least 8 pieces per direction), the twist `σuv · n` counting as curvature in each direction by its share (the face's median speeds: a cell as long as wide), spread over the cumulated need; curved in both directions: half the tolerance in each, so a cell's diagonal holds it; where a direction is straight over part of the face only, lines there at most 4 × the other direction's spacing, never denser than on its curved pieces), straight directions only near boundary runs across the grid (rows at 2 × the other direction's step over such a run and one row beyond, at 32 × elsewhere, a far step ending where the next run's rows begin); no ring about a pole nearer to it than the boundary's first point on a run along the rings (a corner blend tapering to a needle: the first ring goes through that point); grid points inside the loops (even-odd per row, a margin from the boundary); CDT in grid steps (`Warp`): a direction with lines for the chord maps its lines to 0, 1, 2, … (the arc length in between: dense knots), so Delaunay triangles are round for the chord error, long along a gently curved direction and short across a tight one (a torus with a large major radius: long along its circle); a straight direction with rows maps them half a step apart (bands between rows far apart cost no more than near ones), without rows its arc length squashed by 4 (Delaunay joins points along it, not across the curved one); inside by the parity of constraint edges crossed (winding number where loops cross); refinement while it adds points, at most 40 passes, each measuring only the triangles and inner edges that are new: the foot of a triangle centroid or inner edge midpoint on the surface farther than `tol` (where the surface folds under it, its normal at a corner against the one at the middle's parameters: also the surface point at its parameters from the triangle's plane, or along the ends' mean normal) → the midpoint of the triangle's longest splittable edge or the foot (second choice where the first lands on the boundary), an inner triangle facing against the surface → its centroid; never on or within 1e-6 (CDT coordinates) of a boundary edge, never within `tol / 20` of a vertex. Loops that cross in (u, v) (polylines of tangent edges at a cusp) are split at the crossing and the neighbour's triangle on that segment as well (watertight); a triangle whose corners coincide at the `f32` resolution of the output is dropped (its neighbours close the surface), and so are triangles that coincide at the output positions (a pair facing each other: a fin where both faces at a crescent's cusp meshed the same sliver; taking both keeps every edge's uses balanced). After the refinement every triangle is checked against the surface normal at its centroid (not where the surface turns over under it: `f02`'s folding sweep); a fold, or a triangle more than 60° off the normal at a corner, is repaired by flipping an inner edge where neither new triangle folds, both lie better along the surface and neither they nor the new edge leave the surface by more than `tol` (or than the old pair did), after which the deviation is measured again on the final triangles; folds left make `tessellate` fail ("face N: K triangles facing against the surface at tolerance T mm"), never exported. Budget: 2 000 000 triangles per face and in all (a 100 MB STL; a guard against runaway, not a quality limit), checked on the grid, after the CDT and while inserting |
| 2.5 output | positions (shared edge points: identical), normals from the surface (`same_sense`; a hair inside the face at a seam with a crease; a pole: the limit from the inside), seam copies one vertex unless the normals differ, `tri_tags`, `tri_faces` (the B-rep's face ids), every edge's polyline as `edge_segments`/`edge_ids` (the B-rep's edge index = `EdgeId`; with the points where crossing loops split a segment) |
| 2.6 checks | `cad_tess::check(&mesh)`: vertices welded by position, every edge used twice in opposite directions (boundary, non-manifold, misoriented counts), triangles with the corners of another one (`duplicate_triangles`, either orientation), a closed consistent mesh with a connected piece of negative volume outside the others is `inside_out` (a void inside its body is not), positions with NaN/inf are counted (`non_finite`; none of these is watertight), degenerate triangles, volume and area; `kernel-solidcore::tessellate` and `diff-kernels --mesh` use `MeshCheck::watertight`; `MeshCheck::volume_ok(exact, tol)`: \|V − exact\| ≤ tol · area |

Tests: `crates/cad-tess/tests/kernels.rs` (one case per surface kind and tricky situation from
SMLib's and, with `--features solidcore`, solidcore's B-rep: watertight, outward, deviation and
volume within tolerance, every face and edge present) and unit tests per module.

Acceptance run: `cargo run -p cad-tess --example corpus [--features solidcore] -- --kernel both
[--tol t]` meshes every root part of the `diff-kernels` corpus (101 files) from each kernel's
B-rep and compares with SMLib's own `tessellate` (debug build, cad-tess at opt-level 2; SMLib's
own time measured in the same run). Tolerances the app uses: 0.05 display (`DEFAULT_TOL`, or
the node's `$tol`), 10 × that while dragging (`coarse_factor`), 0.01 STL/3MF and part export,
`clamp(2e-4 · size, 0.001, 0.02)` for the assert checks, the display tolerance for `measure`.

| tol | kernel | watertight | \|V − exact\| / (tol · area): median, worst | largest deviation (refinement's measure) | triangles (SMLib's own) | time (SMLib's own) |
| --- | --- | --- | --- | --- | --- | --- |
| 0.5 | SMLib B-rep | 100 / 100 | 0.007, 0.28 (`bottle`) | 0.49 | 98 k (150 k) | 1.5 s (6.9 s) |
| 0.5 | solidcore B-rep | 100 / 100 | 0.007, 0.28 | 0.49 | 96 k | 1.4 s |
| 0.05 | SMLib B-rep | 100 / 100 | 0.037, 0.52 (`02_sphere_variants`) | 0.050 | 178 k (272 k) | 3.0 s (12.7 s) |
| 0.05 | solidcore B-rep | 100 / 100 | 0.035, 0.52 | 0.050 | 175 k | 3.0 s |
| 0.01 | SMLib B-rep | 100 / 100 | 0.042, 0.59 (`f11`) | 0.010 | 618 k (708 k) | 10.8 s (24.8 s) |
| 0.01 | solidcore B-rep | 100 / 100 | 0.038, 0.54 | 0.010 | 608 k | 10.1 s |

After the findings on task G (rows over every run, fold check and repair, rings about poles,
fins; 100 files with a root part, `f03` has none; "watertight" now also means no duplicate
triangles, no folds beyond the forced ones, `strict`): fewer triangles than SMLib's own in 79 /
80 / 70 files at 0.5 / 0.05 / 0.01; forced folds only at 0.5 (`planter` 24); the times were
taken with three other runs on the machine (before task G, quieter: 1.3 / 2.3 / 8.9 s).
Triangles more than 60° off a corner's normal (the tester's measure, solidcore's B-rep): 212 /
692 at 0.05 / 0.01, all in `f02` (its folding sweep) but 4 in `f12_shell_loft_square_circle`;
231 at 0.5 (`f02` 97, `planter` 72 on its rim, `mug` 58: features narrower than the tolerance).

After the findings on task F (anisotropic CDT, rows of straight directions); before them 232 k /
229 k triangles in 3.7 s / 3.5 s at 0.05 and 849 k / 837 k in 17.8 s / 17.0 s at 0.01 (same
machine, same run conditions). Fewer triangles than SMLib's own in 79 / 80 / 70 of 101 files at
0.5 / 0.05 / 0.01; most above it: `f12_shell_loft_square_circle` (25 k against 5.7 k at 0.01:
a twisted loft, where SMLib's mesh misses the tolerance inside its triangles), `heat_sink`
(3.4 k against 2.2 k), `f13_shell_fillet` (344 against 164: rows near the mitred fillet ends).

The times vary with the load of the machine by up to 1.5 × between runs (the same code measured
14.7 s against SMLib's 25.6 s at 0.01 in a quieter run); compare within a row. Since task E's
findings the triangles near the tolerance are measured at three more points (a little slower).

Before the fine-tolerance work (2.4 above, task F): at 0.05 255 k triangles in 7.5 s; at 0.01
11 files kept triangles above the tolerance (worst 0.148 mm on `mug`), 996 k triangles in 63 s.
The cause was the refinement in bands of ruled surfaces (a revolution with a straight profile,
a cylinder between holes): a point put into such a band was joined by Delaunay to neighbours
far along it, so each fix made the next long edge; the passes ran out. Squashing the straight
direction in the CDT, the grid floor and passes that measure only what changed fixed it.
Independently checked by sampling (`cad_tess::surface_distance`, 7 points per triangle, the
chord error without the B-rep's own vertex gap): the 14 files that had problems (the 11 above,
`planter`, `heat_sink`, `02_sphere_variants`) hold 0.5, 0.05, 0.02 and 0.01 everywhere, 5 of them
also 0.002 (the assert tolerance of small models). SMLib's own meshes are not watertight by position for 6, 4 and
6 files at 0.5, 0.05 and 0.01 (`asserts`, `variable_radius`, `f02`, …) and hold the tolerance
only along triangle edges (plan 2.7). At the end of task F (before the tester's findings): fewer triangles than SMLib in 77 / 67 / 59 of 101 files at
0.5 / 0.05 / 0.01; more where the tolerance is held inside long triangles on fillets, tori and
mitres (`f13_shell_fillet` 6 × at 0.01, `f08` tori 3.9 ×, `heat_sink` 2.8 ×) and on revolutions
(`planter` 2.8 ×). Slowest at 0.01: `mug` 2.8 s, `planter` 2.5 s (debug).

### Mesh comparison (plan 2.7)

`mcad diff-kernels --mesh [--mesh-tol MM]` (`crates/cad-cli/src/diff/mesh.rs`) meshes each file's
result with both kernels' `tessellate` and measures per kernel the triangles, watertightness,
`|V − exact| / (tol · area)` against `mass_props`, the distance to the exact surfaces of the
kernel's own B-rep (`cad_tess::surface_distance`: 7 points per triangle projected onto the
surface of its face; the chord error is that distance minus the triangle corners' own distance,
which is the B-rep's where an edge lies off its face; a corner off the surface is inverted again
from its neighbours' parameters, since on a surface that folds over itself, `f02`'s sweep, the
unseeded inversion landed on another branch: 0.46 / 0.57 mm in solidcore's / SMLib's mesh, taken
for a gap of the B-rep until task G, now 4e-6 mm) and the time; between the kernels the sampled
distance in both directions (`cad_tess::mesh_distance`, a bounding-volume hierarchy of the
triangles) and the Hausdorff distance. The verdict per file looks at solidcore's mesh against
the exact surface (watertight and outwards, volume within `tol · area`, chord error ≤
1.1 · tol), not at the distance to SMLib, whose mesh misses the tolerance inside its triangles;
and at its vertices against solidcore's own B-rep, whatever the root nodes' verdict: a vertex
inside a face within `tol` of its surface (`inner_vertex_gap`; a split where loops cross at a
cusp is a point of the edge polylines, since 2026-11-04), a vertex on a face's boundary within `tol` of the B-rep's edges
(`edge_vertex_gap`; the edges of that face, sampled to a chord of 1e-4 mm), however far the edge
lies off the face; no allowance from SMLib (its chord error, 0.24 mm on the twisted extrusion,
would excuse displaced vertices). Where the root nodes are `same`, the distance to SMLib's mesh
beyond SMLib's own distance to its surfaces at each sample (`cad_tess::mesh_excess` with
per-triangle allowances, JSON `hausdorff_excess`) must stay within 2.1 · `tol` + 1e-4 of the
size: only SMLib's distance counts (solidcore's own grows with any displacement of its mesh, see
the findings on task F), and only where it is (before task G SMLib's largest anywhere: `f02`'s
0.57 mm let any solidcore vertex up to 0.62 mm pass anywhere).

Corpus (`scripts/ci/solidcore.sh`, tol 0.05, debug build, after the findings on task G): nodes
same 2596, unsupported 3, blocked 3, differ 2, failed 0 (unchanged); meshes of 100 results `ok`
without notes, 1 without a B-rep result; watertight SMLib 97, solidcore 100; chord error max
SMLib 0.242 mm, solidcore 0.050; triangles SMLib 272 k, solidcore 175 k; time SMLib 12.3 s,
solidcore 15.7 s of which 13.2 s are `brep` (the conversion runs unoptimised in a debug build:
`kernel-solidcore` is a workspace crate; `cad-tess` itself about 2.5 s). Hausdorff distance
3.07 mm in `variable_radius` and 2.6 mm in `f11`, whose roots differ (unsupported fillet; SMLib
fails); where the roots are the same, within the limit everywhere.

After the findings on task H (2026-11-01, same corpus, debug build): at 0.05 the same figures
(100 `ok`, chord max 0.0500, 175 k triangles; time SMLib 13.3 s, solidcore 16.3 s of which
14.1 s `brep`); at 0.01 (`--mesh-tol 0.01`) 99 `ok`, 1 `differ` (`f02`, see "Known points for
phase 5"), 1 without a B-rep result; watertight SMLib 95, solidcore 100; chord error max SMLib
0.045 mm, solidcore 0.0100 (was 0.0136 in `f12` before the flips kept the tolerance); triangles
SMLib 708 k, solidcore 608 k; time SMLib 24.9 s, solidcore 26.0 s (14.5 s `brep`).

After the open points of the handover (2026-11-04, pin `f4f05b1`, another machine with 4
cores; in brackets the same corpus with the tessellator before these fixes, measured there):
at 0.05 100 `ok`, solidcore watertight 100, chord max 0.0500, 177.0 k triangles (175.0 k),
Hausdorff excess max 0.0499 (0.0657), time SMLib 7.9 s, solidcore 8.8 s (8.6 s) of which 6.8 s
`brep`; at 0.01 100 `ok` (99, `f02` `differ`), watertight SMLib 95, solidcore 100, chord max
0.0100, 609.0 k triangles (607.5 k), Hausdorff excess max 0.0100 (0.0311), time SMLib 14.8 s,
solidcore 13.9 s (13.1 s). The fixes: the grid counts a surface's twist (`twist = 7200` at
0.05), straight edges get points where a face's normal turns along them, Newton's foot at the
end of a domain, inner vertices without excuse near edges (split points in the edge polylines),
folds of the surface measured by the refinement (`f02`), edges under forced folds discretised
again (`planter` at 0.5: 24 → 0); see "Known points for phase 5".

## Known points for phase 5

Open at the end of phase 2 (2026-11-01, after the findings on task H; rows marked fixed on
2026-11-04). None is a silent wrong result in the corpus; each is a limitation, a check that could be stricter, or an SMLib weakness
that goes away with SMLib.

| Point | State |
| --- | --- |
| variable-radius fillet with different radius laws on several edges (`variable_radius`: the 3 `unsupported` and 3 `blocked` nodes) | upstream: solidcore's `FilletSession` takes one law for all edges, the executive's per-edge laws (`var_rad_laws`) are not public. `Unsupported`, never wrong |
| the 2 `differ` nodes | `sketch_cable_duct`: solidcore's union of three extrusions loses faces (phase 3, Booleans upstream); `f11`: SMLib's Boolean fails, solidcore's not (a gain) |
| `f02_union_cylinder_sweep` at 0.01: `diff-kernels --mesh` `differ`, distance to SMLib's mesh beyond SMLib's own 0.0311 mm against the limit 0.0291 (at 0.05 `ok`) | **fixed 2026-11-04**: solidcore's mesh was off. The B-reps agree to 7e-14; the point of SMLib's mesh is on the exact surface but 0.032 from solidcore's mesh: its triangles were within 0.010 of their nearest surface points but spanned folds of the sweep (the tube's profile turns over in the bend) and missed the tips (the refinement measured mesh → surface only). Where the surface's normal turns against itself under a triangle or inner edge, the refinement also measures the surface point at its parameters from it: excess 0.0099 at 0.01, `ok` |
| a vertex inside a face that lies off its surface was excused up to its distance from the face's B-rep edges (tester H finding 4) | **fixed 2026-11-04**: inner vertices count with their distance from the surface alone; split points of crossing loops are points of the edge polylines (`edge_segments`) and checked as edge points. Corners are inverted from a corner of the same triangle on the surface first and re-seeded until nothing improves (a B-spline of 20 turns: 7.08 mm false gaps). An inner vertex next to an edge 0.3 mm off its surface, moved onto the edge: 0.3 (was 9e-5) |
| line contacts of staircase-like outlines (tester H finding 6) | contact kind and contact curve come from the mesh there; not analysed further in this round |
| forced folds: a fold on a boundary edge whose chord sags more than half the triangle's height above it (`Stats::forced_folds`) was kept | **fixed 2026-11-04**: such edges are discretised again with half the tolerance (up to 3 rounds, the whole mesh made anew: watertight); `planter` at 0.5: 24 → 0. A fold left after the rounds is still forced, not an error |
| SMLib only (gone with SMLib, phase 5.3): an island in a void meshed inside out; a ball in a seat 0.05 larger reported as line contact at distance 0 (solidcore: gap); a wall below one ulp | nothing to do in mcad |
| `linear_extrude(height = 300, twist = 7200) square([30, 2], center = true)` (`twist_many`) failed loudly at 0.05 | **fixed 2026-11-04**: not the passes but the grid: the ruled sides' straight direction is twisted (`σuv · n`), was taken for straight (rows only, squashed in the CDT), and Delaunay kept joining points across bands 7.5 mm wide; the refinement cycled. The twist counts as curvature now (each direction its share by the face's median speeds, at least 8 sample pieces per direction), and a straight edge along which a face's normal turns gets points by that turn (a twisted loft's rulings: 180° along one line). 0.05: 67.9 k triangles, chord 0.0437; 0.01: 344 k, 0.0100 |
| performance: evaluation about 1.5–1.8 × SMLib in a debug build (the adapter's Boolean checks, plan 3.4); in `diff-kernels --mesh` the B-rep conversion is most of solidcore's mesh time (14.1 of 16.3 s at 0.05: `kernel-solidcore` is a workspace crate and runs unoptimised in a debug build; `cad-tess` itself about 2.5 s) | measure again in a release build for the phase 5 gate (performance factor) |

## Pin routine (bumping solidcore)

solidcore is 0.0.x and changes by thousands of commits a week: the pin moves only on purpose.

1. Measure `main`: `SMLIB_PREBUILT=smlib/build-native scripts/ci/solidcore.sh target/sc-main`
   (or `SOLIDCORE_REV=<commit>`). It rewrites the pin in the working tree and builds against it.
   A build failure is an API break: fix `crates/kernel-solidcore/src/lib.rs` (solidcore's
   `crates/solidcore/tests/*.rs` show the intended use; note the change under "Status").
2. Compare `target/sc-main/summary.md` with the last row of the comparison table: no new
   `failed`, no node that was `same` now `differ` (`diff-kernels.json`, per node). A regression
   upstream: minimal reproducer and issue; keep the old pin.
3. Run the adapter tests: `cargo test -p kernel-solidcore --features solidcore`,
   `cargo test -p cad-cli --no-default-features --features solidcore` (also guards the numbers
   in `diff_kernels_on_examples`; adjust its thresholds to the new figures, a bit below them).
4. Bump the pin, all in one commit:
   - `crates/kernel-solidcore/Cargo.toml`: `rev = "<full hash>"` and the comment with date;
   - `crates/kernel-solidcore/src/lib.rs`: `SOLIDCORE_REV` and `KERNEL_ID` (`solidcore-<7 chars>`,
     part of every cache key) and the state described in the crate doc;
   - `Cargo.lock` (follows from the build: `cargo update -p solidcore` is not needed when the
     `rev` changes);
   - this file: "As of", previous pins under "Status", a new row in the comparison table;
     the brief in `docs/solidcore-plan.md` if a planned item became possible.
5. Restore the working tree if you did not bump: `git checkout -- crates/kernel-solidcore Cargo.lock`.

## Acceptance

`mcad diff-kernels <dateien…> [--json]` (`crates/cad-cli/src/diff.rs`) evaluates each file with
SMLib and with solidcore (`include`/`use`/`import` relative to the file) and compares every
node evaluated by SMLib:

- status, volume (rules below), number of faces and edges of the node result (exactly equal),
  selected edges (fillet/chamfer: count and end points, 1e-6 of the model size, in any order);
- volume (plan 0.4; JSON `volume_check: {rule, tolerance, difference, exact, smlib_error,
  solidcore_error}` per node, `summary.volume_rules` counts the same nodes per rule). Both
  kernels integrate the volume over the faces; on curved faces SMLib is the less accurate one
  (cylinder r 5, h 10: SMLib −2.0e-7 relative to the exact volume, i.e. −1.6e-4 mm³, solidcore
  −5.7e-10, −4.5e-7 mm³; sphere r 5: SMLib −7.9e-7, −4.1e-4 mm³, solidcore +1.8e-7, +9.2e-5 mm³;
  SMLib's skins and sweeps are fitted less closely: twisted bar 8 × 8 × 20, exact 1280, SMLib
  +1.5e-5, solidcore +6e-7), so a fixed 1e-6 against SMLib counted solidcore's better value as a
  difference. The first rule that holds:

  | Rule | Condition |
  | --- | --- |
  | `relative` | \|SMLib − solidcore\| ≤ 1e-6 · \|SMLib\| |
  | `exact` | the exact volume E is known and \|solidcore − E\| / E ≤ max(\|SMLib − E\| / E, 1e-6): solidcore is at least as close to E as SMLib |
  | `operands` | E unknown (Boolean, …): \|SMLib − solidcore\| ≤ min(Σ (\|SMLib − Eᵢ\| + \|solidcore − Eᵢ\|), 1e-4 · max(\|SMLib\|, \|solidcore\|)) + 1e-6 · \|SMLib\| over the nearest descendants with an exact volume Eᵢ (the deviation of the operands carries over to the result; the cap since task D's tester: a 1 mm cube on a sphere of r 1000 has a budget of several mm³ against a volume of 0.5; largest need in the corpus 1.3e-5) |
  | `curved` | the result has a curved edge, \|SMLib − solidcore\| ≤ 3e-5 · \|SMLib\| and, if E is known, \|solidcore − E\| ≤ 3e-5 · E (integration and fit accuracy; largest difference in the corpus 1.9e-5, a loft; primitives and Booleans of them up to 4.3e-6) |
  | `blend` | SMLib blends edges (fillet, chamfer) or offsets faces (`thicken`, `shell`, `offset`) in the history of the node's shapes, \|SMLib − solidcore\| ≤ 8 % of the volume those operations changed (+ 4e-6 of the node if it is curved) where both neutral B-reps have the same surface and curve kinds **and** the same edge and face positions and orientation (`diff/brep.rs`, else `differ` with "blend: B-reps at other positions"), 5 % where the kinds differ (`curved` does not apply to such nodes) (both fit the blend and offset surfaces, SMLib less closely: see "Fillets and chamfers"; largest need in the corpus 3.7 % of the change, `key_rack`; `freeform_lid`'s thickened sheet: SMLib 3.3e-5 off a numerical integration of the offset volume, solidcore 1.6e-6). The volume alone missed a wrong radius: `cube(100, $fillet = 1)` with the radius × 1.05 in one kernel passed; with the positions × 1.02 is caught (OCCT: the blend geometry of both kernels agrees within 1.8e-12; SMLib's integration is off up to 7.7e-5, solidcore's up to 2.7e-6) |
  | otherwise | `differ` with the note `volume x vs y (exact E)` |

  Exact volumes (`crates/cad-cli/src/diff/exact.rs`, a kernel wrapper that carries them next to
  the shapes): box, sphere, cylinder and cone frustum, torus (R > r), extrusion of a polygon or
  circle (with twist and scale: A·|h|·∫₀¹ sx·sy dt, the twist keeps the area), revolution of a polygon or circle in the xz-plane (Pappus), and the
  affine image of each (volume × \|det\|). A node's exact volume is the sum over its parts (none
  if a part has none, or the node has a mesh). Faces and edges must still match exactly, so a
  Boolean that loses faces stays `differ` whatever its volume;
- verdict per node: `same`, `unsupported` (solidcore reports the operation or a query of
  its result as unsupported), `blocked` (differs only because an input node
  is unsupported; for `zone` the sibling body), `differ`, `failed` (solidcore fails,
  SMLib does not), `skipped`;
- "Abdeckung" (coverage): per operation (node label) same / nodes, unsupported, blocked, differ.

Exit code 0 only if all nodes of all files are `same`. The switch happens only when
`diff-kernels` ends with 0 over all examples (`tests/models/openscad-compat`, `tests/models/spec`,
`tests/models/everyday`, `tests/models/project-housing`, `tests/everyday/bugs`);
in addition, `tests/openscad-compat/compare.py` and `tests/step/verify.py` must still pass with solidcore
(`tessellate` exists since plan 2.8, `brep` since plan 1.11).

The test `diff_kernels_on_examples` (`crates/cad-cli/tests/cli.rs`, only with `--features
solidcore`) checks that the comparison runs over all 101 examples without a crash, that at
least 2585 nodes are the same, at most 5 unsupported, at most 5 differ and none fails, that
no node differs in its volume alone, that every volume rule is used (exact ≥ 110, operands ≥ 45,
curved ≤ 40, blend 40 to 100), that the queries need few copies (< 3000) and, with `--mesh`
(plan 2.7), that every result with a B-rep meshes with solidcore watertight, with a chord error
of at most 0.051 mm at tol 0.05 and a volume within `tol · area`; without the feature
`diff_kernels_needs_the_feature` checks the build hint. `crates/kernel-solidcore/tests/coverage.rs`
checks each operation individually (box: volume, area, centroid, bounds; transformation
and classification correct or `Unsupported`).

`scripts/ci/solidcore.sh` (by hand; there is no CI workflow) builds `mcad` with `--features
solidcore` against the latest solidcore `main` instead of the pin and runs `diff-kernels --mesh`
over the same examples; the figures go to `summary.md` (total, meshes and per operation) and
`diff-kernels.json` in the output directory. The script fails only if the build (API change in
solidcore) or `diff-kernels` itself fails, not because nodes are unsupported, blocked or differ: `SMLIB_PREBUILT=smlib/build-native
scripts/ci/solidcore.sh` (changes the pin in the working tree, afterwards `git checkout --
crates/kernel-solidcore Cargo.lock`).
