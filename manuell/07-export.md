# cad export

`cad export FILE -o OUT` – format from the extension (`.stl`, `.3mf`, `.step`/`.stp`) or
`--format`.

| Variant | Result |
| --- | --- |
| `-o part.step [--schema ap214]` | exact B-rep, AP242 default, face colours from colour regions; STL meshes facetted (one plane per face) |
| `-o part.stl` | binary STL |
| `-o part.3mf` | multicolour: one object per part (material outside parts: object "ohne Bauteil"), one sub-part per colour with filament slot; Bambu Studio/OrcaSlicer take the slots without a colour dialog |
| `-o part.3mf --single-color` | one plain mesh |
| `-o part.3mf --color-group` | adds `m:colorgroup` for other programs (Bambu then asks; "Cancel" keeps slots) |
| `--per-color -o colours.zip` | one STL per colour (and part), `F<slot>_<colour>.stl`, shared coordinates |
| `--part NAME` (repeatable) | only these parts, in one file |
| `--all-parts -o dir/ [--format step]` | one file per part (`body.stl`, `k_2.step`; default stl) |

Check colours before a multicolour print: `cad measure FILE --colors`.
