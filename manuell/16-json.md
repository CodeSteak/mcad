# JSON output (`--json`)

Every answer: `{ schema_version: 2, command, file, ok, exit_code, context: string|null,
diagnostics: Diagnostic[], error?: string, daemon?: {used, reason} }` plus per command:

| Command | Additional fields |
| --- | --- |
| `check` | `errors, warnings, violations, asserts: AssertResult[]` |
| `outline` | `files, includes, modules: [{name, qualified, file, line, end_line, params: [{name, default, default_value, type}], grids: [{name, file, line, planes: [{axis: 0\|1\|2, axis_name: "x"\|"y"\|"z", name, value, kind, expr}]}], anchors}], functions, variables: [{name, expr, value, text, literal: range\|null, file, line}], grids, anchors: [{ref, point, dir}], instances: [{name, module, line, part: string\|null}], parts: [{name, module, bounds, volume}], counts, colors, color_regions, asserts: [{node, kind, line, file, checked, ok, violations: [message], bodies, note}]` |
| `eval` | `scene` (nodes, grids, anchors, hints, handles, modules, echoes, context.params, stats) |
| `inspect` | `line, nodes: [{idx, label, instance, range, status, dim, flags, subtracted, world_bounds, volume, area, selected_edges: [[x,y,z]…][] (world), selected_edge_count, opened_faces, hints, messages, cached, assert: AssertResult\|null}], hints, handles, echoes` |
| `render` | `output, width, height, view, views: [{name, x, y, width, height, camera}], camera, visible_nodes, triangles, overlays, section, bounds, notes, timings` |
| `measure` | `asserts?` (as `outline`), `between?, bounds?, volume?, area?, interference?, clearance?, measurements?: [{kind, title, value, unit, values: [{key, value, unit}], picks: [{kind, reference, point, origin}], exact, details, text, cli}], section?, at?` |
| `watch` | one object per line: `run, trigger, changed, ok, errors, warnings, diagnostics, stats, volume, bounds, parts, render, timings` |
| `set` | `changes: [{kind, spec, file, line, col, old, new}], dry_run, written, refused, errors_before, errors_after, diff` |
| `export` | `output, format, bytes, triangles, volume, area, bounds, step?: {solids, sheets, voids, faces, edges, bodies: [label], surfaces, curves, …}, notes, multicolor, colors?, color_regions?` (`--part/--all-parts`: `parts`), `asserts, violations, ignored_asserts, aborted` (violation: `aborted: true`, exit 4, no output fields) |
| `daemon status` | `running, pid, version, build_id, uptime_s, idle_s, requests, cache: {entries, bytes, limit_bytes, hit_rate, …}, rss_bytes, socket, log` |

Types:

- `Diagnostic = { severity: "error"|"warning"|"info", message, source: "syntax"|"eval"|"kernel", node, range, file? }`
- `range = { from, to, line, col, end_line, end_col }` (lines/cols 1-based, offsets UTF-16)
- `Bounds = { min: [x,y,z], max, size }`
- `AssertResult = { node, kind: keepout|disjoint|intersect|touches, range, ok, checked, note, clearance, tol, contact: face|any, eps_volume, bodies, pairs: AssertPair[], ms, cached }` (keepout: only pairs with a problem)
- `AssertPair = { a: {node, label, origin}, b, ok, problem: overlap|too_close|apart|gap|line_contact|point_contact|unchecked|null, message, volume, mesh_based, distance, accuracy, points: [a, b], contact: face|line|point|mesh|null, contact_area, contact_exact, overlap: {positions, indices, edges}|null, contact_mesh, contact_lines, focus: {min, max} }`
- `camera = { projection, azimuth, elevation, dir, up, right, target, eye, scale_px_per_mm, width_mm, height_mm }`

Measurement values are rounded to 12 significant digits. `message` texts are German.

## Conventions (schema 2)

| Rule | Fields |
| --- | --- |
| Edges are always polylines `[[x,y,z],…][]`, a count has its own `…_count` key | `selected_edges` + `selected_edge_count` (`inspect`: world; `eval` scene, `diff-kernels`: node system) |
| Axis as number `0\|1\|2` (as in the scene), readable name beside it | grid planes `axis`, `axis_name` |
| Source position of a literal: `range\|null` | `variables[].literal`, `hints[].literal`, grid planes `literal` |
| Part name: `string\|null` | `instances[].part`, `color_regions[].part`, `colors.parts[].part` |

Same key, other meaning (by design, the parent object tells): counters in `counts`, `overlays`
and `step` (`modules`, `parts`, `nodes`, `grids`, `lines`, `asserts`, `edges`: numbers);
`measure` sections `volume`/`area`/`bounds`/`colors` are objects with the value inside; assert
pairs `a`/`b` are `{node, label, origin}`, interference pairs `a`/`b` part names; `target` is a
point in `camera`, a target spec in `measure`; `from`/`to` are offsets in `range`, points in
`min_wall`; `context` is the module name at top level, an object in `scene`; `version` is the
program version in `daemon status`, the source version in `scene`.

Schema 2 (from 1): `inspect` `selected_edges` list instead of count (+ `selected_edge_count`);
`outline` plane `axis` number (+ `axis_name`), `variables[].literal` range instead of bool,
`instances[].part` part name instead of bool.
