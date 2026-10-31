# render block

`render { … }` at the top level of the main file stores the studio look of the model: lights,
materials, environment, shadows, ground, background, edges. `mcad render` uses it (a block makes
`--style studio` the default), options override single values, the UI shows and edits it
(`mcad usage ui`). It never changes the geometry.

```
render {
    edges = false;
    ambient = 0.45;
    light key { azimuth = -35; elevation = 45; }
    light rim { azimuth = 150; elevation = 25; intensity = 0.6; color = "#dfe8ff"; }
    ground_reflection = 0.3;
    material "silver" { preset = "metal_brushed"; }
    material "red" { preset = "plastic_glossy"; }
    material lid { preset = "rubber"; }
}
```

| Setting | Values (default) | CLI |
| --- | --- | --- |
| `style` | `"studio"` (default with a block), `"standard"` | `--style` |
| `edges` | `true`/`false` (`true`) | `--edges` |
| `environment` | `"studio"`, `"soft"`, `"overcast"`, `"none"` (`"studio"`) | `--environment` |
| `ambient` | 0…1 (0.4): environment share, rest the lights | `--ambient` |
| `light_intensity` | 0…4 (1): factor on all lights | `--light-intensity` |
| `shadows`, `shadow_softness` | bool (`true`), 0…1 (0.35) | `--shadows`, `--shadow-softness` |
| `ao`, `ao_radius`, `ao_strength` | bool (`true`), mm > 0 (5 % of the diagonal), 0…2 (1) | `--ao`, `--ao-radius`, `--ao-strength` |
| `ground`, `ground_color` | bool (`true`), colour (theme) | `--ground`, `--ground-color` |
| `ground_reflection`, `ground_roughness` | 0…1 (0.2): mirror image in the ground, 0…1 (0.3): its blur | `--ground-reflection`, `--ground-roughness` |
| `reflections` | bool (`true`): ray-traced reflections of the model (`mcad render`; the UI shows environment and ground reflections only) | `--reflections` |
| `background` | colour or `[top, bottom]` (theme gradient) | `--background` |
| `light NAME { azimuth; elevation; intensity; color; }` | −360…360° (−40), 1…90° (50), 0…4 (1), colour (white) | `--light AZ,EL` (first light) |
| `material SEL { preset; metalness; roughness; clearcoat; anisotropy; }` | preset (`mcad usage render`), 0…1 each | `--material SEL=SPEC` |

- Colours: palette name, `"#rrggbb"` or `[r, g, b]` (0…1).
- Lights: in order; the first casts the shadows; none given = the default key light (−40°,
  50°). Azimuth relative to the camera (0 = from the camera, + = from the right).
- `material "red"`: the material of that colour (palette name or hex as in `color()`,
  `mcad usage colors`); `material "default"`: uncoloured material; `material lid`: the part
  `lid` (`feet` also names `feet[0]` …). Part entries win over colour entries; unset values
  come from the preset, else plastic (metalness 0, roughness 0.45, clearcoat 0, anisotropy 0).
  Real metal presets bring their own colour; `anodized` and values without a preset use the
  body's colour.
- Values may be expressions with the file's variables (`azimuth = key_az;`); the UI writes
  numbers back (an expression it changes becomes a literal).
- Order of precedence: command line > render block > defaults (theme colours for ground and
  background). `--style standard` ignores the block.
- Only the first block at the top level of the main file applies, also with `--module`.
- `--quality` is a command-line option only (ray tracing effort of `mcad render`).
- Unknown names and bad values: warning with the line, the value is ignored
  (`mcad usage troubleshooting`); `mcad check` stays at 0 errors.
- `mcad outline`: section "Render block (line N)", JSON `render` (`mcad usage json`).

Writing the block: the UI's studio panel ("Studio settings") edits it in the editor (one undo
step per change, saved with the file; panel note "Saved in the file: render block, line N"),
`mcad set FILE --render key=value` does the same from the shell (`mcad usage set`): changed
values in place, new settings after the last of their kind (one-line blocks stay on one line),
removed ones with their line, a new block at the end of the file. Comments and formatting
elsewhere stay.

```sh
mcad render examples/09-flashlight.mcad -o /tmp/torch.png
mcad render examples/09-flashlight.mcad -o /tmp/torch2.png --light 60,30 --material silver=metal_brushed
mcad outline examples/09-flashlight.mcad
mcad set examples/09-flashlight.mcad --render light.rim.intensity=0.8 --dry-run
```
