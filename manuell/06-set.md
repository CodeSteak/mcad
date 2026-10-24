# cad set

Changes code through the engine and writes atomically.

```sh
cad set box.scad --param enclosure.w=70            # default value of a module parameter
cad set box.scad --literal 2:22=70                 # literal at line:col (col anywhere in it)
cad set box.scad --literal lib/parts.scad:12:20=4  # literal in another project file
```

- `--param module.param=value`: nested `outer.inner.param`; modules from `use` files work.
- `--literal`: only literals the UI can edit (variable values, handles, grid planes,
  defaults).
- Repeat `--param`/`--literal` to change several at once. `--dry-run` prints the diff only.
- A change that introduces new errors is refused (exit 1, `refused: true`) unless `--force`.
