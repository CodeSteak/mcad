# mcad measure

`mcad measure FILE [options]` – exact, on the B-rep. Options combine and repeat.

| Option | Result |
| --- | --- |
| `--between A B` | distance + delta of two points; JSON `kind` per point: `anchor`, `grid_point`, `coordinate` |
| `--bounds/--volume/--area [TARGET]` | bounds, volume mm³, area mm² (default `all`) |
| `--colors` | volume and PLA mass per colour/filament slot and part |
| `--interference [--tolerance 0.001]` | pairwise intersection volume of all parts (mm³); a part with an STL mesh: mesh test, `volume: null`, `method: "mesh"` |
| `--clearance A B [--mesh-tol 0.01]` | smallest gap of two parts (mesh-based, ± 2·mesh-tol; 0 + `collision` if overlapping; with an STL mesh `intersection_volume: null`) |
| `--distance A B` | smallest distance with foot points; parallel planes: normal distance; parallel axes: axis distance |
| `--radius REF` | radius, diameter, centre, axis of a circular edge or round face (cylinder, cone, sphere, torus, fillet) |
| `--angle A B` | angle of edges/faces; planes 0–90° plus `opening` angle |
| `--length REF`, `--body REF` | edge length; volume/area/centroid/bounds of the body |
| `--at P` | elements near P with their references – use it to find references |
| `--section z=V` (`--section-plane`, `--section-flip`) | section regions, holes, contours, circles, min wall; JSON segment `source` = cut surface: `plane`, `cylinder`, `cone`, `sphere`, `torus`, `revolution`, `extrusion`, `bspline` |
| `--wall P` | wall thickness at the contour point nearest P (needs `--section`) |

**Element references** pick the element nearest point P: `vertex@P`, `mid@P` (edge
midpoint), `center@P` (circle centre), `edge@P`, `face@P`, `body@P`. With `--section`, they
refer to the section contour, plus `region@P`; `face@`/`body@` are unavailable.

Each result names its origin (`line 10: box (instance body: enclosure)`, `grid plane
g.inner_l`) and, in JSON, `cli` – the arguments that repeat it.

Wrong element type (e.g. `--radius` on a straight edge) or unknown anchor → exit 2.

```sh
mcad measure box.mcad --between body.corner body.rim                   # 67.082 mm
mcad measure box.mcad --module enclosure --between "g[left,front,bottom]" "g[right,back,top]"
mcad measure box.mcad --distance "face@[3,15,10]" "face@[57,15,10]"    # 54 mm inner width
mcad measure box.mcad --radius "face@[30,0.3,19.7]"                    # fillet R 1
mcad measure box.mcad --section z=10 --wall "[0,15,10]"                # 3 mm
mcad measure box.mcad --interference --clearance body cover
```

(`box.mcad`: see `mcad usage language`.)
