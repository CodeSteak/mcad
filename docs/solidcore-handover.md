# Handover: solidcore migration, phases 0–2 and the switch (phase A)

**2026-11-06, phase A: solidcore is the only kernel.** SMLib (C++, `crates/kernel-smlib`,
`smlib/` with the facade, patches, CMake/Emscripten scripts, `third_party/smlib`,
`SMLIB_*`, `scripts/ci/smlib-*.sh`, `emsdk.sh`, `kernel-perf.py`) is removed; nothing in the
build compiles C or C++ for mcad any more. What replaced what:

| Before | Now |
| --- | --- |
| `--kernel smlib\|solidcore`, `MCAD_KERNEL`, feature `solidcore` | gone: solidcore always (`kernel-solidcore` without features; `cad-cli/src/kernel.rs` is a type alias) |
| daemon and desktop engine process on SMLib | on solidcore |
| browser: `cad_wasm_bg.wasm` + Emscripten `smlib.wasm` and a JS bridge | one module, `cad-wasm` over `kernel-solidcore` (wasm32-unknown-unknown, profile `wasm`) |
| `mcad diff-kernels FILE…` (both kernels live) | `mcad diff-reference tests/kernel-reference.json [FILE…] [--mesh]`: solidcore against SMLib's per-node results, frozen with the last SMLib build (same rules; verdicts, rules and notes identical to the live comparison when frozen) |
| `diff_kernels_on_examples` | `solidcore_against_the_reference` (`crates/cad-cli/tests/cli.rs`): the 8 known differences are listed (`REFERENCE_EXCEPTIONS`); a new one fails, and so does one that goes away |
| tests on `SmlibKernel` (cad-session, cad-engine, cad-measure, cad-render, cad-tess, cad-desktop) | on `SolidcoreKernel` |
| variable radius with different laws on several edges: "not supported by this kernel: solidcore: fillet_variable: …" | the engine's diagnostic "variable radius is not supported: fillet of N edges with different radius laws (A–B mm); use a constant radius" (`cad-engine/src/eval.rs`), body unfilleted; one law shared by all edges and constant radii per edge work |

Found on the way (not in the corpus): solidcore's union of bodies that do not overlap can
return one of them a second time inside out as a void (stencil glyph "c" of Allerta Stencil:
3 solids + 1 void, volume 2.02 instead of 7.30, all checks passed). `boolean.rs` now refuses a
union whose result has more shells than its operands with two shells alike, and merges the
bodies instead (as the facade's `disjoint_union`). And: where a Boolean fails because operands
touch tangentially, solidcore's unstitched result often has only open edges, so the places (the
diagnostic's lines and point, `render`'s marks) were missing; they are now the result's edges on
which both operands lie with tangent surfaces (`tangent_places`; `boss_rib`, `f09a`, `f08c`
name both operands and the place as with SMLib). `minkowski` of a tetrahedron and a sphere,
which SMLib only approximated ("offset is not exact"), solidcore builds at the distance.

Measured 2026-11-06 (debug build unless noted, pin `f4f05b1`):

| Measurement | Result |
| --- | --- |
| `diff-reference` (101 files, 2604 nodes) | same 2596, unsupported 3, blocked 3, differ 2, failed 0; volume rules relative 2293, exact 129, operands 67, curved 25, blend 70; meshes at 0.05: ok 100, skipped 1, watertight 100, chord max 0.0500 mm |
| live comparison at the freeze (last SMLib build) | the same verdict, volume rule and notes for every node as the reference mode |
| `compare.py --eval` / `check.sh` / `tests/step/verify.py` | 36 / 36; 19 models, 0 problems (bugs: 10 fixed, 7 open); 64 / 64 |
| Wasm module (profile `wasm`: opt-level 3, fat LTO, one codegen unit; wasm-bindgen 0.2.129) | build 13.5 min (4 cores); 34.3 MB raw, 10.8 MB gzip -9, 6.66 MB Brotli q11. With opt-level "z": 9.5 min, 37.1 MB raw, 7.9 MB gzip, 4.97 MB Brotli, but 2.4 × slower (`dump-scene.mjs examples/08-macropad.mcad` 85 s against 36 s; the e2e tests S04/S06 time out on it); `wasm-opt -Oz` (binaryen 123, 11 min) on the "z" module: 22.3 MB raw, 5.1 MB Brotli. Release profile before (handover 2026-11-04): 39.3 MiB, 6.2 MiB Brotli |
| browser engine in Node (`smoke-node.mjs`) | load 138 ms, housing scene 219 ms, root mesh 237 ms, warm 31 ms |

The sections below are the state before the switch (2026-11-04), kept as history; their
commands with `--kernel`, `diff-kernels`, `SMLIB_PREBUILT` no longer exist.

State on 2026-11-04 (pin bump and the tessellator's open points 4, 5, 7, 8, 9; the state of
2026-11-01 otherwise). Read this first to resume the work; details and history are in
[`solidcore-plan.md`](solidcore-plan.md) (work plan, status per item) and
[`solidcore-migration.md`](solidcore-migration.md) (measurements, tester findings, rules of the
comparison).

## Where

| What | Value |
| --- | --- |
| Branch | `feature/solidcore-phase0-2` (pushed to `origin`; no pull request) |
| Head at handover | the commit that adds this file; the last code commits are `2280207` (flips keep the tolerance), `e2dc0b6` (rim contact), `d13c7d5` (edge sampling), `0a89606` (inside-out speed) on top of `e66d726` (end of tester round H). 2026-11-04 on branch `claude/tessellierung-subagent-hngono`: pin bump `a921b75`, then `1c7762e` (#8), `a5ceafd` (#9), `4c51fd3` (#5), `56ca505` (#4), `c6c23cf` (#7); review round `058acf3` (#7 cost), `a4afa5c` (#4 cost), `b8eab5f` (fold sheets), `bcf9a36` (twist sum), then ruled faces |
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
after the tessellator fixes and the review round (`058acf3`, `a4afa5c`, `b8eab5f`, `bcf9a36` and
the ruled-face commit after them), on another machine (4 cores; times not comparable with the earlier ones, so the state
before the fixes, measured there at the new pin, is given in brackets; "tessellation" is
solidcore's mesh time less the B-rep conversion).

| Measurement | Result |
| --- | --- |
| `diff-kernels` nodes (101 files, 2604 nodes; `scripts/ci/solidcore.sh`) | same 2596, unsupported 3, blocked 3, differ 2, failed 0 |
| meshes at 0.05 (`--mesh`) | 100 `ok`, 1 skipped (`f03`, no B-rep part); solidcore watertight 100, chord max 0.0500 mm, 172.9 k triangles (175.0 k); SMLib watertight 97, chord max 0.242, 272 k; largest Hausdorff excess where the roots are `same` 0.0499 (0.0657); time SMLib 7.9 s, solidcore 8.8 s, tessellation 1.7 s (1.8 s) |
| meshes at 0.01 (`--mesh --mesh-tol 0.01`) | 100 `ok` (99, `f02` `differ`), 1 skipped; solidcore watertight 100, chord max 0.0100; 587.4 k triangles (607.5 k; SMLib 708 k, watertight 95); largest Hausdorff excess 0.0101 (0.0311, `f02`); time SMLib 14.4 s, solidcore 12.9 s, tessellation 6.1 s (6.2 s; 7.1 s before the review round) |
| `tests/openscad-compat/compare.py` | 36 / 36 |
| `tests/everyday/check.sh` | 19 models, 0 with problems |
| STL/3MF, section view, render snapshots | met at task F (`docs/solidcore-plan.md`, phase 2 status), not re-run |
| tests | `cargo test -p cad-tess --features solidcore`, `cargo test -p cad-session`, `cargo test -p cad-cli --no-default-features [--features solidcore]` (incl. `diff_kernels_on_examples`, `usage`): green |

Phase 2 "Done when": met (plan, "Phase 2 closed 2026-11-01"). Phase 1 "Done when": not met only
for `variable_radius` (upstream API).

## Build and test

| Topic | How |
| --- | --- |
| headless (no WebKitGTK) | `cargo build -p cad-cli --no-default-features`; workspace commands with `--exclude cad-app --exclude cad-cli`, cad-cli with `--no-default-features` (its default feature `gui` links WebKitGTK); `cad-desktop` builds and tests without it |
| from an empty target dir | about 10–15 min (debug), then incremental; every test binary links solidcore with debug info (about 500 MB each): run crate by crate and delete them when the disk is short |
| adapter tests | `cargo test -p kernel-solidcore` |
| tessellator tests | `cargo test -p cad-tess` |
| CLI tests | `cargo test -p cad-cli --no-default-features` (`solidcore_against_the_reference` about 5 min); `cargo test -p cad-cli usage` checks the manual |
| reference run | `mcad diff-reference tests/kernel-reference.json --mesh [--json]`; pin bump: `SOLIDCORE_REV=<full hash>\|main scripts/ci/solidcore.sh <out>` → `diff-reference.json`, `summary.md` (rewrites the pin in the working tree: `git checkout -- crates/kernel-solidcore Cargo.lock` afterwards unless bumping) |
| tessellator corpus | `cargo run -p cad-tess --example corpus -- [--tol t]` |
| one file | `mcad diff-reference tests/kernel-reference.json FILE --mesh --mesh-tol 0.01 --json` (FILE one of the reference's) |
| browser | `web/scripts/build-wasm.sh` (profile `wasm`: fat LTO, one codegen unit; wasm-opt -Oz if on PATH), `node web/scripts/smoke-node.mjs`, `cd web && pnpm build && pnpm test` |
| pitfalls | `solidcore::Brep::clone()` is not an independent copy (use `Inner::copy`); face/edge ids of a copy differ; classification through `BrepQuery::of_brep(..).point_3d_classify`; error mapping `Unported` → `Unsupported`, `InvalidInput` → `InvalidInput`, else `Failed`; `Instant::now` panics on wasm32 (no clock there: the copy timing is skipped) |

## Open points

None is a silent wrong result in the corpus. Severity: how much it matters for the switch.

| # | Severity | Point | Where | Repro / state |
| --- | --- | --- | --- | --- |
| 1 | medium (blocks phase 1 "Done when") | variable-radius fillet with different radius laws on several edges: `Unsupported` (the 3 unsupported and 3 blocked nodes) | `kernel-solidcore` fillet path; upstream `FilletSession` takes one law, the executive's per-edge `var_rad_laws` are not public | `tests/models/spec/variable_radius.mcad`; needs a solidcore API, then bump the pin. Still so at `f4f05b1` (`prepare_variable_radius_edges` takes one `FilletLaw`; `FilletParameters::var_rad_laws` is per edge, but the executive is private to the session) |
| 2 | medium (phase 3) | `sketch_cable_duct`: solidcore's union of three extrusions loses faces (`differ`) | solidcore Booleans | `mcad diff-kernels tests/models/spec/sketch_cable_duct.mcad`; plan 3.1–3.3 (reproducer, issue upstream); unchanged at `f4f05b1` (63 against 47 faces) |
| 3 | none | `f11`: SMLib's Boolean fails, solidcore's not (`differ`) | – | a gain; stays `differ` until SMLib is gone |
| 4 | done | `f02_union_cylinder_sweep` at 0.01: `differ`, 0.0311 mm from SMLib's mesh beyond SMLib's own distance (limit 0.0291) | `crates/cad-tess/src/face.rs` (`tri_deviation`, `edge_deviation`) | **fixed 2026-11-04**, solidcore's mesh was off: both B-reps agree (poles to 7e-14); the sample of SMLib's mesh lies on the exact surface (distance 0) but 0.032 from solidcore's mesh, whose triangles were within 0.010 of the surface at their nearest points: they spanned folds of the sweep (its profile turns over) and missed the tips. Where the surface's normal at a corner turns against the one at the middle's parameters, the refinement also measures the surface point at the triangle's (inner edge's) parameters from its plane. Only on surfaces that can fold (B-splines, revolutions and extrusions of a curve; the vertices keep their normals there, so the test costs one normal per triangle or edge). Excess 0.0099 at 0.01, 0.0487 at 0.05; test `folding_sweep_*` (SMLib's mesh within 1.5 · tol of ours beyond its own gap) |
| 5 | done | a vertex inside a face off its surface was excused up to its distance from the face's B-rep edges (tester H finding 4) | `crates/cad-tess/src/compare.rs` (`inner_vertex_gap`), `lib.rs` (edge polylines) | **fixed 2026-11-04**: no excuse any more; the split points of crossing loops are written into the edge polylines (`edge_segments`, where the triangles of both faces use them), so they are checked as points of the edge (`edge_vertex_gap`). The stricter check showed inversion misses on a B-spline of many turns (seed net coarser than a turn: 7.08 mm on `twist = 7200` at 0.01): corners are now seeded from a corner of the same triangle on the surface first, and re-seeded from neighbours until nothing improves. Test `inner_vertices_near_an_edge_off_the_surface` (a cylinder 0.3 mm outside its edges, an inner vertex moved onto the edge: caught, was 9e-5) |
| 6 | low | line contacts of staircase-like outlines (tester H finding 6) | `crates/cad-session/src/asserts.rs` (`mesh_contact`, contact curves) | contact kind and curve come from the mesh there; not analysed further |
| 7 | done | forced folds: a fold on a boundary edge whose chord sags more than half the triangle's height was kept (`planter` 24 at 0.5) | `crates/cad-tess/src/lib.rs` (`tessellate_with`, `FORCED_ROUNDS`), `edges.rs`, `face.rs` (`repair_folds`) | **fixed 2026-11-04**: the edges under forced folds are discretised again with half the tolerance and the faces using them meshed anew (both faces of an edge use its new points: watertight; the others keep their meshes), up to 3 rounds; a fold left after that still counts as forced. `planter` at 0.5: 0 (+20 triangles); mesh time there 35 ms before, 68 ms with every face meshed again per round, 41 ms with only the faces of the refined edges (debug, cad-tess at opt-level 2); test `no_forced_folds_*` |
| 8 | done | `linear_extrude(height = 300, twist = 7200) square([30, 2], center = true)` failed at 0.05 ("tolerance 0.05 mm not reached (3.186e-1 mm …)") | `crates/cad-tess/src/face.rs` (`grid_lines`), `lib.rs` (`turn_lines`) | **fixed 2026-11-04**: the ruled sides are straight across but twisted (`σuv · n`); the grid took that direction for straight (rows only, squashed by 4 in the CDT), so Delaunay joined points across bands 7.5 mm wide whose chords the twist puts off the surface, and the refinement cycled (105 points a pass, worst 0.3186 / 0.3401 alternating, 40 passes). The twist now counts as curvature in both directions: each direction adds its share (by the face's median speeds) to its curvature, as the bound L·hu² + 2·M·hu·hv + N·hv² ≤ (L + M·ρ)·hu² + (N + M/ρ)·hv² gives (first, in `1c7762e`, the larger of the two: up to twice the bound, the refinement filled in, the helix of open point 9 164 k triangles with 49 k refinement points; with the sum 113 k and 42), sampled over at least 8 pieces per direction; straight edges along which a face's normal turns (a loft's rulings) get points by that turn. 0.05: 67.9 k triangles, chord 0.0437 sampled (test `twisted_extrusions_*`, both B-reps); 0.01: 344 k triangles, 0.0100 (was 305 k, 0.0095). Review round: the twist share is added to the curvature (the bound's sum, not the larger of the two), and a ruled face whose curved direction outweighs the twist keeps that direction's lines and gets lines along its rulings by the twist alone: bar 65.6 k / 327 k, helix 113 k / 563 k (SMLib 59 k for the bar at 0.01, chord 0.043, and 66 k / 296 k for the helix, chord 0.14 at 0.05: it misses the tolerance, not like for like). On ruled faces the rulings' lines keep the twist's part of the diagonal sag within the whole tolerance, the curved direction within half (up to 1.5 · tol on the grid, the refinement splits those cells): within half for both, `planter` takes 12.3 k / 53.7 k instead of 10.6 k / 45.0 k, the same tolerance held |
| 9 | done | `linear_extrude(height = 100, twist = 3600, scale = 0.2) circle(20)`: chord 0.0529 at 0.05 | `crates/cad-tess/src/surface.rs` (`newton`) | **fixed 2026-11-04**: a measuring error, not the mesh: the foot of a point beyond the end of the twisted surface (a rim chord's midpoint) took the coupled Newton step's share in the free parameter while the other one was clamped, and stopped at 0.0529 instead of 0.049. A parameter held at its end now leaves the step to the other alone. Chord 0.0500 at 0.05 (0.04998), 0.0100 at 0.01 (test `twisted_extrusions_*`) |
| 10 | low (phase 5 gate) | performance: evaluation 1.48–1.51 × SMLib, queries 0.87 ×, meshing 2–3 × faster than SMLib's (corpus, release build at `ae2e06d`: evaluation 128.6 s against 86.9 s; meshes at 0.05 2.95 s against 8.1 s, B-rep conversion 1.27 s of it; at 0.01 7.0 s against 14.6 s). The debug figures were close already (`[profile.dev.package."*"] opt-level = 2`). Twisted extrusions mesh slower (helix at 0.01 9.2 s against 5.0 s, but SMLib misses the tolerance there) | `crates/kernel-solidcore/src/{boolean,brep}.rs` | agree the factor for the gate; the adapter's Boolean checks are the first place to look |
| 11 | none (SMLib only) | an island in a void meshed inside out; a ball in a seat 0.05 larger reported as line contact at distance 0 (solidcore: gap); a wall below one ulp | SMLib | goes away with SMLib (phase 5.3) |
| 12 | low | solidcore fails where SMLib succeeds: a polyhedron of two cubes sharing an edge, `shell(1) cube(10, $fillet = 2)` (offset self-overlap), a root union of many slivers, an operand below ~1e-4 of the pair on a face | adapter / upstream | `docs/solidcore-migration.md`, "What is missing", item 7; the polyhedron and the shell still fail at `f4f05b1` |

Fixed on 2026-11-04 (open points 4, 5, 7, 8, 9; commits above): see the table. Corpus changes in
triangles at 0.05 / 0.01 against the state before: `planter` −12 % / −19 %, `f12` −21 % / −20 %,
`knob_mirror` −6 % / −13 %, `door_stop` −18 % / −22 %, `f01_sweep_spline_profile` −25 % / −11 %;
more on `loft_sweep` +30 % / +34 % (its four rational loft patches circle → square → circle:
501 → 739 triangles each at 0.05, half of them below tol/4; the twist bound is not sharp there,
see the migration doc), `freeform_lid` +10 % / +10 %, `12_linear_extrude_twist` −3 % / +10 %,
`mug` +1 % / −1 %. Outside the corpus the helix of open point 9 needs 113 k / 563 k triangles
(160 k / 807 k before; SMLib 66 k / 296 k, but it misses the tolerance there, chord 0.14 mm at 0.05, so the counts are not like for like), the bar of open point 8 65.6 k /
327 k (failed / 305 k). Inner vertices are off their surfaces by at most 7.7e-6 mm, boundary
vertices off the B-rep's edges by at most 1.1e-4 mm.

Fixed in the round before (tester H findings 1–3, performance item; commits above): edge flips
keep the chord tolerance and the face's deviation is measured again after flips (`f12` at 0.01
0.0136 → 0.0100); planes at an angle meeting at a rim are line contact; B-rep edges sampled by
chord sag in `surface_distance` (helical seam 0.061 → 1e-4 mm); `check().inside_out` 4.4 → 0.56 s
on a cube minus 125 balls; `diff-kernels --help` gives the `--mesh-tol` range.

## Removing C++: done 2026-11-06

| Area | State |
| --- | --- |
| Booleans (phase 3) | 2596 of 2604 nodes `same` against the reference; `differ` 2 (`sketch_cable_duct`, `f11` a gain); open point 12 |
| Variable-radius fillet | 3 `unsupported` + 3 `blocked` (open point 1): reported as not supported (diagnostic above) |
| Meshes | 100 of 100 `ok` and watertight at 0.05 |
| Performance | open point 10 (evaluation about 1.5 × SMLib's, meshing faster); no longer measurable against SMLib. Debug renders of the examples (studio, 900 × 675, measured 2026-11-06 against the last SMLib binary): `07-mug` 17.8 s → 25.7 s (the item to look at), `09-flashlight` 32.4 → 22.0 s, the others within ±10 %; images: under 0.21 % of the pixels differ by more than 40/255 |
| Code on SMLib | none |

## Next steps

From [`solidcore-plan.md`](solidcore-plan.md):

1. **Phase 3, Booleans (upstream):** reproducers for the Booleans that differ (`sketch_cable_duct`,
   open point 2) as `.mcad` + Rust test on the solidcore API (3.1), check against unpatched
   upstream C++ (3.2), issues in `tangency-labs/solidcore` and pin bumps (3.3; pin routine in the
   migration doc). The guard 3.4 is in place.
2. Ask upstream for per-edge radius laws in `FilletSession` (open point 1), then wire them.
3. **Phase 4, Wasm and UI:** `cad-wasm` over `kernel-solidcore` (`wasm32-unknown-unknown`, 8 MiB
   stack), the web worker without `smlib.wasm`, module size and browser timing, desktop app with
   kernel selection. Measured 2026-11-04 (`f4f05b1`): solidcore and `cad-wasm` over
   `kernel-solidcore` build for wasm32 (release 7 min); the module is 39.3 MiB raw, 6.2 MiB Brotli
   (`wasm-opt -O3`/`-Oz`: 29.5–29.8 MiB, 6.6 MiB Brotli), against 1.2 MiB + `smlib.wasm` 4.85 MiB
   (1.24 MiB Brotli) today. Next: a wasm profile (`opt-level = "z"`, LTO, `codegen-units = 1`),
   browser timing. No WebKitGTK here (desktop app).
4. **Phase 5, switch:** gate (no topology `differ`, no `failed`, `compare.py`, `check.sh`,
   `tests/step/verify.py`, `tests/color-review` with solidcore, performance factor agreed, browser
   build), default kernel solidcore, freeze SMLib reference data (`tests/kernel-reference.json`),
   remove SMLib and its build steps, update the docs and `CLAUDE.md`.
5. The low point 6 (staircase line contacts) when convenient; it does not block the switch.
