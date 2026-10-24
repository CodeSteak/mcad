# Colours and filaments

```
color("white") cube([40, 20, 2]);                                   // slot 1 (first seen)
color("red", filament = 3) translate([5, 5, 2]) cube([30, 10, 1]);  // explicit slot 3
module knob(c) color(c) cylinder(h = 2, r = 3);
filament("teal", 4);                                                // slot for a colour from a parameter
translate([10, 10, 3]) knob("teal");
```

- Palette (case-insensitive): `red, orange, yellow, lime, green, teal, cyan, blue, purple,
  magenta, pink, brown, black, gray/grey, silver, white`; `aqua` = cyan, `fuchsia` = magenta.
- Other names → warning and a stable random colour. `#rgb`, `#rrggbb`, `#rrggbbaa`,
  `[r, g, b, a]` (0–1) stay exact. `alpha` only affects display; `"transparent"`/`""` do
  not colour.
- Slots: `filament = n` on a fixed colour; `filament("c", n);` at top level of the main file
  for colours from parameters/loops; otherwise order of first appearance. Uncoloured material
  is slot 1. More than 16 slots: warning.
- Volume per colour: a coloured subtree owns its body; on overlap the later statement wins;
  subtracted material is removed from all colours.
- Check with `cad outline` (section "Farben") or `cad measure --colors`; see with
  `cad render`.
