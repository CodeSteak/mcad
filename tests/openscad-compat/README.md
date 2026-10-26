# OpenSCAD comparison

Comparison of our language (OpenSCAD dialect, B-Rep) with OpenSCAD 2021.01 on typical
hobby models. Scope: only features from „Voll“ (full) in `docs/technical-plan.md`, section 5.

| Location | Content |
| --- | --- |
| `tests/models/openscad-compat/*.scad` | 36 models, each with a header comment `// Purpose: …` (34–36: `offset`) |
| `tests/models/spec/*.mcad` | 8 models with the new language features (grids, anchors, fillets, …) |
| `tests/openscad-compat/make-reference.py` | generates `reference.json` with OpenSCAD |
| `tests/openscad-compat/reference.json` | reference (checked in): volume, area, bounding box, triangles, echo |
| `tests/openscad-compat/compare.py` | stage 2: our CLI against the reference |
| `tests/openscad-compat/mock_cad.py` | mock of the CLI for testing `compare.py` |
| `tests/openscad-compat/stl_stats.py` | shared helpers (reading STL, metrics, tolerance class) |
| `crates/compat-tests` | stage 1 as Rust integration tests (tests only, `publish = false`) |

STL and echo files are placed under `target/openscad-compat/` and are not checked in.

## Procedure

1. **Generate the reference** (only needed after changing a model; needs `openscad`):

   ```sh
   python3 tests/openscad-compat/make-reference.py            # all models (~2.5 min)
   python3 tests/openscad-compat/make-reference.py tests/models/openscad-compat/19_hex_nut.scad
   ```

   Per file: `openscad -o target/openscad-compat/<name>.stl` and `-o <name>.echo`, then
   volume (divergence theorem), surface area, bounding box, triangle count and the `ECHO:` lines.
   Each file gets a source hash (FNV-1a 64); tests and `compare.py` report an
   outdated reference.

   **`$fn` rule:** `-D '$fn=128'` is set if the file has curved geometry (sphere,
   cylinder, circle without `$fn` = 3…12 at the call, rotate_extrude, linear_extrude with twist)
   and does **not** assign `$fn`/`$fa`/`$fs` **at top level**. A `$fn` at the call
   (`cylinder(d = e, $fn = 6)`) takes precedence over the global value in OpenSCAD, so the hex nut
   stays a hexagon and only the round bore becomes fine. If a file sets `$fn`
   globally itself (`33_global_fn.scad`), it is exported unchanged, because `-D` would override its
   own assignment.

2. **Stage 1** (now; needs no OpenSCAD, only `reference.json`):

   ```sh
   cargo test -p compat-tests                  # everything
   cargo test -p compat-tests --test openscad_compat -- --nocapture   # with table
   ```

   - `openscad_compat`: every file in `tests/models/openscad-compat/` → `cad_eval::eval_source`
     without error diagnostics, no abort, tree with 3D geometry, echo lines the same as
     OpenSCAD. Known failures are listed with a reason in `KNOWN_FAILURES` (the test also
     fails when a known failure is fixed, so that the entry gets removed).
   - `tree_bounds`: bounding box of the evaluated tree (estimated without kernel: union = union,
     intersection = intersection of the boxes, difference = box of the first child) against OpenSCAD's
     bounding box, tolerance 0.01 mm. Checks transformations, `center`, `r`/`d`, `$fn` polygons,
     modifiers and `children()`.
   - `spec_models`: every file in `tests/models/spec/` without errors and warnings; expected
     grids (name, number of planes), instances with anchors, number of handles, nodes (fillet, zone, …)
     and individual values (anchor position after instance transformation, zones, sweep path).

3. **Stage 2** (once the CLI exists):

   ```sh
   python3 tests/openscad-compat/compare.py                       # CLI `mcad` in PATH
   MCAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval
   CAD_EXPORT_CMD='cargo run -q -p cad-cli -- export {src} -o {out}' python3 tests/openscad-compat/compare.py
   python3 tests/openscad-compat/compare.py --eval --metrics eval   # metrics from `eval --json`
   python3 tests/openscad-compat/compare.py 05_nested_booleans.scad # individual files
   ```

   Command templates with placeholders `{cad} {src} {out} {name} {stem}`:
   `CAD_EXPORT_CMD` (default `{cad} export {src} -o {out}`) and `CAD_EVAL_CMD`
   (default `{cad} eval {src} --json`, only with `--eval`). From `eval --json`,
   `echoes` (list of strings or `{"text": …}`) and, with `--metrics eval`, the first
   object with `volume`, `area`, `bounds` are read.

   Output: table on stdout, JSON report in `target/openscad-compat/compare-report.json`
   (`--report`). Exit code 0 = everything within tolerance, 1 = deviation or
   command error, 2 = setup error (reference missing or outdated).

   **Self-test** with the mock (copies OpenSCAD's own STL, optionally distorted):

   ```sh
   python3 tests/openscad-compat/compare.py --self-test
   ```

   Checks: identical STL → exit 0; scaling 1.005 → exit 1; one file shifted by 0.02 mm
   → exit 1; wrong echo → exit 1; export fails → exit 1; metrics
   from `eval --json` → exit 0. Needs the STLs from step 1.

## Tolerances (stage 2)

| Quantity | Tolerance |
| --- | --- |
| Bounding box | 0.01 mm per coordinate |
| Volume, area (relative), class `polygonal` | 1 % |
| Volume, area (relative), class `rounded` | 2 % |

The class is stored per file in `reference.json` (`stl_stats.classify`, can be overridden with
a comment `// compat-class: polygonal|rounded`). `polygonal`: no curved geometry or
only `$fn` = 3…12 at the call; both sides produce the same polygons (plan, section 12,
item 3), differences are only numerical. `rounded`: OpenSCAD facets, we compute
exactly.

Rationale: with `$fn = 128` the faceting error is small. An inscribed
128-gon has `sin(2π/n)/(2π/n) ≈ 1 − 4·10⁻⁴` of the circle area; measured on the models
against the analytical volumes: cylinder (`03`) −0.040 %, torus and disc (`13`)
−0.064 %, spheres (`02`) −0.100 % (the rings reach neither the poles nor the equator). The error
grows where a difference subtracts most of the volume, and our STL export is
tessellated as well. 1 % and 2 % leave more than ten times the margin, while a wrong
dimension of a main feature is still noticed. The bounding box shifts due to facets by
at most `r·(1 − cos(π/128)) ≈ 3·10⁻⁴·r` (0.006 mm at r = 20 mm), spheres with `$fn = 64`
(`33`) by 0.006 mm at r = 5 mm. For this our export must tessellate finely enough
(chord error well below 0.01 mm) or deliver the bounding box exactly via `eval --json`.

Triangle counts are only reported, not compared.
