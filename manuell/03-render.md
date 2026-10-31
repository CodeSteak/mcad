# mcad render

`mcad render FILE -o x.png [options]`

| Option | Effect |
| --- | --- |
| `--view iso\|front\|back\|top\|bottom\|left\|right` | default `iso` |
| `--views a,b,…` / `--views all` | contact sheet, same scale for all views (`all` = iso,front,top,right; default size 1600x1200 total) |
| `--azimuth A --elevation E` | free camera; azimuth 0 = from −y, 90 = from +x; elevation 90 = from above |
| `--zoom Z`, `--target x,y,z`, `--fit TARGET` | framing |
| `--perspective [--fov 35]` | default is orthographic |
| `--part NAME` | only these parts (repeatable) |
| `--line N`, `--node I`, `--only` | highlight nodes, rest dimmed (`--only`: hidden). Zones blue, subtracted bodies red, selected edges orange, failed edges red |
| `--show LIST` | `grids, anchors, handles, edges, labels, bbox, dims, asserts` or `all`; colours: `regions` (as printed, default with colours) / `faces` (faster) |
| `--section x=V\|y=V\|z=V`, `--section-plane px,py,pz,nx,ny,nz`, `--section-flip` | cut; removes the side the normal points to (`z=10` removes z > 10) |
| `--xray`, `--theme dark`, `--size WxH`, `--no-axes` | |
| `--background C` / `--background TOP,BOTTOM` | solid or vertical gradient; `#rrggbb`, `rrggbb` or a palette name (`mcad usage colors`) |
| `--style standard\|studio` | `studio`: physically based lights and materials, shadows, ambient occlusion, ground (table below); default `standard`, `studio` if the file has a render block (`mcad usage render-block`) |

- `--show grids` in file context shows only top-level grids; use `--module NAME`.
- Asserts (always drawn): `assert_keepout` volume translucent yellow, dashed outline on top;
  intersection bodies of violations red with their edges on top; contact faces/lines of
  `assert_touches` green; gaps and too small clearances as red lines with foot points. Labels
  ("intersection 100 mm³", "gap 0.2 mm", "2 mm < 3") for violations; `--show asserts` labels every
  pair and draws the wanted intersections of `assert_intersect` green ("contact 384 mm²",
  "intersection 306 mm³").
- Failed Booleans with a place (always drawn): where the operands touch tangentially, the
  contact edges red on top, a red dot and the label `contact l.15 + 16` (lines of the
  operands); note `n error place(s) marked red …`, JSON `overlays.failures`.
- In `iso`, cut faces of `y=` sections face away: add `--section-flip` or `--view back`.
- `--json` returns the camera per view (`scale_px_per_mm`) to map pixels to mm.

## Studio style (`--style studio`)

Physically based: directional lights (the first casts soft shadows on the model and the
ground), metallic-roughness materials (GGX, Smith, Schlick, multiple-scattering energy
compensation, anisotropic GGX for brushed metal, optional clear coat) that reflect a
procedural high-dynamic-range studio (dark walls, lit floor, rectangular softboxes; prefiltered
for the roughness), ray-traced reflections of the model in itself and in the ground (BVH over
the triangles), screen-space ambient occlusion, ground plane at the model's lowest z fading
into a gradient background, edges at 35 % opacity (`--edges off`: none), Khronos PBR Neutral
tone mapping. Generated in code (no textures, no image files), deterministic. Works with every
other option (views, contact sheet, perspective, section, colours/regions, `--theme`,
`--size`). Same settings in the UI (`mcad usage ui`) and in the file's `render { … }` block
(`mcad usage render-block`; options override it).

| Option | Default | Effect |
| --- | --- | --- |
| `--light AZ,EL` | `-40,50` | first light: azimuth relative to the camera (0 = from the camera, + = from the right, like a turntable: the light moves with the camera), elevation 1…90° above the xy plane |
| `--light-intensity F` | `1` | factor on all lights, 0…4 |
| `--ambient F` | `0.4` | share of the environment light, 0…1 (rest: the lights) |
| `--environment E` | `studio` | see the table below |
| `--material SEL=SPEC` | plastic | repeatable; SEL: colour (`red`, `#rrggbb`), `default` (uncoloured), `part:NAME` (wins over colours); SPEC: preset and/or `metalness=`, `roughness=`, `clearcoat=`, `anisotropy=` (0…1), e.g. `red=plastic_glossy`, `part:lid=chrome,roughness=0.2` |
| `--shadows on\|off` | `on` | shadow map of the opaque bodies |
| `--shadow-softness F` | `0.35` | 0 = hard … 1 = very soft (light radius 0…5°; edges sharper near contact) |
| `--ao on\|off` | `on` | ambient occlusion in corners, gaps, at the ground contact |
| `--ao-radius MM` | 5 % of the model diagonal | reach of the occlusion |
| `--ao-strength F` | `1` | 0…2 |
| `--ground on\|off` | `on` | ground plane, only seen from above (none in `front`/`bottom` …) |
| `--ground-color C` | light `#e6e8ec`, dark `#2c3038` | |
| `--ground-reflection F` | `0.2` | mirror image of the model in the ground, 0 (none) … 1 (strong); stronger at grazing angles, fades with the height above the ground |
| `--ground-roughness F` | `0.3` | blur of the ground reflection, 0 = sharp mirror … 1 = very blurred |
| `--reflections on\|off` | `on` | ray-traced reflections in metals with roughness < 0.6 and plastics < 0.35: the model reflects itself and the ground with its shadow (off: the environment only) |
| `--quality Q` | `normal` | `draft`: no rays for surfaces, one ray per ground sample (fast); `normal`: 4 rays on every 4th sample, smoothed; `high`: 8 rays on every sample (smoother, ~2–4× the ray time) |
| `--edges on\|off` | `on` | mesh edge lines of opaque bodies; dimmed/x-ray bodies and section contours keep theirs |
| background | light `#f8f9fb`→`#d9dde3`, dark `#2b3038`→`#111317` | `--background` overrides |

| Environment | Looks like |
| --- | --- |
| `studio` (default) | dark studio: near-black walls, a lit floor fading into a dark horizon, a large softbox above behind the model, a tall warm key strip left, a cool fill strip right, a dim card behind the camera, a bounce card low right; metals show crisp bright reflections against dark surroundings |
| `soft` | light grey studio with large soft boxes: gentle, low-contrast reflections (the old look) |
| `overcast` | sky gradient only: even light, no highlights |
| `none` | no reflections: hemisphere light only |

| Preset | metalness | roughness | other | Looks like |
| --- | --- | --- | --- | --- |
| `plastic` (default) | 0 | 0.45 | | matte injection-moulded part, soft highlights |
| `plastic_satin` | 0 | 0.3 | | satin plastic, visible softbox highlights |
| `plastic_glossy` | 0 | 0.12 | clear coat 0.6 | glossy plastic, crisp softbox reflections |
| `rubber` | 0 | 0.85 | | dull, almost no highlights |
| `ceramic` | 0 | 0.35 | clear coat 1 | glazed ceramic: soft body, sharp glaze reflections |
| `aluminium` | 1 | 0.2 | colour aluminium | satin aluminium, bright |
| `metal_brushed` | 1 | 0.22 | aluminium, anisotropy 0.8 | brushed/turned aluminium: highlights stretched across the brushing |
| `anodized` | 1 | 0.28 | colour of the body | coloured anodized metal |
| `chrome` | 1 | 0.04 | colour chrome | mirror: dark with sharp softbox reflections |
| `stainless` | 1 | 0.12 | colour stainless steel | polished steel, slightly darker than aluminium |
| `titanium` | 1 | 0.25 | colour titanium | warm grey satin metal |
| `gold`, `copper`, `brass` | 1 | 0.12, 0.18, 0.15 | their colour | polished coloured metals |

- Real metals (`aluminium`, `metal_brushed`, `chrome`, `stainless`, `titanium`, `gold`,
  `copper`, `brass`) have their own colour (measured reflectance); the body's colour only
  counts for `anodized`, plastics and values without a preset (`metalness=1` alone).
- Brushing (`anisotropy`) runs around the body's z axis (lathe-turned look): around walls, in
  circles on faces across the axis.
- The lights act like small round area lights (~10°) in reflections: soft discs on smooth
  metal, no pinpoints.
- Studio options without `--style studio` (and without a render block): exit 2
  (`--ao only applies with --style studio`). `--style standard` ignores the render block.
- Transparent bodies (dimmed rest, x-ray, alpha) and section caps cast no shadow and are not
  in reflections.
- Run time at 800x600 on 4 cores (mug example, `render_ms`): `standard` 1.1 s; studio `--quality
  draft` 2.8 s, `normal` 3.3 s (aluminium 3.7 s), `high` 4.7 s (aluminium 8 s);
  `--shadows off --ao off --quality draft` is cheapest.
- Part materials draw the bodies per part (colour regions, kernel Booleans: first call slower).
- JSON: `style`, `studio` (settings incl. `lights`, `environment`, `materials`,
  `ground_reflection`, `ground_roughness`, `reflections`, `quality`), `background` (hex list),
  `views[].studio` (`light_dir`, `light_dirs`, `ground_z`, `ao_radius_mm`, `shadow_map`, `rays`;
  `null` where off).

```sh
mcad render box.mcad -o /tmp/a.png --module enclosure --views all --show grids,anchors,labels
mcad render box.mcad -o /tmp/b.png --section z=10 --show dims
mcad render box.mcad -o /tmp/c.png --line 8 --azimuth 120 --elevation 30
mcad render box.mcad -o /tmp/d.png --part cover --xray
mcad render box.mcad -o /tmp/e.png --style studio --views all
mcad render box.mcad -o /tmp/f.png --style studio --light 60,30 --shadow-softness 0.8 --ao-strength 1.5
mcad render box.mcad -o /tmp/g.png --style studio --ground off --background '#ffffff,#c8ccd2'
mcad render box.mcad -o /tmp/h.png --style studio --edges off
mcad render box.mcad -o /tmp/i.png --style studio --material default=aluminium --material teal=plastic_glossy
mcad render box.mcad -o /tmp/j.png --style studio --material default=chrome --ground-reflection 0.5 --ground-roughness 0.1
mcad render box.mcad -o /tmp/k.png --style studio --environment soft --quality draft
```
