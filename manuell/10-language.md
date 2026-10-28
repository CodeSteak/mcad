# Language

OpenSCAD syntax and semantics (variables, functions incl. literals, modules, `children()`,
`for`/`if`/`let`/`each`, list comprehensions, `echo`, `assert`, math builtins, primitives,
transforms, booleans, modifiers `# % ! *`, extrusions, `include`/`use`). Files: `.mcad`;
`.scad` works the same, also in `include <lib.scad>`/`use <lib.mcad>` (the path names the file,
extensions are not added or swapped). Differences:

- Exact geometry; `$fn/$fa/$fs` only affect display (except `$fn` = 3…12 at a call: polygon).
- `color()` defines print colours (`mcad usage colors`).
- `hull`, `minkowski`: exact special cases only (below); otherwise warning, children shown
  unchanged.
- Not supported (warning, children shown unchanged): `projection`. `polyhedron` only for
  small planar bodies (`import("x.stl")`: `mcad usage import-stl`); `resize` acts like `scale`.

Additions:

| Construct | Meaning |
| --- | --- |
| `box(a, b)`, `rect(a, b)` | box / rectangle between two corner points |
| `slot(p1, p2, r)`, `slot(p1, p2, d = …)` | 2D slot (stadium) around the segment p1–p2: two half circles + two lines, exact; area π r² + 2 r·\|p2 − p1\|; `p1 == p2`: circle. Points may be grid points/anchors; handles for p1, p2, r. 2D only (like `rect`): `linear_extrude(h) slot(…)` |
| `cylinder(a, b, r)` | cylinder (or cone with `r1`, `r2`) from point a to point b |
| `torus(R, r)` | torus around z |
| `rotate(to = v)` | rotate z onto direction v |
| `grid g { x: …; y: …; z: … }` | named planes (`mcad usage grids`) |
| `anchor name = p, dir = v;` | named point of a module (`mcad usage grids`) |
| `name: module(…);`, `k[i]: module();` | named instance; at top level of the main file: a **part** |
| `fillet`, `chamfer`, `shell`, `zone`, `$fillet` | edge blends (`mcad usage fillets`) |
| `sketch { … }`, `corner`, `fillet2d`, `chamfer2d`, `offset`, `stroke` | exact 2D profiles (`mcad usage sketch`) |
| `text(…)`, `import("x.svg")` | `mcad usage text-svg` |
| `import("x.stl", as = "solid"\|"mesh")` | STL as B-rep or mesh (`mcad usage import-stl`) |
| `assert_keepout(clearance)`, `assert_disjoint()`, `assert_intersect()`, `assert_touches(tol, contact)` | collision checks after evaluation, violation = warning, export stops (`mcad usage asserts`) |
| `loft() { profiles… }` | skin through 2D profiles (`ruled = true` for straight) |
| `sweep(path = pts, interp = "spline"\|"linear") profile;` | sweep a profile along a path (or an open `sketch` as first child) |
| `surface(points = grid4x4…)` + `thicken(t)` | free-form surface and its solid |
| `module ns { … }` | modules as namespaces: `ns.part();`, `ns.f(x)`, `ns.CONST`, `a.b.c();` |
| `render { … }` | studio look of the file: lights, materials per colour/part, environment, … (`mcad usage render-block`); `render()` with parentheses is OpenSCAD's grouping |

```
difference() {                                                    // plate with a slot 26 × 6
    cube([40, 20, 4]);
    translate([0, 0, -1]) linear_extrude(6) slot([10, 10], [30, 10], 3);
}
// mcad measure plate.mcad --volume all   → Volume all: 2606.9027 mm³   (3200 − (9π + 120) · 4)
// mcad measure plate.mcad --section z=2  → two arcs "arc R 3 · ⌀ 6", area 651.7257 mm²
```

## hull and minkowski

Exact or nothing: a combination that is no special case gives the warning
`hull: <reason>; children shown unchanged` (same for `minkowski`) and the children
unchanged. `mcad inspect FILE --line N` names the case taken (hint `exact: …`). Faces of the
result belong to the `hull`/`minkowski` statement.

| `hull()` of | Result (hint) |
| --- | --- |
| 2D profiles of any kind (circles, polygons, sketches, text, booleans, offsets) | convex hull of lines and arcs of the inputs (B-splines as biarcs, 1 µm); collinear input: warning |
| planar bodies: `cube`, `polyhedron`, imported STL (≤ 2000 vertices), `linear_extrude` of polygons without `twist` (any affine transform) | polyhedron of the vertices (`exact: polyhedron (n faces)`) |
| spheres, cylinders, cones on one common axis | revolved 2D hull of the half sections: capsule, cone between two spheres (`exact: solid of revolution`) |
| prisms along one direction with the same start and height: cylinders, cubes, `linear_extrude` without `twist`/`scale` | extruded 2D hull of the sections: plate with round ends (`exact: prism`) |
| ≥ 3 equal spheres, centres not in one plane | hull of the centres offset by r: rounded box (`exact: hull of the centres + offset r`) |

| `minkowski()` of | Result |
| --- | --- |
| 2D: profile + `circle(r)` (also several circles) | `offset(r = Σr)`, moved by the circles' centres |
| 2D: two convex profiles, one of them a polygon | exact sum |
| 3D: body + `sphere(r)` | outward offset by r, moved by the sphere's centre (`exact: offset r`) |
| 3D: two parallel prisms (`linear_extrude` + `cylinder`, `cube` + `cylinder`) | extruded 2D sum of the sections, z from `z0a + z0b` to `z1a + z1b` |
| 3D: two convex planar bodies (cubes, extruded convex polygons) | polyhedron of the vertex sums |

Limits: spheres, cylinders, cones and arcs need rigid transforms and uniform scale. `union`,
groups, `color`, nested `hull` are looked through; `$fillet`/`$chamfer` on a primitive,
`difference`, `rotate_extrude`, `torus` are no special case (`minkowski` with a sphere still
takes any closed body). The offset (`minkowski` with a sphere, equal spheres) is exact for
boxes, cylinders, spheres, rounded bodies and concave edges; at corners whose faces meet at other
than right angles (tetrahedron, 45° wedge) SMLib only approximates: warning `offset is not exact`.

```
hull() for (x = [0, 20], y = [0, 10], z = [0, 5]) translate([x, y, z]) sphere(2);  // rounded box
minkowski() { cube([20, 10, 5]); sphere(2); }                                       // the same body
linear_extrude(3) hull() { circle(3); translate([20, 0]) circle(3); }               // slot plate
hull() { sphere(5); translate([0, 0, 15]) sphere(2); }                              // revolved
hull() { cube([10, 10, 4]); translate([20, 5, 0]) cylinder(r = 5, h = 4); }         // prism
```

Example model: `tests/models/spec/hull_minkowski.mcad`.

## Example (`box.mcad`, used in other pages)

```
module enclosure(w = 60, d = 30, h = 20, t = 3) {
    grid g {
        x: left = 0, inner_l = t, inner_r = w - t, right = w;
        y: front = 0, inner_f = t, inner_b = d - t, back = d;
        z: bottom = 0, floor = t, top = h;
    }
    fillet(1) {
        difference() {
            box(g[left, front, bottom], g[right, back, top]);
            box(g[inner_l, inner_f, floor], g[inner_r, inner_b, g.top + 1]);
        }
        zone(g.top, d = 1);
    }
    anchor corner = g[right, back, top];
    anchor rim = g[left, front, top], dir = [0, 0, 1];
}

module lid(w = 60, d = 30, t = 2) {
    color("teal") cube([w, d, t], $fillet = 0.5);
}

body: enclosure();
translate(body.rim + [0, 0, 10]) cover: lid();
```

Pitfalls:

- Only calls of user modules can be named: `translate(…) cover: lid();`, not
  `cover: translate(…) lid();`.
- Inside `g[…]`, entries are plane names or numbers; for arithmetic use `g.top + 1`.
- A 2D shape in 3D context is ignored with a warning – extrude it.
