# Technical Plan: Parametric CAD Language with Grids

As of: 2026-10-23, current state after test report 11 (first version 2026-10-22) · Basis: `Spec: Parametrische CAD-Sprache mit Rastern` (2026-10-22)

Section 0 summarises what has changed since the first version; sections 1–14 describe the implemented state, section 13 the status of the milestones, section 15 the known limitations, section 16 colours, filaments and theme, section 17 sketches, text and the sketch editor, section 18 measuring. Test reports: `docs/reports/` (B1 … B11 = report 1 … 11, overview in `docs/reports/README.md`).

Goal: the application runs entirely in the browser, desktop only, for small hobby projects. Language, evaluation, cache and write-back are in Rust (Wasm), the UI in TypeScript/React. In addition there is a CLI (also for use by Claude Code) and a desktop version (Tauri 2, B9) with a native kernel, real files, a file watcher and Git markers.

Fixed constraints:

| Topic | Decision |
| --- | --- |
| Kernel | Until release: SMLib (C++, upstream of solidcore) as Wasm, from the fork `tangency-labs/smlib` (`capi-tess` @ `7eba8aa`) with `smlib/patches/0001`. Before release: switch to solidcore (M8, open). |
| Platform | Desktop browser; no touch input |
| Storage | Browser only; no backend |
| Headers | No cross-origin isolation (COOP/COEP) as long as it is not needed. Hence also no `SharedArrayBuffer` and no threads. |
| OpenSCAD | Largely compatible, not fully (section 5) |
| Priority | Grids before fillets |

---

## 0. Changed and refined decisions (current state)

| Topic | First version | Implemented | Evidence |
| --- | --- | --- | --- |
| Kernel source | `usd-brep` @ `c9c5979` directly | Fork `tangency-labs/smlib`, branch `capi-tess` @ `7eba8aa` (= `c9c5979` plus fixes for memory errors found with ASan/UBSan, without the OCCT translators) and `smlib/patches/0001` (pointer tables in insertion order: results no longer depend on heap layout, identical natively and in Wasm). `smlib/fetch.sh` fetches both. | B4 (kernel state `58735e2`), B7 |
| Facade | flat C ABI with handles | additionally: booleans are checked (volume, probe points on the operand faces) with fallbacks (swap operands, order, offset by 10⁻⁶…10⁻⁴); `rand()`/`srand()` fixed; shell walls that are too thick are rejected before SMLib; shell with opening built from booleans when SMLib's shell fails; edge flags (smooth); ABI version 4 (`sc_version`) | B1, B7 |
| Scenes | "send scene description immediately" | two scenes per evaluation: a preliminary one (`complete = false`, without kernel: nodes, grids, anchors, handles) right after `SetSource` and the complete one after the kernel job; meshes are requested per scene version | B1 (6.2), B2 |
| Provenance tag scheme | tags local per subtree, facade shifts them when combining | a tag is the interned index of the cache key of the producing node (equal for equal content as long as the engine lives); the facade only passes tags through; `Evaluation::tag_nodes` maps tag → nodes (several lines for equal content); UI: triangle → `tri_tags` → `tag_nodes` → node → line | B2 (S07) |
| New edges, edge selection | edge between faces of different children | additionally: an edge lying on an edge of an operand is not new (coplanar merged faces); smooth edges (seam, pole, tangent within 2°) are never selected | B7 (F3, F4) |
| Draft mode while dragging | coarse tessellation only | additionally: fillets that last took longer than 25 ms and are not in the cache are suspended by the engine while dragging (notice "Verrundung beim Ziehen ausgesetzt" (fillet suspended while dragging)); full evaluation after `DragEnd` | B4 (C20) |
| Watchdog block | remember key, do not recompute | 20 min (`WorkerTransport`, previously 15 s; large models compute for minutes); the content key of the node from the last `Progress.current` is blocked; the new worker receives `BlockKeys` before `SetSource`; the block ends when the node changes (new key) or the page is reloaded | B2 (S17), B4 (C19) |
| Grid access | plane names | an entry of `g[…]` that is not a plane of the grid is a numeric expression (`g[vorn, mitte, 9]`); lists yield indexed planes (`g[loch[i], …]`); `g.name` as number or plane (12.1) | B1, B4 |
| Grids and anchors in loops | warning per spec | error: `grid` and `anchor` in `for` and `grid` in the children of a module call are not allowed (error at the statement) | `cad-eval/tests/grids.rs` |
| Jump to declaration | click on grid/anchor expression | Ctrl+click (a plain click still places the cursor, otherwise the expressions would not be editable); likewise Ctrl+click on the path of `include`/`use` | B3 (B13), B6 (P04) |
| Snapping of 1D handles | – | height, radius and plane snap to grid planes of the same axis and write `g.name`, only if the handle then lies exactly on the plane | B3 (B06) |
| STEP export | "later" | AP242 (default) or AP214 with a custom writer `cad-step` from the neutral B-rep `cad_kernel::brep` (facade `sc_brep`, exact geometry); verified with OpenCASCADE (`tests/step/verify.py`) | B5 |
| `include`/`use` | "later" | OpenSCAD semantics; paths relative to the including file or absolute, no `OPENSCADPATH`; missing files and cycles are errors at the `include`/`use` line (OpenSCAD: warning) | B6 |
| File IDs in spans | byte spans of one file | each loaded file occupies its own position range (file `f` from `f << 24`, statement IDs from `f << 22`); diagnostics, hints, handles and text edits know their file; the protocol reports positions local to the file with a `file` field | B6 |
| Parts | open point 2 | named instances at the top level of the main file; export per part (STL/3MF/STEP), all as ZIP or as 3MF with one object per part; CLI `export --part`, `--all-parts` | B6 |
| Named instances in loops | not defined | `k[i]: …` with integer index (`k[2].anker`, snap writes `k[2].ecke`); `k: …` in `for` remains an error | B6 (P08) |
| Variable radius | stretch goal | `fillet(r = function (p) …)` and `difference/union/intersection(fillet = function (p) …)`: cad-eval translates the function literal into a radius law `cad_ir::Law` (pure expression over `p`, part of the tree and of the cache key) instead of a callback; the engine evaluates it at sample points per edge, the facade (`sc_fillet_var`) builds a B-spline law from them | B7 |
| solidcore | `kernel-solidcore` from the switch | `kernel-solidcore` and `mcad diff-kernels` exist, opt-in (feature `solidcore`), pin `bba8d87`: 1187 of 1438 nodes identical to SMLib (primitives, transformation, most booleans); tessellation not in solidcore's scope (custom tessellator needed), M8 waits; work plan `docs/solidcore-plan.md` | B5, B6, `docs/solidcore-migration.md` |
| UI libraries | react-three-fiber, `three-mesh-bvh`, `Line2` | three.js directly (raycaster, lines as `LineSegments`), without r3f and BVH; TS protocol types generated from Rust with `ts-rs` | B2 |
| CI | GitHub Actions, cached SMLib builds | no hosted CI (workflows removed 2026-10-30); check scripts in `scripts/ci/`, run locally (section 11) | B9 |
| Tauri version | "later, optional" | `crates/cad-app` (Tauri 2) in the `mcad` binary (feature `gui`, default): `mcad` / `mcad datei.mcad` / `mcad ordner/` starts the UI. Engine in a separate process (`mcad engine-host`, engine thread with 256 MiB stack, stepwise as in the worker), because SMLib has global state and a hanging call can only be stopped by ending the process; responses are sent in binary over a Tauri channel (`cad-desktop/src/wire.rs`). Files on disk, file watcher (`notify`), Git gutter against `HEAD` (`git` call + `similar`), control via `mcad ui status/show/screenshot` over local IPC (`cad-ipc`) | B9 |
| Section view | – | GPU section in browser and Tauri: clipping plane, stencil cap (parity pass per body, hatched like `mcad render --section`), contour, handle with detents on grid planes and anchors; "Als `mcad render …` kopieren" (copy as `mcad render …`) | B9 |
| `color()` | CSS names like OpenSCAD | own palette of 16 muted colours (section 16.1), unknown names: warning and a stable random colour; `filament = n` | B11 |
| Multi-colour printing | – | colour regions via kernel booleans, filament slots, 3MF with one part per colour and Bambu/Orca metadata, one STL per colour (section 16) | B11 |
| Measuring | – | measure mode in the UI and `mcad measure` with shared logic in Rust (`cad-measure`, `cad_session::measure`), exact on the B-rep, in a section via plane ∩ B-rep from the kernel (section 18) | B19 |
| Theme | light/dark switchable | "System" (default, follows `prefers-color-scheme`, also in Tauri), Light, Dark; token set with WCAG check; `mcad render --theme dark` | B11 |

---

## 1. Architecture

```
┌──────────────────────────── Main thread (React/TS) ────────────────────────────┐
│  Editor (CodeMirror 6)      3D view (three.js)         Parameter bar, path bar   │
│        │  ▲                        │  ▲                                          │
│        ▼  │                        ▼  │                                          │
│  ─────────────────────── EngineTransport (TS interface) ──────────────────────── │
│        Browser: Web Worker + postMessage      Tauri: channel + engine process   │
└──────────────────────────────────────┬─────────────────────────────────────────┘
                                       │
┌──────────────────────────── Engine (Rust) ─────────────────────────────────────┐
│  cad-syntax → cad-eval → cad-ir → cad-engine (cache, zones, provenance, handles)│
│                                         │ trait Kernel                          │
│               ┌─────────────────────────┼──────────────────────────┐            │
│        kernel-smlib (interim)     kernel-solidcore (target)   kernel-mock (tests)│
│               │                                                                 │
│        smlib.wasm (C++, Emscripten) or linked natively                          │
└─────────────────────────────────────────────────────────────────────────────────┘
      Entry points:  cad-wasm (browser worker) · cad-cli (native) · cad-app/cad-desktop (Tauri, native)

Desktop (Tauri 2, B9):
  mcad (app process): Tauri main thread + webview (the same React app), supervisor with watchdog,
                     file watcher, Git base, control socket ui-<pid> (cad-ipc)
     │ stdin: frames `post` [ToEngine…]            ▲ stdout: binary response batches (wire.rs)
     ▼                                              │
  mcad engine-host (engine process): engine thread 256 MiB, Session::post/step(16 ms), SMLib native
  mcad ui status|show|screenshot (CLI) ──── Unix socket in the user directory ────▶ app
```

Principles:

- **The engine knows neither the browser nor the transport.** The same crates run in the worker (Wasm), in the CLI (native) and in the engine process of the desktop app (native). The protocol is a set of `serde` types.
- **The engine owns the state** (source version, cache, kernel shapes). The UI holds only display data.
- **Cursor and hover need no recomputation.** For each evaluation the engine delivers a complete scene description (all nodes with source range, transformation, kind, parent). The UI decides on highlighting, dimming and hiding itself. Meshes are loaded per node on demand.
- **Full re-evaluation of the language on every change, kernel operations from the cache.** Language evaluation takes milliseconds. Only kernel calls are expensive; they are reused via content-based keys. "Only the path to the root becomes invalid" follows automatically.

---

## 2. Kernel

### 2.1 Interim solution via SMLib (C++)

solidcore is a Rust port of SMLib from NVIDIA's `usd-brep` (Apache-2.0) with the goal of identical topology and numerical deviation ≤ 1e-9. State `c9de85d` (2026-10-23): 238 of 261 port units ported (89.8 % of the lines); end to end, the box, linear extrusion, queries and mass properties work; primitives with curved faces, transformation, booleans and fillets do not yet.

Until solidcore is ready, the application uses the C++ upstream directly, at the same state solidcore is checked against (`usd-brep` @ `c9c5979`), since report 4 with the memory-error fixes from the fork `tangency-labs/smlib` (branch `capi-tess` @ `7eba8aa`) and since report 7 with `smlib/patches/0001` (`smlib/fetch.sh`). The assessment before the spike:

| Criterion | SMLib (C++) as interim | OCCT as interim |
| --- | --- | --- |
| Results when switching to solidcore | same algorithms, same topology | different fillets, different edge splitting, different failure cases |
| API concepts | like solidcore (`SM_API`) | own model, translation needed |
| Licence | Apache-2.0 | LGPL 2.1 with exception |
| Maturity | production kernel, but known UB/hangs (solidcore: `plan/upstream-issues.md`) | very widespread |
| STEP export | not included (upstream uses OCCT for it) | included |
| Wasm build | unknown, check in the spike | known to be feasible |

Decision (M0): SMLib. The Emscripten build works (`smlib/build-wasm.sh`, smlib.wasm ≈ 1.24 MiB Brotli, B1); OCCT is now only used for independent verification of the STEP export (B5).

### 2.2 Integration in the browser

| Part | Technique |
| --- | --- |
| SMLib | Emscripten build of the libraries `SMLib` and `SM_API` (CMake, `-O3 -fwasm-exceptions`, `ALLOW_MEMORY_GROWTH`, without threads) |
| Facade | Own C++ layer (~1–2 kLOC) with a flat C ABI: handles (`u32`) instead of objects, error codes instead of exceptions |
| Rust → SMLib | `kernel-smlib` calls the facade through a thin JS bridge (`#[wasm_bindgen(module = "/smlib-bridge.js")]`) |
| Data | Small data (points, edge IDs) is copied; meshes go from the C++ heap directly to the main thread as `ArrayBuffer` |

Two Wasm modules instead of one: `wasm-bindgen` does not support the Emscripten target. A shared module (Rust on `wasm32-unknown-emscripten`) would be possible, but the toolchain is more fragile. The bridge costs microseconds per call; kernel operations take milliseconds. Bulk calls (classifying many points) are batched.

**Native:** the same facade is built natively for the CLI and tests and linked via FFI. `smlib/CMakeLists.txt` follows the settings of solidcore's `harness/cpp`; `smlib/build-native.sh` builds `libSMLib.a` and `libSM_API.a` (included via `SMLIB_PREBUILT`), otherwise `crates/kernel-smlib/build.rs` builds them itself.

### 2.3 Switching to solidcore

- `kernel-solidcore` implements the same trait directly in Rust, in the same Wasm module. The JS bridge and `smlib.wasm` go away.
- solidcore builds for `wasm32-unknown-unknown`. According to the solidcore README the consumer needs a larger stack (`-zstack-size=8388608`), because Wasm has no guard page.
- Acceptance: the CLI evaluates all examples with both kernels and compares volume, face and edge counts and selected edges (section 8). Only when all match is the switch made.
- Until then `kernel-solidcore` can run alongside as a second implementation and reports which operations already work.
- Tessellation and polyhedra are explicitly outside solidcore's scope (`plan/closure.md`). M8 needs a custom tessellator in Rust (discretise edges once, planar faces as polygons with holes, curved faces in parameter space with trimming and edge matching, chord-error control; sketch in `docs/solidcore-migration.md`), otherwise SMLib or OCCT stays in the build for this alone.
- Fillet and chamfer have a public API in solidcore since `bba8d87` (not wired in `kernel-solidcore` yet); public entry points for transformation, sweeps (linear, revolution, pipe, skin), offset/shell and classification exist (state and results in `docs/solidcore-migration.md`).

### 2.4 Facade: key points

- **Handle table** with reference counting. Rust holds `Shape(u32)`, `Drop` releases it.
- **Provenance:** every face carries a tag as an SMLib attribute. SMLib passes attributes on in booleans (`SmAttribute::Split/Merge`); newly created faces (fillets, caps) get the tag of the operation. The engine assigns the tags (6.3).
- **New edges of a boolean** are determined independently of the kernel: an edge is new if its two adjacent faces come from different children of this operation and it does not lie on an edge of an operand (B7, F4). Smooth edges (`sc_edges` flag) never count.
- **Errors:** every function catches exceptions, returns a code and stores the message and the affected edges (for "edge red").
- **Hangs:** upstream has known infinite loops for certain inputs. See the watchdog in 7.3.

### 2.5 Export

- STL and 3MF from the tessellation, in Rust, independent of the kernel. 3MF multi-colour (default) with one part per colour and filament metadata for Bambu Studio/OrcaSlicer, "one STL per colour" as ZIP (section 16.4).
- STEP: neither SMLib nor solidcore has a writer. Implemented (test report 5): neutral B-rep description `cad_kernel::brep` (`Kernel::brep`, for SMLib via the facade query `sc_brep`), writer `cad-step` (AP242, optionally AP214; plane, cylinder, cone, sphere, torus, surface of revolution/extrusion, B-spline; line, circle, ellipse, B-spline), `Engine::export_step`, `mcad export -o teil.step`, `Export { format: "step" }`. Verification with OpenCASCADE: `tests/step/verify.py`.

---

## 3. Kernel interface in Rust

The engine knows only this trait. It follows the operations of `SM_API` so that `kernel-smlib` and `kernel-solidcore` implement it without a translation layer.

```rust
pub trait Kernel {
    type Shape: Clone;

    // Primitives (always in the local frame, placement via transform)
    fn make_box(&mut self, min: P3, max: P3, tag: Tag) -> KResult<Self::Shape>;
    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Self::Shape>;
    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Self::Shape>;
    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Self::Shape>;
    fn make_profile(&mut self, p: &Profile2d, tag: Tag) -> KResult<Self::Shape>; // rectangle, circle, polygon, spline

    fn transform(&mut self, s: &Self::Shape, m: &Mat4) -> KResult<Self::Shape>;
    fn boolean(&mut self, op: BoolOp, s: &[Self::Shape], tag: Tag) -> KResult<Self::Shape>;
    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: Radius, tag: Tag) -> KResult<Self::Shape>;
    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape>;
    fn extrude / revolve / loft / sweep / nurbs_surface / thicken / shell(...);

    // Queries
    fn edges(&self, s: &Self::Shape, samples: u32) -> Vec<EdgeInfo>;   // ID, sample points, tags of both faces
    fn faces(&self, s: &Self::Shape, samples: u32) -> Vec<FaceInfo>;
    fn classify(&self, s: &Self::Shape, pts: &[P3]) -> Vec<Class>;    // batched: inside/outside/boundary
    fn tessellate(&self, s: &Self::Shape, tol: f64) -> Mesh;           // triangles + edge polylines, each with tag
    fn mass_props(&self, s: &Self::Shape) -> MassProps;               // volume, area (CLI, tests)
}
```

`kernel-mock` is a pure Rust kernel with boxes and logged calls. It makes the parser, evaluation, cache and zone logic testable without C++.

---

## 4. Language in Rust

### 4.1 Parser (`cad-syntax`)

- Hand-written lexer and recursive descent with a Pratt parser for expressions. Error-tolerant: syntax errors produce error nodes, the rest continues to be parsed.
- AST with byte spans for every node and every literal. A lossless syntax tree (rowan) is not needed: write-back only replaces literals. Comments stay untouched.
- Every change re-parses the whole file (< 1 ms for hobby projects).
- **UTF-8 vs. UTF-16:** Rust counts in bytes, CodeMirror in UTF-16 units. The conversion happens in exactly one place (line index in the engine).
- New syntax compared to OpenSCAD: `grid g { x: …; }`, `g[a, *, _]`, `g.name`, `anchor`, `name: aufruf;`, `zone(...)`, `rotate(to = v)`.

### 4.2 Values with origin (`cad-eval`)

Every value carries its origin. This is the basis for handles, locks and write-back (spec: handles and write-back, rules 1–3).

```rust
struct Val { v: Value, origin: Origin }

enum Origin {
    Literal(Span),                         // directly in the code
    VarLiteral { var: DefId, lit: Span },  // variable with a literal definition
    GridRef { grid: GridId, planes: [PlaneSel; 3], expr: Span },
    AnchorRef { inst: InstId, anchor: Sym, expr: Span },
    Computed,                              // everything else: locked
}
```

- Vectors carry the origin per component. If only some components are literals, the handle is restricted to those axes.
- Arithmetic operations yield `Computed`. A variable with a literal definition that is passed through unchanged keeps `VarLiteral`.

### 4.3 Value types

`Number, Bool, String, Vector, List, Range, Undef, Function` as in OpenSCAD, plus:

| Type | Content | Rule |
| --- | --- | --- |
| `GridPoint` | point in the module frame + grid ID + plane names | behaves like a vector in expressions |
| `GridLine`, `GridPlane` | grid ID, selection, position | only in arguments that accept them; otherwise a type error |
| `PlaneRef` (`g.name`) | grid ID + plane name | becomes a number or a plane depending on context (12.1) |
| `InstanceRef` | named instance `k` | only `k.anker` and `dir(k.anker)` allowed |

Geometry is not a value: statements create nodes in the geometry tree, never values.

### 4.4 Evaluation → geometry tree (`cad-ir`)

```rust
struct Node {
    id: InstanceId,      // statement + instance path (call sites, loop indices)
    stmt: StmtId,        // source statement → line in the editor
    op: Op,              // Primitive, Transform, Boolean, Fillet, Zone, Profilop, ModuleCall, …
    params: Params,      // resolved values
    children: Vec<NodeIdx>,
    local_to_module: Mat4,
    modifier: Option<Modifier>,
    key: CacheKey,       // hash of op, params, keys of the children
}
```

- `InstanceId` says *where* a node is in the program (UI mapping). `CacheKey` says *what* it computes (cache). They are separate so that an inserted line does not empty the cache.
- Additionally per evaluation: grids (planes, values, origin, placement), anchors, named instances, inline hints, diagnostics, visibility scopes (which grids and instances are visible at which point; needed for snap targets).
- Modifiers: `*` and `%` remove the subtree from the result, `%` stays in the scene as helper geometry; `!` sets the root; `#` only marks.
- **Module preview:** entry point `eval_module(name, overrides)` for "cursor in module definition", the parameter bar and the CLI (`-D`). Shares the cache with the full evaluation.

### 4.5 Grids

- A grid is evaluated when the `grid` statement is reached: plane values as `Val` (literal planes draggable, expression planes locked), placement matrix from the preceding transformations.
- Checks: unique names across all axes, non-uniform `scale` → error, placement only on grids declared earlier in the same module.
- `mirror` before `grid` mirrors the matrix; names stay.
- Grid points yield coordinates in the module frame. On a module call, anchors are brought into the caller's frame with the instance transformation.

---

## 5. OpenSCAD compatibility

Goal: typical hobby files run unchanged as long as they need no mesh operations.

| Area | Scope |
| --- | --- |
| Full | syntax, variables, functions and function literals, modules, `children()`, `for`, `if`, `let`, `each`, list comprehensions, strings, ranges, `echo`, `assert`, math functions, `cube`, `sphere`, `cylinder`, `square`, `circle`, `polygon`, transformations, booleans, modifiers, `linear_extrude`, `rotate_extrude`, `offset` (exact arcs instead of facets, 17.9) |
| With deviation | `color()` only as display colour; `$fn/$fa/$fs` see 12.3; `polyhedron` only for small bodies with planar faces; `resize` as `scale` with the same rules |
| Full (projects) | `include`/`use` with multi-file projects (paths relative to the including file; without the search path `OPENSCADPATH`) |
| With deviation (report 16) | `text()` with the bundled fonts or font files of the project (17.10), `import()` of SVG (17.10; deviations for units without `viewBox`, strokes and fill rule) |
| Exact special cases (plan 21) | `hull()` (2D always; 3D: planar bodies, spheres/cylinders/cones on one axis, parallel prisms of equal height, equal-sized spheres) and `minkowski()` (body + sphere, profile + circle, extrusion + cylinder, two convex planar bodies); otherwise a warning, children unchanged (section 20) |
| Not planned | `projection`, `import` of meshes, `surface()` as a height map (the name is taken in the spec for NURBS surfaces) |

A corpus of `.scad` files (check licences first) runs in CI via the CLI. Measured per file: parses, evaluates, produces geometry.

---

## 6. Engine (`cad-engine`)

### 6.1 Flow of an evaluation

1. Parse. On a syntax error: send diagnostics, the last scene stays.
2. Language evaluation → geometry tree.
3. Send the preliminary scene immediately (`complete = false`: nodes, grids, anchors, handles, hints). Grids and handles are thus already visible. After the kernel job the complete scene of the same version follows (`complete = true`).
4. Kernel evaluation bottom-up, in steps (7.3); cache lookup per node via `CacheKey`.
5. Tessellation of the root and of the nodes requested by the UI; meshes as transferables.

### 6.2 Cache

- `CacheKey`: stable hash (e.g. `blake3`) over operation, parameters (floats with canonicalised bit pattern), child keys and kernel version.
- Entry: kernel shape, tag table, optionally meshes per tolerance. One LRU for node results and meshes by estimated memory (`cache_bytes`, default 512 MiB) and count (`cache_entries`); size of a shape via `Kernel::shape_bytes` (SMLib: from faces/edges, calibrated on the examples) or, in the CLI daemon, measured on the heap while the node is evaluated (report 10). Eviction releases kernel handles.
- **Transformations are not baked into the shapes for display.** A mesh is tessellated in the local frame and placed in three.js via a matrix. Identical parts in loops share one mesh.

### 6.3 Provenance (face/edge → line)

- A tag is the interned index of the cache key of the producing node (implemented instead of local tags shifted when combining). Equal content yields the same tag; the table only grows.
- After evaluation, `Evaluation::tag_nodes` maps each tag to the nodes of this tree (several for equal content). A cached result can thus be reused elsewhere without touching the geometry. Each result also carries the set of its possible tags (mapping of faces to children for new edges).
- The UI receives the triangle tags per mesh (`tri_tags`) plus the table `tag_nodes`; edges carry no node tags (hover over edges goes via the adjacent face, B2).

### 6.4 Edge selection via zones

1. Evaluate zones (ordinary bodies, not part of the result). `zone(linie, d)` and `zone(ebene, d)` are limited to the bounding box of the body plus `d`.
2. `edges(körper, n)` returns end points and sample points per edge.
3. Check all points in one batch with `classify` against each zone.
4. `select = "inside"`: all points inside or on the boundary. `"touch"`: at least one point.
5. Nested blocks from inside out; after a fillet the new boundary edges apply.
6. Result: edge IDs + count for the inline hint; empty selection → warning.

`$fillet`/`$chamfer`: fillet the edges of the primitive directly after it is created. `difference(fillet = r)`: new edges as in 2.4.

Fillet failed: the node returns its body without the fillet, diagnostic with the affected edge.

### 6.5 Handles and write-back

- The engine creates a handle list: position, kind (point, radius, height, rotation ring, plane …), reference to one or more literals (`Origin`).
- Dragging: the UI sends `DragHandle { id, pos, snap, alt }`. The engine computes the new literals and returns `TextEdit`s. The editor applies them; this triggers a normal evaluation with coarse tessellation.
- Snapping writes a reference (`g[innen_r, hinten, boden]`, `k.ecke`). Only grids and instances visible at the position of the statement are offered.
- Free dragging with Alt: numbers, rounded to a configurable step (default 1 mm, 15° for rotation). Shortest number representation, no trailing zeros.
- A drag is one undo group in the editor (CodeMirror: shared `userEvent`, `isolateHistory` at start and end).

---

## 7. Worker, concurrency, cancellation

### 7.1 Protocol

UI → engine:

| Message | Purpose |
| --- | --- |
| `SetSource { text, version }` | re-evaluation |
| `SetContext { module, overrides }` | module preview, parameter bar |
| `RequestMeshes { keys, tol }` | meshes for individual nodes |
| `DragStart / DragHandle / DragEnd` | handles (`Drag` always answers with `Edits`, even empty: at most one drag in flight) |
| `SnapTargets` | grid points and anchors (point handles), grid planes `g.name` (height, radius, plane) |
| `SetDragOptions { step_mm, step_deg }` | steps for free dragging (settings) |
| `Export { format, root }` | STL, 3MF, STEP |
| `Sketch { … }` | sketch editor (17.12) |
| `Measure { kind, picks, ray, hit, section, clip, click }` | measure mode (section 18): element under the mouse, measurement after the last click |

Engine → UI: `Diagnostics`, `Scene { version, nodes, grids, anchors, handles, hints }`, `Meshes`, `TextEdits`, `ExportResult`, `Progress`.

Transfer with `serde` + `serde-wasm-bindgen`, meshes as `Float32Array`/`Uint32Array` transferables. TS types are generated from Rust (`tsify` or `ts-rs`).

`EngineTransport` in TS encapsulates the path: `WorkerTransport` (browser) or `TauriTransport` (desktop, `web/src/engine/tauriTransport.ts`). The UI knows only the interface.

`TauriTransport`: messages to the engine are batched per microtask into one `engine_post` command (order is preserved); the responses arrive over a Tauri 2 channel as binary frames `[u32 header length][u32 data length][header JSON, padded to 4 bytes][data]` (format of `cad-ipc`). The header contains the `FromEngine` messages as JSON, the mesh arrays and export bytes are replaced by `{"$b": [offset, länge, "f32"|"u32"|"u8"]}`; the webview places `Float32Array`/`Uint32Array` directly on the received `ArrayBuffer` (no JSON number arrays, no copy). The desktop UI is built without worker and without Wasm (`pnpm build:tauri`, `vite --mode tauri` replaces `workerFactory.ts`).

### 7.2 Fine and coarse tessellation

- During a drag: coarse tolerance (10 × tolerance), root only. Draft mode: fillets over 25 ms (last measurement, not in the cache) are suspended (B4, C20).
- On `DragEnd`: fine tessellation. Meshes are cached per tolerance.

### 7.3 Cancellation without `SharedArrayBuffer`

- The engine computes in steps: `engine.step(budget_ms)` processes nodes until the time budget (e.g. 20 ms) is reached, then returns.
- Between two steps the worker loop yields control to the event loop (`MessageChannel`), processes new messages and on `SetSource` cancels the running evaluation. Finished nodes stay in the cache.
- A single long kernel operation cannot be interrupted. For hobby projects this is rarely a problem.
- **Watchdog:** the main thread terminates the worker if no response arrives for 20 min (protection against kernel hangs) and restarts it. In the desktop app the supervisor in the app process does the same (`cad-desktop/src/engine.rs`): it counts messages sent and messages taken over by the engine, kills the engine process (`kill`) after 20 min without a response and restarts it with the same `BlockKeys`/`SetFiles`/`SetContext`/`SetSource` as in the browser; a thread alone could not be terminated and would share the global SMLib state with the hanging call. The cache is lost in the process. The content key of the node from the last `Progress.current` is blocked; the new worker receives `BlockKeys` before `SetSource`. The node reports "Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung" (timeout, node blocked until the next change), the line turns red (B4, C19).

### 7.4 Versions

The scene carries the source version. Until a new scene arrives, the UI maps source positions of old scenes through the intermediate editor changes (`ChangeSet.mapPos`), so that highlights do not jump while typing.

---

## 8. CLI (`cad-cli`)

A native program `mcad` (crate `cad-cli`; up to 0.2 the binary was called `cad`) on the same crates as the browser, with a natively linked kernel. Purpose: use by Claude Code and other agents (develop and check models entirely without UI, also in parallel with a human in the UI), tests in CI, kernel comparison before switching to solidcore.

| Command | Output |
| --- | --- |
| `check <datei>` | diagnostics with line and column |
| `outline <datei>` | overview in one call: modules (file:line, parameters with default value and type, grids and anchors with default values), functions, variables with values, named instances, parts (bounding box, volume), `include`/`use`, diagnostics |
| `eval <datei> [--module m] [-D name=wert]` | tree, values, inline hints, grids, anchors |
| `inspect <datei> --line n` | what the line produces: nodes, bounding box, volume, selected edges, warnings |
| `render <datei> -o bild.png` | image: `--view`, contact sheet `--views iso,front,top,right\|all` (same scale), free camera (`--azimuth/--elevation/--zoom/--target/--fit`, `--perspective`), selection (`--part`, `--line` repeatable, `--node`, `--only`), overlays `--show grids,anchors,handles,edges,labels,bbox,dims`, section `--section x\|y\|z=wert`/`--section-plane p,n`/`--section-flip`, `--xray` |
| `measure <datei>` | `--between A B` (anchor, grid point, coordinate), `--bounds/--volume/--area ZIEL`, `--interference` (kernel intersection of all part pairs), `--clearance A B` (distance via the tessellation); on the exact B-rep (section 18) `--distance`, `--radius`, `--angle`, `--length`, `--area face@P`, `--body`, `--at`, in a section `--section …` (contours, circles, thinnest wall) and `--wall P` |
| `watch <datei> [--render bild.png …]` | watches the file and project files (`notify`), re-evaluates in the same session (cache stays), one JSON line per run |
| `set <datei> --param m.p=wert \| --literal [datei:]z:s=wert` | change code through the engine (default value, literal), checks before writing, atomic, `--dry-run` |
| `export <datei> -o teil.stl\|.3mf\|.step [-D …] [--schema ap214] [--part name] [--all-parts]` | export file (STEP: exact B-rep, AP242), single or all parts |
| `diff-kernels <datei…>` | comparison SMLib ↔ solidcore (development only, `docs/solidcore-migration.md`) |

- Every command has `--json` with a fixed, versioned schema (`schema_version`) and defined exit codes (0 ok, 1 model error, 2 invocation, 3 kernel). Without `--json` there is readable text.
- `mcad` without arguments starts the desktop UI, `mcad datei.mcad` or `mcad ordner/` with this file or folder as the project (feature `gui`, default; `--no-default-features` builds a CLI without UI). Without a graphical session (no `DISPLAY`/`WAYLAND_DISPLAY`) or without `gui`: message listing the headless commands, exit 2. `mcad --help` explains both.
- `mcad ui status [--json]` / `mcad ui show datei [--line N] [--module m]` / `mcad ui screenshot -o x.png` talk to running windows over local IPC (`cad-ipc`, Unix socket `ui-<pid>` in the user directory, own user only); without a running UI, exit 3. For the workflow "user in the UI, agent edits and checks" see the skill.
- `render` uses a small software rasteriser in Rust (`cad-render`: Z-buffer, shading, edge lines, ortho/perspective, markers, labels with an embedded bitmap font). It needs no GPU and therefore also runs in containers and CI.
- Section: clipping in the renderer with a filled cap using the stencil principle (cap where the view ray crosses the surface an odd number of times up to the plane), no kernel boolean. Same method and same convention as the later GPU section view of the UI: the plane through `p` with normal `n` removes the half that `n` points into (`(q − p) · n ≤ 0` remains).
- **Daemon** (report 10): kernel results are handles of the native library and cannot be serialised; instead of an on-disk cache, a background process holds the engine cache. `check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure` and `set` connect via `cad-ipc` (endpoint `daemon`, socket folder 0700, socket 0600, peer UID) to `mcad daemon serve` or start it detached (new session, output to `daemon.log`); exactly one daemon via an `flock` lock. The client sends arguments, working directory and stdin; the daemon runs the same command function (session on its engine, change into the working directory, file writes are recorded) and returns stdout, stderr, exit code and the files the client writes. Output is the same as without the daemon, including the face tags: the session numbers them in the order a fresh engine would assign them. One engine for all projects (content-based keys); limits 512 MiB (node results + meshes; cost of a node measured on the heap in the daemon, otherwise estimated from the topology), 16384 entries, 8 projects, cache cleared when RSS exceeds start + 1.5 × limit; exit after 15 min without a request; watchdog 30 min (the daemon exits, the command runs without the daemon and reports this); a different build ID replaces the daemon. Disabled with `--no-daemon`, `MCAD_NO_DAEMON=1`, `[daemon] enabled = false` (`$XDG_CONFIG_HOME/mcad/config.toml`); without a daemon (Windows, errors) the command silently computes itself. `mcad daemon status|stop|serve`. `mcad watch` keeps its own in-process cache.
- **Rename `cad` → `mcad` (0.3.0):** binary, program name, app ID `dev.mcad.desktop` (window title "… – mcad", desktop file `dev.mcad.desktop.desktop`, file types `text/x-mcad` and `application/x-openscad` in `packaging/linux/mcad.xml`), Arch package `mcad` (replaces `controller-cad`), configuration `$XDG_CONFIG_HOME/mcad/config.toml`, socket folder `$XDG_RUNTIME_DIR/mcad` or `/tmp/mcad-<uid>`, environment `MCAD_*`. Fallback: `CAD_*` is read if `MCAD_*` is missing (`cad_ipc::env_var`), the old `controller-cad/config.toml` if the new one does not exist; the desktop app's list of recently opened files is taken over once from the folder of the old app ID. New files are named `.mcad` (UI "Neu" (New), "Speichern unter" (Save as), new folder `main.mcad`, examples, test data); `.scad` opens everywhere (CLI, `include`/`use`, file dialogs, folder projects, upload, ZIP), shared check `cad_eval::is_model_file` or `isModelFile` in the UI. These stay `.scad`: `tests/models/openscad-compat/`, `tests/openscad-compat/` (comparison with OpenSCAD), `tests/color-review/cases/` (snapshot of an independent review with images and report) and the case study of report 8. Unchanged: crate names `cad-*`, repository `controller-cad`, font name "cad stencil mono" (appears in models), IndexedDB name of the browser app (saved projects).
- In addition, a guide for Claude Code in the repo (`.claude/skills/cad-cli/SKILL.md`): outline → edit → check → render (contact sheet with grids/anchors) → inspect/measure → export.

---

## 9. UI (TypeScript/React)

| Area | Technique |
| --- | --- |
| Build | Vite, pnpm; Wasm via `wasm-bindgen --target web` |
| State | Zustand (stores for source, scene, selection, parameter bar) |
| Editor | CodeMirror 6: Lezer grammar for syntax highlighting; diagnostics, inline hints, greying out (`*`) and line colours as decorations from the engine data |
| 3D | three.js directly (implemented without react-three-fiber and `three-mesh-bvh`); picking with the raycaster; edges and grid lines as `LineSegments` |
| Picking | raycast → triangle → `tag` → `InstanceId` → line; hover and click without an engine call |
| Display states | material variants per node: full, dimmed, semi-transparent, red semi-transparent (`#`, subtracted), grey (`%`), outline |
| Handles | own gizmos (point, arrow, rotation ring with 15° detents, plane handle); snap search in screen space against grid points and anchors |
| Grids | planes as lines at the edge of the part's bounding box; solid (literal) or dashed (expression); by default only those used by the current line |
| Parameter bar | from the module signature; values per module in the session; "Als Standard übernehmen" (set as default) creates a `TextEdit`; since the UI rework (section 19) the "Parameter" (parameters) tab of the side panel |
| Path bar | from the module context, centred in the header next to the file name (section 19); double-click on an instance jumps into the definition |
| Files | Browser: autosave of the project in IndexedDB; multi-file projects as tabs; open/save folders via the File System Access API (Chromium), otherwise upload and download (file or ZIP). Desktop: real file system (open, atomic save, save as, folder as project, recently opened), unsaved files marked with ●, "Automatisch speichern" (save automatically; default off, delay configurable), no IndexedDB |
| File watcher (desktop) | `notify` on the folders of the project files (atomic save replaces the file); known contents as hashes, own save operations do not come back; clean file: reloaded silently as minimal line changes (cursor/scroll stay, own undo step); unsaved changes: bar "Neu laden / Meine behalten / Vergleichen" (reload / keep mine / compare; diff view); deleted/renamed: bar, saving creates the file anew |
| Git (desktop) | gutter markers against `HEAD`: new green, changed blue (3 px, opacity 0.5–0.55), deleted as a red triangle between the lines; hover shows the old lines; debounced 300 ms while typing, on save, on external changes and on window focus; untracked files without markers |
| Measuring | mode "Messen" (measure; section 18): context bar with measurement kind and "Im Schnitt" (in section), snapping to vertices, edge midpoints, circle centres, edges, faces or the section contour, dimension lines and labels in the view, list "Messungen" (measurements) with copy as text, comment and `mcad measure` |
| Section view | tool "Schnitt" (section): plane x/y/z or free (normal from view direction or input), drag handle (snaps to grid planes of the context module and anchors, Alt free), flip, cap (stencil, hatched, alternating direction per body, transparent bodies without cap), contour; status line and "Als `mcad render …` kopieren" with the same convention as the CLI |
| Parts | "Bauteile" (parts) tab of the side panel: visibility, volume, export per part in the ⋯ menu, material outside the parts ("no part"), total |
| Colours & filaments | "Farben" (colours) tab of the side panel (swatch, name, lines, slot selection, volume, PLA mass), "Farbregionen zeigen" (show colour regions), exports "3MF mehrfarbig" (3MF multi-colour), "STL je Farbe (ZIP)" (STL per colour (ZIP)), "3MF einfarbig" (3MF single-colour); colour per face in the view (vertex colours from `tri_tags` → `tag_nodes` → `color`) |
| Theme | icon button with System/Light/Dark menu in the header (localStorage `cad-theme`); all colours as CSS variables (`styles.css`), the 3D view re-reads them on change; palette colours adjusted per theme (`logic/palette.ts`) |

A line can have several instances (loops, multiple module calls). The UI indexes the scene by `stmt` and highlights all of them.

---

## 10. Repository structure

```
crates/
  cad-syntax/       lexer, parser, AST, spans (with file ID)
  cad-eval/         name resolution, values with origin, grids, anchors, handles, write-back, include/use
  cad-ir/           geometry tree, CacheKey, radius law (Law)
  cad-kernel/       trait Kernel, shared types (P3, Mat4, Mesh, Tag …), neutral B-rep
  kernel-mock/      test kernel in Rust
  kernel-smlib/     kernel via the SMLib facade (features: wasm | native)
  kernel-solidcore/ kernel via solidcore (opt-in, feature solidcore)
  cad-engine/       cache, provenance, zones, parts, export STL/3MF/STEP
  cad-step/         STEP writer (AP242/AP214)
  cad-measure/      measuring on the exact B-rep (distance, radius, angle, length, area, section, wall thickness)
  cad-render/       software rasteriser for the CLI
  cad-session/      protocol and session (worker and CLI), TS types (ts-rs)
  cad-wasm/         wasm-bindgen entry point for the worker
  cad-ipc/          local IPC (Unix socket per user, versioned frames, request/response, events)
  cad-desktop/      desktop backend without Tauri: engine process + supervisor, binary format, files, watcher, Git
  cad-app/          Tauri 2 app (commands, channel, windows, control socket)
  cad-cli/          CLI (starts the UI when given no command)
  compat-tests/     OpenSCAD comparison stage 1
smlib/
  fetch.sh          sources: tangency-labs/smlib @ 7eba8aa (= usd-brep c9c5979 + fixes) + patches/
  facade/           C++ facade; CMakeLists.txt, build-native.sh, build-wasm.sh
third_party/smlib/  SMLib sources (not checked in)
web/
  src/app/  src/editor/  src/view/  src/engine/  src/logic/  src/files/  src/state/  src/platform/  e2e/  e2e-tauri/  scripts/
examples/           example files (also test cases): spec/, openscad-compat/, project-housing/
tests/              openscad-compat/ (reference, compare.py), step/verify.py
tools/report/       generators for the test reports
scripts/ci/         check and build steps (run locally, Makefile)
docs/               plan, solidcore migration, test reports (reports/)
```

---

## 11. Tests

| Level | Tool | Content |
| --- | --- | --- |
| Parser | `cargo test`, `insta` | AST snapshot, error recovery, fuzzing (`cargo-fuzz`) |
| Evaluation | `kernel-mock` | values, origin, grids, anchors, visibility, diagnostics |
| Write-back | Rust | handle → TextEdit → re-parse → expected value |
| Kernel | `kernel-smlib` native | volume, face count, edge selection, provenance after booleans and fillets |
| Examples | CLI `eval --json`, `render` | golden values (volume, bounding box, edge count) and reference images per file |
| OpenSCAD corpus | CLI | share that parses / evaluates / produces geometry |
| UI | Vitest, Playwright | editor ↔ view, dragging, undo, snapping |
| Performance | browser benchmarks | load time, evaluation of the examples, dragging at ≥ 30 fps |

Checks: no hosted CI (the GitHub workflows were removed on 2026-10-30). Each step is a script in `scripts/ci/` and runs locally (also via the `Makefile`):

| Script | Content |
| --- | --- |
| `rust.sh` | Tauri system packages (apt, `scripts/ci/desktop.sh deps`: the workspace contains `cad-app`), `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` (native and `cad-wasm` for wasm32), `cargo test --workspace`, `cargo check -p cad-wasm --target wasm32-unknown-unknown` |
| `web.sh` | emsdk 6.0.13, `smlib/build-wasm.sh`, `web/scripts/build-wasm.sh`, `pnpm install --frozen-lockfile`, `typecheck`, Vitest, Node tests (smlib.wasm, engine, transport), `pnpm build`, Playwright |
| `compat.sh` | OpenSCAD 2021.01 (apt): regenerate the reference and compare with `reference.json`, self-test of `compare.py`, `include`/`use` against OpenSCAD, `compare.py --eval` with the release CLI; STEP check with `cadquery-ocp==8.0.1.1.0` |
| `desktop.sh` | Tauri packages (apt), SMLib native, `pnpm build:tauri`, `cargo build --release -p cad-cli`, Tauri E2E with `tauri-driver` + WebKitWebDriver under Xvfb (`web/e2e-tauri/desktop.e2e.mjs`) |
| `solidcore.sh` | `mcad diff-kernels` against solidcore `main` (`docs/solidcore-migration.md`) |

---

## 12. Open questions in the spec

Proposals for how the plan handles them until they are resolved:

1. **`g.name` is a number or a plane.** In `mirror(g.deckel)` a plane, in expressions a number. Proposal: type `PlaneRef`, which converts according to the expected argument type.
2. **Named instances in loops.** `k: gehaeuse();` inside `for` is not defined. Implemented: `k: …` in `for` is an error; `k[i]: …` names the instances with an integer index (`k[2].anker`, snap writes `k[2].ecke`).
3. **`$fn` from OpenSCAD.** In OpenSCAD `cylinder(r = 5, $fn = 6)` produces a hexagon, which is often used for nuts. A global `$fn = 100`, by contrast, is only meant for smooth display. Proposal: `$fn` between 3 and 12 at the call produces a prism as in OpenSCAD; otherwise exact geometry and a hint.
4. **Length of `zone(linie, d)`.** A grid line is infinite. Proposal: limited to the bounding box of the body plus `d`.
5. **Dragging default parameters.** If a value comes from a default parameter (`w = 40`) and the call does not override it, dragging changes the default value and thus all calls. Proposal: allow it, colour the handle differently.
6. **Fillets under `scale`.** Fillet in the local frame, transformation afterwards. Fits the architecture.
7. **2D shapes with z deviation.** Warning per spec; projected into the xy plane of the current frame.
8. **Alt step size** (open point 1): configurable, default 1 mm and 15°.
9. **Multiple parts** (open point 2): implemented as proposed – named instances at the top level of the main file are parts and determine the export names (export per part as STL/3MF/STEP, several as ZIP or as 3MF with one object per part; CLI `export --part`, `--all-parts`). Assemblies remain open.

---

## 13. Milestones

Each milestone ends with a working state in the browser and in the CLI.

| # | Content | Result | Status |
| --- | --- | --- | --- |
| M0 | **Spike:** build SMLib with Emscripten; facade with box, cylinder, boolean, fillet, classification, tessellation, attribute propagation; call from Rust in the worker and natively; display in three.js | size, load time, call cost, stability measured; decision SMLib or OCCT | done (SMLib, B1) |
| M1 | Language core: parser, variables, functions, modules, `for`/`if`/`let`, 3D primitives, transformations, booleans; editor with diagnostics and inline values; CLI `check`, `eval` | OpenSCAD-like parts can be modelled | done (B1) |
| M2 | Scene and mapping: provenance, cursor/hover in both directions, dimming, modifiers, cache, stepwise evaluation, watchdog, module context, path bar; CLI `inspect`, `render`, `export` (STL) | code ↔ view complete; Claude Code can check and view models | done (B1, B2) |
| M3 | Grids and anchors: `grid`, access, placement, visibility, `anchor`, named instances, `rotate(to = …)`, display | grids usable | done (B1, B2) |
| M4 | Handles and write-back: value origin, handles for primitives, transformations and grid planes, snapping, undo, parameter bar | editing in the view | done (B3) |
| M5 | Fillet and chamfer: `$fillet`, `fillet =` on booleans, zone blocks, `select`, error display | fillets as in the spec | done (B4, B7) |
| M6 | 2D and profile operations: profiles, `linear_extrude`, `rotate_extrude`, `loft`, `sweep`, `surface`, `thicken`, `shell` | free-form shapes | done (B4, B7) |
| M7 | Files, 3MF, OpenSCAD corpus, examples, performance | first complete version | done (B1, B6; performance in B1–B4) |
| M8 | Switch to solidcore: `kernel-solidcore`, comparison via `diff-kernels`, custom tessellator (not in solidcore's scope), remove SMLib Wasm | ready for release | open: waiting for solidcore (pin `c9de85d`: 241 of 942 nodes equal, 0 different; box and simple extrusion work, other primitives, transformation, booleans, fillets do not); estimate from the research (uncertain): all primitives 1–2 weeks, booleans 2–4, fillets/sweeps/shell 6–10 weeks; tessellator separate work; `kernel-solidcore` and `diff-kernels` exist |
| later | STEP export, variable radius, multiple parts, `include`/`use` | | done (B5, B6, B7) |
| later | Tauri version (`TauriTransport`), file watcher, Git gutter, `mcad ui`, section view | | done (B9) |
| later | CLI daemon | | done (B10) |
| later | multi-colour, multi-filament 3MF, dark theme (section 16) | | done (B11) |
| – | CI (GitHub Actions) | | removed (2026-10-30); checks run locally with `scripts/ci/` |

Dependencies: M1 → M2 → M3 → M4 → M5. M6 depends only on M2. M8 depends on solidcore's progress and can partly start at any time after M2.

---

## 14. Risks

| Risk | Countermeasure |
| --- | --- |
| SMLib cannot be built with Emscripten | did not occur: the build works (B1); OCCT fallback not needed |
| Kernel hangs or behaves undefined for certain inputs | watchdog with restart and block (7.3); fork with memory-error fixes and patch 0001; checked booleans in the facade; report found cases to solidcore |
| solidcore is not ready for the planned release | releasing with SMLib C++ is possible licence-wise (Apache-2.0); switch afterwards |
| Attributes are lost in individual operations (provenance) | fallback via geometric mapping; tests per operation |
| Fillets fail in edge cases | clear error message with the edge, show the body without the fillet (spec-compliant) |
| Wasm stack without guard page (solidcore) | set the stack to 8 MiB, as described in the solidcore README |
| Memory (wasm32, 4 GB) | LRU cache with memory estimate, release handles consistently |

---

## 15. Known limitations (from test reports 1–14)

As of report 7; points fixed in earlier reports are omitted (e.g. F1–F4 from B4, fixed in B7).

**Kernel (SMLib)**

- Booleans are unreliable in special configurations; the facade detects wrong results only by sampling (an error between the probe points goes undetected). The check uses SMLib's point classification, which itself errs on rotated spline bodies; if no variant passes, SMLib's result stays. In the offset fallback the second operand deviates by up to 10⁻⁴ of the model size. The check costs time (+38 % across all examples). (B1)
- If bodies touch only along an edge, the union is not a closed body; `mcad` reports volume 0 (the STL is fine). (B1)
- Memory errors in SMLib can make results depend on the heap; the shell pre-check covers only planar, opposing walls. Patch 0001 removes the dependency on the address order of the pointer tables. (B1, B7)
- `shell` built from booleans only for planar opened faces; for curved opened faces SMLib's result or its error message stays. Wall thickness ≥ fillet radius still fails (SMLib's offset). The booleans cost time (`dose_shell` ≈ 1.9 s). (B7)
- Variable radius: SMLib's solver is sensitive (steep gradients at pocket corners, circular edges with varying radius fail); jumps are smoothed over about three sample-point spacings; no variable radius for `chamfer` and `$fillet`; laws only from the pure expression core (no strings, list comprehensions or recursion over `p`). (B7)
- Smooth edges per SMLib's criterion (2°): nearly tangent edges are never selected; the check "old edge of an operand" is only as precise as their sampling for curved edges. (B7)
- Tessellation of free-form faces refines globally, planar faces of the same body get refined too (e.g. 15 730 triangles for `12_linear_extrude_twist`). Twist extrusion is a cubic loft approximation (sections every 7.5°); circular profiles with non-uniform `scale` and `scale = 0` are not supported. (B1)
- SMLib integrates faces only to about 10⁻⁴ accuracy. (B5)
- Prebuilt libraries (`SMLIB_PREBUILT`) must be rebuilt after a patch change (header change); a symlink `third_party/smlib` is only checked by `fetch.sh`. (B7)

**Language and engine**

- OpenSCAD facets, we compute exactly: volumes of round parts differ by 0.01–0.3 %, for `twist` OpenSCAD itself is off by ~1 %. (B1)
- `include`/`use`: no search path (`OPENSCADPATH`); a library's variables are evaluated once per evaluation; files included twice are loaded twice; at most 255 files of 16 MiB each; snap visibility of a grid named in an included file ends at the end of that file. (B6)
- Parts only in the file context; a part inside a boolean operation cannot be hidden separately; STEP of several parts is one product without assembly structure; assemblies are open. (B6)
- The engine's tag table only grows; edges between faces of two children with equal content count as old (same tag). (`cad-engine`, module comment)

**STEP**

- No p-curves (readers project the 3D edges); edge curves of booleans are B-splines, circles/lines are recognised; faces without a B-spline form would be approximated (0 in the examples). Colours as `STYLED_ITEM`, not checked by `verify.py`. 2D results are rejected; in the browser the timestamp is fixed. (B5)

**UI**

- Hover over edges goes via the adjacent face; on ambiguity (equal content in several places) it is resolved by position within the node bounding boxes, otherwise all lines are marked; grid expressions in loops show the point of the first iteration. (B2)
- Snapping of 1D handles only to planes on which the handle lands exactly after writing (not under moved subtrees or with `center = true`/`d =`). Point handles move in the plane perpendicular to the view direction (axis-constrained with Shift or X/Y/Z, B4). Inline toggles only for bool literals of assignments (number literals without an inline slider, it got in the way; values via the parameter bar). Parameters without a default value become 0/false/[0, 0, 0] on reset. (B3)
- Zone edges and opened faces are determined by the view on tessellated zone meshes (the engine does it exactly); in edge cases the display can differ. The display of the `sweep` path interpolates like the facade and differs if the method in the kernel changes. (B4)
- The watchdog block applies to the content key (equal nodes elsewhere are blocked too) and lasts until the page is reloaded; the test blocks the worker via a test message, a real hanging kernel call was not reproduced. (B2, B4)
- Measurements come from headless Chromium with SwiftShader (without GPU) and are only meaningful relative to each other. (B2–B4)

**Desktop, section view (B9)**

- Control IPC only on Unix (Linux, macOS); on Windows `mcad ui …` and the control socket report "nicht umgesetzt" (not implemented; the app itself runs). One window per app process; `mcad datei.mcad` starts another process instead of using the running window.
- The hanging engine process is terminated; the watchdog tests block it via a test message (`__debugHang`), a real hanging kernel call was not reproduced.
- The file watcher compares contents (hash); an external change that writes exactly the UI's content goes unnoticed (intended). A rename is detected as a deletion, not as a new name.
- Git markers only against `HEAD` (not against the index), via the `git` command; without `git` in `PATH` no markers.
- "Speichern unter" and the open dialogs are native dialogs and not automated in the E2E tests (the path via the command line and `mcad ui show` is).
- Section: one plane; caps only for opaque bodies (like the CLI); the contour is computed on the CPU for each position (the frame rate drops for very large meshes); picking ignores the removed part, handles and grids are not clipped. The copied CLI camera is orthographic (UI perspective) – the image matches in the section, not pixel-exact.
- Measurements under Xvfb without GPU (WebKitGTK with software GL, Chromium with SwiftShader) are only meaningful relative to each other.

**CLI daemon (B10)**

- Unix only (Linux, macOS; peer UID check only on Linux, otherwise only the 0700 folder); on Windows each command computes itself.
- One worker thread: simultaneous CLI calls wait for each other (without the daemon they ran in parallel in separate processes).
- The watchdog (30 min) also hits a legitimately long command: the daemon exits, the command is repeated without the daemon (double the time); configurable. A crash in the kernel (signal) ends the daemon as well; the repetition can crash locally in the same way.
- Memory measurement per node only with glibc (`mallinfo2`, allocations of other threads are counted too); otherwise the estimate from faces/edges, which can be off by a factor of 2–10 per model (free-form faces). RSS is checked after each request, not during it.
- Panic in the daemon: exit code and message as without the daemon, but the runtime's panic message goes to the log instead of stderr. The counters `cache_hits`, `kernel_calls`, `cached` and the time fields show what the daemon actually did.
- Build ID from size and modification time of the program: if two `mcad` binaries alternate (debug/release), they keep replacing each other's daemon.
- The engine's tag table keeps growing; after 2²⁰ tags the engine (with cache) is replaced.

**Colours, multi-filament, theme (B11)**

- Colour regions depend on kernel booleans (intersection, difference, union per `color()`): where SMLib loses material (e.g. union with a torus, B1), the regions are wrong; the engine then reports "Farbregionen ungenau" (colour regions inaccurate; sum ≠ volume). Several booleans per colour cost time; the UI only requests them while the colour list is open (cached, colour and slot changes recompute no nodes).
- `color()` inside a 2D subtree (under `linear_extrude` etc.) colours faces but does not split volume; the extrusion is one region. `resize` is taken into account when placing. `hull`/`minkowski` create their own material: whatever extends beyond the children is uncoloured (default entry), unless `color()` is outside.
- Material without `color()` in an otherwise coloured part (e.g. a fillet between two colours) belongs to the colour of the first coloured subtree; it gets its own default entry (slot 1) only if the part has uncoloured primitives.
- The colour list and multi-colour export consider the parts (named instances); material outside of parts is then missing from the list (the export of the root contains it as one object).
- Bambu Studio reads `project_settings.config` only from its own files; there the filament colours are set by the colour dialog for 3MF with a colour group (newer versions) or by the user. OrcaSlicer takes over the slots, not the colours. Not checked in the container (no slicer with the Bambu importer available): the mapping follows the source code (`bbs_3mf.cpp`), checked with a model of the importer, lib3mf, trimesh and PrusaSlicer 2.7 (loads six objects at the right place, without slots).
- Colour vision deficiency: 16 colours cannot all be separated well for every type; smallest ΔE_OK for deuteranopia 0.070 (red/green), protanopia 0.052 (orange/green), tritanopia 0.031 (teal/blue). The UI shows name and slot for each colour.
- White on a light and black on a dark background barely differ in flat colour; shading and edge lines make the bodies visible, the display variants (white #e9e8e5 light, black #3b3d41 dark) help somewhat.

**Corrections colours and daemon (B12)** – replaces the B11 points on 2D colours, material outside of parts and the colour dialog:

- The view shows the exact colour regions only once they are computed (booleans per colour node, cached; always requested for models with colours); before that, while dragging and for the highlighted subtree, the approximate colour per face applies (bounding-box test for nodes of equal content).
- STEP does not split faces at colour boundaries; a face merged across the boundary gets the majority colour of its sample points.
- Mixed colours in a 2D profile under an extrusion are not split into bodies per colour (warning, default); a uniform 2D colour colours the extrusion.
- Random colours that are shifted to avoid collisions in the model depend on the unknown names before them.
- Without a colour group, programs that only read 3MF colours show the model in one colour (`--color-group` for them; Bambu then opens the colour dialog). Bambu Studio/OrcaSlicer not checked in the program itself (source code, model of the importer).
- The daemon's fallback socket is fixed under `/tmp/mcad-<uid>/`.

**Sketches and namespaces (B14)**

- There are no transformations inside a sketch (a module called in the sketch
  draws in sketch coordinates).
- The self-intersection check works on sampled polylines (tolerance relative to the
  sketch size); intersections *between* contours are allowed (union by nonzero).
  The nesting of holes and islands assumes non-intersecting contours.
- Sweep along a sketch: paths with corners only as mitre (planar) and then with a profile
  centred on the origin; a closed path is not supported.
- STEP: surfaces of revolution of arcs stay `SURFACE_OF_REVOLUTION` of the exact curve (sphere and
  torus are not written as such).
- Loft of different segment counts joins each section into one curve (C0 at the joints);
  the faces then no longer follow the segments.
- Namespaces: the constants of a module are evaluated in a new namespace on each access
  (no caching); `use`/`include` provide namespaces like local modules.
  A module with the same name on different paths is selected by path in the context; `--module
  name` without a path takes the first.

**Corners, offset, stroke (B15)**

- `offset(delta)` on arcs extends the tangents (mitre of the tangent lines); OpenSCAD only knows
  polygons there. B-splines enter `offset`, `stroke` and 2D booleans as biarcs (tolerance
  1 µm) – the result contains arcs instead of the spline; `fillet2d` of a single contour and
  `corner` keep the spline (trimmed).
- `fillet2d`/`chamfer2d` have no radius handle (only `corner`); zones are 2D shapes and
  `zone(linie | ebene, d)`, 3D zones are ignored with a warning.
- The exact region classifies each piece via test points at a distance of ~10⁻⁶ of the
  model size: boundaries that come closer without intersecting or overlapping can be classified
  wrongly. Candidate pairs via x sorting (quadratic in the worst case).
- 2D booleans *between* results (`difference() { stroke(…); stroke(…); }`) remain
  3D booleans of the extruded parts; only `offset`, `fillet2d`, `stroke` compute exactly in 2D.
- `stroke`: mitres longer than 4 half-widths are bevelled (fixed); an arc with a radius
  below half the width is swept as a sector plus mirrored sector.

**solidcore (M8)**

- Pin `c9de85d`: `diff-kernels` over 48 examples, 942 nodes: 241 equal (box, linear extrusion of a polygon, 2D nodes), 288 unsupported, 413 blocked, 0 different (previously `3a6a50b`: 101/267/487/0 of 855). Cylinder/cone (`surface.trim_srf_cache`), sphere/torus (`topology.face_p2`), booleans (`topology.edge`, `query.face_p1`) abort; `transform` and `classify` answer but compute wrongly and are checked by `kernel-solidcore` and reported as `Unsupported`. Fillet/chamfer without public API. (B5, `docs/solidcore-migration.md`)
- Tessellation and polyhedra are outside solidcore's scope: no switch without a custom tessellator (display, STL/3MF).

---

## 16. Colours, filaments, dark theme (B11, corrections B12)

### 16.1 Palette

One source: `crates/cad-eval/src/palette.rs`; `web/src/logic/palette.ts` is generated from it
(test `cad-eval --test palette`), the CLI uses the Rust functions. 16 colours, values from
OKLCH targets (muted, chroma ≤ 0.18), optimised for distance in OKLab for normal vision and
simulated deuteranopia/protanopia (Machado 2009): `red #c74240, orange #dc813b, yellow
#f7d974, lime #91ca55, green #4ca167, teal #217274, cyan #6eccec, blue #2e65b9, purple
#693d8c, magenta #cd67b7, pink #f7b8c6, brown #6e4223, black #202225, gray #7b7d81, silver
#abaeb2, white #f4f3f0`; `grey`, `aqua`, `fuchsia` as CSS spellings. Selection: the
most frequent names in OpenSCAD files (red, green, blue, yellow, cyan, gray, black, white,
magenta, orange, silver, lime, purple) plus teal, pink, brown for typical filaments;
`indigo`/`slate` are dropped (too close to blue/purple and gray). Display per theme: black in
dark at L 0.36, dark tones +0.04; white in light at L 0.93; exports use the
base values.

**Unknown names** (B12, F5): warning at the argument; colour from a sequence of candidates per
name (FNV-1a 64 of the lower-cased name, further candidates via SplitMix64): hue,
lightness L 0.45–0.80 and chroma 0.06–0.16 from the hash (chroma reduced until within sRGB). The
first candidate with ΔE_OK ≥ 0.07 to all palette colours and to the default and – in the
model, in order of first occurrence – ≥ 0.06 to the random colours of other names is chosen
(at most 256 candidates, otherwise the one with the largest distance). The same name (case-insensitive)
always has the same colour in the model; without a collision in the model it is the same across models, sessions,
CLI and UI. Different names never coincidentally get the same value.

**Values** (B12, F7): hex and vectors exact; vector values outside 0–1 and `alpha` outside
0–1 are clamped with a warning (for 0–255 values with a hint to `"#rrggbb"`). The identity
of a print colour is its **RGB value**: `alpha` only acts in the view (transparency); entries,
slots, conflict check and regions ignore alpha. `color("transparent")`,
`color("")` and `color()` do not colour (warning, the material stays default/slot 1).

### 16.2 Colour regions

`cad_engine::colors`: the body of a coloured subtree (`color()` node `C`, result placed in
the frame of the part, with `resize` factors) belongs to this colour; overlap →
the later statement wins (pre-order: inner `color()` after the outer one, later sibling
after earlier); subtracted material applies to all (everything is intersected with the result).
`Stück(C) = R ∩ M(C) − ⋃ M(später, andere Farbe)` (piece(C) = R ∩ M(C) − ⋃ M(later, other colour)) – a neighbour of the same colour is not
subtracted (B12, F6: otherwise slivers at contact faces), pieces of one colour are united –,
remainder `R − ⋃ M` → colour on the part, otherwise default (if the part has uncoloured primitives),
otherwise the first colour. Cached under a key from part, colour nodes, positions and colours;
colour and slot are not part of the node keys. Check sum = volume (otherwise a warning).
Without colour nodes under the part there is no boolean: one region from the bodies of the
part, volume = part volume (cached).

`color()` in the 2D profile of an extrusion without its own colour (B12, F3): if all 2D leaves have a
colour, the extrusion is a body of that colour (like `color()` around the extrusion); mixed
or partly uncoloured 2D contents are not separated (one body, default, warning
"Farben im 2D-Profil … werden nicht getrennt" (colours in the 2D profile … are not separated)).

Material outside the parts (B12, F4): the topmost nodes that are neither a part nor
contain one form the entry "no part" of the regions (colour list, export).

### 16.3 Filaments

Slot of a colour, in this order:
1. `filament("red", 4);` at the top level of the main file (B12, F2): slot table of the model,
   evaluated before all other statements (own pass); the first specification of a colour
   applies; a contradicting `filament = m` on `color()` warns and is ignored;
   `filament()` elsewhere warns ("nur auf oberster Ebene" (top level only)); a user's own module
   `filament` takes precedence.
2. `filament = n` (1–16) on a `color()` (first specification applies, contradiction → warning).
3. Uncoloured material slot 1; otherwise the lowest free slot in order of first
   occurrence.

Hints of the colour table (B12, F8): automatic slots above 16 → warning; two colours on
the same fixed slot and a fixed slot 1 next to the default → hint (info).

The UI writes the slot as a text edit (`SetFilament`, `cad_eval::filament_set_edits`): if there is
a `filament()` for the colour, its slot is changed ("automatisch" (automatic) removes the line);
if all `color()` statements of the colour name it as a literal (string or number vector),
`filament = n` goes there; otherwise (parameter, loop value, variable – `filament = n` would apply there
to all colours of the statement) it inserts `filament("<name>", n);` (after existing ones, otherwise
before the first statement that is not a `use`/`include`, a definition or an assignment).
Rationale: the code stays the single source (CLI export and UI export match, undo
and Git see the change, the agent reads the same file); the table needs no new
syntax (a module call, a harmless warning in OpenSCAD) and applies to exactly one colour;
`$filaments = [...]` would have `$` scoping and could be changed anywhere.

### 16.4 Export

3MF (`cad_engine::color_export`): one object per part (material outside the parts: one
object "no part"; without parts the root) with `components` without transformation, one mesh
per (object, colour), `Metadata/model_settings.config` with `extruder` per part and object
(assumptions from `bbs_3mf.cpp` of Bambu Studio and OrcaSlicer, report 11). **No
`m:colorgroup`** (B12): Bambu Studio opens the colour dialog for foreign 3MF exactly when
a colour group and triangle colours are present (`Plater.cpp`), and its OK overwrites
`extruder`; without a colour group it takes over the slots. OrcaSlicer uses colour groups only for
objects without `model_settings.config`. `--color-group` (CLI) writes them for other programs
(only with more than one colour; message: choose "Abbrechen" (cancel) in the dialog). No
`project_settings.config`, no `Application = BambuStudio-…`. CLI and UI call the same
export (`Session::export_colors`), the message names the objects (`color_export_notes`).
Default for 3MF (`--single-color` / `colors: false` for single-colour); `--per-color` /
`Export { format: stl, colors: true }`: ZIP with `F<slot>_<farbe>.stl` (CLI: extension `.zip`
required; `--part` also restricts the colour list).

STEP: face colours from the colour regions without boolean (`Engine::face_region_colors`): per face
the colour of the latest colour node whose material contains its sample points (majority),
otherwise the remainder colour; material without `color()` has no colour. Faces are not split.

### 16.5 View (B12, F1/F3)

A model with colours shows the **colour regions** (the bodies of the 3MF) in the full view
as soon as they are computed (UI: "Farbregionen zeigen", default on; CLI `mcad render` without
`--show faces`). Before that and for highlighted subtrees: colour per face from the node that
created the face – only nodes below the drawn node, and for several with
equal content (shared cache entry, one face tag) the one whose bounding box contains the triangle
(for several, the latest). Cache reuse remains; limitation: colour per face
is only approximate for overlap and fillets at colour boundaries, the regions are exact.

### 16.6 Theme

CSS tokens per theme (`:root[data-theme=…]`), `contrast.test.ts` checks text ≥ 4.5:1 and
UI parts ≥ 3:1; CodeMirror tooltips/search bar via tokens; 3D colours from tokens
(`readPalette`); System follows `matchMedia('(prefers-color-scheme: dark)')` with a listener.
CLI: `mcad render --theme dark`.

---

## 17. Sketches, text, SVG import, sketch editor

As of: plan, agreed with the user on 2026-10-23. **Step 1 implemented** (sketches with
mixed contours, modules as namespaces; test report 14, decisions in 17.8), **step 2
implemented** (`corner`, `fillet2d`/`chamfer2d`, `offset`, `stroke`; test report 15, decisions
in 17.9); **step 3 implemented:** `text()` with the bundled OFL fonts and
SVG import (test report 16, decisions in 17.10), the own stencil font
"cad stencil mono" as the default font (step 3b, test report 17, decisions in 17.11).
**Step 4 implemented:** sketch editor, drawing mode with grid in browser and Tauri
(test report 18, decisions in 17.12).

### 17.1 Sketches

A sketch is a 2D block of segments; each line is a segment with its own
display and its own handles. Anonymous as a child of a profile operation, named via a
module:

```
linear_extrude(5) sketch {
    start([0, 0]);
    line(to = [40, 0]);
    arc(to = [50, 10], r = 10);          // also: center + angle, through, tangent
    line(by = [0, 20]);                  // relative; also length + angle
    bezier(c1 = [45, 40], to = [20, 35]);
    corner(r = 3);                       // rounds the corner to the next segment
    close();
}

module profil(b = 40) sketch { … }       // named sketch = module
rotate_extrude() profil(b = 30);
```

- Segments: `start`, `line` (`to` / `by` / `length` + `angle`), `arc` (end point + radius,
  centre + angle, through a point, tangent), `bezier` (quadratic, cubic),
  `spline` (through points), `corner(r = …)` / `corner(chamfer = …)` between two segments,
  `close()`. Variables, `let`, `for` and `if` are allowed in the block (e.g. tooth profiles).
- Points may be grid points and anchors; a grid may be declared in the sketch
  (`grid s { x: …; y: …; }`), snapping writes references.
- Errors per segment line: contour not closed, self-intersection, impossible arc.
- Holes: `hole { … }` in the sketch or 2D `difference`; open paths as a `sweep` path
  or via `stroke(w)` as an area.
- Kernel: mixed contours of exact edges (line, circular arc, NURBS) for extrude,
  revolve, sweep, loft; STEP stays exact.

### 17.2 Modules as namespaces

- `module name { … }`: the parentheses may be omitted if the module has no parameters.
- Nested modules are reachable from outside via the dot: `teile.schraube(d = 3);`,
  `linear_extrude(2) profile.stern();`. Multi-level: `a.b.c();`. The inner module sees the
  parameters of the outer one with their default values; it does not see the grids and instances of the outer one
  (as before).
- Context module and path bar support the path (`teile › schraube`), `mcad --module`
  takes `teile.schraube`.

### 17.3 2D operations

`offset(r = …)` / `offset(delta = …, chamfer = …)` exact for lines and arcs
(OpenSCAD-compatible), `fillet2d(r)` / `chamfer2d(d)` for all corners of a profile, optionally with
zones as in 3D, `stroke(w, cap = "round" | "flat" | "square", join = "round" | "miter" |
"bevel")` for open paths. Implemented in step 2 (17.9).

### 17.4 Text

- `text(t, size, font, halign, valign, spacing, direction)`, compatible with OpenSCAD.
- **Default font:** an own, very simple monospaced stencil font
  (`"cad stencil mono"`), constructed from lines and arcs, with bridges so that no
  islands fall out (the counters of O, A, B, D, P, R, 0, 4, 6, 8, 9 etc. are connected).
  Coverage: A–Z, a–z, 0–9, ÄÖÜäöüß, common punctuation and special characters. Parameters: stroke width,
  bridge width (`text(…, stroke_width = …, bridge_width = …)` in mm; implemented in 17.11).
- **Bundled free fonts (OFL):** Inter (sans), JetBrains Mono (mono),
  Allerta Stencil (proportional stencil). Own fonts via UI upload or path (CLI).
- Outlines exact as Bézier/NURBS contours, kerning via shaping, holes by winding rule.
- **Required character set** (`cad_font::REQUIRED_CHARS`, test per font; also applies to the
  own stencil font): A–Z a–z 0–9, Ä Ö Ü ä ö ü ß ẞ, space,
  `. , ; : ! ? ' " „ “ ‚ ‘ » « ( ) [ ] { } - – — _ / \ & % # + * = < > ° ² ³ € @ § ~ ^ | · • …`,
  `± × ÷ µ Ø ø`. If a character is missing in a font, it is taken from
  JetBrains Mono of the same weight with a warning at the statement (not as a box or gap).

### 17.5 SVG import (subset)

`import("logo.svg", …)` yields a 2D profile (units: px → mm via `viewBox`/`width`
or parameters `scale`/`dpi`): `path` (M, L, H, V, C, S, Q, T, A, Z, absolute and relative),
`rect` (also with `rx`), `circle`, `ellipse`, `polygon`, `polyline`, `line`, `g` with
`transform`; fill rule `nonzero`/`evenodd`. Not supported (warning): text, images,
stroke widths as area, CSS, masks, gradients. Selection of individual layers via `id`.
No SVG or DXF export. Implemented (17.10).

### 17.6 Sketch editor (drawing mode)

- In the view: tools line, arc (3 kinds), Bézier, rectangle, circle, round corner,
  close; drawing in the sketch plane with a grid (sketch grid or the module's grid),
  snapping to grid and anchor points, end, mid and intersection points, horizontal/vertical,
  tangent; Alt = free with step size.
- Each action writes code (`line(to = g[a, b]);` …) into the current `sketch` block or
  creates a new one; undo as for dragging. Editing via handles, delete segment.
- The code stays the single source.

### 17.7 Order (one agent each, test report per step)

1. Language: `sketch` block, segments, mixed contours in the kernel; modules without parentheses and
   nested modules as namespaces.
2. `corner`, `fillet2d`, `offset`, `stroke`.
3. Text with own stencil font and OFL fonts; SVG import.
4. Sketch editor (drawing mode with grid) in browser and Tauri.

### 17.8 Implementation step 1 (test report 14)

**Language.** `sketch { … }` is a built-in 2D module (`sketch(closed = false) { … }` for
open paths); the parser accepts `sketch {` and `hole {` without parentheses, `module p(…) sketch
{ … }` is a module whose body is a sketch. Segments are separate statements; they
append to the sketch currently being evaluated, also from within modules and loops
(`for (i = …) zahn(i);`). Inside a sketch, segment names take precedence over user modules of the same
name (libraries with `module line(…)`); outside, everything stays as in OpenSCAD (`spline(points)`
is still the 2D primitive there, other segment names without a user module report "nur innerhalb von
`sketch`" (only inside `sketch`)). Other geometry and transformations in a sketch are an error.

| Segment | Forms |
| --- | --- |
| `start(p)` | starts a contour (an open previous one is an error at its last line) |
| `line` | `to = p` (also positional), `by = v`, `length = l, angle = a`; `length` alone continues the direction |
| `arc` | `to = p, r = r [, large, cw]` (as in SVG), `center = c, angle = a` (degrees, sign = direction), `center = c, to = p`, `through = q, to = p`, `to = p` alone = tangent (also `tangent = true`) |
| `bezier` | `c1, to` (quadratic), `c1, c2, to` (cubic), also positional |
| `spline` | `points` after the current point; `tangent = true` (direction of the predecessor) or vector, `end_tangent = v`; cubic global interpolation (chord-length parametrisation, averaged knots) |
| `close()` | closes with a line to the start (if the contour already ends at the start, it is closed without `close()`) |
| `hole { … }` | contours inside are holes |
| `corner(…)` | reserved, warning (step 2) |

Points may be grid points and anchors (z ≠ 0: warning, projected), a `grid` in the
sketch is allowed, as are `let`, assignments, `for`, `if`. **Holes:** contours fill by the
*nonzero* winding rule, contours in `hole { … }` always count clockwise; a contour
is a boundary where the winding number is ≠ 0 on one side only (inside ≠ 0: material, otherwise a hole);
islands in holes are built via the nesting (material minus holes in it, hole
minus islands in it). **Errors per segment line:** not closed (the sketch continues to be shown with a
red dashed closing line), self-intersection (at the later line, with a
reference to the other), impossible arc (radius < half chord, points on a line,
tangent arc on its tangent, angle 0 or > 360°), contour without area, segment before
`start` or after `close()`.

**Sweep along an open sketch:** without `path`, one child of `sweep` is an open sketch
(also through transformations, groups, module calls): `sweep() { sketch(closed = false) { … }
circle(2); }`. If the path is tangent-continuous, it is joined into one B-spline (curve sweep),
otherwise segment by segment with mitres (the path is planar).

**Loft:** if all sections have the same number of segments, they are joined segment by segment
(start = `start(p)`, direction counter-clockwise): each segment becomes its own face,
lines stay ruled surfaces, and the user determines the correspondence. With different counts there is
no natural correspondence: each section is joined into one closed B-spline
(degree raised, C0 at the joints) and starts where the +x ray from the centroid intersects it –
like circles and polygons – so that the sections are not twisted relative to each other.

**Modules as namespaces.** `module name { … }` without parentheses; calls `a.b.c(…)` (the parser
joins the path into a name `a.b.c`, also `k: a.b();`). The first part is looked up lexically,
each further one only directly in the body of the previous module; that body is the namespace,
with the parameters of the outer module at their default values. It belongs to no instance:
grids, named instances and anchors of the outer module are not visible, its geometry
is not created. **Functions and constants** of a module body are reachable from outside
(`teile.kopf(d)`, `teile.M3`): libraries can thus bundle helper functions and dimensions with their
modules, without a separate file; OpenSCAD has no dot after module names, so there is no
conflict, and a variable of the same name hides the module (`v.x` stays component access).
Context module: `mcad --module teile.schraube` (also `teile::schraube` or just `schraube`),
`ContextInfo.module` is the path, the UI selects the module under the cursor with its path, the
path bar shows "teile › schraube"; `mcad outline` lists modules and functions with their path.

**IR, kernel, facade.** `Profile::Sketch { contours, closed }` with `Seg::{Line, Arc (centre,
radius, start/end angle, direction), Bezier, Spline}`; the statements of the segments are in
`EvalOutput::sketches` (not in the IR: the cache key stays independent of source positions).
Kernel: `cad_kernel::Segment::{Line, Arc, Nurbs}` (`curve2.rs`: NURBS form, affine mapping –
arcs stay arcs under similarities, otherwise exact rational elliptical arcs –, sampling,
interpolation, joining), `Loop::Composite`, `PlacedPath`, `Kernel::sweep_path`. The facade
`smlib/facade/sc_sketch.cpp` is included at the end of `sc_facade.cpp` (uses its internal
helpers; compiled on its own the file is empty); loop kind encoding 3. Lines become `SmLine`,
arcs rational quadratic B-splines (form `SM_CF_CIRCULAR_ARC`), Bézier/splines B-splines; the
joints are welded exactly. Extrude (also with twist/scale), revolve, loft and sweep use the
existing entry points; new is `sc_sketch_sweep` (smooth path: curve sweep with the
profile origin as reference point, SMLib patch 0003; corners: mitre). STEP writes the arcs as
`CIRCLE`, the lines as `LINE` (recognised from the exact NURBS); SMLib holds extruded arcs as
extrusion surfaces of the B-spline, the writer writes an extrusion of a circular arc along its
axis as `CYLINDRICAL_SURFACE` (OpenCASCADE computed volumes wrong by up to
2.5·10⁻³ on the extrusion surfaces without p-curves). The 2D display samples arcs and curves
with the display tolerance (polygon clipping in Rust).

**Display and handles.** `OpInfo::Sketch` (segments sampled, control polygons, errors,
start points). If the cursor is in a sketch (segment line, loop, `hole`, call of a
sketch module), the view draws it in its plane: mesh as outline with a light fill
(visible through the body), segments as lines, the segment of the cursor line thick in the
accent colour, errors red, control polygons and arc centres dashed, start as a ring with a
direction arrow, the end of open contours as a square. Handles: end/start points (literal and
grid snap as for points; `line(by = …)` relative, without snap), arc radius, centre,
through point, Bézier control points, spline points, length of `line(length = …)`. On the
`sketch` line the view shows the handles of all segments.

### 17.9 Implementation step 2 (test report 15)

**`corner(r = …)` / `corner(chamfer = d)`** fillets or chamfers, in a sketch, the corner between the
previous and the next segment. Rules: between two segments → that corner (the closing line
of `close()` counts as a segment, so `corner()` before `close()` rounds the corner to the closing line);
directly after `start(p)` → the start corner (between the last and the first segment, closed
contours only); after the last segment of a contour that ends at the start (no closing line needed) →
also the start corner. Errors at the corner line: after `close()`, before `start`, twice at one
corner, start/end of an open contour, value ≤ 0, tangent corner ("keine Ecke" (no corner)), cusp (180°),
radius does not fit ("höchstens ≈ r" (at most ≈ r)). The sketch is built without corners (`arc(to)` tangent,
`line(length)` refer to the sharp corner); afterwards all corners of a contour are computed on the
original segments. If two corners need overlapping pieces of one segment, both
fail with the largest common factor ("höchstens ≈ …") – independent of the order. The
fillet (arc) or chamfer (line) is a segment of the corner line (display, highlighting);
a start corner is placed at the end of the contour, the start moves to the end of the fillet. Chamfer: the
intersection points lie at (Euclidean) distance `d` from the corner point. Exact (centre as the intersection of the
curves offset by `r`, closed forms) for line–line, line–arc and arc–arc; with
Bézier/spline, Newton on `a(u) + r n_a(u) = b(w) + r n_b(w)` (start value from the tangents,
stop at a residual of 10⁻¹³ of the size, warning above 10⁻⁶), the curve is trimmed at the parameter
(`Seg::Nurbs`). **Handle:** radius (chamfer: length) at the middle of the fillet, base at the corner point,
linear in `r` (exact for line–line).

**`fillet2d(r) { profil; zone; … }` / `chamfer2d(d)`** (`Op::Blend2`): the first child is the profile,
further children are zones (2D shapes, also `zone(linie | ebene, d)` via the distance). Corners are the
joints with a kink (|kink angle| > 10⁻⁷), convex and concave (the fillet always lies on the
inside of the kink: convex removes material, concave adds it). Selection as in 3D:
`select = "inside"` (default) – the corner point lies in a zone (boundary counts); `"touch"` –
also if only the piece of the profile that the fillet replaces touches a zone. Inline hint
"n Ecken" (n corners), an empty selection is a warning; radii that are too large are errors per corner ("Ecke bei [x, y]:
Radius … passt nicht (höchstens ≈ …)" (corner at [x, y]: radius … does not fit (at most ≈ …))), the remaining corners are still rounded. Nested
blocks act from inside out; the transitions of a fillet are tangent and not corners.
A single closed contour keeps its segments (splines stay splines), everything else goes
through the exact region.

**`offset(r = …)` / `offset(delta = …, chamfer = false)`** as in OpenSCAD 2021.01: the first
positional argument is `r`, without arguments `r = 1`, `r` wins over `delta` (warning), `chamfer` only
with `delta`. `r > 0`: all points at distance ≤ r (convex corners become exact arcs, concave ones stay
sharp); `r < 0`: the points farther than |r| from the exterior (convex corners stay sharp, concave ones become
round, narrow parts disappear, holes grow); `delta`: edges move by `delta`, corners
extend up to the intersection of the offset tangents (without a mitre limit, like OpenSCAD with
`miter_limit` 10⁶), `chamfer = true` cuts them like Clipper's `jtSquare` perpendicular to the
angle bisector at distance `delta` from the corner point (measured against OpenSCAD). The offset of an arc
is an arc; OpenSCAD facets (`$fn`), we compute exactly.

**`stroke(w, cap = "round" | "flat" | "square", join = "round" | "miter" | "bevel")`**
(default round/round): children are open sketches (their contours as paths; a contour ending at the start
is closed) and arbitrary 2D profiles (their boundary as closed paths, `stroke(1)
circle(5)` = ring). Bands of half the width along the segments (line: rectangle, arc:
annular sector), joints round (circle), mitre (up to 4 half-widths, otherwise bevel) or bevel, ends round
(circle), flat or square (extended by w/2); crossing paths are united.

**Kernel: exact 2D regions** (`cad_kernel::region2`). `combine(operanden, prädikat)`: all
segments (lines, arcs) are split at their exact intersection points (line–line,
line–circle, circle–circle with tangency, overlap of collinear lines and equal circles;
points within 10⁻⁹ of the size merge), duplicate pieces are dropped, each piece stays where
the predicate over the insideness of the operands (winding number per fill rule: ray towards +x, arcs split into
y-monotone pieces) differs on the left and right, with the region on the left. The pieces are
joined into loops (at shared corners the tightest turn: tangencies give separate
loops), pieces of the same line or the same circle merge. Built on this: 2D booleans of the
children (the predicate is the boolean expression of the leaves), `offset_round` = region ∪ (bands
±r ∪ circles at the corners) or region ∖ (…) for r < 0 (Minkowski sum with the circle, exact from
lines and arcs), `offset_delta` = region ∪ one-sided bands ∪ corner pieces (mitre or bevel) at
diverging corners (mirrored for delta < 0), `stroke` = union of bands, joints
and caps, `apply_corners`/`round_corner` for `corner` and `fillet2d`. **B-splines** enter as
G1 biarcs (two tangent arcs per piece, halved until the deviation is ≤ 1 µm, at most
10⁻⁵ of the size): the offset of a B-spline is not a B-spline, that of an arc is an arc – so the
offset stays exact relative to the approximation, tangent-continuous, and STEP contains lines and circles; 1 µm is far
below printing and manufacturing tolerances (≥ 10 µm) and above the numerical precision.

**IR, engine, display.** `Op::Offset { round, d, chamfer }`, `Op::Blend2 { kind, r, select }`,
`Op::Stroke { w, cap, join }` are 2D nodes; `Seg::Nurbs` for trimmed splines. `profile::lift`
computes the region for them (`op2d.rs`, cached by the content of the subtree, 48
entries) and passes it on as sketch leaves: extrude, revolve, loft, sweep and STEP see
exact lines and arcs (STEP: `CIRCLE`, cylinder). Errors, warnings and hints are on the node
itself; `NodeReport::guides` / `SceneNode.guides` carry guide lines (`source` input contour,
`result` offset contour, `centerline` path), the new corner pieces of `fillet2d` are
`selected_edges`. `cad_engine::region_2d` returns the exact region of a 2D node (bounding boxes of the
compatibility tests). UI: cursor on `offset` – children as outline, offset contour dashed in the
accent colour (`guide:offset`); on `stroke` – path as outline, centre line dashed
(`guide:centerline`); on `fillet2d` – zones blue, profile before filleting as outline, new corners
coloured, hint at the line; on `corner` – the fillet as the active segment with a radius handle.
`mcad render --line N` shows the same lines (`--show handles` the handles). OpenSCAD comparison:
`tests/openscad-compat` 34–36 (offset r/delta/chamfer, holes, merging, disappearing),
`stl_stats.classify` counts `offset(r)` among the roundings (`$fn = 128` for the reference).

**Correction from step 1 (arcs > 90°).** The facade builds an arc as one rational
quadratic B-spline with spans ≤ 90°; an arc over 90° is therefore a curve of several
spans. SMLib booleans with extrusions of such curves gave wrong results depending on position
(ring of two 180° arcs ∪ box: "volume 924.12 impossible for operands 976.79, 43.2"; correct with
4 × 90° or rotated). The engine therefore splits arcs over 90° into equal
pieces ≤ 90° before the kernel (`profile::split_arcs`, for extrude, revolve and sweep; not for loft, which joins the
sections segment by segment): each piece is one span, a semicircle becomes two
cylindrical faces (STEP: two `CYLINDRICAL_SURFACE`). Regression test
`ring_of_long_arcs_unites_with_a_box` (arcs 180°/120°/90°/360°, eight rotations of the box).
Found and fixed along the way: the winding number of the exact region took the end points of the arcs from
their angles (rounding ~10⁻¹⁵), a ray through a corner point then counted wrongly; now
the shared corner points of the loop apply.


### 17.10 Implementation step 3, part text and SVG (test report 16)

**Crates.** Three new crates with no relation to language and kernel: `cad-outline` (neutral contours:
`Seg::{Line, Arc, Quad, Cubic, Ellipse}`, affine mapping – circular arcs stay circular arcs under
similarities, otherwise elliptical arcs –, sampling, fill-rule normalisation),
`cad-svg` (parser on `roxmltree`) and `cad-font` (fonts, shaping with `rustybuzz` 0.20 /
`ttf-parser` 0.25, glyph outlines). `cad-eval` has the builtins `import(…)` and `text(…)`
(`import.rs`); they produce `Profile::Sketch` – one node per filled SVG element or per
glyph, several under one group (union: overlapping elements/glyphs). With
`Seg::Nurbs` (from step 2), tangent-continuous chains of Bézier curves become one B-spline with
C1 knots (one kernel face per glyph flank instead of dozens), elliptical arcs rational
quadratic. Engine and facade stayed unchanged except for the colour regions (below) and a
union without SMLib boolean for bodies whose bounding boxes are separate (`disjoint_union` in
`sc_facade.cpp`: both shells into one Brep): in the union of separate
glyph parts (stencil "c" of three pieces) SMLib returned one piece twice, once inverted
(volume 2.0 instead of 7.3 mm³, without an error message); as a side effect text gets faster. (The
interim workaround – splitting arcs over 90° on import – became unnecessary with
`profile::split_arcs` from step 2.)

**Fill rules.** Per element the contours are oriented so that the filled area lies on the left
(outer counter-clockwise, holes clockwise, islands counter-clockwise again): test points left/right
of the longest edge, winding number of all contours of the element by its rule; contours with
fill on both or neither side are dropped. After that nonzero applies as in sketches.
Precondition: the contours of an element do not intersect (glyphs, typical SVGs).

**SVG.** Units as in OpenSCAD where it is consistent: with `viewBox`, `width`/`height` apply
(px = 1/96 in, mm, cm, in, pt, pc; without a unit, pixels with `dpi`, default **96**; OpenSCAD
2021.01: 72), without `width` the `viewBox` size in pixels, `preserveAspectRatio` (default
`xMidYMid meet`). y is mirrored about the document height: origin at the bottom left (as in OpenSCAD).
Deviations from OpenSCAD 2021.01: without `viewBox`, user units are pixels (OpenSCAD: mm with
mirroring about the height in pixels), the y offset of the `viewBox` is subtracted (OpenSCAD adds
it), `fill-rule` applies (OpenSCAD: always evenodd), strokes do not become areas (OpenSCAD:
they do), open paths and `line`/`polyline` are paths (warning; only open paths → open sketch,
usable as a `sweep` path). `center = true` centres the bounding box; `scale` multiplies; `id`
selects an element with content (unknown id: error). Warnings carry the file and SVG line
(`logo.svg:4: …`) and appear once per message.

**Files.** `Project::assets`: every string in the expressions of the project files that
ends in `.svg/.ttf/.otf/.ttc/.otc` is looked up relative to every loaded file and
read when loading (`FileResolver::read_bytes`; browser/`SetFiles`: binary files as
`data:…;base64,…` text). `import`/`font =` resolve relative to the file of the statement; a name
computed at run time that does not appear anywhere as a string is an error ("nicht geladen" (not loaded)). The
content is in the contours of the IR: changed SVGs change the cache key; `mcad watch`
watches the data files, the desktop app reads and writes them (fonts decoded).
UI: "⇡ Datei laden" (load file; SVG, TTF/OTF, .scad), SVGs open in the editor, font tabs show a
hint; ZIP and folders include the data files; single examples that import SVGs from
`tests/models/svg` open as a project with these files.

**Text.** As in OpenSCAD 2021.01: em = `size` / 0.72; advance, offset and alignment
take the shaper's values times 1000/1024 (OpenSCAD computes this way; text thus stays the same
width), `halign` by the advance, `valign` by the bounding box of the glyphs, `spacing` multiplies
the advance, `direction` ltr/rtl/ttb/btt, `language`, `script` (ISO 15924 or names such as
`"latin"`); if unspecified, the shaper determines direction and script. Bounding box and area match
OpenSCAD (report 16). Font selection: `"Familie"`, `"Familie:style=Bold"`; unknown
style → Regular (warning), unknown family → default font (warning; until step 3b
JetBrains Mono), `font = "datei.ttf"` → project file. **Default font:** since step 3b
`"cad stencil mono"` (17.11; previously JetBrains Mono). **Missing characters:** per character from JetBrains
Mono of the same weight (advance of the fallback font, warning "Zeichen … fehlen in …" (characters … missing in …)); Allerta
Stencil (bundled unchanged: the Reserved Font Name forbids modified versions under
that name) lacks ẞ„‚{}_#°²³§~^·•…±µØø. Characters in no font: `.notdef` and a warning.

**Colour regions.** If the result of a node is the plain union of its colour nodes
(only groups, transformations, colours, unions without fillets in between),
a colour region needs no booleans with the whole: material of the colour node minus later
material of another colour whose bounding box overlaps it (touching does not count); parts of one colour
stay separate if their bounding boxes do not overlap (`colors.rs`, `union_regions`).
Multi-colour text on a plate: seconds instead of minutes or abort by the watchdog.

**STEP.** Extrusions of elliptical arcs (rational quadratic B-spline, extruded perpendicular to the plane)
are written by the writer as `SURFACE_OF_LINEAR_EXTRUSION` of an `ELLIPSE` (`cad-step`,
`elliptic_extrusion`: conic through five sample points, checked at 65): OpenCASCADE
integrated the extrusion of the rational B-spline wrongly by 3.6·10⁻³. Open: OpenCASCADE measures the extrusion of a
quadratic Q/T wave (`wave.svg`, `id = "wave"`) 3 % too small (kernel
652.25 mm², analytic 652.19 mm²); `svg_logo.mcad` therefore uses only the ellipse and arc from
`wave.svg`.

**Limitations.** Kernel time grows strongly with the number of glyphs on one shared face
(union plate ∪ text in SMLib: 9 glyphs ≈ 1.5 s, 28 ≈ 9 s native); contours that intersect
within one element are not resolved; no text paths, no `use`,
no CSS classes; variable fonts only in their default instance; `$fn` has no effect (exact
curves).

### 17.11 Implementation step 3b: stencil font "cad stencil mono" (test report 17)

**Font.** `cad_font::stencil` (`crates/cad-font/src/stencil.rs`), no font file: per
character a drawing function over named positions (centre lines left/right/middle; baseline,
x-height, cap height, ascender, descender) that depend on the stroke width – centre line = outer line
∓ half stroke, so cap height and glyph box stay the same at any stroke width.
Building blocks: lines, tangent arcs (`arc_to`: circular arc from the point with the current
direction to the target), arcs around a centre, stadium, bowl, C arc, dots (squares of the
stroke width), commas, mirroring and rotation by 180° (d, q, 9, `)`, `]`, `}`, `»`, `>`, `\`,
„“ ‘). The region of a glyph: `region2::stroke` of the paths (flat ends, mitre, limit
4 half-strokes) ∪ fills ∖ bridges ∩ clip, in one `region2::combine` – exact loops of
lines and circular arcs, outer counter-clockwise; cache per (character, stroke, bridge).
Construction rules: free ends run up to the outer line; joints end on the centre line
of the other stroke (the cross-section then lies in the other band, no notch); diagonals
are extended beyond baseline/cap line and clipped there (straight feet, flat apexes,
mitre tips of Z, 2, 4, 7 in the clip). Coverage: `REQUIRED_CHARS` complete plus `’ ” $`
(`stencil::CHARS`, glyph ID = position + 1); other characters from JetBrains Mono.

**Bridges.** A bridge is a band of the bridge width across a stroke (1.3 half-strokes to
each side), subtracted from the glyph. Each counter gets at least one: B, D, P, R,
b, d, g, p, q at the stem (stem and bowl separated), O, Q, 0, 8, o, Ö, ö, Ø, ø at top and bottom,
A open (legs at the cap line apart by the bridge width, crossbar connects), a and e
at the bowl/crossbar next to the stem, 4 at the crossbar before the stem, 6/9 diagonally at the bowl, ° %
& @ § at a ring, # at the upper crossbar; ß and ẞ are open (gap ≥ bridge width to the
stem). **Without bridges** (`bridge_width = 0`) the counters stay closed (holes like
a normal font, A closed at the top).

**Metrics.** 1000 units per em, em = `size`/0.72 as for the other fonts; cap height
720 (= `size`), x-height 530, ascender 760, descender −210, advance 600 (× 1000/1024 as in
OpenSCAD – same as JetBrains Mono, texts stay as wide as with the previous
default font), side bearing 60. Vertical: advance 1000, glyph centred on the pen;
`rtl`/`btt` reverse the order. `halign`, `valign`, `spacing` as elsewhere (17.10).

**Parameters.** `text(…, stroke_width = w, bridge_width = b)` in mm, only for this font
(otherwise warning "wirkt nur mit der Schrift „cad stencil mono“" (only has an effect with the font "cad stencil mono")). Default stroke 0.14·`size`
(style Bold: 0.2·`size`), bridge 0.1·`size`. Limits from the test of the parameter space: stroke
≤ 0.2·`size` (above that a, e, #, @ grow together and close counters), bridge 0 or
0.03…0.2·`size` (narrower bridges get closed again by heavy strokes); outside: warning at the
argument and clamping. mm instead of fractions, because printing and milling need absolute dimensions
("bridge at least two nozzle widths"). Font names `"cad stencil mono"`,
`"cad stencil mono:style=Bold"`.

**Selection.** `cad_font::select(name)`: no `font`, the own name and unknown families
(warning) → stencil font; bundled TrueType fonts as before (`builtin` now returns
only these); `Font::stencil(bold)`, `FontData::Stencil`. The previous hint
"Schablonenschrift folgt" (stencil font to follow) is dropped.

**OpenSCAD compatibility.** OpenSCAD does not know the font; without `font` it takes its
fontconfig default font. The comparison models (`tests/models/openscad-compat`,
`tests/openscad-compat/projects`) contain no `text()`, the text comparison (report 16,
`hull_and_area_like_openscad`) names each font explicitly – no dependency on the
default font, no change needed. Deliberate deviation: models with `text()` without `font`
look different with us than in OpenSCAD; for identical dimensions, specify a bundled TrueType font.

**Tests.** cad-font: coverage of both styles without fallback, no holes/slivers/overlaps
for 8 combinations of stroke and bridge, counters without bridges, metrics and advance, each
glyph within its cell, areas (H exact, o against formula, against JetBrains Mono), alignment,
directions, fallback; cad-eval: default font, parameters, warnings; cad-engine
(`stencil_text.rs`): each glyph via `region_2d` without a hole, volume = area × height,
kernel time of a sign against Allerta Stencil (native 1.6–1.9 s against 3.3–3.6 s: lines and
circles instead of B-splines, fewer kernel calls); example `tests/models/spec/stencil.mcad`;
Playwright K01–K03 (`web/e2e/stencil.spec.ts`). Character chart:
`cargo run -p cad-font --example stencil_chart -- tafel.svg [strich steg]`.

**Playwright C16 (finding).** C16 (axis-constrained dragging) failed depending on timing, most recently also
when run alone: the viewer rebuilds the handles after a cursor change only in the next animation frame,
until then `__cad.handles()` returned the handles of the previous line (`cube([60, 30, 1])`,
also a point handle); the test grabbed the plate corner, the drag did not start, the mouse
rotated the camera. The test now waits for the point handle of the line (`pointHandleOn`, likewise
C20). The app behaves correctly.

**Limitations.** One stroke width per glyph (only @ with 0.72×, ² ³ with 0.75× stroke),
no optical corrections; W, w, m, @, * become dense in Bold. Bridges on arcs are straight
bands (width on the centre line). Combining characters are not composed (the
character comes from JetBrains Mono); characters from JetBrains Mono have no bridges. Glyphs
touch only with `spacing` < 1 or the cell-wide `—` and `_`.

### 17.12 Implementation step 4: sketch editor (test report 18)

**Operation.** Toggle "Skizze" (sketch) in the view bar (same in browser and Tauri): drawing mode
for the sketch at the cursor (the `sketch` statement contains the cursor or starts on its
line). The camera looks perpendicular to the sketch plane (on entering, when the plane
changes, "⊥ Ansicht" (⊥ view)); for a new sketch it also fits the model. Bar: select,
line, arc 3P (through three points), arc M (centre + angle), arc tan (tangent),
Bézier (cubic), rectangle, circle, round corner; close, delete, done. The status line says
what the tool needs next, where the mouse snaps and why an action did not work.
Esc discards the chosen points, a second Esc ends the mode; Del deletes the segment at the
cursor; Ctrl+Z/Ctrl+Shift+Z also work with focus in the view. The left mouse button
belongs to the drawing tool (also on existing points: the view shows no handles there);
with "Auswahl" (select) a click on a segment places the cursor in its line, the handles of all
segments are visible and are dragged as before (change literal, rewrite grid
reference, Alt: numbers), left-dragging on empty space rotates the camera.

**Tools → code.** Each action is a text edit of the main file and its own
history step; afterwards the cursor is behind the new code, the next action follows there.

| Tool | Clicks | Code |
| --- | --- | --- |
| Line | start, then per point | `start(p);` (if the start is not the end of the contour at the cursor) `line(to = q);` per point; tangent: `line(length = l);`; click on the start of the contour (≥ 2 segments): `close();` |
| Arc 3P | start, through point, end | `arc(through = q, to = p);` |
| Arc M | start, centre, end angle | `arc(center = c, angle = a);` (a rounded to the angle step; sign = direction, beyond 180° following the mouse movement); on a snapped point `arc(center = c, to = p[, cw = true]);` |
| Arc tan | end | `arc(to = p);` at the end of the open contour (needs a direction) |
| Bézier | start, c1, c2, end | `bezier(c1 = …, c2 = …, to = …);` |
| Rectangle | two corners | `start(a); line(to = [bx, ay]); line(to = b); line(to = [ax, by]); close();` – if both corners are points of the same grid, the other corners are too (`g[c, u]`, `g[b, v]`) |
| Circle | centre, point on the circle | `start(p); arc(center = c, angle = 360); close();` |
| Round corner | corner, radius (distance to the corner) | `corner(r = …);` after the segment that ends at the corner (start corner: after `start`); if a `corner()` is already there, its radius changes |
| Close | – | `close();` for the open contour at the cursor |
| Delete | – | removes the segment statement at the cursor (whole line); the only segment of a contour takes the contour's `start`, `close`, `corner` with it, as well as a `corner()` directly after the segment |

Chains: after a line, arc 3P/M and Bézier, the last point is the start of the next
element (no new `start`). Points are written as references (`g[b, u]`, `k.ecke`) or as numbers
(`fmt_num`, free points rounded to the step size); nothing new in the language.

**Insertion point** (`cad_eval::sketch_edit::place`). In the sketch at the cursor: after the statement
of the cursor line or the last one before it, in the innermost `hole { … }` block containing the cursor;
on the `sketch {` line at the end of the block; empty blocks (also `sketch {}` on one line)
are expanded, indented like the neighbours or one level deeper. The drawing state there
(open contour, its start, current point, direction) comes from `SketchInfo::steps`: the
segment statements in evaluation order with the state after each (also from loops and
module calls in the block; geometry before the `corner()`s). If an action starts a new contour
in the middle of one that continues after the insertion point, it goes after that contour's last statement.
Without a sketch at the cursor, the first action creates a `sketch { … }` block: at the end of the
context module or of the file, plane = xy of the context (a plain 2D sketch; e.g.
`linear_extrude` turns it into a body). Visible grids and named instances are those at the insertion point.

**Snap** (`snap`). Tolerance: snap radius in pixels (settings) times pixel size at the hit point
of the ray (`pixel` = 2·tan(fov/2)/height, times distance). Without Alt the nearest point within the radius:
grid points (only those in the sketch plane, z = 0 in the sketch frame) and anchors of named
instances write their reference; start of the open contour (closes), corners, end, mid and
intersection points (line/arc exact, curves via sampling) and arc centres of the sketch
write numbers; at the same position the reference wins. Otherwise: tangent (line from the end of the
contour along its direction, length to the step size), horizontal/vertical to the previous point
and grid lines (one coordinate `g.name`, e.g. `[g.b, 40]`, only for grids without rotation),
otherwise the step size. Alt: free with step size. The response contains text, kind and
label; the view shows a marker per kind (square grid/anchor, ring points, diamond
directions, cross step), the label and guide lines.

**Sketch plane** (`sketch_frame`): the node frame; in the profile of a `rotate_extrude` the
xz plane (where the profile lies in the body), for a centred `linear_extrude` its bottom. The same
plane is used by the segment handles (`Handle.frame`), the display of the sketch
(`OpInfo::Sketch.plane`, also outside drawing mode) and the editor (camera, grid, snap,
preview). Values stay sketch coordinates; grid points are shown in the sketch plane
where the reference places the point (like `result_world` for handle snapping).

**Protocol** (plan 7.1). `Sketch { id, cursor, tool, points, ray?, pixel, snap_px, free, click,
angle?, info }` → `Sketch { id, version, sketch, node, plane, current, snap, guides, preview,
points, edits, select, prompt, message, segment, angle, raster }`. The engine is stateless:
the chosen points (with text and kind) come with each request and go back with the response.
At most one request is in flight (the last mouse position wins, clicks in order).
The UI applies `edits` as one transaction with `isolateHistory`, sets the cursor
(`select`: edit and offset in the text) and sends the source immediately; because the engine processes messages in
order, the next request sees the new text. On syntax errors it responds
only with a message. Same over `WorkerTransport` and `TauriTransport` (JSON).

**Division of work.** Rust (`cad-eval/src/sketch_edit.rs`, session): sketch and insertion point,
drawing state, snap, code, preview, delete, corners, grid, plane, ray in the plane. UI
(`web/src/app/sketch.ts`, `sketchUi.tsx`, `logic/sketchEditor.ts`, viewer): pass on mouse and keys,
apply the response, draw (step grid with spacing ≥ 12 px, grid lines and
grid points, preview, guide lines, marker), camera. Light and dark theme via the
colour values of the view (`--view-snap`, `--view-grid`, accent).

**Small things along the way.** Handle snap: a 2D point written as `g[a, b]` stays
in this form (`g[c, v]` instead of `g[c, v, _]`). The display draws sketches in their plane
(previously a sketch in `rotate_extrude` lay in the xy plane when the cursor was on the segment line).

**Tests.** cad-eval `tests/sketch_edit.rs` (insertion point, new sketch in file and module,
chains, new contour after a continuing one, snap kinds, intersection points, tangent,
rectangle with grid references, arcs, Bézier, circle, centre arc on a grid point,
inserting and changing corners, close, delete, `hole` and empty blocks, planes under
transformation/`rotate_extrude`/centred extrusion, anchors, segment under the mouse, handle snap
in short form) and unit tests in the module; cad-session `tests/sketch_editor.rs` (protocol with ray,
new sketch, chain, close, delete, plane under transformation, syntax error, selection,
`OpInfo.plane`); Vitest `logic/sketchEditor.test.ts`; Playwright Z01–Z17
(`web/e2e/sketch-editor.spec.ts`: each tool, snap display, snap kinds, undo, dragging a handle,
deleting a segment, transformation and `rotate_extrude`, dark theme, response times); Tauri T16
(`web/e2e-tauri/desktop.e2e.mjs`: rectangle on grid points via the `TauriTransport` with the
native kernel, snap display, undo).

**Limitations.** Main file only: in another project file the mode does not start (hint),
sketches from included files are not a target (the first action then creates a new one in the
main file). Statements in
loops or modules are neither deleted nor do their corners get a `corner()` (message);
the drawing state in the middle of a loop is the one after its last iteration. Snapping only to the
sketch itself, visible grids and anchors (not to edges of the body or other sketches);
horizontal/vertical only to the previous point. Bézier only cubic, no spline tool, no
generated `line(by = …)`/`length + angle` forms except tangent. A new sketch is a plain
2D sketch. The camera keeps "z up" (orbit control): in rotated planes the y axis of the
sketch does not necessarily point straight up on the screen. Tauri T16 drives the view with
synthetic pointer events (WebDriver), not with a real mouse.


---

## 18. Measuring (test report 19)

**Goal.** Measuring in the UI (browser and Tauri) and in the CLI with the same logic in Rust:
distance (point–point and minimum distance between points, edges and faces with foot points;
parallel planes: normal distance), radius/diameter (circular edges, arcs, cylinder, cone, sphere,
torus, fillets) with axis and centre, angle between edges and faces, length of an
edge, area of a face, bounding box and volume of a body or part; in the active section, snapping
to the contours, distances in the plane, radius of section circles, wall thickness, area of a
region. Results exact from the B-rep, otherwise with a stated tolerance.

**Division of work.**

| Part | Content |
| --- | --- |
| `cad-measure` (new, kernel-independent) | `curve::EdgeCurve`: edge as a bounded exact curve (line, circular arc, ellipse in closed form; B-spline, polyline numerically) with nearest point, length (Gauss–Legendre), midpoint by arc length, bounding box. `surface`: nearest point on plane, cylinder, cone, sphere, torus in closed form, on surfaces of revolution/extrusion via the generatrix, on B-spline surfaces via grid + Gauss–Newton. `body::Body`: B-rep in context coordinates with the triangles of each face (whether a point lies on the trimmed face, coarse search). `measure`: distance, angle, radius, area. `section`: contours, regions, wall thickness. `dist`: triangle distance with BVH (from `cad-cli/geom.rs`, also `--clearance`) |
| `cad-engine` | `measure_parts` (B-rep and tessellation per part, placed, mass properties), `section_parts` (kernel boolean with the kept half-space) |
| `cad-session::measure` | measurement model per node (once per evaluation), sections per plane (up to 6), snap, selection, measurements with dimension lines, texts and `mcad measure` arguments, provenance, references for the CLI |
| CLI `mcad measure` | resolve references, JSON/text (`measurements`, `section`, `at`) |
| UI | `app/measure.ts` (controller), `app/measureUi.tsx` (context bar `MeasureBar`, list `MeasurePanel`, status line), viewer (hits, highlighting, dimension lines, labels), `logic/measure.ts` (texts, copying) |

**Distance.** Coarse search via the tessellation (faces as triangles, edges as polylines,
points as degenerate triangles; BVH), then alternating projection onto the exact geometry
(point on A nearest the foot point on B and vice versa, until it settles). Projection onto a face:
onto the unbounded surface; if the point does not lie on the trimmed face (distance to its
triangles > 2 chord tolerances), onto the nearest boundary point (exact edge curves). Exact means:
both elements are analytic and the iteration converges; otherwise ± 2 × chord tolerance applies, and
the result of the coarse search is used if the refinement runs away from it. Two parallel planes: the
normal distance is the value, foot points = point on A and its projection onto B (guide line to the
nearest point of face B), the minimum distance of the bounded faces is shown alongside;
parallel axes (cylinders, cones, circles): additionally the axis distance. Δx/Δy/Δz of the foot points.

**Radius.** Circle and ellipse from the edge curve; cylinder, cone (radius at the location,
half-angle, apex), sphere, torus from the surface parameters; the extent along the axis from
triangles and boundary (drawn axis). Surfaces of revolution whose generatrix is a circular arc
(SMLib's fillet along a circular edge) are recognised as a torus (deviation ≤ 10⁻⁹ ·
radius: exact); a straight axis-parallel generatrix as a cylinder; extrusion surfaces of a
circular arc as a cylinder; B-spline edges that follow a circle to within 10⁻⁶ with a fitted circle
(`exact = false`, deviation stated); free-form faces: principal radii of curvature (numerical).
Fillets of straight edges are cylinders in SMLib, corners are spheres.

**Angle.** Direction per element: line (edge), plane (face: outer normal; circular edge:
its plane), axis (cylinder, cone, torus, surface of revolution), otherwise tangent or
tangent plane at the click point. Two planes: angle between the planes 0–90° (chamfer: 45°), plus
the angle of the normals and the opening angle at the intersection line between the half-planes towards
the click points (for adjacent faces the angle through the material or outside); the arc lies at
the intersection line nearest the clicks (for an obtuse opening angle, towards the extension of the second
face). Two edges with a common vertex: 0–180° away from the vertex; otherwise 0–90°. Line–plane:
angle to the plane.

**Area, length, body.** Planar faces exactly via the boundary (Green: ½∮(x dy − y dx) over the
exact edges, lines in closed form, otherwise Gauss–Legendre). Cylinder, cone, sphere, torus: the
triangles subdivided 4- and 8-fold, interior points projected onto the surface, boundary points onto the boundary curves,
Richardson extrapolation (relative error < 10⁻⁶ in the tests). Other faces: from
the triangles with a tolerance statement. Length: exact for lines and arcs, otherwise Gauss–Legendre.
Body: the part of the chosen face (named instance of the file, via tags → nodes →
ancestors), otherwise the body (part) of the node; volume, surface area, centroid from the
kernel (SMLib integrates to ~10⁻⁴), bounding box from the B-rep (vertices, exact edge extrema,
triangles, extreme points of spheres and tori).

**Section.** Plane ∩ B-rep via the kernel: per volume part an `Intersection` with a box
beyond the part that covers exactly the kept half-space `(q − p) · n ≤ 0` (same convention as
`mcad render --section` and the section view). The cap faces of the result (plane with
normal +n in the section plane) bound the section; their loops run counter-clockwise about n around material,
clockwise around holes, the inner normal of a contour is n ×
tangent. Each edge borders a cut face of the body; if that face is a plane, cylinder,
cone, sphere, torus or surface of revolution (the last three cut perpendicular to the axis),
the piece is formed in closed form (line, circle, ellipse; position and direction from the
kernel curve, whose deviation is passed along as `dev`); otherwise the kernel curve stays (`exact =
false`). Region = cap face (outline and holes) with area from Green. Wall thickness at a
contour point: ray perpendicular to the contour into the material up to the next contour (lines and circles
exact, otherwise sampled and refined); `--section` alone additionally reports the thinnest
wall thickness over 8 locations per piece (estimate). Cost: one boolean per part and plane
(cached); when dragging the plane in measure mode the engine computes the positions one after another.

**Snap** (session, `snap`). Tolerance as in the sketch editor: snap radius in pixels × pixel size
× depth. On the model: vertices, edge midpoints (arc length), circle centres, then edges near the
ray, then the hit face. Besides the ray, the UI sends the hit of its raycast
(node, `tri_faces` ID, point); the face ID is mapped via the parts of the node (IDs
follow one another as in `append_offset`) and checked at the hit point, otherwise the nearest
face applies (also for colour regions, whose meshes have different IDs). Hidden points (deeper than the
hit) and the removed half-space of the section view are not offered; at equal
screen distance (coaxial circles) the nearer one wins. Alt: no point snapping. Per measurement kind only matching
elements (radius: round edges and faces, angle: no points and spheres, …). In a section:
ray ∩ plane, vertices, midpoints, circle centres of the contour, contour pieces, for "Fläche" (area) the
region under the mouse.

**Provenance.** For each selection: the lines of the nodes of the face tags (for edges, of both
faces, for vertices, of all adjacent ones), with named instance and module (`Zeile 11: box (Instanz
k: housing)`); grid points (`g[b, h, o]`), grid lines for axes (`g[hole[0], mid, *]`),
grid planes for planar faces (`g.inner_left`) and anchors (`k.corner`) on which the element lies
(tolerance 10⁻⁶ relative, all grids of the scene).

**Protocol** (7.1). `Measure { id, kind, picks, ray?, hit?, pixel, snap_px, free, click,
section?, clip?, node? }` → `Measure { id, version, hover, picks, result, prompt, message,
section }`. As in the sketch editor, stateless in the engine: the selection (kind, node,
body, edge/face or contour/piece/region, point, text, reference, origin,
highlighting) goes back and forth with each request; at most one request in flight.
`Measurement { kind, title, value, unit, label, values[], lines, guides, points, at, details,
picks, exact, tol?, section?, text, cli }`: `text` is the copy as text (with `// ` the one as a
comment), `cli` the arguments after the file name (with `--module`/`-D` of the context and
`--section …`), the UI prepends `mcad measure <datei>`. Same over `WorkerTransport` and
`TauriTransport`.

**CLI.** Element references `vertex@P`, `mid@P`, `center@P`, `edge@P`, `face@P`, `body@P` (the
element nearest P; for faces the exact face decides among equally near ones), with `--section`
the same prefixes for the contour and `region@P`; points as for `--between`. `--at P` lists what
lies at P (with reference) – this is how an agent finds the references. JSON: `measurements`
(protocol type, numbers to 12 digits, without highlight polylines), `section` (regions,
contours with segments, circles, thinnest wall), `at`.

**UI.** Toggle "Messen" in the view bar; context bar `MeasureBar` (measurement kinds
distance, radius, angle first as in the UI concept, then length, area, body, in a section
wall thickness; "Im Schnitt" turns on the section view; done); list `MeasurePanel`
(to the right of the view, later the "Messen" tab of the side panel) with value, accuracy,
details, origin, copy (text, `// Kommentar`, `mcad measure`), delete; "Alle kopieren" (copy all),
"Alle löschen" (delete all). Both are self-contained components (`.contextbar`), the UI rework only has to
place them (context bar top left below the toolbar). In the view: element under
the mouse (snap colour) with label, selected elements (accent), dimension lines, dashed
guide lines (axes, bounding box), points and one label per measurement (DOM, placed every frame,
without overlap; click selects). Colours `--view-measure`/`--on-measure` per theme
(`contrast.test.ts`: text ≥ 4.5:1, line to background ≥ 3:1). No handles in measure mode;
a click is a press without movement (left-dragging still rotates); Esc discards the selection or
exits, Del deletes the selected measurement, letters D/R/W/L/F/K/T choose the measurement kind. A
model change discards open selections; finished measurements stay (hint "veraltet" (outdated)).
Sketch editor and measuring are mutually exclusive.

**Tests.** `cad-measure`: unit tests (curves, surfaces, quadrature) and `tests/analytic.rs` against
SMLib bodies (cylinder: radius, axis, lateral and cap area, circle length, normal distance;
cone, sphere, torus: radii and areas; fillet cylinder, sphere, torus of a circular edge;
chamfer 45°/135°; distances face–sphere, point–sphere, skew edges; section through bores
with exact circles, areas, wall thicknesses, oblique section as an ellipse). `cad-session`
`tests/measure.rs` (snap with provenance, clicks, CLI arguments, perforated plate with bore and
fillet radii, housing from `tests/models/spec` with wall thickness 3 mm in section, inner distance,
part volume, chamfer, section mode via the protocol). CLI `tests/measure.rs`. Vitest
`logic/measure.test.ts`. Playwright M01–M11 (`web/e2e/measure.spec.ts`), Tauri T17.

**Limitations.** The section costs one kernel boolean per part and plane; if it fails, there is
no section (message) and no substitute contour from the mesh. Contours on free-form faces and
on obliquely cut cones, tori and surfaces of revolution stay kernel curves (`exact =
false`). The wall thickness measures perpendicular to the contour at one location (not the minimum distance
to the opposite side), the thinnest wall thickness is an estimate from samples. Distances between
free-form faces and cases in which the alternating projection ends at another local minimum
are given with the tolerance of the tessellation. Selection via points (`face@P`) is unambiguous
as long as P is nearest to one element (CLI references with 6 decimal places). Areas of
free-form faces from the triangles; bounding box of free-form bodies from triangles and edges (interior
extrema of B-spline faces only up to the chord tolerance). Volume from the kernel
(~10⁻⁴). The order of the CLI results follows the options, not the command line.
2D results (sketches without extrusion) cannot be measured. The list of measurements lives
only in the session (not saved). Tauri T17 drives the view with synthetic
pointer events.

## 19. UI rework (test report 20)

**Basis.** Concept `docs/concepts/ui-redesign/concept.html` (+ PDF), accepted by the user
with all proposals (option A) and teal/emerald instead of blue as the accent. No
new functionality; order, emphasis and uniform controls in browser and Tauri.

**Components** (`web/src/app/ui/`): `Button` (primary – at most one per area, standard,
quiet, toggle with `aria-pressed`), `IconButton` (26 × 26), `Segmented` (radio group, arrow keys,
Home/End), `MenuButton`, `SplitButton`, `ContextMenu` (WAI-ARIA menu with `menuitem`,
`menuitemradio`, submenus; popups in `<body>` with fixed position, close on click outside,
Esc returns focus), `Toolbar`/`ToolGroup`/`ToolSep`/`ToolLabel`. One height 26 px
(`--ctl-h`), radius 6 px, 2 px inside, separator + 12 px between groups; Inter 12 px from
`crates/cad-font/fonts` (same widths in Chromium and WebKitGTK). Icons: `lucide-react`
(ISC), own stroke icons for line, arc and "⊥ Ansicht". `aria-label` is the plain
name, the tooltip (`title`) appends the keyboard shortcut, `aria-keyshortcuts` carries it.

**Layout.**

| Area | Content |
| --- | --- |
| Header (`app/header.tsx`) | app name, "Datei ▾" (file; Neu, Öffnen …, Ordner öffnen …, Zuletzt geöffnet › desktop only, Speichern unter …, Datei hinzufügen …, Datei laden …, ZIP laden …, ZIP speichern – new, open, open folder, recently opened, save as, add file, load file, load ZIP, save ZIP), "Beispiele ▾" (examples) with submenus per group; centred the file name with change dot (desktop) and the path into the module context; "Speichern" (save) primary; split export button (last format in localStorage `cad-export`, Ctrl+E; 3MF multi-colour/single-colour, STL, STL per colour, STEP, per part › STL ZIP/STEP ZIP/3MF); theme and settings icon buttons. Shortcuts Ctrl+N (desktop only), Ctrl+O, Ctrl+S, Ctrl+⇧+S, Ctrl+E in the whole window |
| Editor | tabs (`role=tab`, ←/→) and "+" (new file, load file, actions of the displayed file); right-click or Shift+F10/context menu key on a tab: "Als Hauptdatei" (as main file), "Schließen" (close) |
| View | toolbar at the top: left the mode model/section/sketch/measure (`runtime.viewMode`/`setViewMode`, exactly one; sketch and measure leave a section in place, afterwards mode section; colours is not a mode), right fit, view ▾ (iso, top, front, right, back, left, bottom), all grids, display ▾ (all grids, show colour regions, side panel), show side panel (when collapsed). Context bar of the mode at the top left below it (`.ctxslot`). "Neu einpassen" (refit) bottom centre. View cube bottom right (`app/viewCube.tsx`, SVG, 3 × 3 fields per face: face, edge, corner; follows the camera via `Viewer.onCamera`; x/y/z next to it) instead of the axis triad |
| Context bars | Section: X/Y/Z/view, position (`p·n` free), normal, flip, "Als `mcad render …` kopieren". Sketch: select \| line, arc ▾ (last arc kind, `sketchArc`), Bézier, rectangle, circle \| round corner, close, delete \| ⊥ view, "Fertig" (done) primary; keys V, L, A, ⇧A, T, B, R, C, F, Enter, Del, N (`logic/sketchEditor.sketchKeyAction`). Measure: measurement kind as segmented control (D/R/W/L/F/K/T), "Im Schnitt", copy/delete of the selected or last measurement, "Fertig" |
| Side panel (`app/sidePanel.tsx`) | right, collapsible (`sideOpen`, `sideTab` in localStorage), tabs parts, colours, measure and, in the module context, parameters (selected on entering, the previous tab on leaving); measure mode selects "Messen". `colorsVisible()` replaces `colorsOpen` (colour regions are requested for the visible tab) |

**Themes.** Accent light `#0f766e`, dark `#3cc4a8` (plus `--accent-hover`, `--accent-text`,
`--accent-soft`, `--focus`, `--seg-on`, `--ctl-hover`, `--float-shadow`); 3D view
`--view-accent`/`--view-flat` light `#0f766e`, dark `#62dcc2`/`#3cc4a8`, `--hover-line` and
`--viewhover-line` teal; cube `--vc-face`, `--vc-edge`, axes `--axis-x/y/z`. The pairs are in
`logic/contrast.ts` (text ≥ 4.5:1, controls ≥ 3:1, accent on the model ≥ 2:1). The
model palette stays.

**Wayland.** Before GTK, `cad-app` sets the GLib program name to the `identifier` from
`tauri.conf.json` (`dev.mcad.desktop`, up to 0.2 `dev.controller-cad.desktop`); GTK uses it as the Wayland `app_id` and
X11 `WM_CLASS`. The desktop file is named after it (`packaging/linux/dev.mcad.desktop.desktop`,
`StartupWMClass` the same), `make install` derives the name from `tauri.conf.json`.

**Tests.** Playwright selectors via role and name (helpers `pickMenu`, `menuItem`, `mode`,
`sideTab`, `fileTab`, `sketchTool`, `measureKind`, `exportAs` in `web/e2e/helpers.ts`); new
`e2e/ui-overhaul.spec.ts` U01–U09 (bars at 1280/1440/1920 px light/dark in all modes:
`scrollWidth ≤ clientWidth`, nothing sticks out, single line, no overlap; menus via keyboard,
export format with Ctrl+E and after reload, tabs and context menu, mode exclusivity, sketch shortcuts,
view cube, side panel, tab order, contrast on the rendered button); Tauri T18 (bars at
three window widths in WebKitGTK, menus, cube). Before/after images: `tools/report/report20-shots.mjs`.

**Limitations.** No renaming of files; edges and corners of the cube only with the mouse (keyboard:
menu "Ansicht" (view)); no X-ray toggle in the UI (it only existed in the CLI); Ctrl+N only in the
desktop app; the default split editor : view is 33 : 67 (about one third as in the concept), narrow views show mode and
measurement kind as icons.

---

## 20. `hull` and `minkowski` (plan 21, phase A)

User decision: only exact special cases; if none fits, a warning with the reason and the children
unchanged (no approximation). Evaluation produces `Op::Hull` / `Op::Minkowski`; the engine
decides based on the children.

- **Kernel offset** (`Kernel::offset`, facade `sc_offset`, version 5): SmApiOffsetBrepFull
  outwards with rounding and self-intersection handling (without it, box and L profile fail).
  Spike (report 21): exact for box (also rotated), cylinder, sphere, L profile (concave edge
  stays sharp), filleted box – volume to 3·10⁻⁸ of the formula. At corners with angles other than
  right angles (tetrahedron, 45° wedge) SMLib fails (wedge) or returns correct faces with
  approximated trim curves (tetrahedron: up to 0.33 mm off the edges, volume +4.5·10⁻⁴). The
  facade therefore checks the boundary distance to the body and the trim curves against the 3D edges and otherwise
  reports "offset is not exact". A substitute via face offset + fillet of all edges does not work
  (fillet fails on box and L profile).
- **2D hull** (`cad_kernel::hull2`): support function over the normal angle, points and arcs as
  sites, transitions along common tangents (closed form), arc ends; result lines + arcs
  counter-clockwise. B-splines via the biarc approximation.
- **3D** (`cad-engine/src/hull3d.rs`): children as bodies (vertices of planar bodies; spheres, frusta,
  prisms only under similarity transformations); cases polyhedron (Quickhull `cad_kernel::hull3`),
  solid of revolution (2D hull of the half-sections, half ρ ≥ 0, revolve), prism (2D hull of the
  cross-sections, extrude), spheres (polyhedron + offset). `minkowski`: offset, sum of parallel
  prisms (2D sum: offset with circle or convex sum), convex polyhedra (hull of the
  vertex sums).
- Faces of the result carry the tag of the hull/minkowski node. Solids of revolution consist
  of surfaces of revolution (STEP `SURFACE_OF_REVOLUTION`), not of sphere/cylinder.
- Limitations: `rotate_extrude`, torus, `$fillet` on primitives, `difference` as children are not a
  special case; equal-sized spheres in one plane (rounded plate) are not either; the Wasm bundle
  needs a new `smlib.wasm` (export `sc_offset`).

## 21. Collision asserts (plan 21, phase C)

User decision: violation = warning; `mcad export` aborts (exit 4), except with
`--ignore-asserts`; `assert_keepout` clearly visible in the UI.

- **Language**: `assert_keepout(clearance)`, `assert_disjoint()`, `assert_intersect()`,
  `assert_touches(tol, contact)` create an `Op::Group` node (no new operation: all
  engine paths treat it like `group`) and an entry in `Tree.asserts` (`AssertDecl`: node,
  kind, parameters, span; one per evaluated instance). The keep-out volume gets
  `Flags::keepout` and `background`: without further special cases it is thus not in the result,
  not in booleans, export, volume, colour regions and parts, but still evaluated (mesh for
  display). Asserts can be nested; named instances inside them remain parts.
- **Check** (`cad-session/src/asserts.rs`): after the kernel job, in the next `Session::step`
  after the root mesh (the scene appears first), not in draft mode while dragging
  (`checked: false`). Result: message `FromEngine::Asserts`, `Scene.asserts` of the finished
  evaluation, warnings (source kernel) at the statement. Bodies: child = set of all bodies and
  meshes of the child (context coordinates); for the keep-out volume, every body of every part and of the
  material outside (like the colour list), otherwise of the root.
- **Method**: intersection volume exactly via kernel boolean per pair of bodies with intersecting
  bounding boxes (volume from the mass properties, mesh of the intersection with edges for
  display); with meshes without B-rep only yes/no (`solids_overlap`). Distance via the tessellation
  (`min_distance`, foot points, accuracy ± 2·mesh tolerance). Contact at near face pairs of the
  exact B-rep (`cad_measure::Body`): opposite coplanar planes → intersection of the two
  face outlines with `region2` (lines and arcs exact), area in mm², fill for display
  via `i_triangle`; cylinder/sphere on plane, cylinder on cylinder (parallel: line, skew:
  point), sphere/sphere → line/point contact; other faces only on the mesh. A failing
  boolean (coincident faces) is a hint "Überlappung nicht prüfbar" (overlap cannot be checked), not an error.
- **Tolerances** relative to the size L of the pair: ε_V = 10⁻⁹·L³, contact area > 10⁻⁹·L²,
  mesh tolerance clamp(2·10⁻⁴·L, 0.001, 0.02) mm.
- **`assert_intersect`** requires one connected group (union-find over the
  intersecting pairs), not pairwise: a holder that connects two plates is valid; for each
  further group the nearest pair is reported. Touching is not intersecting.
- **Cache**: pair result in the engine LRU (new slot `Aux`, `Engine::aux_get/aux_insert`) under
  the key from node keys, placements, kind and parameters: `watch`, daemon and UI
  recompute only changed pairs (example: 2nd call via the daemon 0 ms).
- **CLI**: `check` (`asserts`, `violations`), `export` (exit 4, `--ignore-asserts`), `render`
  (keep-out volume yellow translucent with dashed outline, intersection red with edges on top,
  contact green, gap as a line with foot points, `--show asserts`), `outline`/`measure`/`inspect`.
- **UI**: display mode `keepout` (hatched via a shader addition, translucent, visible through
  the model, dashed outline, not selectable/measurable), toggle "Sperrvolumen" (keep-out volume) in
  the toolbar and in the display menu (remembered), overlay of the results with labels,
  status bar "n Assert-Verletzungen" (n assert violations; click: next violation, cursor on the statement,
  camera frames the intersection body; likewise click on the warning at the end of the line), export prompt
  "Trotzdem exportieren" (export anyway) for all export paths (equivalent to `--ignore-asserts`).
- Limitations: contact area on curved coincident faces (pin in bore) only estimated
  on the mesh; mesh intersection without volume and without intersection body; the check blocks the
  worker for the duration of the booleans (cached; skipped while dragging).
