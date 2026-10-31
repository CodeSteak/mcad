# mcad set

Changes code through the engine and writes atomically.

```sh
mcad set box.mcad --param enclosure.w=70            # default value of a module parameter
mcad set box.mcad --literal 2:22=70                 # literal at line:col (col anywhere in it)
mcad set box.mcad --literal lib/parts.mcad:12:20=4  # literal in another project file
mcad set box.mcad --render edges=false --render light.key.azimuth=30 --render material.teal.preset=chrome
```

- `--param module.param=value`: nested `outer.inner.param`; modules from `use` files work.
- `--literal`: only literals the UI can edit (variable values, handles, grid planes,
  defaults).
- `--render`: a value of the render block (`mcad usage render-block`), as the UI's studio panel
  writes it: `key=value`, `light.NAME.key=value`, `material.SEL.key=value` (SEL: colour
  `red`/`#rrggbb`/`default`, or `part:NAME`); empty value removes it, `light.NAME=` /
  `material.SEL=` remove the entry; no block yet: one is added at the end. Bare words of string
  settings are quoted (`preset=chrome`).
- Repeat `--param`/`--literal`/`--render` to change several at once. `--dry-run` prints the diff only.
- A change that introduces new errors is refused (exit 1, `refused: true`) unless `--force`.
