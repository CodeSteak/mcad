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
| `--style standard\|studio` | `studio`: realistic lighting, shadows, ambient occlusion, ground (table below) |

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
ground), metallic-roughness materials (GGX, Smith, Schlick; optional clear coat) that reflect a
procedural studio environment (sky gradient, floor bounce, softboxes — spherical Gaussians,
integrated in closed form), screen-space ambient occlusion, ground plane at the model's lowest z
fading into a gradient background, edges at 35 % opacity (`--edges off`: none), Khronos PBR
Neutral tone mapping. Analytic only (no textures, no image files). Works with every other
option (views, contact sheet, perspective, section, colours/regions, `--theme`, `--size`). Same
settings in the UI (`mcad usage ui`).

| Option | Default | Effect |
| --- | --- | --- |
| `--light AZ,EL` | `-40,50` | first light: azimuth relative to the camera (0 = from the camera, + = from the right, like a turntable: the light moves with the camera), elevation 1…90° above the xy plane |
| `--light-intensity F` | `1` | factor on all lights, 0…4 |
| `--ambient F` | `0.4` | share of the environment light, 0…1 (rest: the lights) |
| `--environment E` | `studio` | `studio` (softboxes: crisp reflections), `overcast` (soft sky), `none` (no reflections, hemisphere light) |
| `--material SEL=SPEC` | plastic | repeatable; SEL: colour (`red`, `#rrggbb`), `default` (uncoloured), `part:NAME` (wins over colours); SPEC: preset and/or `metalness=`, `roughness=`, `clearcoat=` (0…1), e.g. `red=plastic_glossy`, `part:lid=chrome,roughness=0.2` |
| `--shadows on\|off` | `on` | shadow map of the opaque bodies |
| `--shadow-softness F` | `0.35` | 0 = hard … 1 = very soft (light radius 0…5°; edges sharper near contact) |
| `--ao on\|off` | `on` | ambient occlusion in corners, gaps, at the ground contact |
| `--ao-radius MM` | 5 % of the model diagonal | reach of the occlusion |
| `--ao-strength F` | `1` | 0…2 |
| `--ground on\|off` | `on` | ground plane, only seen from above (none in `front`/`bottom` …) |
| `--ground-color C` | light `#e6e8ec`, dark `#2c3038` | |
| `--edges on\|off` | `on` | mesh edge lines of opaque bodies; dimmed/x-ray bodies and section contours keep theirs |
| background | light `#f8f9fb`→`#d9dde3`, dark `#2b3038`→`#111317` | `--background` overrides |

| Preset | metalness | roughness | clearcoat |
| --- | --- | --- | --- |
| `plastic` (default) | 0 | 0.5 | 0 |
| `plastic_glossy` | 0 | 0.2 | 0.5 |
| `rubber` | 0 | 0.9 | 0 |
| `aluminium` | 1 | 0.35 | 0 |
| `metal_brushed` | 1 | 0.55 | 0 |
| `chrome` | 1 | 0.08 | 0 |

- Metal reflections take the body's colour (`silver` + `aluminium` = aluminium); plastics
  reflect white.
- Studio options without `--style studio`: exit 2 (`--ao only applies with --style studio`).
- Transparent bodies (dimmed rest, x-ray, alpha) and section caps cast no shadow.
- Run time: about +1 s at 1024x768 on 4 cores (simple models 2–6× `standard`); `--shadows off --ao off` is cheapest.
- Part materials draw the bodies per part (colour regions, kernel Booleans: first call slower).
- JSON: `style`, `studio` (settings incl. `lights`, `environment`, `materials`), `background`
  (hex list), `views[].studio` (`light_dir`, `light_dirs`, `ground_z`, `ao_radius_mm`,
  `shadow_map`; `null` where off).

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
```
