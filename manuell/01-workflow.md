# Workflow

1. `cad outline FILE` – understand the model (one call per project).
2. Edit the file, or `cad set` for parameter defaults/literals.
3. `cad check FILE` – until 0 errors. Warning "0 Kanten" = a zone selected no edges.
4. `cad render FILE -o x.png --views all --show grids,anchors` – view the image. Module grids
   only show with `--module NAME`.
5. `cad inspect FILE --line N`, `cad measure …` – verify numbers; don't trust a picture.
6. `cad export FILE -o part.step` (or `--all-parts -o dir/`).

Speed: the daemon keeps the kernel cache between calls (only changed nodes recompute).
`--no-kernel` (check, outline) skips geometry. For many edits: `cad watch FILE --json
--render x.png` in the background.

Model style that makes checking easy:

- One `module` per part, geometry between named grid points instead of loose numbers.
- Name top-level instances (`body: enclosure();`) – they become parts (export, measure,
  render by name).
- Attach parts via anchors (`translate(body.rim) …`).
- Fillet via zones and confirm the edge count with `cad inspect --line N`.

If the user has the UI open: `cad ui status --json` first; files in `dirty` have unsaved
changes – ask before writing.
