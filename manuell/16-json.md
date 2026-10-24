# JSON output (`--json`)

Every answer: `{ schema_version: 1, command, file, ok, exit_code, context: string|null,
diagnostics: Diagnostic[], error?: string, daemon?: {used, reason} }` plus per command:

| Command | Additional fields |
| --- | --- |
| `check` | `errors, warnings` |
| `outline` | `files, includes, modules: [{name, qualified, file, line, end_line, params: [{name, default, default_value, type}], grids, anchors}], functions, variables: [{name, expr, value, text, literal, file, line}], grids, anchors: [{ref, point, dir}], instances: [{name, module, line, part}], parts: [{name, module, bounds, volume}], counts, colors, color_regions` |
| `eval` | `scene` (nodes, grids, anchors, hints, handles, modules, echoes, context.params, stats) |
| `inspect` | `line, nodes: [{idx, label, instance, range, status, dim, flags, subtracted, world_bounds, volume, area, selected_edges, opened_faces, hints, messages, cached}], hints, handles, echoes` |
| `render` | `output, width, height, view, views: [{name, x, y, width, height, camera}], camera, visible_nodes, triangles, overlays, section, bounds, notes, timings` |
| `measure` | `between?, bounds?, volume?, area?, interference?, clearance?, measurements?: [{kind, title, value, unit, values: [{key, value, unit}], picks: [{kind, reference, point, origin}], exact, details, text, cli}], section?, at?` |
| `watch` | one object per line: `run, trigger, changed, ok, errors, warnings, diagnostics, stats, volume, bounds, parts, render, timings` |
| `set` | `changes: [{kind, spec, file, line, col, old, new}], dry_run, written, refused, errors_before, errors_after, diff` |
| `export` | `output, format, bytes, triangles, volume, area, bounds, step?, multicolor, colors?, color_regions?` (`--part/--all-parts`: `parts`) |
| `daemon status` | `running, pid, version, build_id, uptime_s, idle_s, requests, cache: {entries, bytes, limit_bytes, hit_rate, …}, rss_bytes, socket, log` |

Types:

- `Diagnostic = { severity: "error"|"warning"|"info", message, source: "syntax"|"eval"|"kernel", node, range, file? }`
- `range = { from, to, line, col, end_line, end_col }` (lines/cols 1-based, offsets UTF-16)
- `Bounds = { min: [x,y,z], max, size }`
- `camera = { projection, azimuth, elevation, dir, up, right, target, eye, scale_px_per_mm, width_mm, height_mm }`

Measurement values are rounded to 12 significant digits. `message` texts are German.
