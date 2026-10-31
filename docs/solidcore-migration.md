# Migration to solidcore (plan 2.3, M8)

Work plan up to the switch: [`solidcore-plan.md`](solidcore-plan.md).

As of: 2026-10-31 (phase 0 of the plan done; phase 1: 1.1–1.5, 1.9, 1.10 and a cheap transform check done) · solidcore `main` @ `bba8d87` (2026-10-30 18:31 UTC, tangency-labs/solidcore, Apache-2.0); the operation table "What works today" comes from the run with `c9de85d`, the figures for `d4940b6` and `bba8d87` are in the comparison over the examples and in "State at `bba8d87`".

## Status

- **Crate `kernel-solidcore`** implements `cad_kernel::Kernel` via the public API of
  solidcore. The dependency is a git entry with a fixed commit
  (`crates/kernel-solidcore/Cargo.toml`, constant `SOLIDCORE_REV`, kernel ID
  `solidcore-bba8d87`). Previous pins: `3a6a50b` (2026-10-22), `c9de85d` (2026-10-23),
  `d4940b6` (2026-10-25). From `d4940b6` to `bba8d87` (3101 commits) `face_internal_point` takes
  `&mut Brep`. From
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

Result over the 101 files: 2056 of the 2081 compared nodes agree. The 25 others are differences
of the kernels, not of the mapping:

| Nodes | Cause |
| ---: | --- |
| 9 | `mirror` (`09_rotate_mirror`, `21_angle_bracket`, `knob_mirror`, `pipe_clamp`): solidcore's `SmBrep::Transform` turns every surface into a B-spline under a negative scale (`turn_to_nurbs`); SMLib's 4 × 4 overload keeps planes and cylinders |
| 6 | `phone_stand`: solidcore's fillet of an edge around a hole makes a B-spline surface (SMLib a surface of revolution) and B-spline edges instead of circles |
| 13 | tags of faces merged by a union of coplanar faces (`multicolor` letters, `rib_bracket`): the kernels keep another operand's tag |

STEP export goes through the neutral B-rep and works with solidcore: `tests/step/verify.py`
(OCCT reads, checks validity, closed shells, solids, faces, volume, area, bounds) passes 61 of
64 models with `MCAD_KERNEL=solidcore` (SMLib 64): `17_polyhedron` and `19_hex_nut` need
polyhedra (1.7), `asserts` stops at its asserts, which need the tessellation (phase 2).
`mcad measure` needs the tessellation as well (`MeasurePart.mesh`).

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
2. ~~A transformation that carries curves and faces along~~ (works at `bba8d87`); still missing:
   the 4×4 matrix variant (shear, e.g. `multmatrix`).
3. Booleans that keep all faces (see "State at `bba8d87`"); ~~point classification~~ (works).
4. ~~Public fillet/chamfer API~~ (exists at `bba8d87`; wired here, plan 1.3); still missing: a
   public per-edge radius law in `FilletSession` (the executive has `var_rad_laws` per edge).
5. Here: ~~sketch profiles for `extrude`; wire up `create_skin` (loft, extrusion with twist),
   sweeps~~ (plan 1.1–1.5, see "Profiles and sweeps"); offset/shell; ~~attributes for face tags (provenance)~~ (plan 1.9/1.10);
   `Kernel::brep` and STEP export via `step-brep`/`step-export`; Wasm build in the same module
   (stack 8 MiB, see solidcore README).
6. **Own tessellator** (not in solidcore, see below).

Estimate from the research (uncertain, depends on the units above and on the people who take
over units in solidcore): all primitives end to end in 1–2 weeks, Booleans in 2–4
weeks, fillets, sweeps and shell in 6–10 weeks.

## Tessellation: own tessellator for M8

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
  | `operands` | E unknown (Boolean, …): \|SMLib − solidcore\| ≤ Σ (\|SMLib − Eᵢ\| + \|solidcore − Eᵢ\|) + 1e-6 · \|SMLib\| over the nearest descendants with an exact volume Eᵢ (the deviation of the operands carries over to the result) |
  | `curved` | the result has a curved edge, \|SMLib − solidcore\| ≤ 3e-5 · \|SMLib\| and, if E is known, \|solidcore − E\| ≤ 3e-5 · E (integration and fit accuracy; largest difference in the corpus 1.9e-5, a loft; primitives and Booleans of them up to 4.3e-6) |
  | `blend` | SMLib blends edges (fillet, chamfer) at the node or below it, \|SMLib − solidcore\| ≤ 1e-4 · \|SMLib\| (both fit the blend surfaces, SMLib less closely: see "Fillets and chamfers"; largest difference in the corpus 6.5e-5) |
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
(for this `kernel-solidcore` still needs `tessellate` and `brep`).

The test `diff_kernels_on_examples` (`crates/cad-cli/tests/cli.rs`, only with `--features
solidcore`) checks that the comparison runs over all 101 examples without a crash, that at
least 2510 nodes are the same, at most 30 unsupported, at most 10 differ and none fails, that
no node differs in its volume alone, that every volume rule is used (exact ≥ 110, operands ≥ 45,
curved ≤ 85, blend 1 to 15) and that the queries need few copies (< 2500); without the feature
`diff_kernels_needs_the_feature` checks the build hint. `crates/kernel-solidcore/tests/coverage.rs`
checks each operation individually (box: volume, area, centroid, bounds; transformation
and classification correct or `Unsupported`).

`scripts/ci/solidcore.sh` (by hand; there is no CI workflow) builds `mcad` with `--features
solidcore` against the latest solidcore `main` instead of the pin and runs `diff-kernels` over
the same examples; the figures go to `summary.md` (total and per operation) and
`diff-kernels.json` in the output directory. The script fails only if the build (API change in
solidcore) or `diff-kernels` itself fails, not because nodes are unsupported, blocked or differ: `SMLIB_PREBUILT=smlib/build-native
scripts/ci/solidcore.sh` (changes the pin in the working tree, afterwards `git checkout --
crates/kernel-solidcore Cargo.lock`).
