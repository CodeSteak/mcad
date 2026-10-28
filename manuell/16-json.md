# JSON output (`--json`)

Every answer: `{ schema_version: 3, command, file, ok, exit_code, context: string|null,
diagnostics: Diagnostic[], error?: string, daemon?: {used, reason} }` plus per command:

| Command | Additional fields |
| --- | --- |
| `check` | `errors, warnings, violations, asserts: AssertResult[]` |
| `outline` | `render: RenderBlock\|null, files, includes, modules: [{name, qualified, file, line, end_line, params: [{name, default, default_value, type}], grids: [{name, file, line, planes: [{axis: 0\|1\|2, axis_name: "x"\|"y"\|"z", name, value, kind, expr}]}], anchors}], functions, variables: [{name, expr, value, text, literal: range\|null, file, line}], grids, anchors: [{ref, point, dir}], instances: [{name, module, line, part: string\|null}], parts: [{name, module, bounds, volume}], counts, colors, color_regions, asserts: [{node, kind, line, file, checked, ok, violations: [message], bodies, note}]` |
| `eval` | `scene` (nodes, grids, anchors, hints, handles, modules, echoes, context.params, stats) |
| `inspect` | `line, nodes: [{idx, label, instance, range, status, dim, flags, subtracted, world_bounds, volume, area, selected_edges: [[x,y,z]…][] (world), selected_edge_count, opened_faces, hints, messages, cached, assert: AssertResult\|null}], hints, handles, echoes` |
| `render` | `output, width, height, view, views: [{name, x, y, width, height, camera, studio}], camera, visible_nodes, triangles, overlays, section, theme, style, studio, background, bounds, notes, timings` |
| `measure` | `asserts?` (as `outline`), `between?, bounds?, volume?, area?, interference?, clearance?, measurements?: [{kind, title, value, unit, values: [{key, value, unit}], picks: [{kind, reference, point, origin}], exact, details, text, cli}], section?, at?` |
| `watch` | one object per line: `run, trigger, changed, ok, errors, warnings, diagnostics, stats, volume, bounds, parts, render, timings` |
| `set` | `changes: [{kind, spec, file, line, col, old, new}], dry_run, written, refused, errors_before, errors_after, diff` |
| `export` | `output, format, bytes, triangles, volume, area, bounds, step?: {solids, sheets, voids, faces, edges, bodies: [label], surfaces, curves, …}, notes, multicolor, colors?, color_regions?` (`--part/--all-parts`: `parts`), `asserts, violations, ignored_asserts, aborted` (violation: `aborted: true`, exit 4, no output fields) |
| `daemon status` | `running, pid, version, build_id, uptime_s, idle_s, requests, cache: {entries, bytes, limit_bytes, hit_rate, …}, rss_bytes, socket, log` |

Types:

- `Diagnostic = { severity: "error"|"warning"|"info", message, source: "syntax"|"eval"|"kernel", node, range, file?, at?: [x,y,z], nodes?: [node] }` (`at`: place of a kernel error in context coordinates, e.g. where Boolean operands touch tangentially; `nodes`: the touching operands; scene node messages carry `at` in the node system and the contact edges as `edges`)
- `range = { from, to, line, col, end_line, end_col }` (lines/cols 1-based, offsets UTF-16)
- `Bounds = { min: [x,y,z], max, size }`
- `AssertResult = { node, kind: keepout|disjoint|intersect|touches, range, ok, checked, note, clearance, tol, contact: face|any, eps_volume, bodies, pairs: AssertPair[], ms, cached }` (keepout: only pairs with a problem)
- `AssertPair = { a: {node, label, origin}, b, ok, problem: overlap|too_close|apart|gap|line_contact|point_contact|unchecked|null, message, volume, mesh_based, distance, accuracy, points: [a, b], contact: face|line|point|mesh|null, contact_area, contact_exact, overlap: {positions, indices, edges}|null, contact_mesh, contact_lines, focus: {min, max} }`
- `camera = { projection, azimuth, elevation, dir, up, right, target, eye, scale_px_per_mm, width_mm, height_mm }`
- `RenderBlock = { range, style, edges, environment, ambient, light_intensity, shadows, shadow_softness, ao, ao_radius, ao_strength, ground, ground_color: "#rrggbb", background: ["#rrggbb"…], lights: [{name, azimuth, elevation, intensity, color, range}], materials: [{target: {color}|{part}, preset, metalness, roughness, clearcoat, range}] }` (`null` = not set)
- `render` `studio = { lights: [{azimuth, elevation, intensity, color}], light_azimuth, light_elevation (first light), light_intensity, ambient, environment: studio|overcast|none, shadows, shadow_softness, ao, ao_radius, ao_strength, ground, ground_color, edges, materials: [{target: "#rrggbb"|"default"|"part:NAME", metalness, roughness, clearcoat}], render_block: {line}|null }`; `views[].studio = { light_dir, light_dirs, ground_z, ao_radius_mm, shadow_map }`

Measurement values are rounded to 12 significant digits. Display texts (`message`, `text`, `title`,
`label`, `details`, `origin`, `notes`, hints) follow the output language (`--lang`, `MCAD_LANG`,
locale; default English); keys and machine values (`severity`, `source`, `kind`, `key`, `problem`,
`cli`, `reference`, colour `label` `default`, part `no part`) never change.

Fixed machine values (English in every language):

| Field | Values |
| --- | --- |
| `measure` `between.a/b.kind` | `anchor`, `grid_point`, `coordinate` |
| `measure` `section.regions[].contours[].segments[].source` | cut surface: `plane`, `cylinder`, `cone`, `sphere`, `torus`, `revolution`, `extrusion`, `bspline` |
| `colors[].label`, `color_regions[].regions[].label` | palette name, name as written, hex, or `default` (no `color()`, `source: "default"`) |
| `colors[].source` | `default`, `palette`, `random`, `exact` |
| `color_regions[].part`, `colors.parts[].part` | part name, `no part` (material outside the parts; also the 3MF object name), `null` (no parts) |

## Conventions (schema 3)

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

Schema 3 (from 2): machine values that were German are English: `between` `kind`
`Koordinate`/`Rasterpunkt`/`Anker` → `coordinate`/`grid_point`/`anchor`; section segment
`source` (was the translated surface name, e.g. `Ebene`) → fixed id; colour label `Standard` →
`default`; part `ohne Bauteil` → `no part` (also 3MF object names, `F1_default.stl`). The UI
protocol (`PROTOCOL_VERSION` 3) changed the same way.

Schema 2 (from 1): `inspect` `selected_edges` list instead of count (+ `selected_edge_count`);
`outline` plane `axis` number (+ `axis_name`), `variables[].literal` range instead of bool,
`instances[].part` part name instead of bool; diagnostics may carry `at`/`nodes`; render
`overlays.failures`; export `step.bodies`, `notes`.
