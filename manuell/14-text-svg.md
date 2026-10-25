# Text and SVG

```
linear_extrude(1.2) text("Label", size = 11, font = "Inter:style=Bold", halign = "center", valign = "center");
linear_extrude(1) text("20 mm", font = "fonts/my.ttf");     // font file relative to the .scad
linear_extrude(2) import("logo.svg", center = true);         // also id = "…", scale, dpi = 96
// import("x.stl") (meshes): cad usage import-stl
```

- `text()` as in OpenSCAD (`size`, `spacing`, `direction`, `halign`, `valign`, `language`,
  `script`; metrics match OpenSCAD 2021.01).
- Default font `"cad stencil mono"`: monospaced stencil, no glyph has a hole;
  `stroke_width`, `bridge_width` in mm (`bridge_width = 0`: no bridges). Bundled: JetBrains
  Mono, Inter (Regular/Bold), Allerta Stencil. For OpenSCAD-identical results name a TTF font.
- Unknown font → warning + default font; missing glyphs come from JetBrains Mono.
- SVG: paths, rect, circle/ellipse (exact), polygon, polyline, groups with `transform`,
  `viewBox`/units, `fill-rule`; origin bottom left. Text, images, strokes, CSS classes are
  not supported (warning with the SVG line).
- File names must appear as string literals in the project (also as parameter defaults).
- Text unioned onto a plate is slow (28 glyphs ≈ 9 s); keep text a separate part or use
  `--no-kernel` while iterating.
