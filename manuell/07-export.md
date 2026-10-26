# mcad export

`mcad export FILE -o OUT` – format from the extension (`.stl`, `.3mf`, `.step`/`.stp`) or
`--format`.

| Variant | Result |
| --- | --- |
| `-o part.step [--schema ap214]` | exact B-rep, AP242 default, face colours from colour regions; STL meshes facetted (one plane per face). One named body (`MANIFOLD_SOLID_BREP`) per part and per top-level object outside the parts (named after the object: module name, `box`, `box 2`, …; without parts and with only one object: one body as before) – touching parts stay apart |
| `-o part.step --merge` | the whole result as one fused body (implicit union of the top level) |
| `-o part.stl` | binary STL |
| `-o part.3mf` | multicolour: one object per part (material outside parts: object "no part", a fixed name in every language), one sub-part per colour with filament slot; Bambu Studio/OrcaSlicer take the slots without a colour dialog |
| `-o part.3mf --single-color` | one plain mesh |
| `-o part.3mf --color-group` | adds `m:colorgroup` for other programs (Bambu then asks; "Cancel" keeps slots) |
| `--per-color -o colours.zip` | one STL per colour (and part), `F<slot>_<colour>.stl`, shared coordinates; uncoloured material: colour label `default` (`F1_default.stl`, 3MF sub-part `<object> – default`) in every language |
| `--part NAME` (repeatable) | only these parts, in one file |
| `--all-parts -o dir/ [--format step]` | one file per part (`body.stl`, `k_2.step`; default stl) |
| `-m NAME [-D p=v]`, `-m 'NAME(args)'` | only that module, with its defaults and the given parameters (`-m 'holder(30, h = 5)'`, `-m 'parts.screw(d = 3)'`; `mcad usage commands`) |
| `--ignore-asserts` | export although an `assert_…` is violated (warning line `exported despite n assert violation(s)`) |

Separate bodies (STEP): objects that only touch or lie apart stay separate solids (contact faces
kept); objects that overlap are fused into one solid named `a + b` – the export says `note:
bodies a, b overlap: written as one body “a + b” …` (JSON `notes`). Overlap is
the exact intersection volume (as `assert_disjoint`); if that boolean or the union fails, the
bodies are written apart with a note. STL is always one mesh of the fused result; 3MF keeps
parts as objects (the material outside the parts is one object "no part"), objects outside
the parts are fused.

```sh
mcad export pair.mcad -o pair.step   # a: block();  translate([10, 0, 0]) b: block();  (touching cubes)
#   pair.step: 2 bodies (a, b), 12 faces, 24 edges, …             before: 1 body (fused)
```

Violated asserts (`mcad usage asserts`) stop every variant: exit 4, nothing written, the
violations listed (`export stopped: 2 assert violation(s), nothing written:`); JSON
`aborted: true, violations, asserts`. `assert_keepout` volumes are never exported.

Check colours before a multicolour print: `mcad measure FILE --colors`.

```sh
mcad export holder.mcad -m 'holder(30, h = 5)' -o holder.stl   # module holder(w = 10, h = 2, d = 5) cube([w, d, h]);
#   holder.stl: 12 triangles, 684 bytes, volume 750 mm³
mcad export holder.mcad -m holder -D w=30 -o holder.stl        # volume 300 mm³
```
