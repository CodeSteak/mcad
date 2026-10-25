# Language

OpenSCAD syntax and semantics (variables, functions incl. literals, modules, `children()`,
`for`/`if`/`let`/`each`, list comprehensions, `echo`, `assert`, math builtins, primitives,
transforms, booleans, modifiers `# % ! *`, extrusions, `include`/`use`). Differences:

- Exact geometry; `$fn/$fa/$fs` only affect display (except `$fn` = 3…12 at a call: polygon).
- `color()` defines print colours (`cad usage colors`).
- `hull`, `minkowski`: exact special cases only (below); otherwise warning, children shown
  unchanged.
- Not supported (warning, children shown unchanged): `projection`; `import` of meshes.
  `polyhedron` only for small planar bodies; `resize` acts like `scale`.

Additions:

| Construct | Meaning |
| --- | --- |
| `box(a, b)`, `rect(a, b)` | box / rectangle between two corner points |
| `cylinder(a, b, r)` | cylinder (or cone with `r1`, `r2`) from point a to point b |
| `torus(R, r)` | torus around z |
| `rotate(to = v)` | rotate z onto direction v |
| `grid g { x: …; y: …; z: … }` | named planes (`cad usage grids`) |
| `anchor name = p, dir = v;` | named point of a module (`cad usage grids`) |
| `name: module(…);`, `k[i]: module();` | named instance; at top level of the main file: a **part** |
| `fillet`, `chamfer`, `shell`, `zone`, `$fillet` | edge blends (`cad usage fillets`) |
| `sketch { … }`, `corner`, `fillet2d`, `chamfer2d`, `offset`, `stroke` | exact 2D profiles (`cad usage sketch`) |
| `text(…)`, `import("x.svg")` | `cad usage text-svg` |
| `loft() { profiles… }` | skin through 2D profiles (`ruled = true` for straight) |
| `sweep(path = pts, interp = "spline"\|"linear") profile;` | sweep a profile along a path (or an open `sketch` as first child) |
| `surface(points = grid4x4…)` + `thicken(t)` | free-form surface and its solid |
| `module ns { … }` | modules as namespaces: `ns.part();`, `ns.f(x)`, `ns.CONST`, `a.b.c();` |

## hull and minkowski

Exact or nothing: a combination that is no special case gives the warning
`hull: <reason>; Kinder werden unverändert gezeigt` (same for `minkowski`) and the children
unchanged. `cad inspect FILE --line N` names the case taken (hint `exakt: …`). Faces of the
result belong to the `hull`/`minkowski` statement.

| `hull()` of | Result (hint) |
| --- | --- |
| 2D profiles of any kind (circles, polygons, sketches, text, booleans, offsets) | convex hull of lines and arcs of the inputs (B-splines as biarcs, 1 µm); collinear input: warning |
| planar bodies: `cube`, `polyhedron`, `linear_extrude` of polygons without `twist` (any affine transform) | polyhedron of the vertices (`exakt: Polyeder (n Flächen)`) |
| spheres, cylinders, cones on one common axis | revolved 2D hull of the half sections: capsule, cone between two spheres (`exakt: Rotationskörper`) |
| prisms along one direction with the same start and height: cylinders, cubes, `linear_extrude` without `twist`/`scale` | extruded 2D hull of the sections: plate with round ends (`exakt: Prisma`) |
| ≥ 3 equal spheres, centres not in one plane | hull of the centres offset by r: rounded box (`exakt: Hülle der Mittelpunkte + Offset r`) |

| `minkowski()` of | Result |
| --- | --- |
| 2D: profile + `circle(r)` (also several circles) | `offset(r = Σr)`, moved by the circles' centres |
| 2D: two convex profiles, one of them a polygon | exact sum |
| 3D: body + `sphere(r)` | outward offset by r, moved by the sphere's centre (`exakt: Offset r`) |
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

Example model: `examples/spec/hull_minkowski.scad`.

## Example (`box.scad`, used in other pages)

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
