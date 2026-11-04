# Handover: solidcore migration, phases 0–2

State on 2026-11-01. Read this first to resume the work; details and history are in
[`solidcore-plan.md`](solidcore-plan.md) (work plan, status per item) and
[`solidcore-migration.md`](solidcore-migration.md) (measurements, tester findings, rules of the
comparison).

## Where

| What | Value |
| --- | --- |
| Branch | `feature/solidcore-phase0-2` (pushed to `origin`; no pull request) |
| Head at handover | the commit that adds this file; the last code commits are `2280207` (flips keep the tolerance), `e2dc0b6` (rim contact), `d13c7d5` (edge sampling), `0a89606` (inside-out speed) on top of `e66d726` (end of tester round H) |
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
applies). Tester round H measured the same node figures at `e66d726`.

| Measurement | Result |
| --- | --- |
| `diff-kernels` nodes (101 files, 2604 nodes; `scripts/ci/solidcore.sh`) | same 2596, unsupported 3, blocked 3, differ 2, failed 0 |
| meshes at 0.05 (`--mesh`) | 100 `ok`, 1 skipped (`f03`, no B-rep part); solidcore watertight 100, chord max 0.0500 mm, 175 k triangles; SMLib watertight 97, chord max 0.242, 272 k; time SMLib 13.3 s, solidcore 16.3 s (14.1 s B-rep conversion) |
| meshes at 0.01 (`--mesh --mesh-tol 0.01`) | 99 `ok`, 1 `differ` (`f02`), 1 skipped; solidcore watertight 100, chord max 0.0100 (tester H at `e66d726`: 98 `ok`, `f12` 0.0136); 608 k triangles (SMLib 708 k); time SMLib 24.9 s, solidcore 26.0 s |
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
| 4 | low | `f02_union_cylinder_sweep` at 0.01: `differ`, 0.0311 mm from SMLib's mesh beyond SMLib's own distance (limit 0.0291); `ok` at 0.05 | `crates/cad-cli/src/diff/mesh.rs` (Hausdorff excess) | `mcad diff-kernels tests/everyday/bugs/f02_union_cylinder_sweep.mcad --mesh --mesh-tol 0.01`; solidcore's chord there ≤ 0.010; the sweep folds over itself. Find out which mesh is off, or allow locally where a surface turns over |
| 5 | low (check too lenient) | a vertex inside a face that lies off its surface is excused up to its distance from the face's B-rep edges (tester H finding 4) | `crates/cad-tess/src/compare.rs:244-248` (`inner_vertex_gap`) | a vertex displaced near an edge passes `diff-kernels --mesh`; excuse only split points of crossing loops (`FaceMesh::splits`, not in the output mesh today) |
| 6 | low | line contacts of staircase-like outlines (tester H finding 6) | `crates/cad-session/src/asserts.rs` (`mesh_contact`, contact curves) | contact kind and curve come from the mesh there; not analysed further |
| 7 | low | forced folds: a fold on a boundary edge whose chord sags more than half the triangle's height is kept at any tolerance (`Stats::forced_folds`) | `crates/cad-tess/src/face.rs:1140` | 0 in the corpus at 0.05 / 0.01, `planter` 24 at 0.5; manual 17 now states the rule as coded |
| 8 | low (loud) | `linear_extrude(height = 300, twist = 7200) square([30, 2], center = true)`: `render --kernel solidcore` fails at 0.05 ("tolerance 0.05 mm not reached (3.186e-1 mm …)", exit 3); 0.01 works (0.0095) | `crates/cad-tess/src/face.rs` (grid, refinement passes), message `lib.rs:159` | grid lines from the turn of the surface normal as well as from the chord |
| 9 | low | strongly twisted extrusions slightly above the tolerance but within the verdict's 1.1 · tol: `linear_extrude(height = 100, twist = 3600, scale = 0.2) circle(20)` chord 0.0529 at 0.05 | `crates/cad-tess/src/face.rs` (refinement) | `mcad diff-kernels <file> --mesh` → `ok` |
| 10 | medium (phase 5 gate) | performance: evaluation about 1.5–1.8 × SMLib in debug (the adapter's Boolean checks); the B-rep conversion is most of solidcore's mesh time (14.1 of 16.3 s at 0.05; `kernel-solidcore` runs unoptimised as a workspace crate in debug) | `crates/kernel-solidcore/src/{boolean,brep}.rs` | measure in a release build (plan 0.5 / 4.3) before deciding |
| 11 | none (SMLib only) | an island in a void meshed inside out; a ball in a seat 0.05 larger reported as line contact at distance 0 (solidcore: gap); a wall below one ulp | SMLib | goes away with SMLib (phase 5.3) |
| 12 | low | solidcore fails where SMLib succeeds: a polyhedron of two cubes sharing an edge, `shell(1) cube(10, $fillet = 2)` (offset self-overlap), a root union of many slivers, an operand below ~1e-4 of the pair on a face | adapter / upstream | `docs/solidcore-migration.md`, "What is missing", item 7; the polyhedron and the shell still fail at `f4f05b1` |

Fixed in the last round (tester H findings 1–3, performance item; commits above): edge flips
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
5. The low points above (4–9) when convenient; none blocks the switch.
