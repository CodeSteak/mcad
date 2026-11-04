# Handover: solidcore migration, phases 0–2

State on 2026-11-04 (pin bump and the tessellator's open points 4, 5, 7, 8, 9; the state of
2026-11-01 otherwise). Read this first to resume the work; details and history are in
[`solidcore-plan.md`](solidcore-plan.md) (work plan, status per item) and
[`solidcore-migration.md`](solidcore-migration.md) (measurements, tester findings, rules of the
comparison).

## Where

| What | Value |
| --- | --- |
| Branch | `feature/solidcore-phase0-2` (pushed to `origin`; no pull request) |
| Head at handover | the commit that adds this file; the last code commits are `2280207` (flips keep the tolerance), `e2dc0b6` (rim contact), `d13c7d5` (edge sampling), `0a89606` (inside-out speed) on top of `e66d726` (end of tester round H). 2026-11-04 on branch `claude/tessellierung-subagent-hngono`: pin bump `a921b75`, then `1c7762e` (#8), `a5ceafd` (#9), `4c51fd3` (#5), `56ca505` (#4), `c6c23cf` (#7) |
| solidcore pin | `f4f05b147551201e2a7b44a34fbc41ddc0fc6578` (2026-11-04, crate 1.0.1; was `bba8d87` of 2026-10-30, bumped without API changes and with the same verdicts), `crates/kernel-solidcore/Cargo.toml`, `SOLIDCORE_REV` / `KERNEL_ID` in `crates/kernel-solidcore/src/lib.rs` |
| Goal | replace SMLib (C++, `crates/kernel-smlib`, facade `smlib/facade/sc_facade.cpp`) by solidcore (Rust) and drop all C++ from the build |

## What phases 0–2 delivered

| Part | Where | What |
| --- | --- | --- |
| Kernel selection | `crates/cad-cli/src/kernel.rs` (`AnyKernel`), `--kernel smlib\|solidcore`, `MCAD_KERNEL` | every command can run on solidcore; without the feature exit 2 with a build hint; the daemon keeps SMLib, solidcore runs locally |
| Adapter | `crates/kernel-solidcore/src/` (`lib.rs`, `boolean.rs`, `brep.rs`, `offset.rs`, `polyhedron.rs`; tests in `tests/`) | `cad_kernel::Kernel` over solidcore's public API: primitives, transforms (incl. shear by SVD), sketches/extrude/revolve/loft/twist, sweeps, fillets/chamfers (one radius law per session), shell/thicken/offset, NURBS surface, polyhedra, Booleans with the facade's checks (`checked_boolean`), face tags as attributes, neutral B-rep (`Kernel::brep`), `tessellate` = `brep` + `cad-tess` |
| Tessellator | `crates/cad-tess` (pure Rust, `spade` CDT) | meshes the neutral `cad_kernel::brep::Brep` of either kernel: edges once per edge (watertight), surfaces/inversion, anisotropic grid + CDT + refinement, fold check and repair by flips (within `tol`), checks (`check`, `MeshCheck::watertight`, `volume_ok`), `surface_distance`, `mesh_distance`, `mesh_excess` |
| Comparison | `mcad diff-kernels FILE… [--json] [--brep] [--mesh [--mesh-tol MM]]`, `crates/cad-cli/src/diff.rs`, `diff/{exact,brep,mesh}.rs` | per node: status, volume (rules `relative`, `exact`, `operands`, `curved`, `blend`), faces, edges, selected edges; `--brep` the neutral B-reps; `--mesh` per file: watertight, volume vs `mass_props`, chord error vs exact surfaces, vertices vs solidcore's own B-rep, Hausdorff excess over SMLib's mesh |
| Asserts | `crates/cad-session/src/asserts.rs` | contact kinds independent of the mesher where surfaces cannot coincide; tolerance over the common extent; balls in seats; planes at an angle only face contact if within `tol` over the whole overlap |
| Manual | `manuell/02-commands.md` (`--kernel`, `diff-kernels`), `17-troubleshooting.md` (solidcore and tessellator messages), `19-asserts.md` (contact rules) | |

## Numbers

Re-measured 2026-11-01 at the handover head (debug build, `MCAD_KERNEL=solidcore` where it
applies). Tester round H measured the same node figures at `e66d726`. The mesh rows: 2026-11-04
after the tessellator fixes, on another machine (4 cores; times not comparable with the
earlier ones, so the state before the fixes, measured there at the new pin, is given in
brackets).

| Measurement | Result |
| --- | --- |
| `diff-kernels` nodes (101 files, 2604 nodes; `scripts/ci/solidcore.sh`) | same 2596, unsupported 3, blocked 3, differ 2, failed 0 |
| meshes at 0.05 (`--mesh`) | 100 `ok`, 1 skipped (`f03`, no B-rep part); solidcore watertight 100, chord max 0.0500 mm, 177.0 k triangles (175.0 k); SMLib watertight 97, chord max 0.242, 272 k; largest Hausdorff excess where the roots are `same` 0.0499 (0.0657); time SMLib 7.9 s, solidcore 8.8 s (8.6 s), 6.8 s of it the B-rep conversion |
| meshes at 0.01 (`--mesh --mesh-tol 0.01`) | 100 `ok` (99, `f02` `differ`), 1 skipped; solidcore watertight 100, chord max 0.0100; 609.0 k triangles (607.5 k; SMLib 708 k, watertight 95); largest Hausdorff excess 0.0100 (0.0311, `f02`); time SMLib 14.8 s, solidcore 13.9 s (13.1 s) |
| `tests/openscad-compat/compare.py` | 36 / 36 |
| `tests/everyday/check.sh` | 19 models, 0 with problems |
| STL/3MF, section view, render snapshots | met at task F (`docs/solidcore-plan.md`, phase 2 status), not re-run |
| tests | `cargo test -p cad-tess --features solidcore`, `cargo test -p cad-session`, `cargo test -p cad-cli --no-default-features [--features solidcore]` (incl. `diff_kernels_on_examples`, `usage`): green |

Phase 2 "Done when": met (plan, "Phase 2 closed 2026-11-01"). Phase 1 "Done when": not met only
for `variable_radius` (upstream API).

## Build and test

| Topic | How |
| --- | --- |
| SMLib (reference kernel, still needed) | `export SMLIB_PREBUILT=$PWD/smlib/build-native` (prebuilt); without it build from `third_party/smlib` (`SMLIB_SRC`, `Makefile`, `smlib/fetch.sh`) |
| headless (no WebKitGTK) | `cargo build -p cad-cli --no-default-features --features solidcore`; workspace commands with `--exclude cad-app --exclude cad-desktop` |
| feature `solidcore` | opt-in on `kernel-solidcore` and `cad-cli`; without it every solidcore operation is `Unsupported` and `diff-kernels` exits 2. From an empty target dir about 10–15 min, then incremental |
| adapter tests | `cargo test -p kernel-solidcore --features solidcore` |
| tessellator tests | `cargo test -p cad-tess --features solidcore` (about 10 min in debug; both kernels' B-reps) |
| CLI tests | `cargo test -p cad-cli --no-default-features --features solidcore` (`diff_kernels_on_examples` guards the numbers, about 15 min); `cargo test -p cad-cli usage` checks the manual |
| corpus run | `SOLIDCORE_REV=<full hash>\|main SMLIB_PREBUILT=smlib/build-native scripts/ci/solidcore.sh <out>` → `diff-kernels.json`, `summary.md` (rewrites the pin in the working tree: `git checkout -- crates/kernel-solidcore Cargo.lock` afterwards unless bumping; pin routine in the migration doc) |
| tessellator corpus | `cargo run -p cad-tess --example corpus [--features solidcore] -- --kernel both [--tol t]` |
| one file | `mcad diff-kernels FILE --mesh --mesh-tol 0.01 --json`; `mcad check/render/export FILE --kernel solidcore` |
| disk | about 24 GB were free at the start; keep ≥ 8 GB, delete stale test binaries in `target/debug/deps`, `CARGO_INCREMENTAL=0` when short; never `cargo clean` the main target dir |
| pitfalls | `solidcore::Brep::clone()` is not an independent copy (use `Inner::copy`); face/edge ids of a copy differ; classification through `BrepQuery::of_brep(..).point_3d_classify`; error mapping `Unported` → `Unsupported`, `InvalidInput` → `InvalidInput`, else `Failed` |

## Open points

None is a silent wrong result in the corpus. Severity: how much it matters for the switch.

| # | Severity | Point | Where | Repro / state |
| --- | --- | --- | --- | --- |
| 1 | medium (blocks phase 1 "Done when") | variable-radius fillet with different radius laws on several edges: `Unsupported` (the 3 unsupported and 3 blocked nodes) | `kernel-solidcore` fillet path; upstream `FilletSession` takes one law, the executive's per-edge `var_rad_laws` are not public | `tests/models/spec/variable_radius.mcad`; needs a solidcore API, then bump the pin. Still so at `f4f05b1` (`prepare_variable_radius_edges` takes one `FilletLaw`; `FilletParameters::var_rad_laws` is per edge, but the executive is private to the session) |
| 2 | medium (phase 3) | `sketch_cable_duct`: solidcore's union of three extrusions loses faces (`differ`) | solidcore Booleans | `mcad diff-kernels tests/models/spec/sketch_cable_duct.mcad`; plan 3.1–3.3 (reproducer, issue upstream); unchanged at `f4f05b1` (63 against 47 faces) |
| 3 | none | `f11`: SMLib's Boolean fails, solidcore's not (`differ`) | – | a gain; stays `differ` until SMLib is gone |
| 4 | done | `f02_union_cylinder_sweep` at 0.01: `differ`, 0.0311 mm from SMLib's mesh beyond SMLib's own distance (limit 0.0291) | `crates/cad-tess/src/face.rs` (`tri_deviation`, `edge_deviation`) | **fixed 2026-11-04**, solidcore's mesh was off: both B-reps agree (poles to 7e-14); the sample of SMLib's mesh lies on the exact surface (distance 0) but 0.032 from solidcore's mesh, whose triangles were within 0.010 of the surface at their nearest points: they spanned folds of the sweep (its profile turns over) and missed the tips. Where the surface's normal at a corner turns against the one at the middle's parameters, the refinement also measures the surface point at the triangle's (inner edge's) parameters from its plane. Excess 0.0099 at 0.01, 0.0487 at 0.05; test `folding_sweep_*` (SMLib's mesh within 1.5 · tol of ours beyond its own gap) |
| 5 | done | a vertex inside a face off its surface was excused up to its distance from the face's B-rep edges (tester H finding 4) | `crates/cad-tess/src/compare.rs` (`inner_vertex_gap`), `lib.rs` (edge polylines) | **fixed 2026-11-04**: no excuse any more; the split points of crossing loops are written into the edge polylines (`edge_segments`, where the triangles of both faces use them), so they are checked as points of the edge (`edge_vertex_gap`). The stricter check showed inversion misses on a B-spline of many turns (seed net coarser than a turn: 7.08 mm on `twist = 7200` at 0.01): corners are now seeded from a corner of the same triangle on the surface first, and re-seeded from neighbours until nothing improves. Test `inner_vertices_near_an_edge_off_the_surface` (a cylinder 0.3 mm outside its edges, an inner vertex moved onto the edge: caught, was 9e-5) |
| 6 | low | line contacts of staircase-like outlines (tester H finding 6) | `crates/cad-session/src/asserts.rs` (`mesh_contact`, contact curves) | contact kind and curve come from the mesh there; not analysed further |
| 7 | done | forced folds: a fold on a boundary edge whose chord sags more than half the triangle's height was kept (`planter` 24 at 0.5) | `crates/cad-tess/src/lib.rs` (`tessellate_with`, `FORCED_ROUNDS`), `edges.rs`, `face.rs` (`repair_folds`) | **fixed 2026-11-04**: the edges under forced folds are discretised again with half the tolerance and the faces using them meshed anew (both faces of an edge use its new points: watertight; the others keep their meshes), up to 3 rounds; a fold left after that still counts as forced. `planter` at 0.5: 0 (+20 triangles); mesh time there 35 ms before, 68 ms with every face meshed again per round, 41 ms with only the faces of the refined edges (debug, cad-tess at opt-level 2); test `no_forced_folds_*` |
| 8 | done | `linear_extrude(height = 300, twist = 7200) square([30, 2], center = true)` failed at 0.05 ("tolerance 0.05 mm not reached (3.186e-1 mm …)") | `crates/cad-tess/src/face.rs` (`grid_lines`), `lib.rs` (`turn_lines`) | **fixed 2026-11-04**: the ruled sides are straight across but twisted (`σuv · n`); the grid took that direction for straight (rows only, squashed by 4 in the CDT), so Delaunay joined points across bands 7.5 mm wide whose chords the twist puts off the surface, and the refinement cycled (105 points a pass, worst 0.3186 / 0.3401 alternating, 40 passes). The twist now counts as curvature in both directions (its share by the face's median speeds), sampled over at least 8 pieces per direction; straight edges along which a face's normal turns (a loft's rulings) get points by that turn. 0.05: 67.9 k triangles, chord 0.0437 sampled (test `twisted_extrusions_*`, both B-reps); 0.01: 344 k triangles, 0.0100 (was 305 k, 0.0095) |
| 9 | done | `linear_extrude(height = 100, twist = 3600, scale = 0.2) circle(20)`: chord 0.0529 at 0.05 | `crates/cad-tess/src/surface.rs` (`newton`) | **fixed 2026-11-04**: a measuring error, not the mesh: the foot of a point beyond the end of the twisted surface (a rim chord's midpoint) took the coupled Newton step's share in the free parameter while the other one was clamped, and stopped at 0.0529 instead of 0.049. A parameter held at its end now leaves the step to the other alone. Chord 0.0500 at 0.05 (0.04998), 0.0100 at 0.01 (test `twisted_extrusions_*`) |
| 10 | medium (phase 5 gate) | performance: evaluation about 1.5–1.8 × SMLib in debug (the adapter's Boolean checks); the B-rep conversion is most of solidcore's mesh time (14.1 of 16.3 s at 0.05; `kernel-solidcore` runs unoptimised as a workspace crate in debug) | `crates/kernel-solidcore/src/{boolean,brep}.rs` | measure in a release build (plan 0.5 / 4.3) before deciding |
| 11 | none (SMLib only) | an island in a void meshed inside out; a ball in a seat 0.05 larger reported as line contact at distance 0 (solidcore: gap); a wall below one ulp | SMLib | goes away with SMLib (phase 5.3) |
| 12 | low | solidcore fails where SMLib succeeds: a polyhedron of two cubes sharing an edge, `shell(1) cube(10, $fillet = 2)` (offset self-overlap), a root union of many slivers, an operand below ~1e-4 of the pair on a face | adapter / upstream | `docs/solidcore-migration.md`, "What is missing", item 7; the polyhedron and the shell still fail at `f4f05b1` |

Fixed on 2026-11-04 (open points 4, 5, 7, 8, 9; commits above): see the table; corpus changes in
triangles at 0.05 / 0.01: `planter` +21 % / +15 % (its B-spline faces; which of the grid changes, not analysed),
`12_linear_extrude_twist` +13 % / +39 %, the lofts `f12`, `loft_sweep`, `freeform_lid` +4 to
+7 %; `knob_mirror` −6 % / −13 %, `door_stop` −18 % / −24 %, `f01_sweep_spline_profile` −25 % /
−11 %; inner vertices are off their surfaces by at most 7.6e-6 mm, boundary vertices off the
B-rep's edges by at most 1.1e-4 mm.

Fixed in the round before (tester H findings 1–3, performance item; commits above): edge flips
keep the chord tolerance and the face's deviation is measured again after flips (`f12` at 0.01
0.0136 → 0.0100); planes at an angle meeting at a rim are line contact; B-rep edges sampled by
chord sag in `surface_distance` (helical seam 0.061 → 1e-4 mm); `check().inside_out` 4.4 → 0.56 s
on a cube minus 125 balls; `diff-kernels --help` gives the `--mesh-tol` range.

## Next steps

From [`solidcore-plan.md`](solidcore-plan.md):

1. **Phase 3, Booleans (upstream):** reproducers for the Booleans that differ (`sketch_cable_duct`,
   open point 2) as `.mcad` + Rust test on the solidcore API (3.1), check against unpatched
   upstream C++ (3.2), issues in `tangency-labs/solidcore` and pin bumps (3.3; pin routine in the
   migration doc). The guard 3.4 is in place.
2. Ask upstream for per-edge radius laws in `FilletSession` (open point 1), then wire them.
3. **Phase 4, Wasm and UI:** `cad-wasm` over `kernel-solidcore` (`wasm32-unknown-unknown`, 8 MiB
   stack), the web worker without `smlib.wasm`, module size and browser timing, desktop app with
   kernel selection. Not possible in this container (no wasm32 target, no WebKitGTK).
4. **Phase 5, switch:** gate (no topology `differ`, no `failed`, `compare.py`, `check.sh`,
   `tests/step/verify.py`, `tests/color-review` with solidcore, performance factor agreed, browser
   build), default kernel solidcore, freeze SMLib reference data (`tests/kernel-reference.json`),
   remove SMLib and its build steps, update the docs and `CLAUDE.md`.
5. The low point 6 (staircase line contacts) when convenient; it does not block the switch.
