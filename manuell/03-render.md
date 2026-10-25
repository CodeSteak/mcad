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

- `--show grids` in file context shows only top-level grids; use `--module NAME`.
- Asserts (always drawn): `assert_keepout` volume translucent yellow, dashed outline on top;
  intersection bodies of violations red with their edges on top; contact faces/lines of
  `assert_touches` green; gaps and too small clearances as red lines with foot points. Labels
  ("Schnitt 100 mm³", "Spalt 0.2 mm", "2 mm < 3") for violations; `--show asserts` labels every
  pair and draws the wanted intersections of `assert_intersect` green ("Kontakt 384 mm²",
  "Schnitt 306 mm³").
- Failed Booleans with a place (always drawn): where the operands touch tangentially, the
  contact edges red on top, a red dot and the label `Berührung Z.15 + 16` (lines of the
  operands); note `n Fehlerstelle(n) rot markiert …`, JSON `overlays.failures`.
- In `iso`, cut faces of `y=` sections face away: add `--section-flip` or `--view back`.
- `--json` returns the camera per view (`scale_px_per_mm`) to map pixels to mm.

```sh
mcad render box.mcad -o /tmp/a.png --module enclosure --views all --show grids,anchors,labels
mcad render box.mcad -o /tmp/b.png --section z=10 --show dims
mcad render box.mcad -o /tmp/c.png --line 8 --azimuth 120 --elevation 30
mcad render box.mcad -o /tmp/d.png --part cover --xray
```
