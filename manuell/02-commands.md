# Commands

| Command | Output |
| --- | --- |
| `cad check FILE [--no-kernel]` | `file:line:col: Fehler/Warnung (Syntax/Auswertung/Kernel): …` |
| `cad outline FILE [--no-kernel]` | modules + params, grids, anchors, functions, variables, instances, parts (bounds, volume), colours, files |
| `cad eval FILE` | node tree (index, line, status, bounds), expression values, grids, anchors, `echo` |
| `cad inspect FILE --line N [--whole-file]` | nodes of line N: bounds, volume/area, selected edges, hints, errors, handles |
| `cad render FILE -o x.png …` | image (`cad usage render`) |
| `cad measure FILE …` | measurements (`cad usage measure`) |
| `cad watch FILE …` | re-evaluates on change (`cad usage watch`) |
| `cad set FILE …` | edits code (`cad usage set`) |
| `cad export FILE -o …` | STL/3MF/STEP (`cad usage export`) |
| `cad ui status\|show\|screenshot` | running desktop UI (`cad usage ui`) |
| `cad daemon status\|stop\|serve` | cache process (`cad usage daemon`) |
| `cad usage [TOPIC…] [--all\|--list]` | this manual |
| `cad diff-kernels FILE…` | SMLib ↔ solidcore (development, `--features solidcore`) |

## Common options

| Option | Effect |
| --- | --- |
| `-m, --module NAME` | module as context: shown alone with defaults, its grids/anchors visible; nested `outer.inner` |
| `-D NAME=VALUE` | override a context-module parameter; JSON value (`-D w=60`, `-D p=[1,2]`), else string. Needs a module context |
| `--no-daemon`, `--verbose` | compute locally / daemon info on stderr |

`--line N` inside a module definition makes that module the context (`--whole-file` keeps the
file context).

**Targets** (`--fit`, `--bounds`, `--volume`, `--area`): `all`, `part:NAME` or `NAME`,
`line:N`, `node:I` (index from `eval`).

**Points**: `[x,y,z]`, anchor `body.corner` / `k[2].top` (in module context `corner`), grid
point of the context `g[right, back, top]` (entries may be numbers or `_`). Quote them.

`include <…>`/`use <…>` resolve relative to the including file; diagnostics name the file.
