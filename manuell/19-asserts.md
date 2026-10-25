# Collision asserts

Checks after the evaluation; a violation is a **warning** at the statement (`cad check` exit 0),
`cad export` stops with **exit 4** unless `--ignore-asserts`.

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
| `assert_touches(tol = 0.001, contact = "face")` | normal geometry | per pair, in this order: gap > `tol` + mesh accuracy → "berührt nicht, Spalt X mm"; intersection > ε → "überschneiden sich (V mm³)"; `contact = "face"` and only line/point contact → "nur mit Linienkontakt/Punktkontakt (nicht-mannigfaltig, schlecht druckbar)". `contact = "any"` accepts every contact |

- Children are the direct child statements (a `for` loop counts as one child); asserts are groups
  and nest: `assert_touches() { assert_intersect() { g: housing(); h: holder(); } k: lid(); }`.
  Named instances inside stay parts.
- Keep-out bodies are found by face tags, labels are the part name or `Zeile N: code`.
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
on each other are a SMLib borderline case) is an info "Überlappung nicht prüfbar", no error.

| Command | Behaviour |
| --- | --- |
| `cad check` | warnings `assert_touches: a und b überschneiden sich (…)`, line `Asserts: 4 von 4 geprüft, 1 verletzt`; JSON `asserts`, `violations` |
| `cad export` | violation → exit 4, nothing written, list of violations; `--ignore-asserts` writes anyway (warning line); keep-out volumes never exported |
| `cad render` | keep-out translucent yellow with dashed outline on top; intersection bodies red (edges on top), contact faces/lines green, gaps and clearances as lines with foot points; labels for violations, `--show asserts` labels every pair |
| `cad outline` / `measure` | section `Asserts` (`Z.21 assert_touches(tol = 0.001, contact = "face"): ok – g berührt k: Flächenkontakt 384 mm²`); JSON `asserts` |
| `cad inspect --line N` | node `[Sperrvolumen: nicht im Ergebnis]`, each pair `ok:` / `VERLETZT:`; JSON `nodes[].assert` |

```sh
cad check examples/spec/asserts.scad          # Asserts: 4 von 4 geprüft, 0 verletzt
# luft = 3 and a taller holder:
#   asserts.scad:21:1: Warnung (Kernel): assert_touches: g + hl + kf und k überschneiden sich (Schnittvolumen 100 mm³)
#   asserts.scad:33:1: Warnung (Kernel): assert_keepout: g hält den Abstand 3 mm zum Sperrvolumen nicht ein: 2 mm (± 0.0305) von [42.4444, 12, 2] bis [42.4444, 12, 4] [Zeile 14: box (Instanz g: gehaeuse)]
cad export bad.scad -o x.3mf                  # Export abgebrochen: 2 Assert-Verletzung(en), nichts geschrieben … exit 4
cad export bad.scad -o x.3mf --ignore-asserts # Warnung: trotz 2 Assert-Verletzung(en) exportiert
cad render examples/spec/asserts.scad -o /tmp/a.png --show asserts
```

Example model: `examples/spec/asserts.scad` (housing, lid, holder, cable clip, STL keep-out).
