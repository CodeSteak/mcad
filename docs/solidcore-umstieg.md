# Umstieg auf solidcore (Plan 2.3, M8)

Stand: 2026-10-23 · solidcore `main` @ `3a6a50b` (tangency-labs/solidcore, Apache-2.0)

## Stand

- **Crate `kernel-solidcore`** implementiert `cad_kernel::Kernel` über die öffentliche API von
  solidcore. Die Abhängigkeit ist ein git-Eintrag mit festem Commit
  (`crates/kernel-solidcore/Cargo.toml`, Konstante `SOLIDCORE_REV`).
- **Toolchain:** solidcore pinnt in seinem eigenen Repository Rust 1.98
  (`rust-toolchain.toml`); diese Datei gilt nicht für abhängige Crates. Mit unserem Stand
  (Rust 1.97) baut solidcore ohne Änderung (Release ca. 3 min, Debug mit `opt-level = 2` für
  Abhängigkeiten ca. 6 min, einmalig).
- **Feature `solidcore`** (Standard in `kernel-solidcore` und `cad-cli`). Ohne das Feature
  (`--no-default-features`) wird solidcore nicht kompiliert; jede Operation meldet dann
  `Unsupported("solidcore: … (built without feature `solidcore`)")`. Der git-Eintrag wird
  für `Cargo.lock` trotzdem aufgelöst (einmal Netz nötig).
- **Was heute geht:** nichts Durchgängiges. Alle Primitive (`make_box`, `make_sphere`,
  `make_cylinder`, Kegel, `make_torus`) brechen in solidcore mit
  `SolidError::Unported` ab: `SmSurface::DropCurve` (Unit `intersect.surface_drop_lib_p3`)
  ist nicht portiert. Damit sind auch Booleans, Abfragen und Massen nicht prüfbar. Das deckt
  sich mit dem README von solidcore („Nothing is usable end to end yet").

| Kernel-Operation | Weg in `kernel-solidcore` | Ergebnis heute |
| --- | --- | --- |
| `make_box`, `make_sphere`, `make_cylinder` (Zylinder/Kegel), `make_torus` | `primitive::api_primitives::create_*` | `Unsupported` (`SmSurface::DropCurve` nicht portiert) |
| Kegel mit Spitze unten | bräuchte eine B-Rep-Transformation | `Unsupported` |
| `boolean` | `boolean::api_brep::boolean` | nicht erreichbar (keine Operanden) |
| `faces`, `edges`, `bounds`, `is_solid`, `is_empty` | `query::api_queries` | nicht erreichbar |
| `mass_props` | `mass::integrator::properties::brep_mass_properties`, `brep_area` | nicht erreichbar |
| `make_polyhedron`, `transform`, `fillet`, `chamfer`, `extrude`, `revolve`, `loft`, `sweep`, `nurbs_surface`, `thicken`, `shell`, `classify`, `tessellate`, `brep` | keine öffentliche Einstiegsstelle | `Unsupported` |

Fehlerabbildung: `SolidError::Unported` → `KernelError::Unsupported("solidcore: <op>: …")`,
`SolidError::InvalidInput` → `InvalidInput`, sonst `Failed`. Flächen tragen vorerst den Tag der
erzeugenden Operation (Attribut-Weitergabe für Provenienz ist noch nicht verdrahtet).

## Was fehlt (in solidcore bzw. hier)

1. Primitive end to end (`DropCurve`, danach die B-Rep-Konstruktion der Primitive).
2. Booleans (`SmMerge` ist teilweise portiert, die API endet heute an nicht portierten Units).
3. Transformation eines B-Reps, Polyeder (`CreateFromPolyBrep`), Fillet/Fase, Skins
   (Extrusion, Rotation, Loft, Sweep), Offset (Shell, Thicken), Tessellierung, Punktklassifikation.
4. Hier: Attribute für Face-Tags (Provenienz), `Kernel::brep` für den STEP-Export, Wasm-Build
   im selben Modul (Stack 8 MiB, siehe solidcore-README).

## Abnahme

`cad diff-kernels <dateien…> [--json]` (`crates/cad-cli/src/diff.rs`) wertet jede Datei mit
SMLib und mit solidcore aus und vergleicht jeden von SMLib ausgewerteten Knoten:

- Status, Volumen (relativ 1e-6), Anzahl Flächen und Kanten des Knotenergebnisses,
  ausgewählte Kanten (Fillet/Fase: Anzahl und Endpunkte, 1e-6 der Modellgröße);
- Urteil je Knoten: `same`, `unsupported` (solidcore meldet die Operation als nicht
  unterstützt), `blocked` (weicht nur ab, weil ein Eingangsknoten unsupported ist; bei `zone`
  der Geschwisterkörper), `differ`, `failed` (solidcore scheitert, SMLib nicht), `skipped`;
- „Abdeckung": je Operation (Knotenlabel) gleich / Knoten, unsupported, blockiert, abweichend.

Exit-Code 0 nur, wenn alle Knoten aller Dateien `same` sind. Umgestellt wird erst, wenn
`diff-kernels` über alle Beispiele (`examples/openscad-compat`, `examples/spec`) mit 0 endet;
zusätzlich müssen `tests/openscad-compat/compare.py` und `tests/step/verify.py` mit solidcore
weiter bestehen (dafür braucht `kernel-solidcore` noch `tessellate` und `brep`).

Heutiger Lauf über die 46 Beispiele (Testbericht 5): 855 Knoten, 101 gleich (2D-Knoten ohne
Kernel), 267 unsupported, 487 blockiert, 0 abweichend, 0 gescheitert. Der Test
`diff_kernels_on_examples` (`crates/cad-cli/tests/cli.rs`) prüft, dass der Vergleich über alle
Beispiele ohne Absturz durchläuft.
