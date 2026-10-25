# Commands

| Command | Output |
| --- | --- |
| `mcad check FILE [--no-kernel]` | `file:line:col: Fehler/Warnung (Syntax/Auswertung/Kernel): …`, assert results (`Asserts: 4 von 4 geprüft, 0 verletzt`) |
| `mcad outline FILE [--no-kernel] [-D …]` | modules + params, grids, anchors, functions, variables, instances, parts (bounds, volume), colours, files |
| `mcad eval FILE` | node tree (index, line, status, bounds), expression values, grids, anchors, `echo` |
| `mcad inspect FILE --line N [--whole-file]` | nodes of line N: bounds, volume/area, selected edges, hints, errors, handles |
| `mcad render FILE -o x.png …` | image (`mcad usage render`) |
| `mcad measure FILE …` | measurements (`mcad usage measure`) |
| `mcad watch FILE …` | re-evaluates on change (`mcad usage watch`) |
| `mcad set FILE …` | edits code (`mcad usage set`) |
| `mcad export FILE -o … [--ignore-asserts]` | STL/3MF/STEP (`mcad usage export`); stops with exit 4 on a violated assert (`mcad usage asserts`) |
| `mcad ui status\|show\|screenshot` | running desktop UI (`mcad usage ui`) |
| `mcad daemon status\|stop\|serve` | cache process (`mcad usage daemon`) |
| `mcad usage [TOPIC…] [--all\|--list]` | this manual |
| `mcad diff-kernels FILE…` | SMLib ↔ solidcore (development, `--features solidcore`) |

## Common options

| Option | Effect |
| --- | --- |
| `-m, --module NAME` | module as context: shown alone with defaults, its grids/anchors visible; nested `outer.inner` |
| `-m 'NAME(args)'` | same, with arguments as in a call: positional then named, expressions see the file's variables and functions (`-m 'holder(30, h = 5)'`, `-m 'parts.screw(d = 3)'`); combines with `-D` for other parameters. Exit 2: unknown module/parameter, too many positional arguments, a parameter twice or also in `-D`, an argument that fails or warns |
| `-D NAME=VALUE` | with a module context: override a parameter of that module; without: override a top-level assignment of the main file (like `openscad -D`: the last assignment of the name takes the value, variables computed from it follow; `$fn` etc. need no assignment; unknown name → warning). JSON value (`-D w=60`, `-D p=[1,2]`), else string. Repeatable, the last `-D` of a name wins |
| `--no-daemon`, `--verbose` | compute locally / daemon info on stderr |

```sh
mcad measure teil.mcad -D w=40 --volume all   # teil.mcad: w = 20; d = w / 2; h = 8; cube([w, d, h]);
#   Volumen all: 6400 mm³                     (40 × 20 × 8: d follows w)
```

```sh
mcad export holder.mcad -m 'holder(width, h = twice(2))' -o h.stl   # width = 30; function twice(x) = 2 * x;
#   h.stl: 12 Dreiecke, 684 Bytes, Volumen 600 mm³                   (module holder(w = 10, h = 2, d = 5) cube([w, d, h]);)
mcad check holder.mcad -m 'holder(1, 2, 3, 4)'
#   mcad check: -m 'holder(1, 2, 3, 4)': zu viele Positionsargumente – `holder` hat 3 Parameter (w, h, d)   (exit 2)
```

`--line N` inside a module definition makes that module the context (`--whole-file` keeps the
file context).

**Targets** (`--fit`, `--bounds`, `--volume`, `--area`): `all`, `part:NAME` or `NAME`,
`line:N`, `node:I` (index from `eval`).

**Points**: `[x,y,z]`, anchor `body.corner` / `k[2].top` (in module context `corner`), grid
point of the context `g[right, back, top]` (entries may be numbers or `_`). Quote them.

`include <…>`/`use <…>` resolve relative to the including file; diagnostics name the file.
