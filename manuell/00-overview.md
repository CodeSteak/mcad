# cad – usage manual

Parametric CAD in an OpenSCAD-style language (`*.scad`) with grids, anchors, named parts,
fillet zones and sketches. Exact B-rep kernel: measurements and STEP are exact, `$fn` only
affects display. Units mm and degrees, Z up.

Headless commands are all an agent needs; `cad`/`cad FILE` without a command opens the
desktop UI for the human.

```sh
cad outline part.scad                     # modules, params, grids, anchors, parts
cad check part.scad                       # diagnostics; fix until 0 errors
cad render part.scad -o /tmp/p.png --views all --show grids,anchors   # then view the image
cad measure part.scad --at "[30,0,10]"    # find element references, then measure them
cad export part.scad -o part.step         # .stl / .3mf / .step
```

- `--json` on every command: one object, fixed schema (`cad usage json`). Prefer it.
- Text output is German (glossary: `cad usage troubleshooting`); JSON keys are English.
- `FILE` `-` reads stdin. Lines/columns are 1-based.
- Exit codes: 0 ok · 1 diagnostics with errors (output complete) · 2 usage error (arguments,
  file, unknown anchor/part/plane) · 3 kernel/internal error, or no running UI for `cad ui`.

Topics: `cad usage --list`; all pages: `cad usage --all`; prefixes work (`cad usage meas`).
