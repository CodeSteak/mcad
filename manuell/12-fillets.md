# Fillets, chamfers, shells

| Form | Edges affected |
| --- | --- |
| `difference(fillet = 1) { … }`, `difference(chamfer = 1) { … }` | edges created by the subtraction |
| `cube(20, $fillet = 1)` | all edges of that primitive |
| `fillet(r) { solid; zone…; }` | edges of the first child lying in a zone |
| `chamfer(d) { solid; zone…; }` | same, chamfer |
| `shell(t) { solid; zone…; }` | hollow with wall t, faces in zones are removed (openings) |

Zones (children 2…n):

```
fillet(2) {
    box(g[left, front, bottom], g[right, back, top]);
    zone(g[right, *, top], d = 2);         // cylinder around a grid line
    zone(g.bottom, d = 1);                 // slab around a grid plane
    translate([15, 10, 15]) sphere(6);     // any volume
}
fillet(1.5, select = "touch") { … }        // edges touching a zone (default "inside": fully inside)
fillet(r = function (p) p.z < 10 ? 2 : 4) { … }   // variable radius over position p
```

- Variable radius: pure expressions of `p` (no geometry); not for `chamfer` and `$fillet`.
  Hint "r 2–4".
- Seam/pole edges of cylinders and spheres and tangent edges never count.
- Empty selection → warning "0 Kanten"; radius too large → kernel error, body stays
  unfilleted.
- Check: `mcad inspect FILE --line N` (hint "4 Kanten"), `mcad render FILE --line N` (zones
  blue, selected edges orange).
