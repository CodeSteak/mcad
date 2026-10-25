# cad measure

`cad measure FILE [options]` – exact, on the B-rep. Options combine and repeat.

| Option | Result |
| --- | --- |
| `--between A B` | distance + delta of two points |
| `--bounds/--volume/--area [TARGET]` | bounds, volume mm³, area mm² (default `all`) |
| `--colors` | volume and PLA mass per colour/filament slot and part |
| `--interference [--tolerance 0.001]` | pairwise intersection volume of all parts (mm³); a part with an STL mesh: mesh test, `volume: null`, `method: "mesh"` |
| `--clearance A B [--mesh-tol 0.01]` | smallest gap of two parts (mesh-based, ± 2·mesh-tol; 0 + `collision` if overlapping; with an STL mesh `intersection_volume: null`) |
| `--distance A B` | smallest distance with foot points; parallel planes: normal distance; parallel axes: axis distance |
| `--radius REF` | radius, diameter, centre, axis of a circular edge or round face (cylinder, cone, sphere, torus, fillet) |
| `--angle A B` | angle of edges/faces; planes 0–90° plus `opening` angle |
| `--length REF`, `--body REF` | edge length; volume/area/centroid/bounds of the body |
| `--at P` | elements near P with their references – use it to find references |
| `--section z=V` (`--section-plane`, `--section-flip`) | section regions, holes, contours, circles, min wall |
| `--wall P` | wall thickness at the contour point nearest P (needs `--section`) |

**Element references** pick the element nearest point P: `vertex@P`, `mid@P` (edge
midpoint), `center@P` (circle centre), `edge@P`, `face@P`, `body@P`. With `--section`, they
refer to the section contour, plus `region@P`; `face@`/`body@` are unavailable.

Each result names its origin (`Zeile 11: box (Instanz body: enclosure)`, `Rasterebene
g.inner_l`) and, in JSON, `cli` – the arguments that repeat it.

Wrong element type (e.g. `--radius` on a straight edge) or unknown anchor → exit 2.

```sh
cad measure box.scad --between body.corner body.rim                   # 67.082 mm
cad measure box.scad --module enclosure --between "g[left,front,bottom]" "g[right,back,top]"
cad measure box.scad --distance "face@[3,15,10]" "face@[57,15,10]"    # 54 mm inner width
cad measure box.scad --radius "face@[30,0.3,19.7]"                    # fillet R 1
cad measure box.scad --section z=10 --wall "[0,15,10]"                # 3 mm
cad measure box.scad --interference --clearance body cover
```

(`box.scad`: see `cad usage language`.)
