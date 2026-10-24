# Language

OpenSCAD syntax and semantics (variables, functions incl. literals, modules, `children()`,
`for`/`if`/`let`/`each`, list comprehensions, `echo`, `assert`, math builtins, primitives,
transforms, booleans, modifiers `# % ! *`, extrusions, `include`/`use`). Differences:

- Exact geometry; `$fn/$fa/$fs` only affect display (except `$fn` = 3…12 at a call: polygon).
- `color()` defines print colours (`cad usage colors`).
- Not supported (warning, children shown unchanged): `hull`, `minkowski`, `projection`;
  `import` of meshes. `polyhedron` only for small planar bodies; `resize` acts like `scale`.

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
