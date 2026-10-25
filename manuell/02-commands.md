# Commands

| Command | Output |
| --- | --- |
| `cad check FILE [--no-kernel]` | `file:line:col: Fehler/Warnung (Syntax/Auswertung/Kernel): …`, assert results (`Asserts: 4 von 4 geprüft, 0 verletzt`) |
| `cad outline FILE [--no-kernel] [-D …]` | modules + params, grids, anchors, functions, variables, instances, parts (bounds, volume), colours, files |
| `cad eval FILE` | node tree (index, line, status, bounds), expression values, grids, anchors, `echo` |
| `cad inspect FILE --line N [--whole-file]` | nodes of line N: bounds, volume/area, selected edges, hints, errors, handles |
| `cad render FILE -o x.png …` | image (`cad usage render`) |
| `cad measure FILE …` | measurements (`cad usage measure`) |
| `cad watch FILE …` | re-evaluates on change (`cad usage watch`) |
| `cad set FILE …` | edits code (`cad usage set`) |
| `cad export FILE -o … [--ignore-asserts]` | STL/3MF/STEP (`cad usage export`); stops with exit 4 on a violated assert (`cad usage asserts`) |
| `cad ui status\|show\|screenshot` | running desktop UI (`cad usage ui`) |
| `cad daemon status\|stop\|serve` | cache process (`cad usage daemon`) |
| `cad usage [TOPIC…] [--all\|--list]` | this manual |
| `cad diff-kernels FILE…` | SMLib ↔ solidcore (development, `--features solidcore`) |

## Common options

| Option | Effect |
| --- | --- |
| `-m, --module NAME` | module as context: shown alone with defaults, its grids/anchors visible; nested `outer.inner` |
| `-D NAME=VALUE` | with a module context: override a parameter of that module; without: override a top-level assignment of the main file (like `openscad -D`: the last assignment of the name takes the value, variables computed from it follow; `$fn` etc. need no assignment; unknown name → warning). JSON value (`-D w=60`, `-D p=[1,2]`), else string. Repeatable, the last `-D` of a name wins |
| `--no-daemon`, `--verbose` | compute locally / daemon info on stderr |

```sh
cad measure teil.scad -D w=40 --volume all   # teil.scad: w = 20; d = w / 2; h = 8; cube([w, d, h]);
#   Volumen all: 6400 mm³                     (40 × 20 × 8: d follows w)
```

`--line N` inside a module definition makes that module the context (`--whole-file` keeps the
file context).

**Targets** (`--fit`, `--bounds`, `--volume`, `--area`): `all`, `part:NAME` or `NAME`,
`line:N`, `node:I` (index from `eval`).

**Points**: `[x,y,z]`, anchor `body.corner` / `k[2].top` (in module context `corner`), grid
point of the context `g[right, back, top]` (entries may be numbers or `_`). Quote them.

`include <…>`/`use <…>` resolve relative to the including file; diagnostics name the file.
