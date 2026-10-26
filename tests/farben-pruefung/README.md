# Independent review of the colour features (commit d8fe983)

Carried out by an independent test agent on 2026-10-23 against the user's
requirements (multi-colour support with export for Bambu Studio/OrcaSlicer, small muted palette with
English names, warning + random colour for unknown names, dark theme). Test cases in
`cases/`, images in `img/`, helper scripts `check3mf.py` (3MF structure, meshes, overlap,
lib3mf), `contrast.py` (WCAG contrast of the tokens), `deltae.py` (colour distances).

## Result per requirement

| Requirement | Result |
| --- | --- |
| Multi-colour, export for Bambu/Orca | partial: 3MF formally clean (lib3mf, trimesh, parts per colour, `extruder` per part); display and UI slots faulty (F1–F4) |
| Palette | met |
| Unknown names | partial: warning + stable colour yes, collisions (F5) |
| Dark theme | CLI met, token contrasts recalculated; UI not checked (no build) |

## Bugs (by severity)

- **F1 (high)** Identical geometry with different colours is shown in the view (CLI/UI) and STEP in
  the first colour (shared cached node → one tag → `tagColors` takes the first
  node). `color("red") cube(5); translate([10,0,0]) color("blue") cube(5);` → two red
  cubes. Loops `for (i) color(names[i]) cube(…)` → everything red. 3MF is correct.
  Cases: `cases/pal2.scad`, images `img/pal2.png` vs. `img/pal2_regions.png`, `img/pal_iso_light.png`.
- **F2 (high)** UI slot selection (`SetFilament`) writes `filament = n` to the `color()` statement;
  if the colour is a parameter (`module bar(c) color(c) …`) or comes from a loop,
  all colours of this statement get the same slot. Case: `cases/slot_mod.scad`, `cases/palette2.scad`.
- **F3 (medium)** Face colour in the view differs from the print: (a) overlap
  (`cases/r_overlap2.scad`, `r_overlap3.scad`, `img/r_overlap23.png`; also the frame in
  `tests/models/spec/mehrfarbig.scad`), (b) fillet at a colour boundary shown grey, exported red
  (`cases/r_fillet2.scad`, `img/r_fillet2*.png`), (c) `linear_extrude(5) color("red") square(10);`
  shown red, exported as default/slot 1, missing from the colour list, no warning
  (`cases/r_extrude.scad`, `img/r_extrude_*.png`).
- **F4 (medium)** Material outside of parts: the colour list shows 0 mm³, the UI export drops it
  without notice; CLI export without `--part` merges parts into one object and still reports
  „je Bauteil ein Objekt“ (one object per part) (`cases/inc/main_use.scad`).
- **F5 (medium)** Random colours collide (only the hue varies): `baz`/`violet` and
  `x`/`rose` exactly equal, several almost equal or close to palette colours; same colour →
  merged, same slot (`cases/randnames.scad`).
- **F6 (medium/low)** Slivers in colour regions with neighbours of the same colour: thin faces
  (0.00016 mm) overlap the neighbour, volume 2000.032 instead of 2000, without warning
  (`cases/r_touch*.scad`).
- **F7 (low)** Alpha variants become separate entries/slots with the same name; the conflict check
  ignores alpha; `[255,0,0]`, negative values, `alpha = 2` silently clamped;
  `color("transparent")` without warning, listed as a random colour, exports #00000000;
  `color("")` empty name (`cases/values.scad`).
- **F8 (low)** Automatic slots above 16 without warning; two colours on the same explicit
  slot or slot 1 next to „Standard“ (default) without notice (`cases/f1.scad`); a single-colour model still exports
  a `colorgroup` (probably triggers the colour dialog in Bambu; `cases/plain.scad`).
- **F9 (low)** `--per-color -o x.stl` writes a ZIP under `.stl`; `--per-color --part figur`
  prints the colour list of the whole file; the colour list names no file for `use`/`include`.
- **Daemon:** a long `CAD_IPC_DIR` exceeds the Unix socket limit (107 characters,
  "path must be shorter than SUN_LEN"); the daemon does not start, without a visible notice.

## Status of the findings (report 12, `docs/reports/12-farben-korrekturen/report.pdf`)

All cases from `cases/` run as the regression test `crates/cad-cli/tests/farben_pruefung.rs`
(every file without errors, region volume = part volume, random colours at a distance from the
palette), plus a dedicated test per finding.

| Finding | Status | Fix | Tests |
| --- | --- | --- | --- |
| F1 | fixed | The view shows the colour regions (UI default, `cad render`); colour per face only from nodes below the drawn node, for identical content the one with the matching bounding box; STEP face colours from the regions (`face_region_colors`). Cache reuse remains. | `f1_equal_geometry_…`, Vitest "F1", Playwright G01, Tauri T15 |
| F2 | fixed | `filament("red", 4);` at top level: slot table with precedence; the UI writes it when the colour at a statement is not a literal | `set_filament_of_a_parameter_colour_writes_a_table`, `filament_table_rules`, `filament_declarations`, G02, T15 |
| F3 | fixed | (a, b) regions in the view; (c) a uniform 2D colour colours the extrusion, mixed → warning, default | `f3_view_matches_the_print`, G03 |
| F4 | fixed | object "no part" (fixed name; was „ohne Bauteil“) in list and export, CLI = UI, the message names the objects | `f4_material_outside_parts`, `material_outside_parts_is_exported`, G04, T15 |
| F5 | fixed | L, C, hue from the hash; ΔE ≥ 0.07 to the palette, ≥ 0.06 to other random colours in the model | `f5_random_colours_keep_their_distance`, `random_colours_do_not_collide_in_a_model` |
| F6 | fixed | only later material of a different colour is subtracted | `f6_no_slivers_…`, all cases (sum to 10⁻⁶) |
| F7 | fixed | identity = RGB (alpha only for the view), clamping with warning, `transparent`/`""`/`color()` do not colour (warning) | `f7_value_rules` |
| F8 | fixed | warning above 16 slots, notice for a shared slot and slot 1 next to default; no colour group (also for single colour) | `f8_slot_remarks_and_single_colour`, `no_color_group_by_default` |
| F9 | fixed | `--per-color` only with `.zip`, `--part` filters the colour list, lines of other files as `lib.scad:1` | `f9_per_colour_export_and_list`, Vitest "F9" |
| Daemon | fixed | socket path too long → `/tmp/cad-<uid>/<hash>-daemon.sock`, shown in `cad daemon status` | `long_ipc_dir_uses_short_socket_path` |
| Bambu dialog | avoided | 3MF without `m:colorgroup` (the dialog only opens with a colour group); `--color-group` for other programs, message „Abbrechen“ (Cancel) | `no_color_group_by_default`, `export_multicolour_single_and_per_colour` |

## Notes on Bambu Studio (from the source code, not checked in the program)

For 3MF files from other programs with `colorgroup`, Bambu Studio opens a colour dialog; its
default „Anhängen“ (Append) appends colours as new filaments, and on OK
`apply_volume_filament_ids` overwrites the `extruder` of every part (so also `filament = n`); on
„Abbrechen“ (Cancel) the slots are kept (`Plater.cpp` ~8890, `ObjColorDialog.cpp` 647 ff.).

## Verified as correct

Nested `color()`, overlap rule (later wins), `intersection`, `difference`,
modules/`children()`/loops/`include`/`use`/`scale`/`resize`/`mirror`, modifiers,
warnings for invalid hex/vectors, `filament` conflicts with warning, `--single-color`,
ZIP per colour, XML escaping, umlauts, 40 bodies in 8 colours ≈ 1 s.
