# STL import

```
import("../stl/winkel.stl");                     // closed, ≤ 500 faces: B-rep (Booleans work)
import("board.stl", as = "mesh");                // keep it a mesh (display, export, measure)
import("part.stl", center = true, scale = 25.4); // inch file → mm, bounds centred at the origin
```

| Option | Meaning |
| --- | --- |
| `file` | path relative to the `.scad`; must be a string literal in the project (like SVG) |
| `as` | `"solid"`: B-rep, `"mesh"`: mesh without B-rep; default: automatic (below) |
| `center` | centre of the bounds to the origin (all three axes) |
| `scale` | number or `[x, y, z]`, positive, applied first (mirror with `mirror()`) |
| `convexity` | accepted, ignored |
| `id`, `dpi` | SVG only (warning) |

Units mm. ASCII and binary STL; a file of exactly 84 + 50·n bytes is binary even if it starts
with `solid`. The file is part of the project: CLI paths relative to the `.scad`, browser
upload, desktop and `watch` reload it on change; results are cached by file content.

Preparation (every mode): corners welded (tolerance 10⁻⁶ × bounds diagonal); triangles with
NaN, two equal corners or repeated are dropped; orientation made consistent and outward
(voids inward); coplanar neighbouring triangles merged into planar faces (a plane with holes
becomes a few faces without holes); vertices snapped onto their faces' planes (a face that
cannot be exactly planar stays triangles).

| Mode | Automatic when | Works | Not available |
| --- | --- | --- | --- |
| B-rep (`solid`) | closed, manifold, ≤ 500 faces after merging | everything: Booleans, `fillet`/`$fillet`, exact measuring, STEP | – |
| mesh (`mesh`) | > 500 faces, open, non-manifold | display, transforms, `color`, parts, `hull` (≤ 2000 vertices), STL/3MF, STEP (facetted: one plane per face), `measure` bounds/volume/area/faces/`--section`, `--clearance`, `--interference` (mesh test: overlap yes/no, no volume) | Booleans, `fillet`/`chamfer`/`shell`, `$fillet`, `minkowski`, `thicken` |

With a mesh child: `union()` shows it next to the result (warning), `difference()` keeps a
mesh base uncut or drops a mesh operand (error), `intersection()`/`fillet` leave the children
unchanged (error); every message names `as = "solid"`. A failed B-rep build falls back to the
mesh (warning; error with `as = "solid"`). Hint at the node (`cad inspect FILE --line N`):
`B-Rep: 8 Flächen aus 20 Dreiecken` or `Netz: 20000 Dreiecke, 18556 Flächen`.

Kernel time (sphere-like meshes; the limit 500 keeps import + one Boolean ≲ 2 s):

| Faces | B-rep build | + Boolean (cylinder through it) | as mesh |
| --- | --- | --- | --- |
| 445 | 0.3 s | 1.6 s | 0 s |
| 890 | 0.7 s | 4 s | 0 s |
| 1807 | 1.2 s | 20 s | 0 s |
| 4646 | 7.5 s | 107 s | 0 s |
| 18556 (20000 triangles) | – | – | 0 s (parse + prepare 0.08 s) |

Example model: `examples/spec/stl_import.scad`.
