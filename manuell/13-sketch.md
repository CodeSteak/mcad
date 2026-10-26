# Sketches and 2D operations

`sketch { … }` is an exact 2D profile; each statement is one segment with its own errors.

```
linear_extrude(5) sketch {
    grid s { x: l = 0, r = 40; y: lo = 0, hi = 20; }
    start(s[l, lo]);                      // new contour
    line(to = s[r, lo]);                  // or by = [dx, dy], length = l, angle = a
    corner(r = 4);                        // round the corner between the segments around it
    line(to = s[r, hi]);
    arc(to = [0, 20], r = 30);            // + large = true / cw = true (as in SVG)
    close();                              // line back to start
    hole { start([10, 5]); arc(to = [16, 5], r = 3); arc(to = [10, 5], r = 3); }
}
```

| Segment | Form |
| --- | --- |
| `arc` | `to` + `r`; `center` + `angle` (negative = clockwise); `center` + `to`; `through` + `to`; only `to` = tangent to the previous segment |
| `bezier(c1, c2, to)` | cubic (`c2` optional) |
| `spline(points, tangent, end_tangent)` | through points |
| `corner(r = …)` / `corner(chamfer = …)` | after `start`: start corner; before `close()`: closing corner |
| `start(…)` again | another contour; opposite winding = hole, or use `hole { … }` |

- `sketch(closed = false)` is an open path (for `sweep`, `stroke`).
- Named sketch: `module profile(w = 40) sketch { … }`; `for`/`if`/`let` and segment modules
  work inside.
- `loft` connects sketches with equal segment counts segment by segment.
- Errors: "contour not closed", "impossible arc: …", "self-intersection of the contour",
  "contour without area".

2D operations (exact arcs, no facets):

```
fillet2d(2) square([30, 20]);                  // all corners; with zones as children: only those
chamfer2d(1) polygon(…);
offset(r = 2) …;  offset(-1) …;  offset(delta = 1, chamfer = true) …;   // as OpenSCAD
linear_extrude(2) stroke(2, cap = "round", join = "miter") sketch(closed = false) { … };
```

`stroke(w, cap = "round"|"flat"|"square", join = "round"|"miter"|"bevel")` turns a path into
an area; `stroke(1) circle(5)` is a ring.
