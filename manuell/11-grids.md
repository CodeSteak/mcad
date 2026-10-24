# Grids, anchors, parts

## Grids

```
grid g {
    x: left = 0, mid = w / 2, right = w;
    y: front = 0, back = d;
    z: bottom = 0, top = h;
}
```

- Local to the module (or file) that defines it; transforms before `grid` place it:
  `translate(g[right, front, bottom]) grid l { … }`.
- `g[x, y, z]` is a point; an entry may be a plane name, a number, or `_` (axis not given).
- `*` gives a line or plane: `g[right, *, top]` (line along y), `g.top` (plane; as a number
  in expressions).
- A list makes indexed planes: `x: hole = [for (i = [0:3]) 6 + i * 12];` → `g[hole[2], …]`.
- Grids are allowed in sketches (2 axes).
- `cad outline` shows each plane with its kind: literal, expression, reference, implicit.

## Anchors and named instances

```
module knob() { cylinder(h = 4, r = 5); anchor top = [0, 0, 4], dir = [0, 0, 1]; }

k: knob();
for (i = [0:2]) translate([i * 15, 0, 20]) row[i]: knob();
translate(k.top) rotate(to = dir(k.top)) cylinder(h = 5, r = 1);
translate(row[2].top) sphere(1);
```

- `k.top` is a point in the current coordinate system, `dir(k.top)` its direction.
- Loop instances need an integer index: `row[i]`.
- Named instances at the top level of the main file are **parts**: export names
  (`--part k`, `--all-parts`), `--clearance`, `--interference`, render labels.
- In a module context (`--module knob`), the module's own anchors are named without prefix.
