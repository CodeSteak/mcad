# Collision asserts

Checks after the evaluation; a violation is a **warning** at the statement (`mcad check` exit 0),
`mcad export` stops with **exit 4** unless `--ignore-asserts`.

```
assert_keepout(clearance = 0.5) translate([5, 5, 2]) import("pcb.stl");  // must stay free
assert_disjoint() { housing(); lid(); }                                  // no pair may intersect
assert_intersect() { holder(); plate(); }                                // must form one connected piece
assert_touches(tol = 0.001, contact = "face") { lid(); housing(); }      // touch without overlap
```

| Assert | Children | Violation (warning) |
| --- | --- | --- |
| `assert_keepout(clearance = 0)` | keep-out volume (B-rep or mesh): shown, **not** in the result, export, volume, colours, parts or Booleans | a real body (each solid of each part and of the material outside parts, else of the root) intersects it (volume > ε) or is closer than `clearance` |
| `assert_disjoint()` | normal geometry (like `group`) | a pair intersects (volume > ε) |
| `assert_intersect()` | normal geometry | the children do not form **one connected group** of intersecting bodies (chain a–b–c is fine; touching is no intersection); the closest pair between groups is reported |
| `assert_touches(tol = 0.001, contact = "face")` | normal geometry | per pair, in this order: gap > `tol` + mesh accuracy → "does not touch, gap X mm"; intersection > ε → "overlap (V mm³)"; `contact = "face"` and only line/point contact → "only with line contact/point contact (non-manifold, hard to print)". `contact = "any"` accepts every contact |

- Children are the direct child statements (a `for` loop counts as one child); asserts are groups
  and nest: `assert_touches() { assert_intersect() { g: housing(); h: holder(); } k: lid(); }`.
  Named instances inside stay parts.
- Keep-out bodies are found by face tags, labels are the part name or `line N: code`.
- Not checked while a handle is dragged (UI); results are cached by content (watch, daemon, UI
  recompute only changed pairs).

| Check | Method | Accuracy |
| --- | --- | --- |
| intersection, B-rep | kernel boolean per solid pair whose boxes meet, volume from mass properties | exact |
| intersection with a mesh (`import(…, as = "mesh")`) | mesh test (surfaces cross or inside), yes/no, no volume | the meshes |
| distance, gap, clearance | smallest triangle–triangle distance with foot points | ± 2 · mesh tol (planar faces exact) |
| contact | opposite coplanar planes → face contact, area = intersection of both outlines (`region2`, exact for lines/arcs); cylinder/sphere on plane, cylinder on cylinder → line/point; other surfaces on the mesh (`contact_exact: false`) | exact / mesh |

Tolerances (L = diagonal of the bounds of the pair): ε = 10⁻⁹·L³ mm³ (`eps_volume`), contact
area > 10⁻⁹·L² mm², mesh tol = clamp(2·10⁻⁴·L, 0.001, 0.02) mm. A failing boolean (faces lying
on each other are a SMLib borderline case) is an info "overlap could not be checked", no error.

| Command | Behaviour |
| --- | --- |
| `mcad check` | warnings `assert_touches: a and b overlap (…)`, line `Asserts: 4 of 4 checked, 1 violated`; JSON `asserts`, `violations` |
| `mcad export` | violation → exit 4, nothing written, list of violations; `--ignore-asserts` writes anyway (warning line); keep-out volumes never exported |
| `mcad render` | keep-out translucent yellow with dashed outline on top; intersection bodies red (edges on top), contact faces/lines green, gaps and clearances as lines with foot points; labels for violations, `--show asserts` labels every pair |
| `mcad outline` / `measure` | section `Asserts` (`line 21 assert_touches(tol = 0.001, contact = "face"): ok – g + holder + guide touches k: face contact 384 mm²`); JSON `asserts` |
| `mcad inspect --line N` | node `[keep-out volume: not in the result]`, each pair `ok:` / `VIOLATED:`; JSON `nodes[].assert` |

```sh
mcad check tests/models/spec/asserts.mcad          # Asserts: 4 of 4 checked, 0 violated
# min_gap = 3 and a taller holder:
#   asserts.mcad:21:1: warning (kernel): assert_touches: g + holder + guide and k overlap (intersection volume 100 mm³)
#   asserts.mcad:33:1: warning (kernel): assert_keepout: g violates the clearance 3 mm to the keep-out volume: 2 mm (± 0.0305) from [42.4444, 12, 2] to [42.4444, 12, 4] [line 14: box (instance g: housing)]
mcad export bad.mcad -o x.3mf                  # export stopped: 2 assert violation(s), nothing written: … exit 4
mcad export bad.mcad -o x.3mf --ignore-asserts # warning: exported despite 2 assert violation(s) (--ignore-asserts)
mcad render tests/models/spec/asserts.mcad -o /tmp/a.png --show asserts
```

Example model: `tests/models/spec/asserts.mcad` (housing, lid, holder, cable clip, STL keep-out).
