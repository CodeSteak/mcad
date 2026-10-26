# Everyday models and typical CAD parts

19 parametric models (15 single files, 4 assembly folders with `include`/`use`), developed
and checked with the CLI only (`mcad outline`, `check`, `render --views all --show grids,anchors`,
`inspect`, `measure --interference/--clearance`, `export` STL/STEP). Every file starts with
`// Purpose: …`; all dimensions in mm; every model is a module with default values, instantiated at top
level as a named instance (part, export per part with `mcad export … --all-parts`).
Catalogue with image, key data and parameters per model: [`docs/models/everyday-models.pdf`](../../../docs/models/everyday-models.pdf).

Regression: `tests/everyday/check.sh` (all models: `check --json`, STEP export per part, STL
and STEP overall into a temporary directory, comparison with `tests/everyday/expected.json`;
`--update` rewrites the file, `--bugs` additionally runs the bug examples).

## Models

Volume: exact B-Rep (kernel), sum of the parts. Kernel: `scene.stats.kernel_ms` of
`mcad eval` without cache; evaluation: `timings.evaluate_ms` of `mcad render` (evaluation including
edge queries and tessellation), both measured once on the development machine
(Linux, one core per call, without daemon – see F14).

| File | Purpose | Language features | Parts | Volume mm³ | Kernel / evaluation | Preview |
| --- | --- | --- | --- | ---: | ---: | --- |
| [`phone_stand.mcad`](phone_stand.mcad) | phone stand, tilt 65°, cable hole with channel | 2D grid for the profile + 3D grid for zones, `polygon` from grid points, `linear_extrude`, function with `let` (line intersection), nested `fillet` zones on grid lines (r 3 / 1.5) | `stand` | 109 978 | 0.2 / 0.5 s | [img](img/phone_stand.png) |
| [`mug.mcad`](mug.mcad) | mug with handle, ~370 ml | `rotate_extrude`, zone on grid plane, **`shell`** (open at the top), `fillet` around `shell`, **`sweep`** (spline path), `union(fillet = …)`, function | `b` | 77 709 | 1.6 / 4.5 s | [img](img/mug.png) |
| [`cable_clip.mcad`](cable_clip.mcad) | cable holder for the desk edge (19 mm), 4 channels | indexed grid planes from a list, `linear_extrude`, zones on grid lines, `difference(fillet = …)` (40 edges), `cylinder(a, b, r)`, `for` | `clip` | 24 457 | 0.6 / 1.7 s | [img](img/cable_clip.png) |
| [`wall_hook.mcad`](wall_hook.mcad) | wall hook with ball knob, 2 countersunk holes | `fillet` zones (lines) inside a `chamfer` zone (plane) – the chamfer covers the boundary edges of the fillet, **`sweep`**, `union(fillet)`, cone `cylinder(h, r1, r2)`, anchor with `dir` | `hook` | 18 818 | 1.0 / 2.2 s | [img](img/wall_hook.png) |
| [`soap_dish.mcad`](soap_dish.mcad) | soap dish with drainage slots, ribs, feet | nested zones (vertical lines r 14, bottom plane r 5), **`shell`**, `cylinder(a, b, r)` between grid points, `for` with two variables, `difference(fillet)` | `dish` | 36 961 | 1.6 / 2.1 s | [img](img/soap_dish.png) |
| [`pen_box.mcad`](pen_box.mcad) | pen box, 3 tall compartments + low compartment | recursive function, plane lists from expressions, 4 nested zone blocks, `zone` in `for`, `chamfer` zone | `box_` | 115 179 | 1.3 / 1.5 s | [img](img/pen_box.png) |
| [`door_stop.mcad`](door_stop.mcad) | door stop wedge with grooves, finger recess, hanging hole | **`loft`** through 3 profiles (function), `rotate([90, 0, 90])`, `chamfer` zone on the bottom plane, `for`, sphere as tool | `stopper` | 64 237 | 0.2 / 0.6 s | [img](img/door_stop.png) |
| [`trivet.mcad`](trivet.mcad) | pot trivet Ø 180, rings + spokes | `rotate_extrude` in `for`, list as grid planes, `box` between points under `rotate`, `chamfer` zone (103 edges) | `u` | 83 250 | 5.4 / 7.2 s | [img](img/trivet.png) |
| [`flange.mcad`](flange.mcad) | flange plate Ø 120, collar, bolt circle 6 × M10, sealing groove | cross-section from 2D grid, `rotate_extrude`, **own module as zone** (`ring_zone`), `fillet` and `chamfer` zones, `difference(chamfer)`, bolt circle via `for`/`rotate` | `f` | 150 300 | 1.2 / 2.4 s | [img](img/flange.png) |
| [`pillow_block.mcad`](pillow_block.mcad) | pillow block for ball bearing 608, M6 slots | zones on grid lines in x, y and z (fillets around the eye), `chamfer` zone, `difference(chamfer)`, helper module for the slot, `cylinder(a, b, r)` tangential to `box` | `block` | 34 404 | 0.8 / 1.2 s | [img](img/pillow_block.png) |
| [`rib_bracket.mcad`](rib_bracket.mcad) | mounting bracket 60 × 50 with rib, M5 | indexed planes (ribs), zones in `for`, fillets with corner transition in one block, `linear_extrude`, `difference(chamfer)` | `w` | 22 246 | 0.6 / 1.1 s | [img](img/rib_bracket.png) |
| [`heat_sink.mcad`](heat_sink.mcad) | heat sink 60 × 50, 9 fins | indexed planes, fins via `for`, zones on grid lines in `for` (fin roots), plane zone (tips), `floor` | `hs` | 34 652 | 0.9 / 1.5 s | [img](img/heat_sink.png) |
| [`gear.mcad`](gear.mcad) | spur gear pair m 2, z 20/12, keyway, D bore | involute from list functions (`let`, `each`, `concat`), `polygon`, `if`, `intersection`, `difference(chamfer)`, two meshing parts | `gear1`, `gear2` | 16 628 | 5.1 / 5.5 s | [img](img/gear.png) |
| [`pipe_clamp.mcad`](pipe_clamp.mcad) | two-part pipe clamp Ø 32, M5/M8 | `rotate_extrude(angle = 180)`, `chamfer` zones, `mirror`, parts in a rotated block, anchor with `?:` | `top`, `bottom`, `pipe` | 25 870 | 2.1 / 2.8 s | [img](img/pipe_clamp.png) |
| [`pi_plate.mcad`](pi_plate.mcad) | Raspberry Pi mounting plate (58 × 49, M2.5) + board dummy | named instances `boss[i]` in a loop inside the module, anchor from an instance anchor (`boss[0].top`), `union(fillet)`, zones, `color`, `$fillet`, board on the anchor | `plate`, `pi` | 46 048 | 1.6 / 2.1 s | [img](img/pi_plate.png) |
| [`housing/`](housing/housing.mcad) | electronics enclosure with lid, 4 M3 screws | **assembly**: `include <dimensions.mcad>` (dimensions + function `boss_pos`), `use <parts.mcad>`, **`shell`** of a filleted box, `union(fillet)`, bosses `boss[i]`, lid on the anchor, screws `s[i]`, `explosion` | `base`, `lid`, `s[0]`…`s[3]` | 42 068 | 4.0 / 3.5 s¹ | [img](img/housing.png) (exploded) |
| [`hinge/`](hinge/hinge.mcad) | hinge 60 mm, 5 knuckles, clearance 0.4 | **assembly**: `include`/`use`, knuckles from one function, `for` with step, clearances, leaf B rotated to the opening angle | `a`, `b`, `pin` | 10 151 | 2.6 / 3.7 s | [img](img/hinge.png) |
| [`planter/`](planter/planter.mcad) | plant pot with saucer | **assembly**: superellipse as `spline` (function with `pow`, `sign`), **`loft`** square → circle, difference of two lofts, `rotate_extrude` + **`shell`**, volume zones, anchor on anchor | `saucer`, `pot` | 107 178 | 1.1 / 1.8 s | [img](img/planter.png) |
| [`key_rack/`](key_rack/key_rack.mcad) | key rack with 5 glued-in hooks | **assembly**: `include`/`use`, planes from a function, zones + chamfer, keyhole hanger, **`sweep`**, `union(fillet)`, hooks `h[i]` in `for` | `board`, `h[0]`…`h[4]` | 158 077 | 4.0 / 15.9 s² | [img](img/key_rack.png) |

¹ Evaluation of the exploded preview state (`explosion = 22`). ² See "Run time" below.

Coverage of the requirements: assemblies with multiple parts: 4 folders + `gear`, `pipe_clamp`,
`pi_plate`; fillet/chamfer via zones: 18 models (all except `gear`); `shell`: `mug`, `soap_dish`,
`housing`, `planter` (saucer); `sweep`/`loft`: `mug`, `wall_hook`,
`key_rack` / `door_stop`, `planter`.

## Fits and collisions (`mcad measure`)

All assemblies: `--interference` without collision (tolerance 0.001 mm³).

| Model | Pair | Distance | Meaning |
| --- | --- | ---: | --- |
| `gear` | `gear1` ↔ `gear2` | 0.116 mm | backlash (tooth thickness reduced by 0.1 mm) |
| `pipe_clamp` | `top` ↔ `bottom` / `top` ↔ `pipe` | 2.0 / 0.035 mm | clamping gap at the lugs / pipe touches |
| `pi_plate` | `plate` ↔ `pi` | 0 mm | board rests on the bosses (contact, no collision); hole pattern 58 × 49 via `--between boss[0].top boss[3].top` |
| `housing` | `base` ↔ `lid`, `s[0]` ↔ `lid` / `s[0]` ↔ `base` | 0 / 0.05 mm | lid rests on the base, screw head rests on the lid / shank (core 2.4) in pilot hole 2.5 |
| `hinge` | `a` ↔ `b` / `pin` ↔ `b` / `pin` ↔ `a` | 0.397 / 0.223 / 0 mm | clearance of the knuckles / bearing clearance / pin head touches; at `angle` < approx. 40° the leaves collide |
| `planter` | `pot` ↔ `saucer` | 0 mm | feet stand on the saucer base, wall 4 mm from the rim |
| `key_rack` | `board` ↔ `h[0]` | 0.081 mm | peg 6 in hole 6.3 (glue gap), narrowest point at the chamfer |

## Bugs and weaknesses found

Minimal examples are in [`tests/everyday/bugs/`](../../everyday/bugs/) (header comment:
finding, status, expected state `// EXPECT…`; `tests/everyday/check.sh --bugs` reports per example
"behoben" (fixed) or "offen" (open)). Findings with CLI build `target/release/mcad` of 2026-10-23 14:31
(merge `384d31b`); fixes and evidence in
[test report 13](../../../docs/reports/13-geometrie-korrekturen/report.pdf).

**Status after report 13:**

| Finding | Status | Cause / fix |
| --- | --- | --- |
| F1 sweep with spline profile | fixed | SMLib places the centre of the *control point* hull on the path; patch `0003` lets the facade specify the profile origin (arbitrary profiles, including off-centre ones, on curved paths) |
| F2 cylinder ∪ sweep | open, single message | SMLib intersection near the tilted end cap (`SM_ERR_NULL_POINTER`); duplicate diagnostic fixed (one message per failed operation) |
| F3 sweep 16-gon | open, message with cause | SMLib leaves seams between side faces open (twin edges at distance 0, `StitchFaces` does not join them) |
| F4 union(fillet) on 32-gon sweep | open | fillet of the facet edges (SMLib `SM_ERR`), workaround `circle()` |
| F5 chamfer → volume 0 | fixed (error message) | inner/outer chamfers meet at the corner (wall 1.76 mm); the result is checked for a closed shell and volume, open shell = error; `chamfer(0.7)` works |
| F6 chamfer on a short concave edge | open | SMLib corner type convex/concave; the edge is highlighted |
| F7 `torus()` as zone | fixed | as F8 (tolerance 0 after translation) |
| F8 Booleans with torus/`rotate_extrude` | fixed (a, b); c as error message | `SmBrep::Transform` derived the tolerances from the hull of the vertices – a torus and a solid of revolution of a circle have only one vertex → tolerance 0; rigid transformations now leave the tolerances unchanged. F8c (tangential contact) has no manifold result → message instead of volume 0 |
| F9 crash | fixed | type confusion in `SmFilletExecutive::CreateFilletCorners` (patch `0002`): missing corner type → error; tangential contact → message |
| F10 union(fillet) loses the shell | reported, correct fillet open | the fillet yields an open body → error (plausibility check), the union remains |
| F11 coincident cylinders | open | SMLib Boolean with coincident cylinder faces; workaround clearance ≥ 0.05 mm |
| F12 shell loft square → circle | open | SMLib shell and fallback route fail |
| F13 shell message | fixed | message names the smallest convex radius ≤ wall thickness |
| F15 knob fillet | open | fillet sweep/sphere (SMLib) |
| S1 profile grid | fixed (CLI) | grids whose references all lie in the profile of a `rotate_extrude` are drawn by `mcad render` in the profile plane; UI: see report 13 |
| S2 plane names as values, anchor lists | proposal | report 13, section S2 |
| S3 `outline` | fixed | named instances in modules and their anchors listed separately ("Benannte Instanzen in Modulen" (named instances in modules), JSON `scope`) |
| S4 section/labels | fixed | section of a model made of parts per part (own cap, contour, hatching); grid rows `g.rib[0..7]` with one label, spacing between labels |
| Run time | fixed | 70 % of the time in `SmApiBrepComputeVolume(…, 1e-8)` for the part volumes; accuracy 1e-6: evaluation of the key rack 16.7 → 6.1 s (median of 3) |

The findings as originally recorded (before report 13):

### Wrong result without a message (most severe)

- **F5 – chamfer/fillet yields an open shell, status ok.** Box (wall 3) with vertical
  corners r 6, then `chamfer(0.8)` via the zone of the top edge: 0 errors, volume 0 mm³, STEP with
  26 faces. Likewise in the pen box (corners r 6, wall 2.4), also with `fillet(0.8)`. Expected:
  kernel error if the body is not closed. [`f05_chamfer_volume_zero.mcad`](../../everyday/bugs/f05_chamfer_volume_zero.mcad)
- **F8 – Booleans with a torus.** `union() { cylinder(h = 2, r = 30); translate([0, 0, 2]) torus(20, 1); }`
  yields two overlapping bodies (6049.65 instead of 5852.26 mm³); the same with
  `rotate_extrude() translate([20, 0]) circle(1)` loses the ring entirely (5654.87 mm³);
  `difference() { cylinder(h = 2, r = 30, center = true); torus(20, 1); }` (the torus touches the top and
  bottom faces) has volume 0. All without a message. `f08a…`, `f08b…`, `f08c…`
- **F10 – `union(fillet)` loses an operand.** Filleted shell (`shell`) plus a boss near
  the inner corner: the result is only the boss with its fillet, volume 0, status ok (the
  `difference(chamfer)` above it then reports "0 Kanten" (0 edges)). With the boss further inside it is correct.
  [`f10_union_fillet_shell_lost.mcad`](../../everyday/bugs/f10_union_fillet_shell_lost.mcad)

### Crash

- **F9 – segmentation fault (exit 139) of the CLI** with `union(fillet = 1)` when two bodies
  touch tangentially: (a) plate + wall + cylinder touching the wall
  (`union(fillet = 1) { cube([20, 20, 2]); cube([2, 20, 10]); translate([5.5, 10, 0]) cylinder(h = 10, r = 3.5); }`),
  (b) peg d 6 + an equally thick `sweep` starting at its end face. No crash without `fillet`. No
  diagnostic text, no output – `measure`/`export` abort as well. `f09a…`, `f09b…`

### Kernel errors with a message

- **F1 – `sweep` with a `spline` profile** on a curved path rejected ("profile that is not
  centred on its origin"), although the profile is symmetric (check via the NURBS hull with
  tolerance 1e-9 in `smlib/facade/sc_facade.cpp`). As a result there are no smooth elliptical
  sweep profiles; workaround `circle` or polygon. `f01…`
- **F2 – cylinder ∪ curved `sweep`** whose end caps intersect the cylinder face:
  `SM_ERR_NULL_POINTER`. In `union() { … }` the same diagnostic appears twice. `f02…`
- **F3 – `sweep` of a 16-gon** on a curved path: "did not produce a closed solid" (32-gon
  and `circle` work). `f03…`
- **F6 – chamfer/fillet via a plane zone fails** when convex edges end at a short
  concave edge (step in front of a wall, short outer edges between cooling fins, front
  compartment of the pen box). `render --line` highlights the edge in red (helpful); workaround: zones on
  grid lines instead of on the plane. `f06…`
- **F11 – coincident cylinders** (pin 3.0 in hole 3.0 of the hinge): root union
  `SM_ERR_NULL_POINTER`, `--interference` reports 0.24 mm³. Not reduced further (one knuckle with
  the pin alone works). `f11…`
- **F12 – `shell` of a loft square → circle** fails (`SmMerge.cpp:672`); circle → circle works.
  Workaround: difference of two lofts (plant pot). `f12…`
- **F15 – `union(fillet)` sweep (d 6) + sphere d 9** fails for every radius, with sphere d 10
  it does not. `f15…`
- F4 (minor): `union(fillet)` on a sweep with a 32-gon profile fails (many facet edges);
  with `circle` it works.
- F13 (message): `shell(2)` of a body with fillet r 2 (inner radius 0) only reports
  "SmApiShellBrepFull failed: SM_ERR (1001)"; a hint "Verrundungsradius ≤ Wandstärke" (fillet radius ≤ wall thickness)
  would name the cause.
- No bug in the program, but instructive: collisions in one's own design (foot 0.3 mm next to
  a slot, slot in the rib, keyhole in the letter slot) only show up as
  "SM_ERR (1001)" of the fillet/chamfer – a hint at the faces involved is missing.

### CLI, display, language

- **F14 – daemon silently unavailable:** with `MCAD_IPC_DIR=/tmp/claude-0/…/scratchpad/everyday-ipc`
  (98 characters, 110 with `/daemon.sock` – above the limit of 107 for Unix sockets) the result is "path must
  be shorter than SUN_LEN"; every command computes without
  cache. Visible only in `--json` (`daemon.reason`) or with `--verbose`, the normal output
  is silent. Proposal: hint on stderr, or create the socket under a short path (hash).
- **Run time:** `kernel_ms` often covers only part of the evaluation: key rack 4.0 s kernel,
  but 15.9 s `evaluate_ms` and 16 s for `mcad check` (without image); mug 1.6 / 4.5 s. The rest is
  not shown in the counters (edge queries of the zones/new edges, tessellation?).
- **S1 – profile grid in the wrong plane:** a 2D grid for a `rotate_extrude` profile
  (x = radius, y = height) is drawn in the xy plane of the module, but the profile in xz
  (mug, flange, phone stand: `p.rim` appears in the image 95 mm behind the part). Placing a grid before
  `rotate` does not help, because `g[…]` then returns module coordinates that no longer lie in
  the profile plane – access to grid-local coordinates is missing.
- **S2 – plane names are not values:** `g[i % 2 == 0 ? hole_left : hole_right, …]` yields "unbekannte
  Variable `hole_left`" (unknown variable `hole_left`); workaround via numbers `[i % 2 == 0 ? g.hole_left : g.hole_right, …]`. Anchors
  cannot be declared as a list (`anchor hole[i] = …`), hence positions via functions.
- **S3 – visibility:** `mcad outline` lists instances from a module (`boss[0]` … in
  `pi_plate`, `housing`) under "Benannte Instanzen" (named instances) of the file and their anchors as file anchors;
  `measure --between boss[0].top …` works in the file context, although according to the spec the name is only valid in the
  module.
- **S4 – display:** section caps of touching parts merge (no dividing line,
  enclosure section); with many grid planes the labels of
  `--show grids` overlap heavily (heat sink, soap dish).
- Pitfall (OpenSCAD-compatible, but costly): `for (i = [0 : n - 1])` with `n = 0` runs over
  `[-1, 0]` (only a warning "Anfang > Ende … vertauscht" (start > end … swapped)) – in the gear this produced two huge
  lightening holes.

### Language features that were missing

`offset`/`hull` and 2D Booleans (rounded rectangles, slots, arcs in the profile had to be
built via 3D fillets, superellipse splines or helper modules); an arc or
rounded corner in `polygon`; a non-uniformly scaled circle (ellipse) as profile;
`text` (labelling of the key rack); threads; anchor lists and plane names as values (S2);
grid-local coordinates for profiles in rotated planes (S1); a zone "edge between face
A and B" (concave fillets without the neighbouring edges, cf. F6); parameters (`-D`) for variables of the
file, not only for module parameters (opening angle, `explosion`).
