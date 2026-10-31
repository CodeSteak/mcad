# Quick reference

All language constructs on one page. Details: the topic in the last column (`mcad usage <topic>`).
Units mm and degrees, Z up.

## 3D primitives

| Call | Result | Topic |
| --- | --- | --- |
| `cube(size, center)` | box; `size` number or `[x, y, z]` | language |
| `box(a, b)` | box between two corner points | language |
| `sphere(r \| d)` | sphere | language |
| `cylinder(h, r \| d, r1, r2, d1, d2, center)` | cylinder or cone along z | language |
| `cylinder(a, b, r \| d)` | cylinder (or cone with `r1`, `r2`) from point a to b | language |
| `torus(R, r)` | torus around z | language |
| `polyhedron(points, faces)` | small planar bodies only | language |
| `surface(points = …)` + `thicken(t)` | free-form surface and its solid | language |
| `import("x.stl", as = "solid" \| "mesh", center, scale)` | STL as B-rep (≤ 500 faces) or mesh | import-stl |

## 2D primitives and operations

| Call | Result | Topic |
| --- | --- | --- |
| `square(size, center)`, `rect(a, b)` | rectangle; `rect` between two corner points | language |
| `circle(r \| d)` | circle (exact) | language |
| `polygon(points, paths)` | polygon | language |
| `slot(p1, p2, r \| d)` | stadium around the segment p1–p2 | language |
| `sketch { … }`, `sketch(closed = false) { … }` | exact profile / open path (segments below) | sketch |
| `text(t, size, font, halign, valign, spacing, direction)` | text outline; default font `"cad stencil mono"` | text-svg |
| `import("x.svg", id, center, scale, dpi)` | SVG as exact profile | text-svg |
| `offset(r = … \| delta = …, chamfer)` | offset as OpenSCAD, exact arcs | sketch |
| `fillet2d(r) …`, `chamfer2d(d) …` | round/chamfer corners (all, or those in 2D zones) | sketch |
| `stroke(w, cap, join) path` | path → area; `cap` round/flat/square, `join` round/miter/bevel | sketch |

## Sketch segments (inside `sketch { … }`)

| Segment | Form |
| --- | --- |
| `start(p)` | new contour (opposite winding = hole) |
| `line(to = p)`, `line(by = [dx, dy])`, `line(length = l, angle = a)` | straight segment |
| `arc(to, r)`, `arc(center, angle)`, `arc(center, to)`, `arc(through, to)`, `arc(to)` | circular arc; `to` alone = tangent; `large`, `cw` |
| `bezier(c1, c2, to)` | cubic Bézier (`c2` optional) |
| `spline(points, tangent, end_tangent)` | spline through points |
| `corner(r = …)`, `corner(chamfer = …)` | round/chamfer the corner between the segments around it |
| `close()` | line back to the start |
| `hole { … }` | contour that is a hole |

## Extrusions

| Call | Result | Topic |
| --- | --- | --- |
| `linear_extrude(height, center, twist, scale, slices) 2d` | prism | language |
| `rotate_extrude(angle) 2d` | revolved body around z | language |
| `loft(ruled) { 2d; translate([0, 0, h]) 2d; … }` | skin through profiles with equal segment counts | sketch |
| `sweep(path = pts, interp = "spline" \| "linear") 2d` | profile along a path (or an open `sketch` as first child) | language |

## Transforms

| Call | Effect |
| --- | --- |
| `translate(v)`, `rotate(a)`, `rotate(a, v)`, `rotate(to = v)` | move, rotate (Euler, about an axis, z onto v) |
| `scale(v)`, `resize(v)` (acts like `scale`), `mirror(v)`, `multmatrix(m)` | as OpenSCAD |
| `color(c, alpha, filament = n)` | print colour and slot (`mcad usage colors`) |

## Booleans and blends

| Call | Effect | Topic |
| --- | --- | --- |
| `union()`, `difference()`, `intersection()` | Booleans; each takes `fillet = r` or `chamfer = d` for the edges it creates | fillets |
| `hull()`, `minkowski()` | exact special cases only, else warning | language |
| `fillet(r, select) { body; zone…; }` | round the body's edges in the zones; `r` may be `function (p) …` | fillets |
| `chamfer(d, select) { body; zone…; }` | chamfer, same selection | fillets |
| `shell(t) { body; zone…; }` | hollow with wall t, faces in the zones removed | fillets |
| `zone(g[x, y, *], d)`, `zone(g.plane, d)`, any body | zone around a grid line / plane / volume | fillets |
| `$fillet = r`, `$chamfer = d` on a primitive | all its edges | fillets |
| `group()`, `render()` | grouping | language |
| `render { setting = v; light NAME { … } material SEL { … } }` | studio look of the file (no geometry) | render-block |

## Grids, anchors, parts

| Construct | Meaning | Topic |
| --- | --- | --- |
| `grid g { x: a = 0, b = w; y: …; z: …; }` | named planes; lists give indexed planes `key[i]` | grids |
| `g[a, b, c]`, `g[a, *, c]`, `g.top` | point, line, plane (number in expressions); entries may be numbers or `_` | grids |
| `anchor name = p, dir = v;` | named point of a module | grids |
| `name: module();`, `k[i]: module();` | named instance; top level of the main file: part | grids |
| `name.anchor`, `dir(name.anchor)` | anchor point / direction from outside | grids |
| `module ns { module m() … }`, `ns.m();`, `ns.f(x)`, `ns.CONST` | modules as namespaces | language |

## Checks and output

| Call | Effect | Topic |
| --- | --- | --- |
| `assert_keepout(clearance) …` | keep-out volume: must stay free, not exported | asserts |
| `assert_disjoint() { … }` | children must not intersect | asserts |
| `assert_intersect() { … }` | children must form one connected piece | asserts |
| `assert_touches(tol, contact) { … }` | children touch without overlap | asserts |
| `filament("c", n);` | slot for a colour from a parameter | colors |
| `echo(…)`, `assert(cond, msg)` | as OpenSCAD | language |
| modifiers `#` `%` `!` `*` | highlight, transparent, root, disable | language |

## Language and functions

| Kind | Names |
| --- | --- |
| statements | `x = …;`, `module m(a = 1) …`, `function f(x) = …;`, `for`, `if`/`else`, `let`, `each`, list comprehensions, `include <f>`, `use <f>`, `children()` |
| math | `abs sign sin cos tan asin acos atan atan2 floor ceil round ln log exp sqrt pow min max norm cross` |
| lists, strings | `len concat lookup search str chr ord rands` |
| types | `is_undef is_bool is_num is_string is_list is_function` |
| other | `version() version_num() dir(anchor)`; `$fn $fa $fs` only affect display (except `$fn` = 3…12 at a call: polygon) |

## CLI

| Command | Use |
| --- | --- |
| `mcad outline FILE` | modules, parameters, grids, anchors, parts, colours |
| `mcad check FILE` | diagnostics with line:column |
| `mcad eval FILE` / `mcad inspect FILE --line N` | tree and values / what one line produces |
| `mcad render FILE -o x.png` | image: views, contact sheet, overlays, section, X-ray (`mcad usage render`) |
| `mcad render FILE -o x.png --style studio` | realistic look: lights, soft shadows, ambient occlusion, ground, ray-traced reflections (`mcad usage render`) |
| `… --style studio --material default=chrome --ground-reflection 0.5` | material presets (plastic … chrome, stainless, titanium, gold, copper, brass), mirror image in the ground |
| `… --style studio --quality draft\|high`, `--environment soft` | faster / smoother reflections; light, low-contrast studio |
| `mcad measure FILE …` | distance, radius, angle, length, area, volume, section, wall, collisions (`mcad usage measure`) |
| `mcad set FILE --param m.p=v` | change a default value or literal |
| `mcad watch FILE` | re-evaluate on every change |
| `mcad export FILE -o x.step\|.stl\|.3mf` | export, `--all-parts`, `--part NAME` |
| `mcad ui status\|show\|screenshot` | the running desktop UI |
| `mcad daemon status\|stop\|serve` | background cache |
| `mcad usage [topic]` | this manual |

All commands: `--json`. Model commands (`check` … `export`, not `set`): `-D name=value`
(override parameters), `-m/--module NAME` (module context). Kernel: `--kernel smlib|solidcore` or
`MCAD_KERNEL` (default SMLib; solidcore needs `--features solidcore`).
