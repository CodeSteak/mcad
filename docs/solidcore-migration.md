# Migration to solidcore (plan 2.3, M8)

Work plan up to the switch: [`solidcore-plan.md`](solidcore-plan.md).

As of: 2026-10-30 · solidcore `main` @ `bba8d87` (2026-10-30 18:31 UTC, tangency-labs/solidcore, Apache-2.0); the operation table "What works today" comes from the run with `c9de85d`, the figures for `d4940b6` and `bba8d87` are in the comparison over the examples and in "State at `bba8d87`".

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
`SmApiCreateCurveInterpPoints` (interpolating spline curve, used here for `Loop::Spline`),
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
otherwise `Failed`. For now faces carry the tag of the generating operation (attribute propagation
for provenance is not wired up yet). The checks for `transform` and `classify`
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
| `bba8d87`, `include`/`use`/`import` resolved, unsupported warnings count (64 files as before) | 64 | 1462 | 1190 | 90 | 131 | 51 | 0 |
| `bba8d87`, corpus extended (plan 0.2) | 101 | 2604 | **1927** | 175 | 290 | 212 | 0 |

- Corpus (plan 0.2): up to then `diff-kernels` evaluated each file without a resolver, so
  `include`/`use` and `import("x.stl")` found nothing (an assembly gave 3 nodes instead of 60);
  it now resolves them relative to the file like the other commands. A node whose warning
  names an unsupported operation counts as `unsupported` (STL import: solidcore has no
  polyhedron, the mesh stays a mesh and the following `difference` failed). New per corpus
  (same / nodes): `everyday` 621 / 947, `project-housing` 20 / 33, `everyday/bugs` 96 / 162.

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
| face ids of a copy differ from the original (`face N not found`) | `faces` queries the copy only |
| `BrepClassifier` with `BrepIdentity::default()` puts every point outside | `BrepQuery::of_brep(..).point_3d_classify` on a copy; the self-test passes |

Remaining nodes:

| Kind | Nodes | Cause |
| --- | ---: | --- |
| differ: volume only (≤ 1e-4 relative, same faces/edges) | 27 | curved solids, about 1e-5 off |
| differ: faces/edges and volume (0.1 % to 95 %) | 24 | booleans that lose faces (`hole_plate`, `vent_grille`, `chamfers`, `grip_shell`), `hull`/`minkowski` |
| unsupported: sketch profile in `extrude` | 42 | not wired here |
| unsupported: fillet/chamfer | 9 | API exists, not wired here |
| unsupported: polyhedron | 8 | out of scope of solidcore |
| unsupported: sweep, skin (twist/scale), shell, spline revolve, shear | 17 | not wired here or not ported |

## What is missing (in solidcore or here)

1. ~~Primitives other than the box~~ (works at `bba8d87`).
2. ~~A transformation that carries curves and faces along~~ (works at `bba8d87`); still missing:
   the 4×4 matrix variant (shear, e.g. `multmatrix`).
3. Booleans that keep all faces (see "State at `bba8d87`"); ~~point classification~~ (works).
4. ~~Public fillet/chamfer API~~ (exists at `bba8d87`); wire it up here.
5. Here: sketch profiles for `extrude`; wire up `create_skin` (loft, extrusion with twist),
   `create_pipe_sweep` (sweep), offset/shell; attributes for face tags (provenance);
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

## Acceptance

`mcad diff-kernels <dateien…> [--json]` (`crates/cad-cli/src/diff.rs`) evaluates each file with
SMLib and with solidcore (`include`/`use`/`import` relative to the file) and compares every
node evaluated by SMLib:

- status, volume (relative 1e-6), number of faces and edges of the node result,
  selected edges (fillet/chamfer: count and end points, 1e-6 of the model size);
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
solidcore`) checks that the comparison runs over all examples without a crash, that more
than 1100 nodes are the same, none fails, and differing cylinders, spheres, tori and solids of revolution differ only in volume; without the feature
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
