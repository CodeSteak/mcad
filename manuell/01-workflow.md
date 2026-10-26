# Workflow

1. `mcad outline FILE` – understand the model (one call per project).
2. Edit the file, or `mcad set` for parameter defaults/literals.
3. `mcad check FILE` – until 0 errors. Warning "0 edges" = a zone selected no edges.
4. `mcad render FILE -o x.png --views all --show grids,anchors` – view the image. Module grids
   only show with `--module NAME`.
5. `mcad inspect FILE --line N`, `mcad measure …` – verify numbers; don't trust a picture.
6. `mcad export FILE -o part.step` (or `--all-parts -o dir/`).

Speed: the daemon keeps the kernel cache between calls (only changed nodes recompute).
`--no-kernel` (check, outline) skips geometry. For many edits: `mcad watch FILE --json
--render x.png` in the background.

Model style that makes checking easy:

- One `module` per part, geometry between named grid points instead of loose numbers.
- Name top-level instances (`body: enclosure();`) – they become parts (export, measure,
  render by name).
- Attach parts via anchors (`translate(body.rim) …`).
- Fillet via zones and confirm the edge count with `mcad inspect --line N`.

If the user has the UI open: `mcad ui status --json` first; files in `dirty` have unsaved
changes – ask before writing.
