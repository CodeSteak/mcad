# mcad – usage manual

Parametric CAD in an OpenSCAD-style language (files `*.mcad`) with grids, anchors, named
parts, fillet zones and sketches. Exact B-rep kernel: measurements and STEP are exact, `$fn` only
affects display. Units mm and degrees, Z up.

Headless commands are all an agent needs; `mcad`/`mcad FILE` without a command opens the
desktop UI for the human.

```sh
mcad outline part.mcad                     # modules, params, grids, anchors, parts
mcad check part.mcad                       # diagnostics; fix until 0 errors
mcad render part.mcad -o /tmp/p.png --views all --show grids,anchors   # then view the image
mcad measure part.mcad --at "[30,0,10]"    # find element references, then measure them
mcad export part.mcad -o part.step         # .stl / .3mf / .step
```

- `--json` on every command: one object, fixed schema (`mcad usage json`). Prefer it.
- Text output is German (glossary: `mcad usage troubleshooting`); JSON keys are English.
- `FILE` `-` reads stdin. Lines/columns are 1-based.
- Exit codes: 0 ok · 1 diagnostics with errors (output complete) · 2 usage error (arguments,
  file, unknown anchor/part/plane) · 3 kernel/internal error, or no running UI for `mcad ui` ·
  4 `mcad export` stopped by a violated assert (`mcad usage asserts`, `--ignore-asserts`).

Model files: `.mcad` for new files (UI "New", "Save as", examples). `.scad` files
(OpenSCAD, older projects) open, evaluate, export and work in `include <…>`/`use <…>` the same
way, also mixed in one project. The program was called `cad` before 0.3.0; old environment
variables `CAD_*` still work (`mcad usage daemon`).

Tutorial models: `examples/01-plate.mcad` … `08-macropad.mcad` in the repository (installed:
`/usr/share/doc/mcad/examples`), each a step more complex; test models: `tests/models/`.

Topics: `mcad usage --list`; all pages: `mcad usage --all`; prefixes work (`mcad usage meas`).
