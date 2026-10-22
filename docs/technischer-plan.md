# Technischer Plan: Parametrische CAD-Sprache mit Rastern

Stand: 2026-10-22 · Grundlage: `Spec: Parametrische CAD-Sprache mit Rastern` (22.10.2026)

Ziel: Die Anwendung läuft vollständig im Browser. Sprache, Auswertung, Cache und Rückschreiben sind in Rust (Wasm). Die Geometrie rechnet OpenCascade (OCCT), ebenfalls als Wasm. Die UI ist TypeScript/React.

---

## 1. Architektur

```
┌──────────────────────────── Main Thread (React/TS) ────────────────────────────┐
│  Editor (CodeMirror 6)      3D-Ansicht (three.js)      Parameterleiste, Pfadleiste │
│        │  ▲                        │  ▲                                          │
│        │  │ Diagnosen, Hinweise,   │  │ Szene (Knoten, Meshes, Griffe, Raster)  │
│        │  │ TextEdits              │  │                                          │
│        ▼  │                        ▼  │                                          │
│  ───────────────── Worker-Protokoll (postMessage, Transferables) ─────────────── │
└──────────────────────────────────────┬─────────────────────────────────────────┘
                                       │
┌──────────────────────────── Web Worker ────────────────────────────────────────┐
│  cad-wasm (Rust, wasm32-unknown-unknown, wasm-bindgen)                          │
│    Parser → Auswertung → Geometriebaum (IR) → Engine (Cache, Zonen, Provenienz) │
│                                                   │ trait Kernel                 │
│                                                   ▼                              │
│                                   kernel-occt: Aufrufe über JS-Imports           │
│                                                   │                              │
│  occt.wasm (C++, Emscripten): eigene Fassade über OCCT, C-ABI, Handle-Tabelle    │
└─────────────────────────────────────────────────────────────────────────────────┘
```

Grundsätze:

- **Der Worker besitzt den Zustand** (Quelltext-Version, Cache, Kernel-Shapes). Die UI hält nur Darstellungsdaten.
- **Cursor und Hover brauchen keine Neuberechnung.** Der Worker liefert pro Auswertung eine vollständige Szenenbeschreibung (alle Knoten mit Quellbereich, Transformation, Art, Eltern). Die UI entscheidet selbst, was hervorgehoben, gedimmt oder ausgeblendet wird. Meshes werden pro Knoten bei Bedarf nachgeladen.
- **Volle Neuauswertung der Sprache bei jeder Änderung, Kernel-Operationen aus dem Cache.** Die Sprachauswertung ist billig (Millisekunden). Teuer sind nur Kernel-Aufrufe, und die werden über inhaltsbasierte Schlüssel wiederverwendet. Damit folgt „nur der Pfad zur Wurzel wird ungültig“ automatisch.

---

## 2. Kernel-Anbindung (Entscheidung zu Offener Punkt 3)

### Empfehlung: Zwei Wasm-Module im selben Worker

| Teil | Technik |
| --- | --- |
| OCCT | Eigener Emscripten-Build (OCCT 7.9.x oder neuer), nur benötigte Toolkits |
| Fassade | Eigene C++-Schicht (~1–2 kLOC) mit flacher C-ABI: Handles (`u32`) statt Objekte, Fehlercodes statt Exceptions |
| Rust → OCCT | `kernel-occt` ruft die exportierten C-Funktionen über eine dünne JS-Brücke (`#[wasm_bindgen(module = "/occt-bridge.js")]`) |
| Daten | Kleine Daten (Punkte, Kanten-IDs) werden kopiert; Meshes gehen direkt aus dem OCCT-Heap als `ArrayBuffer` an den Main Thread |

Begründung:

- `wasm-bindgen` unterstützt das Emscripten-Target nicht. Ein gemeinsames Modul (Rust auf `wasm32-unknown-emscripten`, OCCT statisch gelinkt) ist möglich, aber die Toolchain (Panics vs. C++-Exceptions, Linker-Flags) ist deutlich fragiler. Bleibt als Alternative, falls die Brücke zu langsam ist.
- Der Overhead pro Aufruf über JS liegt im Mikrosekundenbereich; OCCT-Operationen dauern Millisekunden. Massenaufrufe (Klassifizierung vieler Punkte) werden gebündelt.
- `opencascade-rs` baut nicht für Wasm und bindet OCCT breit über `cxx`. `opencascade.js` bringt riesige generierte Bindings und ist nicht auf eine Rust-Steuerung ausgelegt. Eine schmale eigene Fassade ist leichter zu warten und passt 1:1 zur Kernel-Schnittstelle der Spec.
- Reine Rust-Kernel (truck, Fornjot) laufen problemlos in Wasm, haben aber keine belastbaren Fillets, Shells oder Sweeps.

**Native Variante derselben Fassade:** Die Fassade wird zusätzlich nativ gebaut und in `kernel-occt` per FFI gelinkt (Feature `native`). Damit laufen Integrationstests mit echtem OCCT unter `cargo test`, und Kernel-Fehler lassen sich mit nativen Debuggern untersuchen.

### OCCT-Build

- Toolkits: `TKernel TKMath TKG2d TKG3d TKGeomBase TKBRep TKGeomAlgo TKTopAlgo TKPrim TKBO TKBool TKFillet TKOffset TKShHealing TKMesh` und für Export `TKXSBase TKDESTEP TKDESTL` (Namen je nach OCCT-Version).
- Flags: `-O3 -fwasm-exceptions` (native Wasm-Exceptions, OCCT wirft `Standard_Failure`), `ALLOW_MEMORY_GROWTH`, zunächst ohne Threads.
- Erwartete Größe: 10–20 MB unkomprimiert, 3–6 MB mit Brotli. Wird im Spike gemessen.
- Reproduzierbar über ein Docker-Image mit fester emsdk-Version; das Build-Ergebnis wird in CI zwischengespeichert (OCCT-Build dauert 30–60 min).

### Fassade: Kernpunkte

- **Handle-Tabelle** mit Referenzzählung. Rust hält `Shape(u32)`, `Drop` gibt frei.
- **Provenienz:** Jeder Shape-Eintrag trägt eine Tabelle `Fläche/Kante → lokaler Tag`. Nach jeder Operation wird sie über `BRepTools_History` (`Modified`, `Generated`, `IsRemoved`) fortgeschrieben. Neu erzeugte Flächen (Fillet, Schnittflächen) erhalten den Tag der erzeugenden Operation.
- **Neue Kanten eines Booleans:** `BRepAlgoAPI_BuilderAlgo::SectionEdges()` liefert genau die Kanten auf Schnittkurven verschiedener Kinder. Grundlage für `difference(fillet = r)`.
- **Fehler:** Jede Funktion fängt `Standard_Failure`, gibt einen Code zurück und legt Meldung sowie betroffene Kanten ab (für „Kante rot“).
- **Abbruch:** `Message_ProgressRange` mit eigenem `UserBreak()`, das ein Abbruch-Flag liest (siehe 6.3).

---

## 3. Kernel-Schnittstelle in Rust (Entscheidung zu Offener Punkt 4)

Die Engine kennt nur diesen Trait. Er ist so geschnitten, dass ein späterer Rust-Kernel (solidcore) ihn direkt im selben Wasm-Modul implementiert, ohne JS-Brücke und ohne Änderung an Sprache oder Engine.

```rust
pub trait Kernel {
    type Shape: Clone;

    // Primitive (immer im lokalen System, Platzierung über transform)
    fn make_box(&mut self, min: P3, max: P3, tag: Tag) -> KResult<Self::Shape>;
    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Self::Shape>;
    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Self::Shape>;
    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Self::Shape>;
    fn make_profile(&mut self, p: &Profile2d, tag: Tag) -> KResult<Self::Shape>; // Rechteck, Kreis, Polygon, Spline

    fn transform(&mut self, s: &Self::Shape, m: &Mat4) -> KResult<Self::Shape>;
    fn boolean(&mut self, op: BoolOp, s: &[Self::Shape], tag: Tag) -> KResult<BoolResult<Self::Shape>>; // inkl. new_edges
    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: Radius, tag: Tag) -> KResult<Self::Shape>;
    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape>;
    fn extrude / revolve / loft / sweep / nurbs_surface / thicken / shell(...);

    // Abfragen
    fn edges(&self, s: &Self::Shape, samples: u32) -> Vec<EdgeInfo>;   // ID, Abtastpunkte, Tag
    fn faces(&self, s: &Self::Shape, samples: u32) -> Vec<FaceInfo>;
    fn classify(&self, s: &Self::Shape, pts: &[P3]) -> Vec<Class>;    // gebündelt: Innen/Außen/Rand
    fn tessellate(&self, s: &Self::Shape, tol: f64) -> Mesh;           // Dreiecke + Kantenpolylinien, je mit Tag
    fn export(&self, s: &[Self::Shape], fmt: ExportFormat) -> KResult<Vec<u8>>;
}
```

Zusätzlich: `kernel-mock`, ein reiner Rust-Kernel mit Quadern/Hüllen und protokollierten Aufrufen. Damit sind Parser, Auswertung, Cache und Zonenlogik ohne OCCT testbar.

---

## 4. Sprache in Rust

### 4.1 Parser (`cad-syntax`)

- Handgeschriebener Lexer und rekursiver Abstieg mit Pratt-Parser für Ausdrücke. Fehlertolerant: Syntaxfehler erzeugen Fehlerknoten, der Rest wird weiter geparst.
- AST mit Byte-Spans für jeden Knoten und jedes Literal. Ein verlustfreier Syntaxbaum (rowan) ist nicht nötig: Rückschreiben ersetzt nur Literale, dafür reichen Spans. Kommentare bleiben unberührt.
- Dateien sind klein; jede Änderung parst die ganze Datei neu (< 1 ms).
- **UTF-8 vs. UTF-16:** Rust rechnet in Bytes, CodeMirror in UTF-16-Einheiten. Die Umrechnung passiert an genau einer Stelle (Zeilen-Index im Worker).
- Neue Syntax gegenüber OpenSCAD: `grid g { x: …; }`, `g[a, *, _]`, `g.name`, `anchor`, `name: aufruf;`, `zone(...)`, `rotate(to = v)`.

### 4.2 Werte mit Herkunft (`cad-eval`)

Jeder Wert trägt seine Herkunft. Das ist die Grundlage für Griffe, Sperren und Rückschreiben (Spec: Griffe und Rückschreiben, Regeln 1–3).

```rust
struct Val { v: Value, origin: Origin }

enum Origin {
    Literal(Span),                  // direkt im Code
    VarLiteral { var: DefId, lit: Span }, // Variable mit Literal-Definition
    GridRef { grid: GridId, planes: [PlaneSel; 3], expr: Span },
    AnchorRef { inst: InstId, anchor: Sym, expr: Span },
    Computed,                       // alles andere: gesperrt
}
```

- Vektoren tragen die Herkunft pro Komponente. Sind nur einzelne Komponenten Literale, ist der Griff auf diese Achsen beschränkt.
- Rechenoperationen ergeben `Computed`. Wird eine Variable mit Literal-Definition verwendet, bleibt `VarLiteral` erhalten, solange der Wert unverändert durchgereicht wird.

### 4.3 Werttypen

`Number, Bool, String, Vector, List, Range, Undef, Function` wie in OpenSCAD, dazu:

| Typ | Inhalt | Regel |
| --- | --- | --- |
| `GridPoint` | Punkt im Modulsystem + Raster-ID + Ebenennamen | verhält sich in Ausdrücken wie ein Vektor |
| `GridLine`, `GridPlane` | Raster-ID, Auswahl, Lage | nur in Argumenten, die sie annehmen; sonst Typfehler |
| `PlaneRef` (`g.name`) | Raster-ID + Ebenenname | wird kontextabhängig zu Zahl oder Ebene (siehe 10) |
| `InstanceRef` | benannte Instanz `k` | nur `k.anker` und `dir(k.anker)` zulässig |

Geometrie ist kein Wert: Anweisungen erzeugen Knoten im Geometriebaum, nie Werte.

### 4.4 Auswertung → Geometriebaum (`cad-ir`)

Die Auswertung erzeugt einen Baum aus Knoten:

```rust
struct Node {
    id: InstanceId,      // Statement + Instanzpfad (Aufrufstellen, Schleifenindizes)
    stmt: StmtId,        // Quellanweisung → Zeile im Editor
    op: Op,              // Primitive, Transform, Boolean, Fillet, Zone, Profilop, ModuleCall, …
    params: Params,      // aufgelöste Werte
    children: Vec<NodeIdx>,
    local_to_module: Mat4,
    modifier: Option<Modifier>,
    key: CacheKey,       // Hash aus op, params, Schlüsseln der Kinder
}
```

- `InstanceId` identifiziert, *wo* ein Knoten im Programm steht (für UI-Zuordnung). `CacheKey` identifiziert, *was* er berechnet (für Cache). Beides ist getrennt, damit eine Zeile oberhalb nicht den Cache leert.
- Zusätzlich pro Auswertung: Raster (mit Ebenen, Werten, Herkunft, Platzierung), Anker, benannte Instanzen, Inline-Hinweise, Diagnosen, Sichtbarkeitsbereiche (welche Raster und Instanzen an welcher Stelle sichtbar sind; nötig für Snap-Ziele).
- Modifier: `*` und `%` entfernen den Teilbaum aus dem Ergebnis, `%` bleibt als Hilfsgeometrie in der Szene; `!` setzt die Wurzel; `#` markiert nur.
- **Modulvorschau:** Einstiegspunkt `eval_module(name, overrides)` für „Cursor in Moduldefinition“ und die Parameterleiste. Teilt den Cache mit der Gesamtauswertung.

### 4.5 Raster

- Ein Raster wird beim Erreichen der `grid`-Anweisung ausgewertet: Ebenenwerte als `Val` (mit Herkunft → Literal-Ebenen ziehbar, Ausdrucks-Ebenen gesperrt), Platzierungsmatrix aus den vorangehenden Transformationen.
- Prüfungen: eindeutige Namen über alle Achsen, ungleichmäßiges `scale` → Fehler, Platzierung nur an früher deklarierten Rastern im selben Modul.
- `mirror` vor `grid` spiegelt die Matrix; Namen bleiben.
- Rasterpunkte liefern Koordinaten im Modulsystem. Beim Modulaufruf werden Anker mit der Instanztransformation in das Aufrufersystem gebracht.

---

## 5. Engine (`cad-engine`)

### 5.1 Ablauf einer Auswertung

1. Parsen. Bei Syntaxfehler: Diagnosen senden, letzte Szene bleibt.
2. Sprachauswertung → Geometriebaum.
3. Szenenbeschreibung sofort senden (Knoten, Raster, Anker, Griffe, Hinweise). Die UI kann Raster und Griffe schon zeigen.
4. Kernel-Auswertung von unten nach oben; pro Knoten Cache-Lookup über `CacheKey`.
5. Tessellierung der Wurzel und der vom UI angeforderten Knoten; Meshes als Transferables.

### 5.2 Cache

- `CacheKey`: stabiler Hash (z. B. `blake3`) über Operation, Parameter (Floats mit kanonisiertem Bitmuster), Kinder-Schlüssel und Kernel-Version.
- Eintrag: Kernel-Shape, Tag-Tabelle, optional Meshes pro Toleranz. LRU nach geschätztem Speicher; Freigabe gibt OCCT-Handles frei.
- **Transformationen werden für die Anzeige nicht in die Shapes gebacken.** Ein Mesh wird im lokalen System tesselliert und in three.js per Matrix platziert. Gleiche Teile in Schleifen teilen sich ein Mesh.

### 5.3 Provenienz (Fläche/Kante → Zeile)

- Kernel-Tags sind lokal zum gecachten Teilbaum: Index in die Liste der erzeugenden Knoten dieses Teilbaums.
- Beim Kombinieren (Boolean) verschiebt die Fassade die Tags der Kinder in den Bereich des Elternknotens.
- Nach der Auswertung kennt die Engine für jeden Knoten die Liste `lokaler Tag → InstanceId`. Ein gecachtes Ergebnis wird dadurch an einer anderen Stelle wiederverwendet, ohne die Geometrie anzufassen.
- Die UI bekommt pro Mesh ein Attribut `tag` pro Dreieck und Kantensegment plus die Tabelle `tag → InstanceId → Zeile`.

### 5.4 Kantenauswahl über Zonen

1. Zonen auswerten (normale Körper, gehen nicht ins Ergebnis). `zone(linie, d)` und `zone(ebene, d)` werden auf die Hülle des Körpers plus Rand begrenzt.
2. `edges(körper, n)` liefert pro Kante Endpunkte und Abtastpunkte.
3. Alle Punkte gebündelt mit `classify` gegen jede Zone prüfen.
4. `select = "inside"`: alle Punkte innen oder auf dem Rand. `"touch"`: mindestens ein Punkt.
5. Verschachtelte Blöcke von innen nach außen; nach einem Fillet gelten die neuen Randkanten (über History).
6. Ergebnis: Kanten-IDs + Anzahl für den Inline-Hinweis; leere Auswahl → Warnung.

`$fillet`/`$chamfer`: Kanten des Primitivs direkt nach seiner Erzeugung verrunden. `difference(fillet = r)`: `new_edges` aus `boolean`.

Fillet gescheitert: Knoten liefert seinen Körper ohne Verrundung, Diagnose mit betroffener Kante.

### 5.5 Griffe und Rückschreiben

- Die Engine erzeugt eine Griffliste: Position, Art (Punkt, Radius, Höhe, Drehring, Ebene …), Bezug auf ein oder mehrere Literale (`Origin`).
- Ziehen: UI sendet `DragHandle { id, pos, snap, alt }`. Der Worker berechnet die neuen Literale und gibt `TextEdit`s zurück. Der Editor wendet sie an, das löst eine normale Auswertung aus (mit grober Tessellierung, siehe 6.2).
- Snappen schreibt eine Referenz (`g[innen_r, hinten, boden]`, `k.ecke`). Nur Raster und Instanzen, die an der Stelle der Anweisung sichtbar sind, werden als Ziele angeboten.
- Freies Ziehen mit Alt: Zahlen, gerundet auf eine einstellbare Schrittweite (Standard 1 mm, 15° bei Drehung).
- Zahlenformat: kürzeste Darstellung nach Rundung, keine nachlaufenden Nullen.
- Ein Ziehvorgang ist eine Undo-Gruppe im Editor (CodeMirror: gemeinsames `userEvent` und `isolateHistory` am Anfang und Ende).

---

## 6. Worker, Nebenläufigkeit, Abbruch

### 6.1 Protokoll

UI → Worker:

| Nachricht | Zweck |
| --- | --- |
| `SetSource { text, version }` | Neuauswertung |
| `SetContext { module, overrides }` | Modulvorschau, Parameterleiste |
| `RequestMeshes { keys, tol }` | Meshes für gedimmte/hervorgehobene Einzelknoten |
| `DragStart / DragHandle / DragEnd` | Griffe |
| `Export { format, root }` | STEP, STL |

Worker → UI: `Diagnostics`, `Scene { version, nodes, grids, anchors, handles, hints }`, `Meshes`, `TextEdits`, `ExportResult`, `Progress`.

Nachrichten werden mit `serde` + `serde-wasm-bindgen` übergeben, Meshes als `Float32Array`/`Uint32Array`-Transferables. TS-Typen werden aus Rust generiert (`tsify` oder `ts-rs`), damit das Protokoll auf beiden Seiten gleich ist.

### 6.2 Feine und grobe Tessellierung

- Während eines Ziehvorgangs: grobe Toleranz (z. B. 10 × `$tol`), Tessellierung nur der Wurzel.
- Bei `DragEnd`: feine Tessellierung. Meshes werden pro Toleranz gecacht.

### 6.3 Abbruch

- Die Wasm-Auswertung blockiert den Worker. Abbruch läuft über ein `SharedArrayBuffer`-Flag, das der Main Thread setzt:
  - Rust prüft es zwischen zwei Kernel-Aufrufen.
  - Die OCCT-Fassade prüft es in `UserBreak()` während langer Operationen (Booleans, Fillets, Meshing).
- Nach einem Abbruch bleiben fertige Knoten im Cache; die nächste Auswertung setzt dort fort.
- Voraussetzung: Cross-Origin-Isolation (Header `COOP: same-origin`, `COEP: require-corp`). Das ermöglicht später auch OCCT mit Threads (parallele Booleans und Meshing). Auf Hostern ohne eigene Header (GitHub Pages) hilft `coi-serviceworker`.

### 6.4 Versionen

Die Szene trägt die Quelltext-Version. Bis eine neue Szene da ist, bildet die UI Quellpositionen alter Szenen über die zwischenzeitlichen Editor-Änderungen ab (`ChangeSet.mapPos`), damit Hervorhebungen beim Tippen nicht springen.

---

## 7. UI (TypeScript/React)

| Bereich | Technik |
| --- | --- |
| Build | Vite, pnpm; Wasm über `wasm-bindgen --target web` |
| Zustand | Zustand (Store pro Bereich: Quelltext, Szene, Auswahl, Parameterleiste) |
| Editor | CodeMirror 6: Lezer-Grammatik für Syntaxhervorhebung; Diagnosen, Inline-Hinweise, Ausgrauen (`*`) und Zeilenfarben als Decorations aus den Worker-Daten |
| 3D | three.js über react-three-fiber; `three-mesh-bvh` für schnelles Picking; `Line2` für Kanten und Rasterlinien |
| Picking | Raycast liefert Dreieck → `tag` → `InstanceId` → Zeile; Hover und Klick ohne Worker-Aufruf |
| Darstellungszustände | Shader-Uniforms und Material-Varianten pro Knoten: voll, gedimmt, halbtransparent, rot halbtransparent (`#`, abgezogen), grau (`%`), Umriss |
| Griffe | Eigene Gizmos (Punkt, Pfeil, Drehring mit 15°-Rasten, Ebenengriff), Snap-Suche im Bildschirmraum gegen Rasterpunkte und Anker |
| Raster | Ebenen als Linien am Rand der Bauteilhülle; durchgezogen (Literal) oder gestrichelt (Ausdruck); standardmäßig nur die von der aktuellen Zeile verwendeten |
| Parameterleiste | aus der Modulsignatur; Werte pro Modul in der Sitzung; „Als Standard übernehmen“ erzeugt einen `TextEdit` |
| Pfadleiste | aus dem aktuellen Modulkontext; Doppelklick auf Instanz springt in die Definition |

Highlighting und Gruppierung der Knoten pro Zeile: Die UI indiziert die Szene nach `stmt`. Eine Zeile kann mehrere Instanzen haben (Schleifen, mehrfache Modulaufrufe); alle werden hervorgehoben.

---

## 8. Repository-Struktur

```
crates/
  cad-syntax/     Lexer, Parser, AST, Spans
  cad-eval/       Namensauflösung, Werte mit Herkunft, Raster, Anker, Auswertung
  cad-ir/         Geometriebaum, CacheKey
  cad-kernel/     trait Kernel, gemeinsame Typen (P3, Mat4, Mesh, Tag …)
  kernel-mock/    Testkernel in Rust
  kernel-occt/    Kernel über OCCT-Fassade (Features: wasm | native)
  cad-engine/     Cache, Provenienz, Zonen, Griffe, Rückschreiben, Tessellierungs-Cache
  cad-wasm/       wasm-bindgen-Einstieg für den Worker, Protokoll
occt/
  facade/         C++-Fassade + CMake
  docker/         emsdk- und OCCT-Build
web/
  src/editor/  src/viewport/  src/worker/  src/panels/  src/state/
examples/         .cad-Beispieldateien (zugleich Testfälle)
docs/
```

---

## 9. Tests

| Ebene | Werkzeug | Inhalt |
| --- | --- | --- |
| Parser | `cargo test`, `insta` | Snapshot des AST, Fehlerwiederherstellung, Fuzzing (`cargo-fuzz`) |
| Auswertung | `kernel-mock` | Werte, Herkunft, Raster, Anker, Sichtbarkeit, Diagnosen |
| Rückschreiben | Rust | Griff → TextEdit → neu parsen → erwarteter Wert |
| Kernel | `kernel-occt` nativ | Volumen, Flächenanzahl, Kantenauswahl, Provenienz nach Booleans und Fillets |
| Beispiele | `examples/` | Golden-Werte (Volumen, Hülle, Kantenanzahl) für jede Datei |
| UI | Vitest, Playwright | Editor ↔ Ansicht, Ziehen, Undo, Snappen |
| Leistung | Benchmarks im Browser | Ladezeit, Auswertung typischer Bauteile, Ziehen mit ≥ 30 fps |

CI: GitHub Actions; OCCT-Wasm und OCCT-nativ als gecachte Artefakte.

---

## 10. Klärungsbedarf in der Spec

Vorschläge, wie der Plan sie bis zur Klärung behandelt:

1. **`g.name` ist Zahl oder Ebene.** `g.deckel` ist in `mirror(g.deckel)` eine Ebene, in Ausdrücken eine Zahl. Vorschlag: eigener Typ `PlaneRef`, der sich je nach erwartetem Argumenttyp umwandelt.
2. **Länge von `zone(linie, d)`.** Eine Rasterlinie ist unendlich. Vorschlag: begrenzt auf die Hülle des Körpers plus `d`.
3. **Benannte Instanzen in Schleifen.** `k: gehaeuse();` innerhalb von `for` ist nicht definiert. Vorschlag: zunächst Fehler; später `k[i]`.
4. **Fillets unter `scale`.** „Vor der Skalierung berechnet“ heißt: Fillet im lokalen System, danach Transformation. Das passt zur Architektur; nur gleichmäßiges `scale` ist erlaubt.
5. **Rückschreiben bei Literalen in Modulparametern.** Ist ein Wert ein Standardparameter (`w = 40`) und der Aufruf überschreibt ihn nicht, ändert Ziehen den Standardwert. Das ist eine Codeänderung mit Wirkung auf alle Aufrufe. Vorschlag: zulassen, aber den Griff anders einfärben.
6. **2D-Formen mit z-Abweichung.** Warnung laut Spec; projiziert wird in die xy-Ebene des aktuellen Systems.
7. **Alt-Schrittweite** (Offener Punkt 1): einstellbar, Standard 1 mm und 15°.
8. **Mehrere Bauteile** (Offener Punkt 2): später. Vorschlag zur Diskussion: oberste Anweisungen sind Bauteile, benannte Instanzen auf oberster Ebene bestimmen Exportnamen.

---

## 11. Meilensteine

Jeder Meilenstein endet mit einem lauffähigen Stand im Browser.

| # | Inhalt | Ergebnis |
| --- | --- | --- |
| M0 | **Spike:** OCCT mit Emscripten bauen, Fassade mit Quader, Zylinder, Boolean, Fillet, Tessellierung; Aufruf aus Rust im Worker; Anzeige in three.js | Größe, Ladezeit, Aufrufkosten gemessen; Entscheidung zur Kernel-Anbindung bestätigt |
| M1 | Sprachkern: Parser, Variablen, Funktionen, Module, `for`/`if`, 3D-Primitive, Transformationen, Booleans; Editor mit Diagnosen und Inline-Werten | OpenSCAD-ähnliche Teile modellierbar |
| M2 | Szene und Zuordnung: Provenienz, Cursor/Hover in beide Richtungen, gedimmte Darstellung, Modifier, Cache, Abbruch, Modulkontext, Pfadleiste | Code ↔ Ansicht vollständig |
| M3 | Raster und Anker: `grid`, Zugriffe, Platzierung, Sichtbarkeit, `anchor`, benannte Instanzen, `rotate(to = …)`, Darstellung | Raster nutzbar |
| M4 | Griffe und Rückschreiben: Herkunft der Werte, Griffe für Primitive, Transformationen und Rasterebenen, Snappen, Undo, Parameterleiste | Bearbeiten in der Ansicht |
| M5 | Verrundung und Fase: `$fillet`, `fillet =` an Booleans, Zonenblöcke, `select`, Fehlerdarstellung | Fillets wie in der Spec |
| M6 | 2D und Profiloperationen: Profile, `linear_extrude`, `rotate_extrude`, `loft`, `sweep`, `surface`, `thicken`, `shell` | Freiformen |
| M7 | Export (STEP, STL), Dateien öffnen/speichern, Beispiele, Leistung | erste nutzbare Version |
| später | Variabler Radius, OCCT mit Threads, mehrere Bauteile, solidcore | |

Abhängigkeiten: M1 → M2 → (M3, M5 parallel möglich) → M4. M6 hängt nur an M2.

---

## 12. Risiken

| Risiko | Gegenmaßnahme |
| --- | --- |
| OCCT-Wasm zu groß oder zu langsam beim Start | Toolkits reduzieren, Brotli, Streaming-Kompilierung, HTTP-Cache; im Spike messen |
| Fillets in OCCT scheitern bei Grenzfällen | Klare Fehlermeldung mit Kante, Körper ohne Fillet anzeigen (Spec-konform) |
| Provenienz über History lückenhaft (z. B. bei Shape-Healing) | Fallback: Tag über nächstgelegene Fläche der Eingaben bestimmen; Tests pro Operation |
| Speicher (wasm32, 4 GB) | LRU-Cache mit Speicherschätzung, Handles konsequent freigeben |
| Cross-Origin-Isolation verhindert Einbindung fremder Ressourcen | Alle Assets selbst ausliefern |
| Lizenz: OCCT ist LGPL 2.1 mit Ausnahme | Bei proprietärem Vertrieb Pflichten prüfen (Austauschbarkeit der Bibliothek; spricht für ein separates `occt.wasm`) |
