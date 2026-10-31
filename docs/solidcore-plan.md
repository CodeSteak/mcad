# Plan: switch to solidcore, no C++ in the build

As of 2026-10-30, solidcore pin `bba8d87`. Status and measurements: `docs/solidcore-migration.md`.

## Goal

| Today | Target |
| --- | --- |
| `kernel-smlib`: SMLib + SM_API (C++, fork + `smlib/patches/`), facade `smlib/facade/sc_facade.cpp` (3729 lines) | `kernel-solidcore` over solidcore (Rust, Apache-2.0) |
| Native: CMake build of SMLib, `SMLIB_PREBUILT`/`SMLIB_SRC` | `cargo build` only |
| Browser: `smlib.wasm` (Emscripten) next to the `cad-wasm` module, loaded in `web/src/engine/worker.ts`/`transport.ts` | one `wasm32-unknown-unknown` module |
| Tessellation in C++ (`SmApiTessellate` + refinement passes in `sc_tessellate`) | own tessellator in Rust (`cad-tess`) |

## Principles

- SMLib stays the reference until the switch. Every step is measured with `mcad diff-kernels`
  (later also mesh comparison, 2.7).
- Kernel-independent parts (tessellator, polyhedron construction, spline interpolation) work on
  the neutral types of `cad-kernel` (`brep::Brep`, `curve2`, `trimesh`). They can be validated
  against SMLib today, before solidcore is complete.
- solidcore defects get a minimal reproducer (Rust test against the solidcore API) and an issue
  upstream. A workaround in the adapter only with a comment and an entry in
  `docs/solidcore-migration.md`.
- solidcore is 0.0.x: pin a commit, bump deliberately (0.3).

## Phase 0: foundations (about 1 week)

Done 2026-10-31: all six items; results in `docs/solidcore-migration.md` (comparison table, "Pin
routine", "Performance", "Acceptance"). Measured for 0.6: `Inner::copy` was cheap, the gain came
from keeping solidcore's caches and the mass properties; the next performance item is the
volume check in the adapter's `transform` (two integrations per transform).

| # | Task | Where | Done when |
| --- | --- | --- | --- |
| 0.1 | Kernel selection at runtime: `--kernel smlib\|solidcore`, `MCAD_KERNEL`, for every command; the engine is already generic (`Session<K>`) | `crates/cad-cli/src/main.rs:411`, `daemon.rs`, `host.rs`, `set.rs`; manual `02-commands.md`, `17-troubleshooting.md` | `mcad render --kernel solidcore box.mcad` works or reports `Unsupported` per node |
| 0.2 | `diff-kernels` corpus: add `tests/models/everyday` (21), `tests/models/project-housing`, `tests/everyday/bugs` | `scripts/ci/solidcore.sh`, `crates/cad-cli/tests/cli.rs` | numbers for all models |
| 0.3 | Pin routine: run `scripts/ci/solidcore.sh` against `main`, fix API breaks, update the table in `solidcore-migration.md`, bump pin | `scripts/ci/solidcore.sh`, docs | checklist in `solidcore-migration.md` |
| 0.4 | Volume comparison: `diff-kernels` compares volumes relative 1e-6. On curved solids solidcore is closer to the exact value than SMLib (cylinder r 5, h 10: solidcore −4e-7, SMLib −1.6e-4; sphere r 5: +9e-5 against −4e-4). Compare against the exact volume where it is known (primitives), otherwise with a tolerance from the integration accuracy | `crates/cad-cli/src/diff.rs` | the 27 volume-only nodes are `same` or explained |
| 0.5 | Performance baseline: eval time and peak memory per example, SMLib against solidcore; cost of `Inner::copy` (a copy per query: `faces`, `classify`, `mass_props`) | `diff-kernels --json` (time per node), report in `docs/reports/` | table; decision on 0.6 |
| 0.6 | Fewer copies: read-only queries need `&mut Brep` only for caches. Keep one owned copy per shape (`RefCell<Brep>` in `ShapeData`) and query it in place | `crates/kernel-solidcore/src/lib.rs` | `faces`/`mass_props` without a copy per call |

## Phase 1: complete the adapter (3–5 weeks, partly in parallel with phase 2)

Nodes = unsupported nodes in `diff-kernels` at `bba8d87` (1438 nodes in 64 models).

| # | Kernel operation | solidcore entry point | Nodes | Effort | Notes |
| --- | --- | --- | ---: | --- | --- |
| 1.1 | `extrude`/`revolve`/`loft` with `Loop::Composite` (lines, arcs, B-splines) | `primitive::api_curves::{create_line_segment, create_arc}`, B-spline curve from `curve2::Segment::to_nurbs`, `sweep::api_brep::create_linear_sweep`/`create_rotational_sweep` | 42 | 2–3 d | biggest single item. **Done 2026-10-31**: lines as line segments, arcs as rational quadratic B-splines of the circular-arc form (pieces of at most 90°), B-splines as given, joints welded, all as the SMLib facade (`sc_sketch.cpp`); faces/edges as SMLib. 101 files: same 2110 → 2231, unsupported 180 → 129, blocked 287 → 216, differ 27 → 28 (`sketch_cable_duct`: the union of the extrusions loses faces, a Boolean) |
| 1.2 | `Loop::Spline` (closed interpolating spline) | interpolation in Rust (`cad_kernel::curve2::interpolate`) instead of `SmApiCreateCurveInterpPoints` (out of scope) | 2 | 1 d | same curve as SMLib? compare control points. **Done 2026-10-31**: solidcore ports `SmBSplineCurve::CreateInterpolatingCurve`, which `SmApiCreateCurveInterpPoints` calls; the adapter calls it as the facade does (closed: through the first point again; revolve: axis segments as lines, open runs between). Control points and knots equal SMLib's and `curve2::interpolate`'s to 4e-15 (unit test). `knob_mirror`: both `rotate_extrude` same; same 2231 → 2233, unsupported 129 → 128, blocked 216 → 211, differ 28 → 32 (the 4 `difference(fillet = 0.5)` nodes find no new edges without provenance, 1.9) |
| 1.3 | `fillet`, `chamfer`, `fillet_variable`, `law_stations` | `blend::fillet_executive::api::{fillet_edges, fillet_edges_per_edge, variable_radius_fillet, chamfer_fillet}` | 9 + blocked ones | 3–5 d | map `EdgeId` (index of `edges()`) to solidcore `EdgeId` in a stable order; zones depend on it |
| 1.4 | `sweep`, `sweep_path` | `create_pipe_sweep`, `create_sweep_along_planar_path`, `create_curve_sweep*` | 6 | 3–5 d | SMLib patch 0003 (`bUseProfileRefPoint`) has no counterpart in solidcore: move the profile in the adapter or propose upstream. **Done 2026-10-31**: solidcore has a counterpart now, `SweepOptions::profile_at_path_start` (solidcore#1450): the profile frame's origin lies at the path start, so it is the same as patch 0003 with that point. Curve sweep (`create_curve_sweep`) for straight and spline paths and tangent-continuous sketch paths, mitered `create_sweep_along_planar_path` for planar polylines and sketch paths with corners, the facade's profile frame and its refusals (non-planar polyline, off-centre profile along corners). All 19 sweep nodes same; same 2233 → 2262, unsupported 128 → 108, blocked 211 → 184, differ 32 → 50 (`union(fillet = …)` of a sweep finds no new edges without provenance, 1.9: `key_rack` 16, `f04`/`f15` 2, where SMLib's fillet fails) |
| 1.5 | `extrude` with twist/scale, `loft` | `sweep::api_surfaces::create_skin`, `create_ruled_surface` | 4 | 2–3 d | **Done 2026-10-31**: skin (`create_skin_primitive`) through the facade's sections (twist: one per 7.5°, bicubic; scale only: ruled), loft segment by segment or one curve per section as `sc_loft`; faces/edges as SMLib. same 2262 → 2280, unsupported 108 → 102, blocked 184 → 172, differ 50 (unchanged) |
| 1.6 | `nurbs_surface`, `thicken`, `shell`, `offset` | B-spline surface constructor, `offset::offset_executive::p2::{api_offset_brep_full, api_shell_brep_full}` | 2 + blocked | 3–5 d | `shell` stopped in `topology.brep_p3` at `c9de85d`; test again |
| 1.7 | `make_polyhedron` (also `hull`, `minkowski`, STL as B-rep) | planar faces + `heal` (`SmApiStitchIntoSolid`), or `step_brep` from a planar `step_import::Model`; face loops from `cad_kernel::trimesh` | 8 + hull/minkowski | 2–3 d | SmApiPoly* is out of scope; planar faces are enough |
| 1.8 | `transform` with shear/non-uniform matrix (`multmatrix`) | not ported (4×4 `SmBrep::Transform`) | 2 | open | options: surfaces to NURBS and map control points (exact for affine maps); or a clear error. Count uses first |
| 1.9 | Provenance: face tags through booleans, fillets, sweeps | solidcore attributes (`geom::attribute::AttributeStore`, `Brep::add_attribute`) | – | 3–5 d | colours (3MF per colour), zones and `assert_*` depend on it. Check: `tests/color-review`, volume per colour |
| 1.10 | `EdgeInfo::smooth` (seam, pole, tangent-continuous), `face_tags` of edges | edge uses, face normals at the edge | – | 1–2 d | fillet/zone selection |
| 1.11 | `Kernel::brep` (neutral `brep::Brep`) | own mapping of solidcore topology/geometry; `step_brep::brep_to_model` as reference | – | 3–4 d | unlocks measuring (`cad-measure`), STEP (`cad-step`) and the tessellator; conventions as in `crates/kernel-smlib/src/brep.rs` |
| 1.12 | `shape_bytes`, `set_draft`, `export` | – | – | 1 d | |

Done when: `diff-kernels` has no `unsupported` left except 1.8; `tests/step/verify.py` and `mcad measure`
tests pass with `--kernel solidcore` (tessellation still from phase 2).

## Phase 2: own tessellator `cad-tess` (4–6 weeks)

Not part of solidcore and not planned there (`plan/closure.md`, "Out of scope": `SmApiTessellate`,
`SmApiPoly*`). Usable from solidcore: `tessellate_boundaries` (`SmApiTessellateBoundaries`, edge
polylines) and the crate `nurbs-grid-mesh` (Apache-2.0, no dependencies, Wasm: trimmed NURBS
faces as a grid in (u, v), planar faces with holes).

**Input is the neutral `cad_kernel::brep::Brep`** (from `Kernel::brep`, 1.11), not the solidcore
B-rep. So the tessellator works for both kernels and can be developed and compared against SMLib
before phase 1 is done. The neutral B-rep has no UV trim curves, so (u, v) of boundary points
comes from point inversion.

| # | Step | Content | Effort |
| --- | --- | --- | --- |
| 2.1 | Edges | Each edge discretised once, shared by both faces (watertight). Line: 1 segment; circle/ellipse: step `θ = 2·acos(1 − tol/r)`; B-spline: adaptive bisection on chord error and tangent angle | 3 d |
| 2.2 | Surface evaluation and inversion | Point, derivatives, normal for `Plane`, `Cylinder`, `Cone`, `Sphere`, `Torus` (closed form), `Revolution`, `Extrusion`, `BSpline` (de Boor, Newton with a grid start). Periodic seams, poles (`Loop::Vertex`, sphere/cone apex) | 4–5 d |
| 2.3 | Planar faces | Project the loops, polygon with holes: ear clipping with hole bridges, or CDT (crate `spade` + `robust`, MIT/Apache; check licence and Wasm size) | 2–3 d |
| 2.4 | Curved faces | UV grid by chord error (analytic: from the radii; NURBS: second derivatives / control net), trim against the UV loops, CDT with the edge points as constrained edges, refine where the 3D deviation at triangle/edge midpoints exceeds `tol`, anisotropic scaling of u/v. Dense NURBS faces: `nurbs-grid-mesh` | 8–10 d |
| 2.5 | Output | `Mesh`: normals exact from the surface (orientation by `same_sense`), `tri_tags`, `tri_faces`, `edge_segments`, `edge_ids`; coarse/fine by `tol` (plan 7.2) | 2 d |
| 2.6 | Checks | Every inner edge used twice, consistent orientation, no degenerate triangles; mesh volume against exact volume (`mass_props`) within `tol · area` | 2 d |
| 2.7 | Comparison with SMLib | `diff-kernels --mesh` or a test: mesh volume, Hausdorff distance to the SMLib mesh, watertightness, triangle count, time. SMLib's mesh misses the tolerance in triangle centres on twisted free-form faces (comment in `sc_tessellate`): compare against the exact surface, not only against SMLib | 3 d |
| 2.8 | Integration | `kernel-solidcore::tessellate` = `brep()` + `cad-tess`. Optional: `kernel-smlib` with `cad-tess` behind a switch, for a direct comparison in the UI | 1–2 d |

Done when: all models mesh watertight; `tests/openscad-compat/compare.py`, `tests/everyday/check.sh`,
STL/3MF export, section view and render snapshots pass with `--kernel solidcore`.

## Phase 3: booleans (depends on upstream)

24 nodes differ in topology and volume (0.1 % to 95 %) without an error: `hole_plate`, `vent_grille`,
`chamfers`, `grip_shell`, `hull`/`minkowski` (the last ones also via 1.7).

| # | Task |
| --- | --- |
| 3.1 | Minimal reproducers per case (`.mcad` + Rust test on the solidcore API), with the SMLib result as expectation |
| 3.2 | Check against unpatched upstream C++ (solidcore's `difftest` uses upstream usd-brep, our SMLib carries `smlib/patches/` and fork fixes): a difference can come from our patches |
| 3.3 | Issues in `tangency-labs/solidcore`; bump the pin when fixed (0.3) |
| 3.4 | Until then, a guard in the adapter: after a boolean check `brep_is_manifold_solid` and volume plausibility (union ≥ max operand, difference ≤ first operand, intersection ≤ min operand); otherwise `Failed` instead of a wrong solid |

## Phase 4: Wasm and UI (1–2 weeks)

| # | Task | Where |
| --- | --- | --- |
| 4.1 | `cad-wasm` over `kernel-solidcore` (`wasm32-unknown-unknown`), stack 8 MiB (`-C link-arg=-zstack-size=8388608`, no guard page on Wasm) | `crates/cad-wasm`, `.cargo/config.toml`, `scripts/ci/web.sh` |
| 4.2 | Worker without `smlib.wasm`: drop loading and transport of the second module | `web/src/engine/worker.ts`, `transport.ts` |
| 4.3 | Measure module size (after `wasm-opt`) and eval time in the browser against today | report |
| 4.4 | Desktop app with kernel selection | `crates/cad-desktop` |

## Phase 5: switch and removal (about 1 week)

**Gate** (all of them):

- `diff-kernels` over all models: no `differ` with a topology difference, no `failed`;
  `unsupported` only for documented exceptions (1.8).
- `compare.py`, `tests/everyday/check.sh`, `tests/step/verify.py`, `tests/color-review` pass with solidcore.
- Performance (0.5, 4.3) not worse than SMLib by more than an agreed factor.
- Browser build runs (phase 4).

| # | Task |
| --- | --- |
| 5.1 | Default kernel solidcore; SMLib remains as feature `smlib` for one release (comparison, fallback) |
| 5.2 | Freeze reference data before removal: volume, faces, edges, selected edges per node from the last SMLib run as `tests/kernel-reference.json`; a test compares solidcore against it (replaces `diff-kernels` once SMLib is gone) |
| 5.3 | Remove: `crates/kernel-smlib`, `smlib/` (facade, patches, scripts), `third_party/smlib`, Emscripten/`make emsdk`, Makefile targets `smlib*`, SMLib steps in `scripts/ci/{rust,web,desktop,compat}.sh`, `kernel-smlib` dev-dependencies of `cad-measure`, `cad-render`, `cad-session`, `cad-engine`, `cad-desktop` |
| 5.4 | Docs: `README.md`, `docs/development.md`, `docs/technical-plan.md`, `CLAUDE.md` (building section), manual (`mcad diff-kernels`: remove or keep only with feature `smlib`) |

## Order and duration

```
week     1   2   3   4   5   6   7   8   9  10
phase 0  ███
phase 1      ████████████████
phase 2      ████████████████████████
phase 3      ·······(upstream)········███
phase 4                          ██████
phase 5                                  ███
```

About 2–3 months with phases 1 and 2 in parallel. Uncertain: phase 3 (upstream), 1.8 (shear), the
effort of 2.4.

## Risks

| Risk | Measure |
| --- | --- |
| solidcore API changes (0.0.x, thousands of commits per week) | pin, adapter tests, deliberate bumps (0.3) |
| Booleans silently wrong | guard 3.4, `diff-kernels` |
| Performance unknown | measure early (0.5), fewer copies (0.6) |
| Tessellator takes longer | start early, independent of solidcore (input: neutral B-rep) |
| Stack overflow in Wasm without a guard page | 8 MiB stack (4.1), recursion limits of solidcore |
| Continuity of solidcore (one external project) | Apache-2.0: vendoring or fork possible |
