# mcad set

Changes code through the engine and writes atomically.

```sh
mcad set box.mcad --param enclosure.w=70            # default value of a module parameter
mcad set box.mcad --literal 2:22=70                 # literal at line:col (col anywhere in it)
mcad set box.mcad --literal lib/parts.mcad:12:20=4  # literal in another project file
```

- `--param module.param=value`: nested `outer.inner.param`; modules from `use` files work.
- `--literal`: only literals the UI can edit (variable values, handles, grid planes,
  defaults).
- Repeat `--param`/`--literal` to change several at once. `--dry-run` prints the diff only.
- A change that introduces new errors is refused (exit 1, `refused: true`) unless `--force`.
