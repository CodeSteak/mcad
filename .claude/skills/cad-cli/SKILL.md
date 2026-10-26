---
name: cad-cli
description: Develop and check models in the parametric CAD language (OpenSCAD style with grids, anchors and fillet zones, files *.mcad, also *.scad) without a UI – outline, diagnostics, images (contact sheet, grids/anchors overlaid, section, X-ray), measurements on the exact B-Rep (distance with foot points, radius/diameter, angle, length, area, body, section contours and wall thickness, volume, collisions, volume per colour), watching for file changes, setting parameters and export STL/3MF (multi-colour for Bambu Studio/OrcaSlicer)/STEP with the CLI `mcad` (crates/cad-cli). Use when a model is written, changed, examined or debugged.
---

# CAD CLI `mcad`

The CLI evaluates a source file with the same crates as the UI (parser, evaluation,
engine, native SMLib kernel) and shows the result as text, JSON or image. This makes it possible
to build and check a model entirely headless – also in parallel with a human who
edits the same files in the UI (`mcad watch` notices every save).

Create new models as `*.mcad`. `*.scad` files (OpenSCAD, older projects) open, compute
and export the same way, also mixed in `include <…>`/`use <…>` (the path names the file,
extensions are not added). Up to 0.2 the program was called `cad`; the old environment variables
`CAD_*` still work (deprecated, `MCAD_*` takes precedence).

## Building and running

```sh
# SMLib native: prebuilt libraries (fast) or build via CMake (~3 min the first time)
export SMLIB_PREBUILT=/home/user/controller-cad/smlib/build-native   # if present
smlib/fetch.sh                                                         # sources (tangency-labs/smlib @ 7eba8aa), if missing
cargo build --release -p cad-cli
target/release/mcad --help              # help (without arguments: desktop UI)
target/release/mcad usage               # manual for agents (topics: mcad usage --list)
```

File `-` reads from stdin: `echo 'cube(5);' | mcad check -`. Without a command, `mcad` starts the
desktop UI (`mcad`, `mcad datei.mcad`, `mcad ordner/`); without a graphical session (no
`DISPLAY`/`WAYLAND_DISPLAY`, e.g. in containers and CI) this ends with exit 2 and a hint –
agents always work with the commands below, the UI is never needed. Headless build without UI:
`cargo build --release -p cad-cli --no-default-features`.

## Recommended workflow for agents

1. **Understand:** `mcad outline datei.mcad` – modules with parameters (default value, type), grids
   and anchors per module, functions, variables with values, named instances, parts (bounding box,
   volume), `include`/`use`, diagnostics. One call is enough for a project.
2. **Edit:** change the file (Edit tool) or `mcad set` for default values/literals.
3. **Check:** `mcad check datei.mcad` – only continue when there are no errors. Read warnings
   (e.g. „0 Kanten“ (0 edges) for an empty zone selection).
4. **Look:** `mcad render datei.mcad -o /tmp/x.png --views all --show grids,anchors` and
   **look at the image** (Read tool). For a module `--module NAME` (grids are only visible in their own
   module), for details `--line N`, `--section z=…`, `--xray`, `--show labels,dims`.
5. **Measure:** `mcad inspect datei.mcad --line N` (node, bounding box, volume, edges,
   kernel message) and `mcad measure`: exactly on the B-Rep distance (`--distance face@P face@Q`),
   radius (`--radius edge@P`), angle, length, area, body; in a section (`--section z=…`)
   contours, bore circles and wall thickness (`--wall P`); `--at P` says which elements lie at P
   and how to refer to them. Also distances between anchors/grid points, bounding box, volume,
   collisions of the parts, distance between two parts.
6. **Output:** `mcad export datei.mcad -o teil.step` (or `--all-parts -o ordner/`). For
   multi-colour printing `mcad export datei.mcad -o teil.3mf` (multi-colour by default: one
   object per part, material outside the parts as object „ohne Bauteil“ (without part), one part per colour with
   filament slot; without colour group, so Bambu Studio and OrcaSlicer take over the slots without
   a colour dialog; beforehand `mcad measure datei.mcad --colors` for slots, volume and mass per colour).

The kernel cache persists between calls (**daemon**, below): a second call
on the same or a slightly changed file only recomputes the changed nodes. For
many small steps there is additionally `mcad watch datei.mcad --json --render /tmp/x.png --views all`
in a background process – every file change yields one JSON line and a new image.

## Daemon (cache between calls)

`check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure` and `set` compute in a
background process `mcad daemon serve`, which the first such call starts (detached, log
`daemon.log` in the socket directory) and which holds the engine cache. Output, exit codes, files and
JSON are the same as without the daemon (only `--json` additionally has `daemon: {used, reason}`, and
the counters `stats.cache_hits`/`kernel_calls`/`cached` as well as the timing fields show what actually
happened). The cache is content-based: changed files (including included ones) are always
re-read, a result is never stale. Nothing to do for agents – just use the
commands; `watch` has its own cache and does not use the daemon.

| Command / setting | Effect |
| --- | --- |
| `mcad daemon status [--json]` | PID, version/build, uptime, idle time, requests, projects, cache (entries, MiB, limit, hit rate, evicted), RSS |
| `mcad daemon stop` | stop (waits for the running command) |
| `mcad daemon serve [--cache-mb N] [--idle-minutes M] [--max-projects N] [--watchdog-secs S]` | in the foreground (debugging) |
| `--no-daemon` (any command), `MCAD_NO_DAEMON=1`, `[daemon] enabled = false` | compute without the daemon |
| `--verbose` | on stderr: daemon used (time, cache hits) or why not |

Limits (default): 512 MiB cache (node results + meshes, measured per node on the heap in the daemon),
16384 entries, results of 8 projects (main files), clear the cache when RSS exceeds
start + 1.5 × limit, exit after 15 min without a request, watchdog 30 min (a hanging command terminates
the daemon, the command then runs without the daemon and reports this on stderr). Configurable in
`$XDG_CONFIG_HOME/mcad/config.toml` (or `~/.config/…`, path also `MCAD_CONFIG`; the old
`controller-cad/config.toml` still applies if the new one does not exist):

```toml
[daemon]
enabled = true
cache_mb = 512
max_projects = 8
idle_minutes = 15
watchdog_secs = 1800
rss_factor = 1.5
```

or via `MCAD_DAEMON_CACHE_MB`, `MCAD_DAEMON_MAX_PROJECTS`, `MCAD_DAEMON_IDLE_SECS`/`_IDLE_MINUTES`,
`MCAD_DAEMON_WATCHDOG_SECS`, `MCAD_DAEMON_RSS_FACTOR` (environment before file, arguments of `serve`
before both). If there is no daemon (Windows, unusable socket directory, error), the
command silently computes by itself. An `mcad` with a different build ID replaces the running daemon. Socket directory
as for `mcad ui`: `$MCAD_IPC_DIR`, otherwise `$XDG_RUNTIME_DIR/mcad`, otherwise
`/tmp/mcad-<uid>` (0700, socket 0600); if the socket path is too long (> 107 bytes),
the socket is placed at `/tmp/mcad-<uid>/<hash>-daemon.sock` (`mcad daemon status` shows it).

## Collaboration: user in the desktop UI, agent edits and checks

The user has the file open in the UI (`mcad datei.mcad`), the agent changes the same file
and checks with the CLI. The UI watches the file and all included files:

- **File without unsaved changes in the UI:** every change by the agent (Edit tool,
  `mcad set`) appears immediately in the editor and in the 3D view (~0.2–0.3 s, report 9); the user's cursor and
  viewport stay, the change is a separate undo step.
- **User has unsaved changes:** the UI overwrites nothing, but shows the bar
  „Datei wurde extern geändert: Neu laden / Meine behalten / Vergleichen“ (file was changed externally: reload / keep mine / compare). Therefore check
  `mcad ui status --json` before writing: if the file is listed in `dirty`, ask the user instead of
  writing.
- Workflow: `mcad ui status` (what is open, cursor line, selected node, context module, values
  of the parameter bar, diagnostics) → edit → `mcad check` → `mcad render …` (own images) →
  if needed `mcad ui show datei.mcad --line N [--module m]` (shows the user the location: window
  to the front, cursor there) and `mcad ui screenshot -o /tmp/ui.png` (the 3D view as the
  user sees it; look at the image).
- Without a running UI, `mcad ui …` ends with exit 3 („keine laufende Desktop-UI“ (no running desktop UI)) – then simply
  continue headless. Everything else works unchanged without a UI.
- Section from the UI: „Als `mcad render …` kopieren“ (copy as `mcad render …`) in the section view yields the same
  section as a CLI call (`--section z=12 --azimuth … --elevation …`); `mcad ui status` reports it
  under `section.cli`.

| Command | Result |
| --- | --- |
| `mcad ui status [--json]` | per window: `project {root, main, active, files, folder}`, `dirty` (unsaved files), `cursor {file, line, col}`, `selection {line, nodes}`, `context` (module), `params` (parameter bar), `diagnostics`, `scene {version, complete, nodes, parts}`, `section` |
| `mcad ui show DATEI [--line N] [--module m]` | open the file or switch to it, set the cursor, bring the window to the front |
| `mcad ui screenshot -o bild.png` | PNG of the 3D view |

## Commands

| Command | Purpose |
| --- | --- |
| `mcad check DATEI [--no-kernel]` | diagnostics `datei:zeile:spalte: Fehler/Warnung (Syntax/Auswertung/Kernel): …` (file:line:column: error/warning (syntax/evaluation/kernel)) |
| `mcad outline DATEI [--no-kernel]` | outline (see above); `--no-kernel`: without bounding boxes/volumes, faster |
| `mcad eval DATEI [--module m] [-D name=wert]…` | tree (nodes, line, status, bounding box), values of the variables, grids, anchors, `echo` |
| `mcad inspect DATEI --line N` | what line N produces: nodes, bounding box, volume/area, selected edges, hints („4 Kanten“ (4 edges)), errors, handles |
| `mcad render DATEI -o bild.png [options]` | image (options below) |
| `mcad measure DATEI [options]` | measure (options below) |
| `mcad watch DATEI [--json] [--render bild.png …]` | watch and re-evaluate on every change (below) |
| `mcad set DATEI --param modul.param=wert \| --literal [datei:]zeile:spalte=wert [--dry-run]` | change code via the engine (below) |
| `mcad export DATEI -o teil.stl\|teil.3mf\|teil.step [--schema ap214] [--merge]` | export file (STEP: exact B-Rep, AP242, face colours from the colour regions, one named body per part and per top-level object outside the parts – touching parts stay separate, intersecting ones are united into one body „a + b“ (hint); `--merge`: one merged body as before; 3MF: **multi-colour**, one object per part + „ohne Bauteil“, one part per colour with filament slot for Bambu Studio/OrcaSlicer, without colour group; `--color-group` with colour group for other programs – Bambu then opens a colour dialog, „Abbrechen“ (Cancel) keeps the slots; single colour with `--single-color`) |
| `mcad export DATEI --per-color [--part NAME]… -o farben.zip` | ZIP with one STL per colour (and part): `F<slot>_<farbe>.stl`, all in the same coordinate system (extension `.zip` required) |
| `mcad export DATEI --part NAME [--part NAME2] -o …` | only these parts (named instances at top level, e.g. `deckel`, `k[2]`) |
| `mcad export DATEI --all-parts -o ordner/ [--format stl\|3mf\|step]` | each part as a separate file (`deckel.stl`, `k_2.step`) |
| `mcad diff-kernels DATEI…` | comparison SMLib ↔ solidcore (development only, `--features solidcore`) |

Common options: `--json`, `--module m` (module as context, nested `aussen.innen`;
with arguments as in the code `-m 'halter(30, h = 5)'`, `-m 'teile.schraube(d = 3)'` – expressions
see variables and functions of the file, can be combined with `-D` for other parameters),
`-D name=wert` (parameter of the context module, value as JSON: `-D w=60`, `-D hohl=true`,
`-D p=[1,2,3]`). Without a module context, `-D` sets top-level assignments of the main file
(like `openscad -D`: the last assignment of the name gets the value, dependent variables
use it, `$fn` etc. also without an assignment; unknown name → warning).

**Context as in the editor:** if `--line` lies in a module definition, `inspect` and
`render` show only this module with its default values (and `-D`). `--whole-file` keeps the
file context.

**Targets** (`--fit`, `measure --bounds/--volume/--area`): `all` (root), `part:NAME` (or just
`NAME`), `line:N` (nodes of the line), `node:I` (index from `mcad eval`).

### `render`

| Option | Effect |
| --- | --- |
| `--view iso\|front\|back\|top\|bottom\|left\|right` | standard view (default `iso`) |
| `--views iso,front,top,right` / `--views all` | contact sheet: labelled sub-images with **the same scale** (`all` = iso, front, top, right); `--size` is then the total size (default 1600x1200) |
| `--azimuth A --elevation E` | free camera: azimuth 0° = from the front (−y), 90° = from the right (+x); elevation 90° = from above; `iso` = 45°/35.26° |
| `--zoom Z`, `--target x,y,z`, `--fit ZIEL` | zoom, look-at point, fit to a target (default: everything drawn) |
| `--ortho` (default) / `--perspective [--fov 35]` | projection |
| `--part NAME` (repeatable) | only these parts |
| `--line N` (repeatable), `--node I` (repeatable), `--only` | nodes in full, the rest dimmed (omitted with `--only`); zones blue, subtracted bodies red semi-transparent, selected edges orange, failed edges red |
| `--show grids,anchors,handles,edges,labels,bbox,dims` (or `all`) | overlays: grids of the context module (planes as rectangles at the edge, expression planes dashed, intersection points as dots, plane names `g.links`), anchors as yellow diamonds with name and direction arrow, handles as orange squares, hidden edges dashed, `labels`: grid points `g[a,b,c]`, handles, part names; bounding box, `dims`: dimensions W/D/H in mm |
| `--section x=WERT\|y=WERT\|z=WERT` | section at an axis plane, cut face filled (red hatched) |
| `--section-plane px,py,pz,nx,ny,nz` | section at an arbitrary plane (point, normal) |
| `--section-flip` | keep the other half |
| `--xray` | everything semi-transparent with edges |
| `--theme dark` | dark colour scheme (as in the UI; palette colours in the dark variant) |
| `--show regions` / `--show faces` | colour regions (body per colour as exported; default for models with colours) or colour per face (faster, inaccurate with overlap/fillets) |
| `--size BxH`, `--no-axes`, `--whole-file` | image size, without axis cross, file context |

**Section convention (same in the UI):** plane through point `p` with normal `n`; the half
`n` points into is removed, `(q − p) · n ≤ 0` remains. `x=10` means `p = (10,0,0)`,
`n = +x` (removes x > 10), likewise `y=`/`z=`; `--section-flip` reverses `n`. Method:
clipping in the renderer with a filled cap (stencil principle: cap where the plane lies inside the
body), no kernel Boolean – fast and the same for every plane. In `iso` you see
the cap of `z=`/`x=` sections; for `y=` use `--section-flip` or `--view back`.

Grids are only visible in their own module: `--show grids` in the file context only shows grids
at top level (hint in `notes`); for module grids use `--module NAME` or `--line N` in
the module definition.

### `measure`

| Option | Result |
| --- | --- |
| `--between A B` | distance and difference vector. A/B: anchor `k.ecke`, `k[2].ecke` (in module context also `ecke`), grid point in the context `g[rechts, hinten, deckel]` (entries also numbers or `_`), coordinate `[x,y,z]` |
| `--bounds [ZIEL]` | axis-aligned bounding box (min, max, size) |
| `--volume [ZIEL]`, `--area [ZIEL]` | volume mm³ / area mm² (kernel, exact) |
| `--colors` | colour regions: volume and mass (PLA 1.24 g/cm³) per colour and filament slot, per part (sum = part volume) |
| `--interference [--tolerance 0.001]` | pairwise intersection volume of all parts (kernel intersection of the exact B-Rep), collisions above the tolerance (mm³) |
| `--clearance A B [--mesh-tol 0.01]` | smallest distance between two parts via the tessellation (triangle-triangle distance), accuracy ± 2·mesh-tol; if they intersect: 0 and `collision: true` |
| `--distance A B` | smallest distance between two **elements** with foot points, exact from the B-Rep (planes, cylinders, cones, spheres, tori, lines, circles in closed form; otherwise numerically with tolerance); two parallel planes: normal distance (`details`: „parallele Ebenen“ (parallel planes), plus the smallest distance of the bounded faces); parallel axes: axis distance. A/B: point as for `--between` or element reference (below). Repeatable |
| `--radius REF` | radius, diameter, centre, axis of a circular edge (`edge@P`) or round face (`face@P`: cylinder, cone with half-angle and apex, sphere, torus with ring radius; fillets on circular edges recognised as surfaces of revolution; free-form: radii of curvature, `exact: false`) |
| `--angle A B` | angle between two edges or faces: planes 0–90° (`values.opening` = opening angle at the intersection line, `normals`), lines 0–90° (at a shared vertex 0–180°), line–plane; cylinder/cone via the axis |
| `--length REF`, `--area face@P` | length of an edge (exact for line/arc), area of a face (exact if planar) |
| `--body REF` | part or body of the element: volume, surface area, centroid, bounding box from the B-Rep (`details[0]`: „Bauteil k (Zeile 21)“ (part k (line 21))) |
| `--section z=W` (`--section-plane p,n`, `--section-flip`) | section as in `mcad render`: plane ∩ B-Rep via the kernel; alone: regions with area and holes, contours with segments (`kind`, `center`, `radius`, `exact`, `source` = intersected face), circles (bores), smallest wall thickness (estimate over 8 positions per segment). With `--distance`/`--radius`/`--angle`/`--length`/`--area region@P`/`--wall` you measure in the plane |
| `--wall P` | wall thickness in the section at the contour point nearest to P, perpendicular to the contour through the material (needs `--section`) |
| `--at P` | what lies at P: nearest vertex, edge midpoint, circle centre, edge, face (in a section: contour, region) with reference, distance and origin – for finding the references |

**Element references:** `vertex@P`, `mid@P` (edge midpoint), `center@P` (circle centre), `edge@P`,
`face@P`, `body@P` take the element of the result nearest to P (P as for `--between`:
`[x,y,z]`, anchor, grid point); with `--section`, `vertex@`, `mid@`, `center@`, `edge@` refer
to the section contour and `region@P` to the region containing P (`face@`/`body@` are then not
available). Order of the results in `measurements`: distances, radii, angles, lengths,
area, body, wall thicknesses. Each measurement has `text` (lines as in the UI), `cli` (the
arguments that repeat it) and `picks[].origin` (origin: `Zeile 11: box (Instanz k:
gehaeuse)`, `Rasterebene g.innen_l`, `Rasterpunkt g[b, h, o]`, `Rasterlinie g[loch[0],
mitte, *]`, `Anker k.ecke`). The UI („Messen“ (Measure), „Als mcad measure kopieren“ (copy as mcad measure)) produces the same
calls.

Example (spec `gehaeuse`): `mcad measure gehaeuse.mcad --between k.ecke k.oben` → 67.082 mm,
Δ [−60, −30, 0]; `mcad measure gehaeuse.mcad --module gehaeuse --between "g[links,vorne,boden]" "g[rechts,hinten,deckel]"` → 53.852 mm.
Wall thickness: `mcad measure gehaeuse.mcad --section z=10 --wall "[0,15,10]"` → 3 mm (two regions
of 90 mm² each, smallest wall ≈ 3 mm); inner dimension: `--distance "face@[3,15,10]" "face@[57,15,10]"` →
54 mm (parallel planes); perforated plate: `mcad measure lochplatte.mcad --radius "face@[8,6,2]"` →
R 2 · ⌀ 4 (axis through [6, 6, …], grid line g[loch[0], mitte, *]), `--section z=2` → four
circles R 2, `--distance "center@[6,6,0]" "center@[18,6,0]"` → 12 mm.

### `watch`

`mcad watch DATEI [--json] [--render bild.png RENDER-OPTIONEN] [--once-after-change] [--timeout S]`
watches the file and all included files (`include`/`use`, also newly added ones),
re-evaluates on change in the same session (the engine cache persists: only changed nodes
go to the kernel) and writes one line per run to stdout – with `--json` one
JSON object per line. Stop with Ctrl+C, `--once-after-change` (after the first run triggered by a
change) or `--timeout S` (S seconds without a change).

### `set`

- `mcad set datei.mcad --param gehaeuse.w=50` writes the default value (like „Als Standard
  übernehmen“ (apply as default) in the UI; module also from `use` files, nested `aussen::innen.p`).
- `mcad set datei.mcad --literal 21:17=70` replaces the editable literal at line:column
  (column anywhere in the literal; the literals the UI changes: variable values, handles,
  grid planes, default values); in project files `--literal lib/teile.mcad:12:20=4`.
- Several `--param`/`--literal` in one call; `--dry-run` only shows the diff. Before
  writing, the changed project is evaluated: if the change produces new errors,
  nothing is written (exit 1, `refused: true`), except with `--force`. Writing is atomic
  (temporary file + rename).

Exit codes (all commands): `0` ok · `1` diagnostics with errors (output still complete;
`set`: change refused) · `2` invocation error (arguments, file, unknown anchor/part/
plane) · `3` kernel/internal error.

## JSON (summary)

Every response is an object with `schema_version` (2), `command`, `file`, `ok`, `exit_code`,
`context` (module name or `null`), `diagnostics: Diagnostic[]`, for invocation errors `error`, for
the commands with daemon `daemon: {used, reason}` (`reason` e.g. `--no-daemon`, otherwise `null`).
`Diagnostic = { severity: "error"|"warning"|"info", message, source: "syntax"|"eval"|"kernel",
node, range, file?, at?, nodes? }` (`at`: location of a kernel error, e.g. where the operands
of a Boolean touch tangentially – „Zeile 12 (dom) und Zeile 30 (rippe) berühren sich
tangential bei [x, y, z]“ (line 12 (dom) and line 30 (rippe) touch tangentially at [x, y, z]); `mcad render` marks the location in red), `range = { from, to, line, col, end_line, end_col }` (1-based,
offsets UTF-16). `Bounds = { min, max, size }`. Complete schema: header of
`crates/cad-cli/src/main.rs`.

- `check`: `+ { errors, warnings }`
- `outline`: `+ { files, includes: [{file, line, path, use, found}], modules: [{name, qualified,
  file, line, end_line, params: [{name, default, default_value, type}], grids: [{name, line,
  planes: [{axis (0|1|2), axis_name, name, value, kind: literal|expression|reference|implicit, expr}]}], anchors:
  [{name, point, dir, line}]}], functions: [{name, params, file, line, body}], variables:
  [{name, expr, value, text, literal (range|null), file, line}], grids, anchors: [{ref, point, dir}],
  instances: [{name, module, line, part (part name|null)}], parts: [{name, module, bounds, volume}], counts }`
- `eval`: `+ { scene }` – complete scene as in the protocol (`web/src/engine/protocol.ts`,
  type `Scene`): `nodes[]`, `grids[]`, `anchors[]` (`text` = `k.ecke`), `hints[]`, `handles[]`,
  `modules[]`, `echoes[]`, `context.params`, `stats`.
- `inspect`: `+ { line, nodes: [{ idx, label, instance, range, status, dim, flags, subtracted,
  world_bounds, volume, area, selected_edges (polylines, world), selected_edge_count, opened_faces,
  hints, messages, cached }], hints,
  handles, echoes }`
- `render`: `+ { output, width, height, view, views: [{name, x, y, width, height, camera}],
  camera: {projection, fov_deg, azimuth, elevation, dir, up, right, target, eye,
  scale_px_per_mm, width_mm, height_mm}, lines, highlighted, visible_nodes: [{idx, label,
  instance, role, line}], triangles, show, overlays: {grids, grid_points, anchors, handles,
  labels}, section: {point, normal, keeps, method}|null, xray, bounds, notes, timings }`
- `measure`: `+ { between?: {a: {ref, kind, point}, b, delta, distance}, bounds?: {target,
  bounds}, volume?: {target, volume}, area?: {target, area}, interference?: {parts, pairs,
  collisions: [{a, b, volume}]}, clearance?: {a, b, distance, point_a, point_b, collision,
  accuracy}, measurements?: [{kind: distance|radius|angle|length|area|body|wall, title, value,
  unit, label, values: [{key, label, value, unit}], lines, guides, points, at, details,
  picks: [{kind, reference, label, point, origin}], exact, tol?, section?, text, cli}],
  section?: {plane, exact, max_dev, area, regions: [{area, holes, bounds, reference,
  contours: [{hole, area, segments}]}], circles: [{center, radius, diameter, full}],
  min_wall}, at?: [{point, elements: [{kind, reference, label, point, distance, origin}]}] }`
  – values rounded to 12 digits; `values` e.g. `distance, dx, dy, dz, min_distance,
  axis_distance` · `radius, diameter, sweep, semi_angle, major` · `angle, opening, normals` ·
  `volume, area, size_x/y/z` · `wall`
- `watch` (one line per run): `{ schema_version, command, run, trigger: start|change,
  changed, ok, errors, warnings, diagnostics, stats: {nodes, cache_hits, kernel_calls}, volume,
  bounds, parts: [{name, volume, bounds}], render, timings: {total_ms, eval_ms, kernel_ms,
  render_ms} }`
- `set`: `+ { changes: [{kind, spec, file, line, col, old, new}], dry_run, written, refused,
  errors_before, errors_after, diff }`
- `export`: `+ { output, format, bytes, triangles, volume, area, bounds, step?, multicolor,
  color_group?, notes?, colors?, color_regions? }` (`--per-color`: `format: "zip"`, `files`)
- `outline`/`measure --colors`: `colors: [{key, label, source: default|palette|random|exact,
  filament, explicit, lines, places, volume, mass_g}]` (`places`: `Z.3` or `lib.mcad:3`), `color_regions`/`colors.parts: [{part, node,
  total, warning, regions: [{color, label, filament, volume, mass_g}]}]`
- `daemon status`: `{ running, pid, version, build_id, uptime_s, idle_s, idle_limit_s, watchdog_s,
  busy, current, pending, requests, failed, projects, max_projects, project_evictions, cache:
  {entries, nodes, meshes, bytes, limit_bytes, max_entries, evictions, hits, misses, hit_rate},
  rss_bytes, rss_start_bytes, rss_limit_bytes, rss_clears, socket, log }` (`running: false` without daemon)

## Examples

```sh
mcad outline tests/models/projekt-gehaeuse/gehaeuse.mcad
mcad render tests/models/spec/gehaeuse.mcad -o /tmp/g.png --module gehaeuse --views all --show grids,anchors,labels
mcad render tests/models/spec/gehaeuse.mcad -o /tmp/s.png --section z=10 --show dims
mcad render tests/models/spec/fillet_zonen.mcad -o /tmp/f.png --line 12 --azimuth 120 --elevation 30
mcad render tests/models/projekt-gehaeuse/gehaeuse.mcad -o /tmp/d.png --part deckel --xray
mcad measure tests/models/spec/gehaeuse.mcad --between k.ecke k.oben --volume part:k
mcad measure tests/models/projekt-gehaeuse/gehaeuse.mcad --interference --clearance gehaeuse deckel
mcad measure tests/models/spec/lochplatte.mcad --at "[8,6,2]"
mcad measure tests/models/spec/lochplatte.mcad --radius "face@[8,6,2]" --distance "center@[6,6,0]" "center@[18,6,0]" --json
mcad measure tests/models/spec/gehaeuse.mcad --section z=10 --wall "[0,15,10]" --distance "edge@[3,15,10]" "edge@[57,15,10]"
mcad measure tests/models/spec/fase_chamfer.mcad --angle "face@[39.5,12,9.5]" "face@[20,12,10]"
mcad set tests/models/spec/gehaeuse.mcad --param gehaeuse.w=50 --dry-run
mcad watch teil.mcad --json --render /tmp/teil.png --views all --show grids,anchors &
```

## Language: differences from OpenSCAD

Lengths in mm, angles in degrees. Geometry is not a value; variables hold numbers, vectors,
lists. B-Rep kernel: exact cylinders/spheres, `$fn` only affects display.

**Primitives between points:** `box(a, b)`, `cylinder(a, b, r)`, `rect(a, b)`,
`slot(p1, p2, r)` (2D slot, exact: two semicircles + two lines, `d = …` instead of `r`) besides
`cube`, `cylinder(h, r)`, `sphere(r)`, `torus(R, r)`.

**Grids** (local to the module, planes per axis, access `g[x, y, z]`, `_` for a missing
axis, `*` for line/plane, `g.name` as number or plane):

```
module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }
    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }
    anchor ecke = g[rechts, hinten, deckel];
    anchor oben = g[links, vorne, deckel], dir = [0, 0, 1];
}
```

Lists create indexed planes: `x: loch = [for (i = [0:3]) 6 + i * 12];` → `g[loch[2], …]`.
Transformations before `grid` place the grid: `translate(g[rechts, vorne, boden]) grid l { … }`.

**Anchors and named instances:** `k: gehaeuse(w = 60);` names the instance; `k.ecke` is a
point in the current coordinate system, `dir(k.oben)` the direction, `rotate(to = v)` rotates z onto `v`. In
loops with an index: `for (i = [0:3]) k[i]: halter();` → `k[2].oben` (integer index).
Named instances at top level of the main file are parts (export names):

```
k: gehaeuse(w = 60);
translate(k.ecke) cube([10, 10, 2]);
translate(k.oben) rotate(to = dir(k.oben)) cylinder(h = 5, r = 1);
```

**Fillet/chamfer** – three forms:

```
difference(fillet = 1) { cube(20); translate([10, 10, -1]) cylinder(h = 22, r = 4); } // new edges
cube(20, $fillet = 1);                                   // own edges of each primitive
fillet(2) {                                              // zone block: 1st child body, rest zones
    box(g[links, vorne, boden], g[rechts, hinten, deckel]);
    zone(g[rechts, *, deckel], d = 2);                   // cylinder around a grid line
    zone(g.boden, d = 1);                                // slab around a grid plane
    translate([15, 10, 15]) sphere(6);                   // any volume as a zone
}
fillet(1.5, select = "touch") { … }                      // also touching edges
fillet(r = function (p) p.z < 10 ? 2 : 4) { … }          // variable radius (position p)
difference(fillet = function (p) 1 + p.x / 40) { … }     // also on Booleans
```

Variable radius: the function is evaluated at sample points of each edge (`p` in the coordinate system
of the statement); hint „r 2–4“. Only pure expressions over `p` (arithmetic, `?:`, `let`,
maths builtins, `lookup`, captured variables, user functions); not for `chamfer`
and `$fillet`. Seam and pole edges (cylinder, sphere) and tangential edges never count.

Edges count if they lie entirely within a zone (`select = "inside"`). An empty selection is
a warning („0 Kanten“), a radius that is too large is a kernel error (the body stays without
fillet). `chamfer` and `shell(t) { körper; zone; }` follow the same pattern.
`inspect --line` of the block shows the edge count, `render --line` the zones (blue) and the
edges (orange).

**Colours and filaments** (report 11, fixes in report 12): `color("name")` takes the
palette (16 muted colours, case-insensitive): `red, orange, yellow, lime, green, teal, cyan,
blue, purple, magenta, pink, brown, black, gray (grey), silver, white`; `aqua` = cyan,
`fuchsia` = magenta. Other names (also CSS ones like `lightblue`, German ones like `rot`) give a
warning and a stable random colour (at a distance from the palette and from the other random colours
of the model). `#rgb`, `#rrggbb`, `#rrggbbaa`, `[r, g, b(, a)]` (values 0–1; anything else is
clamped with a warning) stay exact; `alpha` only affects the view (`red` with alpha is the
entry `red`); `transparent` and `""` do not colour (warning).
Slot: `color("teal", filament = 2)` (named) on a `color()` with a fixed colour value; if
the colour comes from a parameter or a loop (`module bar(c) color(c) …`), use `filament("red", 4);` at top
level of the main file – this applies to the whole model and takes precedence over
`filament = n`. Otherwise slots in order of first appearance, material without `color()`
always slot 1; above 16 slots a warning is given, shared slots are shown by a hint. Volume per colour: the
body of a coloured subtree belongs to that colour, on overlap the later
statement wins (also an inner `color()`), subtracted material applies to all; `color()` in a 2D profile
colours the extrusion (mixed 2D colours: warning). Check with `mcad outline` (section
„Farben“ (colours), lines of other files as `lib.mcad:3`) or `mcad measure --colors`, view with
`mcad render` (a model with colours shows the colour regions as printed; `--show faces` the
faster colour per face).

```
color("white") cube([40, 20, 2]);                              // plate: slot 1
color("red", filament = 3) translate([5, 5, 2]) cube([30, 10, 1]);  // lettering: slot 3
module knopf(c) color(c) cylinder(h = 2, r = 3);
filament("teal", 4);                                           // slot of a parameter colour
translate([10, 10, 3]) knopf("teal");
```

**Modifiers:** `#` additionally red semi-transparent, `%` helper geometry (grey, not in the
result), `!` only this subtree, `*` ignored.

**Sketches** (plan 17, report 14): `sketch { … }` is a 2D profile of segments, each
line a segment with its own error message (line:column in `mcad check`):

```
linear_extrude(5) sketch {
    grid s { x: l = 0, r = 40; y: u = 0, o = 20; }  // grid allowed in the sketch
    start(s[l, u]);                       // new contour
    line(to = s[r, u]);                   // also: by = [dx, dy], length = l, angle = a
    arc(to = [40, 20], r = 10);           // + large = true / cw = true (as in SVG)
    arc(center = [20, 20], angle = 90);   // degrees, negative = clockwise; or center + to
    arc(through = [5, 30], to = [0, 20]); // through a point
    arc(to = [0, 10]);                    // without r: tangent to the previous segment
    bezier(c1 = [-5, 5], to = [0, 0]);    // cubic: c1, c2, to
    // spline([[…], …], tangent = true, end_tangent = [1, 0]);
    close();                              // line to the start (unnecessary if the contour ends there)
    start([10, 5]); …                     // further contour: hole by winding rule (opposite direction)
    hole { start(…); …; }                 // hole in either direction
}
module profil(b = 40) sketch { … }        // named sketch; for/if/let and modules with segments
sweep() { sketch(closed = false) { start([0, 0]); line(to = [30, 0]); arc(to = [45, 15]); } circle(2); }
```

Errors: „Kontur nicht geschlossen“ (contour not closed), „unmöglicher Bogen“ (impossible arc) (radius < half the chord, …),
„Selbstüberschneidung … mit Segment in Zeile n“ (self-intersection … with segment in line n), „Kontur ohne Fläche“ (contour without area). `loft` connects
sketches with the same number of segments segment by segment (start = `start`), otherwise as one curve each.
In `mcad render --line N` of a segment line the sketch is highlighted.

**Corners, offset, stroke** (plan 17.3, report 15) – everything exact from lines and arcs (STEP:
`CIRCLE`), B-splines as biarcs (1 µm):

```
sketch {
    start([0, 0]);
    corner(r = 2);                 // directly after start: the start corner
    line(to = [30, 0]);
    corner(r = 4);                 // between two segments: rounds this corner
    line(to = [30, 20]);
    corner(chamfer = 3);           // chamfer: intersection points at distance 3 from the vertex
    line(to = [0, 20]);
    corner(r = 1);                 // before close(): corner to the closing line
    close();
}
fillet2d(2) square([30, 20]);                       // all corners, convex and concave
fillet2d(5) { profil(); translate([6, 6]) circle(1); }   // only corners in 2D zones (select = "touch")
chamfer2d(1) polygon(…);
offset(r = 2) …;  offset(-1) …;  offset(delta = 1, chamfer = true) …;   // as in OpenSCAD
stroke(2, cap = "round" | "flat" | "square", join = "round" | "miter" | "bevel")
    sketch(closed = false) { … };  // open path → area; stroke(1) circle(5) = ring
```

`corner` does not fit → error at the corner line with „höchstens ≈ r“ (at most ≈ r); tangential corner →
„keine Ecke“ (no corner). `fillet2d` reports „n Ecken“ (n corners) (inline hint), radii that are too large per corner
(„Ecke bei [x, y]: … passt nicht“ (corner at [x, y]: … does not fit)). `offset` without argument: `r = 1`; `r` takes precedence over `delta`;
negative values shrink (narrow parts disappear, holes grow), positive values close
holes. `mcad render --line N` on offset/stroke/fillet2d shows the source contour, the
offset contour dashed, the centre line of the stroke and the new corner arcs; `--show handles`
shows the radius handle of `corner`.

**`hull` and `minkowski`** (plan 21): only exact special cases, otherwise the warning
„hull: <Grund>; Kinder werden unverändert gezeigt“ (hull: <reason>; children are shown unchanged) (`mcad usage language` has the table).
`hull` in 2D always (lines + arcs); in 3D: only planar bodies (polyhedra), spheres/cylinders/cones
on one axis (solids of revolution), parallel prisms of equal height (cylinder + box →
plate with round ends), ≥ 3 equally sized spheres not in one plane (rounded box).
`minkowski`: body + `sphere(r)` (offset), 2D profile + `circle(r)` (`offset(r)`),
`linear_extrude` + `cylinder`, two convex planar bodies. `mcad inspect --line N` shows the
case („exakt: Prisma“ (exact: prism)). Offset is only exact at right-angled corners (tetrahedron → warning).

```
hull() for (x = [0, 20], y = [0, 10], z = [0, 5]) translate([x, y, z]) sphere(2);   // rounded box
linear_extrude(3) hull() { circle(3); translate([20, 0]) circle(3); }                // slotted plate
minkowski() { cube([20, 10, 5]); sphere(2); }
```

**Modules as namespaces:** `module teile { M3 = 3.2; function kopf(d) = 1.8 * d;
module schraube(d = 3) { … } }` (braces may be omitted) → `teile.schraube(d = 4);`,
`k: teile.schraube();`, `a.b.c();`, `teile.kopf(teile.M3)`. The inner module sees the parameters
of the outer one with default values, not its grids/instances. `mcad --module teile.schraube`,
`mcad set --param teile.schraube.d=4`; `mcad outline` shows modules and functions with their path.

**Text and SVG** (plan 17.4/17.5, report 16):

```
linear_extrude(1.2) text("Werkstatt", size = 11, font = "Inter:style=Bold", halign = "center", valign = "center");
linear_extrude(1) text("Maß 20 mm", font = "fonts/meine.ttf");   // font file relative to the model file
linear_extrude(2) import("logo.svg", center = true);             // also id = "…", scale = …, dpi = 96
```

- `text()` as in OpenSCAD (`size` ≈ cap height, em = size/0.72; tracking/alignment the same as
  OpenSCAD 2021.01; `spacing`, `direction` ltr/rtl/ttb/btt, `language`, `script`).
  Default: `"cad stencil mono"` (own monospaced stencil font, Regular/Bold, no
  holes – inner spaces connected via bridges; `stroke_width`/`bridge_width` in mm, default
  0.14·size and 0.1·size, limits 0.2·size and 0.03…0.2·size, `bridge_width = 0` without
  bridges). Bundled: JetBrains Mono (Regular/Bold), Inter (Regular/Bold), Allerta Stencil.
  For dimensions identical to OpenSCAD specify a TTF font explicitly (`font = "Inter"`). Unknown
  font → warning + cad stencil mono; missing characters (e.g. `°` in Allerta Stencil) → warning, character
  from JetBrains Mono. Umlauts, ß/ẞ and common punctuation are in all bundled fonts
  (exceptions in Allerta Stencil: ẞ„‚{}_#°²³§~^·•…±µØø).
- `import("….svg")`: paths, rectangles (rx/ry), circles/ellipses exact, polygons, groups with
  `transform`, `viewBox`/mm/in/pt, `fill-rule`. Origin bottom left (y up) as in OpenSCAD.
  Warnings name the SVG line (`logo.svg:4: \`text\`: Text nicht unterstützt …`): convert text to
  paths; strokes as areas are not supported, open paths are not filled.
- File names must appear as text in the project (also as a parameter default value); `mcad watch`
  also watches the SVG/TTF files.
- Kernel time: "Hello World" extruded ≈ 0.7 s; a lot of text **on** a plate (union)
  costs seconds (28 glyphs ≈ 9 s) – text as a separate part or fewer characters, use
  `--no-kernel` while experimenting.

**STL import** (plan 21 B, `mcad usage import-stl`): `import("teil.stl", center, scale, as)`.
Closed mesh with ≤ 500 faces (after merging coplanar triangles) → B-Rep
(Booleans, fillets, exact measuring, STEP); larger, open or non-manifold → pure
**mesh** (display, transformations, colours, parts, `hull` up to 2000 vertices, export STL/3MF,
STEP faceted, `measure` bounding box/volume/section/`--clearance`/`--interference` mesh-based;
Booleans and fillets report an error with the hint `as = "solid"`). Hint at the node
(`mcad inspect --line N`): „B-Rep: 6 Flächen aus 12 Dreiecken“ (B-Rep: 6 faces from 12 triangles) or „Netz: 20000 Dreiecke, …“ (mesh: 20000 triangles, …).
`as = "solid"` forces the B-Rep (slow: 900 faces + one Boolean ≈ 4 s), `as = "mesh"` the
mesh. Example: `tests/models/spec/stl_import.mcad`.

**Collision asserts** (plan 21 C, `mcad usage asserts`): checks after evaluation,
violation = warning (`mcad check` exit 0, line „Asserts: 4 von 4 geprüft, 0 verletzt“ (asserts: 4 of 4 checked, 0 violated)),
`mcad export` then aborts with **exit 4** (`--ignore-asserts` exports anyway).
`assert_keepout(clearance = 0.5) …` – keep-out volume (B-Rep or STL mesh): shown (yellow,
translucent), never exported/counted, checked against all real bodies (intersection volume exact
via Boolean, mesh only yes/no; distance < clearance). `assert_disjoint() { a; b; }` (no pair
intersects), `assert_intersect() { … }` (children form **one connected** group;
touching does not count), `assert_touches(tol = 0.001, contact = "face") { … }` (gap → „berührt
nicht“ (does not touch), intersection, only line/point contact → warning; contact face exact from `region2`).
Asserts are groups and can be nested; named instances inside them remain parts.
`mcad render` draws intersection bodies red, contact faces green, gaps as a dimension line
(`--show asserts` labels everything); `mcad inspect --line N` lists the pairs.
Example: `tests/models/spec/asserts.mcad`.

**Profiles:** 2D shapes (`square`, `rect`, `circle`, `polygon`, `spline`) in `linear_extrude`,
`rotate_extrude`, `loft`, `sweep`; free-form surfaces with `surface(points)` + `thicken(t)`.
