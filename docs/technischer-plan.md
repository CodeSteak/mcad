# Technischer Plan: Parametrische CAD-Sprache mit Rastern

Stand: 2026-10-23, Ist-Stand nach Testbericht 7 (erste Fassung 2026-10-22) · Grundlage: `Spec: Parametrische CAD-Sprache mit Rastern` (22.10.2026)

Abschnitt 0 fasst zusammen, was sich gegenüber der ersten Fassung geändert hat; die Abschnitte 1–14 beschreiben den umgesetzten Stand, Abschnitt 13 den Status der Meilensteine, Abschnitt 15 die bekannten Grenzen. Testberichte: `docs/reports/` (B1 … B7 = Bericht 1 … 7, Übersicht in `docs/reports/README.md`).

Ziel: Die Anwendung läuft vollständig im Browser, nur auf dem Desktop, für kleine Hobbyprojekte. Sprache, Auswertung, Cache und Rückschreiben sind in Rust (Wasm), die UI in TypeScript/React. Dazu gibt es eine CLI (auch zur Nutzung durch Claude Code); eine Tauri-Version ist optional und noch offen.

Festgelegte Rahmenbedingungen:

| Thema | Entscheidung |
| --- | --- |
| Kernel | Bis zur Veröffentlichung: SMLib (C++, Upstream von solidcore) als Wasm, aus dem Fork `tangency-labs/smlib` (`capi-tess` @ `7eba8aa`) mit `smlib/patches/0001`. Vor der Veröffentlichung: Umstieg auf solidcore (M8, offen). |
| Plattform | Desktop-Browser; keine Touch-Bedienung |
| Speicherung | Nur im Browser; kein Backend |
| Header | Keine Cross-Origin-Isolation (COOP/COEP), solange nicht nötig. Damit auch kein `SharedArrayBuffer` und keine Threads. |
| OpenSCAD | Weitgehend kompatibel, nicht vollständig (Abschnitt 5) |
| Priorität | Raster vor Fillets |

---

## 0. Geänderte und konkretisierte Entscheidungen (Ist-Stand)

| Thema | Erste Fassung | Umgesetzt | Nachweis |
| --- | --- | --- | --- |
| Kernel-Quelle | `usd-brep` @ `c9c5979` direkt | Fork `tangency-labs/smlib`, Zweig `capi-tess` @ `7eba8aa` (= `c9c5979` plus Korrekturen von Speicherfehlern aus ASan/UBSan, ohne OCCT-Übersetzer) und `smlib/patches/0001` (Zeigertabellen in Einfügereihenfolge: Ergebnisse hängen nicht mehr von der Heap-Lage ab, nativ und Wasm gleich). `smlib/fetch.sh` holt beides. | B4 (Kernel-Stand `58735e2`), B7 |
| Fassade | flache C-ABI mit Handles | zusätzlich: Booleans werden geprüft (Volumen, Probepunkte an den Operandenflächen) mit Rückfällen (Operanden tauschen, Reihenfolge, Verschiebung um 10⁻⁶…10⁻⁴); `rand()`/`srand()` fest; zu dicke Shell-Wände vor SMLib abgewiesen; Shell mit Öffnung aus Booleans, wenn SMLibs Shell scheitert; Kantenflags (glatt); ABI-Version 4 (`sc_version`) | B1, B7 |
| Szenen | „Szenenbeschreibung sofort senden“ | zwei Szenen pro Auswertung: eine vorläufige (`complete = false`, ohne Kernel: Knoten, Raster, Anker, Griffe) direkt nach `SetSource` und die vollständige nach dem Kernel-Job; Meshes werden pro Szenen-Version angefordert | B1 (6.2), B2 |
| Tag-Schema der Provenienz | Tags lokal pro Teilbaum, Fassade verschiebt sie beim Kombinieren | ein Tag ist der internierte Index des Cache-Schlüssels des erzeugenden Knotens (gleich für gleichen Inhalt, solange die Engine lebt); die Fassade reicht Tags nur durch; `Evaluation::tag_nodes` bildet Tag → Knoten ab (mehrere Zeilen bei gleichem Inhalt); UI: Dreieck → `tri_tags` → `tag_nodes` → Knoten → Zeile | B2 (S07) |
| Neue Kanten, Kantenwahl | Kante zwischen Flächen verschiedener Kinder | dazu: eine Kante, die auf einer Kante eines Operanden liegt, ist nicht neu (koplanar verschmolzene Flächen); glatte Kanten (Naht, Pol, tangential innerhalb 2°) werden nie gewählt | B7 (F3, F4) |
| Entwurfsmodus beim Ziehen | nur grobe Tessellierung | zusätzlich: Verrundungen, die zuletzt länger als 25 ms brauchten und nicht im Cache liegen, setzt die Engine während des Ziehens aus (Hinweis „Verrundung beim Ziehen ausgesetzt“); nach `DragEnd` vollständige Auswertung | B4 (C20) |
| Watchdog-Sperre | Schlüssel merken, nicht erneut rechnen | 15 s (`WorkerTransport`); gesperrt wird der Inhaltsschlüssel des Knotens aus dem letzten `Progress.current`; der neue Worker bekommt `BlockKeys` vor `SetSource`; die Sperre endet mit einer Änderung des Knotens (neuer Schlüssel) oder dem Neuladen der Seite | B2 (S17), B4 (C19) |
| Rasterzugriff | Ebenennamen | ein Eintrag von `g[…]`, der keine Ebene des Rasters ist, ist ein Zahlenausdruck (`g[vorn, mitte, 9]`); Listen ergeben indizierte Ebenen (`g[loch[i], …]`); `g.name` als Zahl oder Ebene (12.1) | B1, B4 |
| Raster und Anker in Schleifen | Warnung laut Spec | Fehler: `grid` und `anchor` in `for` sowie `grid` in den Kindern eines Modulaufrufs sind nicht erlaubt (Fehler an der Anweisung) | `cad-eval/tests/grids.rs` |
| Sprung zur Deklaration | Klick auf Raster-/Ankerausdruck | Strg+Klick (ein einfacher Klick setzt weiter den Cursor, sonst wären die Ausdrücke nicht editierbar); ebenso Strg+Klick auf den Pfad von `include`/`use` | B3 (B13), B6 (P04) |
| Snap von 1D-Griffen | – | Höhe, Radius und Ebene snappen auf Rasterebenen derselben Achse und schreiben `g.name`, nur wenn der Griff danach exakt auf der Ebene liegt | B3 (B06) |
| STEP-Export | „später“ | AP242 (Standard) oder AP214 mit eigenem Writer `cad-step` aus der neutralen B-Rep `cad_kernel::brep` (Fassade `sc_brep`, exakte Geometrie); geprüft mit OpenCASCADE (`tests/step/verify.py`) | B5 |
| `include`/`use` | „später“ | OpenSCAD-Semantik; Pfade relativ zur einbindenden Datei oder absolut, kein `OPENSCADPATH`; fehlende Dateien und Zyklen sind Fehler an der `include`/`use`-Zeile (OpenSCAD: Warnung) | B6 |
| Datei-IDs in Spans | Byte-Spans einer Datei | jede geladene Datei belegt einen eigenen Positionsbereich (Datei `f` ab `f << 24`, Anweisungs-IDs ab `f << 22`); Diagnosen, Hinweise, Griffe und Text-Edits kennen ihre Datei; das Protokoll gibt Positionen lokal zur Datei mit Feld `file` aus | B6 |
| Bauteile | Offener Punkt 2 | benannte Instanzen auf oberster Ebene der Hauptdatei; Export je Bauteil (STL/3MF/STEP), alle als ZIP oder als 3MF mit einem Objekt je Bauteil; CLI `export --part`, `--all-parts` | B6 |
| Benannte Instanzen in Schleifen | nicht definiert | `k[i]: …` mit ganzzahligem Index (`k[2].anker`, Snap schreibt `k[2].ecke`); `k: …` in `for` bleibt ein Fehler | B6 (P08) |
| Variabler Radius | Stretch Goal | `fillet(r = function (p) …)` und `difference/union/intersection(fillet = function (p) …)`: cad-eval übersetzt das Funktionsliteral in ein Radiusgesetz `cad_ir::Law` (reiner Ausdruck über `p`, Teil des Baums und des Cache-Schlüssels) statt eines Rückrufs; die Engine wertet es an Stützpunkten je Kante aus, die Fassade (`sc_fillet_var`) baut daraus ein B-Spline-Gesetz | B7 |
| solidcore | `kernel-solidcore` ab Umstieg | `kernel-solidcore` und `cad diff-kernels` vorhanden, opt-in (Feature `solidcore`); heute bricht jedes Primitiv in solidcore ab (`Unported`), M8 wartet | B5, B6, `docs/solidcore-umstieg.md` |
| UI-Bibliotheken | react-three-fiber, `three-mesh-bvh`, `Line2` | three.js direkt (Raycaster, Linien als `LineSegments`), ohne r3f und BVH; TS-Protokolltypen aus Rust mit `ts-rs` | B2 |
| CI | GitHub Actions, gecachte SMLib-Builds | Workflows `rust`, `web`, `compat`, `pages` (manuell) in `.github/workflows/`, Schritte in `scripts/ci/` (Abschnitt 11) | – |

---

## 1. Architektur

```
┌──────────────────────────── Main Thread (React/TS) ────────────────────────────┐
│  Editor (CodeMirror 6)      3D-Ansicht (three.js)      Parameterleiste, Pfadleiste │
│        │  ▲                        │  ▲                                          │
│        ▼  │                        ▼  │                                          │
│  ─────────────────────── EngineTransport (TS-Interface) ──────────────────────── │
│        Browser: Web Worker + postMessage      später Tauri: IPC                  │
└──────────────────────────────────────┬─────────────────────────────────────────┘
                                       │
┌──────────────────────────── Engine (Rust) ─────────────────────────────────────┐
│  cad-syntax → cad-eval → cad-ir → cad-engine (Cache, Zonen, Provenienz, Griffe) │
│                                         │ trait Kernel                          │
│               ┌─────────────────────────┼──────────────────────────┐            │
│        kernel-smlib (Übergang)    kernel-solidcore (Ziel)     kernel-mock (Tests)│
│               │                                                                 │
│        smlib.wasm (C++, Emscripten) bzw. nativ gelinkt                           │
└─────────────────────────────────────────────────────────────────────────────────┘
      Einstiege:  cad-wasm (Browser-Worker) · cad-cli (nativ) · später Tauri-Backend
```

Grundsätze:

- **Die Engine kennt weder Browser noch Transport.** Dieselben Crates laufen im Worker (Wasm), in der CLI (nativ) und später im Tauri-Backend (nativ). Das Protokoll ist ein Satz `serde`-Typen.
- **Die Engine besitzt den Zustand** (Quelltext-Version, Cache, Kernel-Shapes). Die UI hält nur Darstellungsdaten.
- **Cursor und Hover brauchen keine Neuberechnung.** Die Engine liefert pro Auswertung eine vollständige Szenenbeschreibung (alle Knoten mit Quellbereich, Transformation, Art, Eltern). Die UI entscheidet selbst über Hervorheben, Dimmen und Ausblenden. Meshes werden pro Knoten bei Bedarf nachgeladen.
- **Volle Neuauswertung der Sprache bei jeder Änderung, Kernel-Operationen aus dem Cache.** Die Sprachauswertung dauert Millisekunden. Teuer sind nur Kernel-Aufrufe; sie werden über inhaltsbasierte Schlüssel wiederverwendet. Damit folgt „nur der Pfad zur Wurzel wird ungültig“ automatisch.

---

## 2. Kernel

### 2.1 Übergang über SMLib (C++)

solidcore ist ein Rust-Port von SMLib aus NVIDIAs `usd-brep` (Apache-2.0) mit dem Ziel gleicher Topologie und numerischer Abweichung ≤ 1e-9. Heute ist etwa ein Drittel portiert; Primitive, Booleans und Fillets laufen noch nicht durchgängig.

Bis solidcore so weit ist, nutzt die Anwendung den C++-Upstream direkt, auf demselben Stand, gegen den solidcore prüft (`usd-brep` @ `c9c5979`), seit Bericht 4 mit den Speicherfehler-Korrekturen aus dem Fork `tangency-labs/smlib` (Zweig `capi-tess` @ `7eba8aa`) und seit Bericht 7 mit `smlib/patches/0001` (`smlib/fetch.sh`). Die Bewertung vor dem Spike:

| Kriterium | SMLib (C++) als Übergang | OCCT als Übergang |
| --- | --- | --- |
| Ergebnisse beim Umstieg auf solidcore | gleiche Algorithmen, gleiche Topologie | andere Fillets, andere Kantenaufteilung, andere Fehlerfälle |
| API-Konzepte | wie solidcore (`SM_API`) | eigenes Modell, Übersetzung nötig |
| Lizenz | Apache-2.0 | LGPL 2.1 mit Ausnahme |
| Reife | Produktivkernel, aber bekannte UB/Hänger (solidcore: `plan/upstream-issues.md`) | sehr verbreitet |
| STEP-Export | nicht enthalten (Upstream nutzt dafür OCCT) | enthalten |
| Wasm-Build | unbekannt, im Spike prüfen | bekannt machbar |

Entscheidung (M0): SMLib. Der Emscripten-Build gelingt (`smlib/build-wasm.sh`, smlib.wasm ≈ 1,24 MiB Brotli, B1); OCCT wird nur noch zur unabhängigen Prüfung des STEP-Exports verwendet (B5).

### 2.2 Anbindung im Browser

| Teil | Technik |
| --- | --- |
| SMLib | Emscripten-Build der Bibliotheken `SMLib` und `SM_API` (CMake, `-O3 -fwasm-exceptions`, `ALLOW_MEMORY_GROWTH`, ohne Threads) |
| Fassade | Eigene C++-Schicht (~1–2 kLOC) mit flacher C-ABI: Handles (`u32`) statt Objekte, Fehlercodes statt Exceptions |
| Rust → SMLib | `kernel-smlib` ruft die Fassade über eine dünne JS-Brücke (`#[wasm_bindgen(module = "/smlib-bridge.js")]`) |
| Daten | Kleine Daten (Punkte, Kanten-IDs) werden kopiert; Meshes gehen aus dem C++-Heap direkt als `ArrayBuffer` an den Main Thread |

Zwei Wasm-Module statt einem: `wasm-bindgen` unterstützt das Emscripten-Target nicht. Ein gemeinsames Modul (Rust auf `wasm32-unknown-emscripten`) wäre möglich, die Toolchain aber fragiler. Die Brücke kostet Mikrosekunden pro Aufruf; Kernel-Operationen dauern Millisekunden. Massenaufrufe (Klassifizieren vieler Punkte) werden gebündelt.

**Nativ:** Dieselbe Fassade wird für CLI und Tests nativ gebaut und per FFI gelinkt. `smlib/CMakeLists.txt` folgt den Einstellungen von solidcores `harness/cpp`; `smlib/build-native.sh` baut `libSMLib.a` und `libSM_API.a` (über `SMLIB_PREBUILT` eingebunden), sonst baut `crates/kernel-smlib/build.rs` sie selbst.

### 2.3 Umstieg auf solidcore

- `kernel-solidcore` implementiert denselben Trait direkt in Rust, im selben Wasm-Modul. Die JS-Brücke und `smlib.wasm` entfallen.
- solidcore baut für `wasm32-unknown-unknown`. Laut solidcore-README braucht der Konsument einen größeren Stack (`-zstack-size=8388608`), weil Wasm keine Guard-Page hat.
- Abnahme: Die CLI wertet alle Beispiele mit beiden Kerneln aus und vergleicht Volumen, Flächen- und Kantenanzahl und ausgewählte Kanten (Abschnitt 8). Erst wenn alle übereinstimmen, wird umgestellt.
- Bis dahin kann `kernel-solidcore` als zweite Implementierung mitlaufen und meldet, welche Operationen schon gehen.

### 2.4 Fassade: Kernpunkte

- **Handle-Tabelle** mit Referenzzählung. Rust hält `Shape(u32)`, `Drop` gibt frei.
- **Provenienz:** Jede Fläche trägt einen Tag als SMLib-Attribut. SMLib reicht Attribute bei Booleans weiter (`SmAttribute::Split/Merge`); neu erzeugte Flächen (Rundungen, Deckel) bekommen den Tag der Operation. Die Tags vergibt die Engine (6.3).
- **Neue Kanten eines Booleans** werden kernelunabhängig bestimmt: Eine Kante ist neu, wenn ihre beiden angrenzenden Flächen von verschiedenen Kindern dieser Operation stammen und sie nicht auf einer Kante eines Operanden liegt (B7, F4). Glatte Kanten (`sc_edges`-Flag) zählen nie.
- **Fehler:** Jede Funktion fängt Exceptions, gibt einen Code zurück und legt Meldung sowie betroffene Kanten ab (für „Kante rot“).
- **Hänger:** Upstream hat bekannte Endlosschleifen bei bestimmten Eingaben. Siehe Watchdog in 7.3.

### 2.5 Export

- STL und 3MF aus der Tessellierung, in Rust, kernelunabhängig.
- STEP: weder SMLib noch solidcore haben einen Writer. Umgesetzt (Testbericht 5): neutrale B-Rep-Beschreibung `cad_kernel::brep` (`Kernel::brep`, bei SMLib über die Fassadenabfrage `sc_brep`), Writer `cad-step` (AP242, wahlweise AP214; Ebene, Zylinder, Kegel, Kugel, Torus, Rotations-/Extrusionsfläche, B-Spline; Linie, Kreis, Ellipse, B-Spline), `Engine::export_step`, `cad export -o teil.step`, `Export { format: "step" }`. Prüfung mit OpenCASCADE: `tests/step/verify.py`.

---

## 3. Kernel-Schnittstelle in Rust

Die Engine kennt nur diesen Trait. Er orientiert sich an den Operationen von `SM_API`, damit `kernel-smlib` und `kernel-solidcore` ihn ohne Übersetzungsschicht implementieren.

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
    fn boolean(&mut self, op: BoolOp, s: &[Self::Shape], tag: Tag) -> KResult<Self::Shape>;
    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: Radius, tag: Tag) -> KResult<Self::Shape>;
    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape>;
    fn extrude / revolve / loft / sweep / nurbs_surface / thicken / shell(...);

    // Abfragen
    fn edges(&self, s: &Self::Shape, samples: u32) -> Vec<EdgeInfo>;   // ID, Abtastpunkte, Tags der beiden Flächen
    fn faces(&self, s: &Self::Shape, samples: u32) -> Vec<FaceInfo>;
    fn classify(&self, s: &Self::Shape, pts: &[P3]) -> Vec<Class>;    // gebündelt: Innen/Außen/Rand
    fn tessellate(&self, s: &Self::Shape, tol: f64) -> Mesh;           // Dreiecke + Kantenpolylinien, je mit Tag
    fn mass_props(&self, s: &Self::Shape) -> MassProps;               // Volumen, Fläche (CLI, Tests)
}
```

`kernel-mock` ist ein reiner Rust-Kernel mit Quadern und protokollierten Aufrufen. Damit sind Parser, Auswertung, Cache und Zonenlogik ohne C++ testbar.

---

## 4. Sprache in Rust

### 4.1 Parser (`cad-syntax`)

- Handgeschriebener Lexer und rekursiver Abstieg mit Pratt-Parser für Ausdrücke. Fehlertolerant: Syntaxfehler erzeugen Fehlerknoten, der Rest wird weiter geparst.
- AST mit Byte-Spans für jeden Knoten und jedes Literal. Ein verlustfreier Syntaxbaum (rowan) ist nicht nötig: Rückschreiben ersetzt nur Literale. Kommentare bleiben unberührt.
- Jede Änderung parst die ganze Datei neu (< 1 ms bei Hobbyprojekten).
- **UTF-8 vs. UTF-16:** Rust rechnet in Bytes, CodeMirror in UTF-16-Einheiten. Die Umrechnung passiert an genau einer Stelle (Zeilen-Index in der Engine).
- Neue Syntax gegenüber OpenSCAD: `grid g { x: …; }`, `g[a, *, _]`, `g.name`, `anchor`, `name: aufruf;`, `zone(...)`, `rotate(to = v)`.

### 4.2 Werte mit Herkunft (`cad-eval`)

Jeder Wert trägt seine Herkunft. Das ist die Grundlage für Griffe, Sperren und Rückschreiben (Spec: Griffe und Rückschreiben, Regeln 1–3).

```rust
struct Val { v: Value, origin: Origin }

enum Origin {
    Literal(Span),                         // direkt im Code
    VarLiteral { var: DefId, lit: Span },  // Variable mit Literal-Definition
    GridRef { grid: GridId, planes: [PlaneSel; 3], expr: Span },
    AnchorRef { inst: InstId, anchor: Sym, expr: Span },
    Computed,                              // alles andere: gesperrt
}
```

- Vektoren tragen die Herkunft pro Komponente. Sind nur einzelne Komponenten Literale, ist der Griff auf diese Achsen beschränkt.
- Rechenoperationen ergeben `Computed`. Eine unverändert durchgereichte Variable mit Literal-Definition behält `VarLiteral`.

### 4.3 Werttypen

`Number, Bool, String, Vector, List, Range, Undef, Function` wie in OpenSCAD, dazu:

| Typ | Inhalt | Regel |
| --- | --- | --- |
| `GridPoint` | Punkt im Modulsystem + Raster-ID + Ebenennamen | verhält sich in Ausdrücken wie ein Vektor |
| `GridLine`, `GridPlane` | Raster-ID, Auswahl, Lage | nur in Argumenten, die sie annehmen; sonst Typfehler |
| `PlaneRef` (`g.name`) | Raster-ID + Ebenenname | wird kontextabhängig zu Zahl oder Ebene (12.1) |
| `InstanceRef` | benannte Instanz `k` | nur `k.anker` und `dir(k.anker)` zulässig |

Geometrie ist kein Wert: Anweisungen erzeugen Knoten im Geometriebaum, nie Werte.

### 4.4 Auswertung → Geometriebaum (`cad-ir`)

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

- `InstanceId` sagt, *wo* ein Knoten im Programm steht (UI-Zuordnung). `CacheKey` sagt, *was* er berechnet (Cache). Getrennt, damit eine eingefügte Zeile nicht den Cache leert.
- Zusätzlich pro Auswertung: Raster (Ebenen, Werte, Herkunft, Platzierung), Anker, benannte Instanzen, Inline-Hinweise, Diagnosen, Sichtbarkeitsbereiche (welche Raster und Instanzen an welcher Stelle sichtbar sind; nötig für Snap-Ziele).
- Modifier: `*` und `%` entfernen den Teilbaum aus dem Ergebnis, `%` bleibt als Hilfsgeometrie in der Szene; `!` setzt die Wurzel; `#` markiert nur.
- **Modulvorschau:** Einstieg `eval_module(name, overrides)` für „Cursor in Moduldefinition“, Parameterleiste und CLI (`-D`). Teilt den Cache mit der Gesamtauswertung.

### 4.5 Raster

- Ein Raster wird beim Erreichen der `grid`-Anweisung ausgewertet: Ebenenwerte als `Val` (Literal-Ebenen ziehbar, Ausdrucks-Ebenen gesperrt), Platzierungsmatrix aus den vorangehenden Transformationen.
- Prüfungen: eindeutige Namen über alle Achsen, ungleichmäßiges `scale` → Fehler, Platzierung nur an früher deklarierten Rastern im selben Modul.
- `mirror` vor `grid` spiegelt die Matrix; Namen bleiben.
- Rasterpunkte liefern Koordinaten im Modulsystem. Beim Modulaufruf werden Anker mit der Instanztransformation in das Aufrufersystem gebracht.

---

## 5. OpenSCAD-Kompatibilität

Ziel: Typische Hobby-Dateien laufen ohne Änderung, solange sie keine Mesh-Operationen brauchen.

| Bereich | Umfang |
| --- | --- |
| Voll | Syntax, Variablen, Funktionen und Funktionsliterale, Module, `children()`, `for`, `if`, `let`, `each`, Listenausdrücke, Strings, Bereiche, `echo`, `assert`, Mathefunktionen, `cube`, `sphere`, `cylinder`, `square`, `circle`, `polygon`, Transformationen, Booleans, Modifier, `linear_extrude`, `rotate_extrude` |
| Mit Abweichung | `color()` nur als Anzeigefarbe; `$fn/$fa/$fs` siehe 12.3; `polyhedron` nur für kleine Körper mit ebenen Flächen; `resize` als `scale` mit denselben Regeln |
| Voll (Projekte) | `include`/`use` mit Projekten aus mehreren Dateien (Pfade relativ zur einbindenden Datei; ohne Suchpfad `OPENSCADPATH`) |
| Später | `text()` (Schriften), `offset()` in 2D, `hull()` |
| Nicht vorgesehen | `minkowski`, `projection`, `import` von Meshes, `surface()` als Höhenkarte (der Name ist in der Spec für NURBS-Flächen vergeben) |

Ein Korpus von `.scad`-Dateien (Lizenzen vorher prüfen) läuft in CI über die CLI. Gemessen wird pro Datei: parst, wertet aus, erzeugt Geometrie.

---

## 6. Engine (`cad-engine`)

### 6.1 Ablauf einer Auswertung

1. Parsen. Bei Syntaxfehler: Diagnosen senden, letzte Szene bleibt.
2. Sprachauswertung → Geometriebaum.
3. Vorläufige Szene sofort senden (`complete = false`: Knoten, Raster, Anker, Griffe, Hinweise). Raster und Griffe sind damit schon sichtbar. Nach dem Kernel-Job folgt die vollständige Szene derselben Version (`complete = true`).
4. Kernel-Auswertung von unten nach oben, in Schritten (7.3); pro Knoten Cache-Lookup über `CacheKey`.
5. Tessellierung der Wurzel und der von der UI angeforderten Knoten; Meshes als Transferables.

### 6.2 Cache

- `CacheKey`: stabiler Hash (z. B. `blake3`) über Operation, Parameter (Floats mit kanonisiertem Bitmuster), Kinder-Schlüssel und Kernel-Version.
- Eintrag: Kernel-Shape, Tag-Tabelle, optional Meshes pro Toleranz. LRU nach geschätztem Speicher; Freigabe gibt Kernel-Handles frei.
- **Transformationen werden für die Anzeige nicht in die Shapes gebacken.** Ein Mesh wird im lokalen System tesselliert und in three.js per Matrix platziert. Gleiche Teile in Schleifen teilen sich ein Mesh.

### 6.3 Provenienz (Fläche/Kante → Zeile)

- Ein Tag ist der internierte Index des Cache-Schlüssels des erzeugenden Knotens (umgesetzt statt lokaler, beim Kombinieren verschobener Tags). Gleicher Inhalt ergibt denselben Tag, die Tabelle wächst nur.
- Nach der Auswertung bildet `Evaluation::tag_nodes` jeden Tag auf die Knoten dieses Baums ab (mehrere bei gleichem Inhalt). Ein gecachtes Ergebnis wird dadurch an anderer Stelle wiederverwendet, ohne die Geometrie anzufassen. Jedes Ergebnis trägt außerdem die Menge seiner möglichen Tags (Zuordnung von Flächen zu Kindern für neue Kanten).
- Die UI bekommt pro Mesh die Tags der Dreiecke (`tri_tags`) plus die Tabelle `tag_nodes`; Kanten tragen keine Knoten-Tags (Hover über Kanten geht über die angrenzende Fläche, B2).

### 6.4 Kantenauswahl über Zonen

1. Zonen auswerten (normale Körper, gehen nicht ins Ergebnis). `zone(linie, d)` und `zone(ebene, d)` werden auf die Hülle des Körpers plus `d` begrenzt.
2. `edges(körper, n)` liefert pro Kante Endpunkte und Abtastpunkte.
3. Alle Punkte gebündelt mit `classify` gegen jede Zone prüfen.
4. `select = "inside"`: alle Punkte innen oder auf dem Rand. `"touch"`: mindestens ein Punkt.
5. Verschachtelte Blöcke von innen nach außen; nach einem Fillet gelten die neuen Randkanten.
6. Ergebnis: Kanten-IDs + Anzahl für den Inline-Hinweis; leere Auswahl → Warnung.

`$fillet`/`$chamfer`: Kanten des Primitivs direkt nach seiner Erzeugung verrunden. `difference(fillet = r)`: neue Kanten nach 2.4.

Fillet gescheitert: Knoten liefert seinen Körper ohne Verrundung, Diagnose mit betroffener Kante.

### 6.5 Griffe und Rückschreiben

- Die Engine erzeugt eine Griffliste: Position, Art (Punkt, Radius, Höhe, Drehring, Ebene …), Bezug auf ein oder mehrere Literale (`Origin`).
- Ziehen: UI sendet `DragHandle { id, pos, snap, alt }`. Die Engine berechnet die neuen Literale und gibt `TextEdit`s zurück. Der Editor wendet sie an; das löst eine normale Auswertung mit grober Tessellierung aus.
- Snappen schreibt eine Referenz (`g[innen_r, hinten, boden]`, `k.ecke`). Angeboten werden nur Raster und Instanzen, die an der Stelle der Anweisung sichtbar sind.
- Freies Ziehen mit Alt: Zahlen, gerundet auf eine einstellbare Schrittweite (Standard 1 mm, 15° bei Drehung). Kürzeste Zahlendarstellung, keine nachlaufenden Nullen.
- Ein Ziehvorgang ist eine Undo-Gruppe im Editor (CodeMirror: gemeinsames `userEvent`, `isolateHistory` an Anfang und Ende).

---

## 7. Worker, Nebenläufigkeit, Abbruch

### 7.1 Protokoll

UI → Engine:

| Nachricht | Zweck |
| --- | --- |
| `SetSource { text, version }` | Neuauswertung |
| `SetContext { module, overrides }` | Modulvorschau, Parameterleiste |
| `RequestMeshes { keys, tol }` | Meshes für einzelne Knoten |
| `DragStart / DragHandle / DragEnd` | Griffe (`Drag` antwortet immer mit `Edits`, auch leer: höchstens ein Drag in Flug) |
| `SnapTargets` | Rasterpunkte und Anker (Punktgriffe), Rasterebenen `g.name` (Höhe, Radius, Ebene) |
| `SetDragOptions { step_mm, step_deg }` | Schrittweiten beim freien Ziehen (Einstellungen) |
| `Export { format, root }` | STL, 3MF, STEP |

Engine → UI: `Diagnostics`, `Scene { version, nodes, grids, anchors, handles, hints }`, `Meshes`, `TextEdits`, `ExportResult`, `Progress`.

Übergabe mit `serde` + `serde-wasm-bindgen`, Meshes als `Float32Array`/`Uint32Array`-Transferables. TS-Typen werden aus Rust generiert (`tsify` oder `ts-rs`).

`EngineTransport` in TS kapselt den Weg: `WorkerTransport` jetzt, `TauriTransport` später (Tauri-2-Kanäle für Binärdaten). Die UI kennt nur das Interface.

### 7.2 Feine und grobe Tessellierung

- Während eines Ziehvorgangs: grobe Toleranz (10 × Toleranz), nur die Wurzel. Entwurfsmodus: Verrundungen über 25 ms (letzte Messung, nicht im Cache) werden ausgesetzt (B4, C20).
- Bei `DragEnd`: feine Tessellierung. Meshes werden pro Toleranz gecacht.

### 7.3 Abbruch ohne `SharedArrayBuffer`

- Die Engine rechnet in Schritten: `engine.step(budget_ms)` bearbeitet Knoten, bis das Zeitbudget (z. B. 20 ms) erreicht ist, und kehrt zurück.
- Die Worker-Schleife gibt zwischen zwei Schritten die Kontrolle an die Event-Loop ab (`MessageChannel`), verarbeitet neue Nachrichten und bricht bei `SetSource` die laufende Auswertung ab. Fertige Knoten bleiben im Cache.
- Eine einzelne lange Kernel-Operation ist nicht unterbrechbar. Bei Hobbyprojekten ist das selten ein Problem.
- **Watchdog:** Der Main Thread beendet den Worker, wenn 15 s keine Antwort kommt (Schutz vor Hängern im Kernel), und startet ihn neu. Der Cache geht dabei verloren. Der Inhaltsschlüssel des Knotens aus dem letzten `Progress.current` wird gesperrt; der neue Worker bekommt `BlockKeys` vor `SetSource`. Der Knoten meldet „Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung“, die Zeile wird rot (B4, C19).

### 7.4 Versionen

Die Szene trägt die Quelltext-Version. Bis eine neue Szene da ist, bildet die UI Quellpositionen alter Szenen über die zwischenzeitlichen Editor-Änderungen ab (`ChangeSet.mapPos`), damit Hervorhebungen beim Tippen nicht springen.

---

## 8. CLI (`cad-cli`)

Ein natives Programm auf denselben Crates wie der Browser, mit nativ gelinktem Kernel. Zweck: Nutzung durch Claude Code und andere Werkzeuge, Tests in CI, Kernel-Vergleich vor dem Umstieg auf solidcore.

| Befehl | Ausgabe |
| --- | --- |
| `check <datei>` | Diagnosen mit Zeile und Spalte |
| `eval <datei> [--module m] [-D name=wert]` | Baum, Werte, Inline-Hinweise, Raster, Anker |
| `inspect <datei> --line n` | Was die Zeile erzeugt: Knoten, Hülle, Volumen, ausgewählte Kanten, Warnungen |
| `render <datei> -o bild.png [--view iso\|front\|top\|right] [--line n] [--size 1024x768]` | Bild des Modells, optional mit hervorgehobener Zeile wie im Editor |
| `export <datei> -o teil.stl\|.3mf\|.step [-D …] [--schema ap214] [--part name] [--all-parts]` | Exportdatei (STEP: exakte B-Rep, AP242), einzelne oder alle Bauteile |
| `diff-kernels <datei…>` | Vergleich SMLib ↔ solidcore (nur Entwicklung, `docs/solidcore-umstieg.md`) |

- Jeder Befehl hat `--json` mit festem Schema und definierte Exit-Codes. Ohne `--json` gibt es lesbaren Text.
- `render` nutzt einen kleinen Software-Rasterizer in Rust (Z-Buffer, flache Schattierung, Kantenlinien). Er braucht keine GPU und läuft damit auch in Containern und CI.
- Dazu eine kurze Anleitung für Claude Code im Repo (Skill-Datei), die die Befehle und das Vorgehen beschreibt: ändern → `check` → `render` → Bild ansehen.

---

## 9. UI (TypeScript/React)

| Bereich | Technik |
| --- | --- |
| Build | Vite, pnpm; Wasm über `wasm-bindgen --target web` |
| Zustand | Zustand (Stores für Quelltext, Szene, Auswahl, Parameterleiste) |
| Editor | CodeMirror 6: Lezer-Grammatik für Syntaxhervorhebung; Diagnosen, Inline-Hinweise, Ausgrauen (`*`) und Zeilenfarben als Decorations aus den Engine-Daten |
| 3D | three.js direkt (umgesetzt ohne react-three-fiber und `three-mesh-bvh`); Picking mit dem Raycaster; Kanten und Rasterlinien als `LineSegments` |
| Picking | Raycast → Dreieck → `tag` → `InstanceId` → Zeile; Hover und Klick ohne Engine-Aufruf |
| Darstellungszustände | Material-Varianten pro Knoten: voll, gedimmt, halbtransparent, rot halbtransparent (`#`, abgezogen), grau (`%`), Umriss |
| Griffe | Eigene Gizmos (Punkt, Pfeil, Drehring mit 15°-Rasten, Ebenengriff); Snap-Suche im Bildschirmraum gegen Rasterpunkte und Anker |
| Raster | Ebenen als Linien am Rand der Bauteilhülle; durchgezogen (Literal) oder gestrichelt (Ausdruck); standardmäßig nur die von der aktuellen Zeile verwendeten |
| Parameterleiste | aus der Modulsignatur; Werte pro Modul in der Sitzung; „Als Standard übernehmen“ erzeugt einen `TextEdit` |
| Pfadleiste | aus dem Modulkontext; Doppelklick auf Instanz springt in die Definition |
| Dateien | Autospeichern des Projekts in IndexedDB; Projekte aus mehreren Dateien als Tabs; Ordner öffnen/speichern über File System Access API (Chromium), sonst Hoch- und Herunterladen (Datei oder ZIP) |
| Bauteile | Bauteilleiste mit Sichtbarkeit, Volumen und Export je Bauteil |

Eine Zeile kann mehrere Instanzen haben (Schleifen, mehrfache Modulaufrufe). Die UI indiziert die Szene nach `stmt` und hebt alle hervor.

---

## 10. Repository-Struktur

```
crates/
  cad-syntax/       Lexer, Parser, AST, Spans (mit Datei-ID)
  cad-eval/         Namensauflösung, Werte mit Herkunft, Raster, Anker, Griffe, Rückschreiben, include/use
  cad-ir/           Geometriebaum, CacheKey, Radiusgesetz (Law)
  cad-kernel/       trait Kernel, gemeinsame Typen (P3, Mat4, Mesh, Tag …), neutrale B-Rep
  kernel-mock/      Testkernel in Rust
  kernel-smlib/     Kernel über SMLib-Fassade (Features: wasm | native)
  kernel-solidcore/ Kernel über solidcore (opt-in, Feature solidcore)
  cad-engine/       Cache, Provenienz, Zonen, Bauteile, Export STL/3MF/STEP
  cad-step/         STEP-Writer (AP242/AP214)
  cad-render/       Software-Rasterizer für die CLI
  cad-session/      Protokoll und Sitzung (Worker und CLI), TS-Typen (ts-rs)
  cad-wasm/         wasm-bindgen-Einstieg für den Worker
  cad-cli/          CLI
  compat-tests/     OpenSCAD-Vergleich Stufe 1
smlib/
  fetch.sh          Quellen: tangency-labs/smlib @ 7eba8aa (= usd-brep c9c5979 + Korrekturen) + patches/
  facade/           C++-Fassade; CMakeLists.txt, build-native.sh, build-wasm.sh
third_party/smlib/  SMLib-Quellen (nicht eingecheckt)
web/
  src/app/  src/editor/  src/view/  src/engine/  src/logic/  src/files/  src/state/  e2e/  scripts/
examples/           Beispieldateien (zugleich Testfälle): spec/, openscad-compat/, projekt-gehaeuse/
tests/              openscad-compat/ (Referenz, compare.py), step/verify.py
tools/report/       Generatoren der Testberichte
scripts/ci/         Schritte der CI-Workflows
.github/            Workflows und Composite-Actions (SMLib nativ/Wasm mit Cache)
docs/               Plan, solidcore-Umstieg, Testberichte (reports/)
```

---

## 11. Tests

| Ebene | Werkzeug | Inhalt |
| --- | --- | --- |
| Parser | `cargo test`, `insta` | Snapshot des AST, Fehlerwiederherstellung, Fuzzing (`cargo-fuzz`) |
| Auswertung | `kernel-mock` | Werte, Herkunft, Raster, Anker, Sichtbarkeit, Diagnosen |
| Rückschreiben | Rust | Griff → TextEdit → neu parsen → erwarteter Wert |
| Kernel | `kernel-smlib` nativ | Volumen, Flächenanzahl, Kantenauswahl, Provenienz nach Booleans und Fillets |
| Beispiele | CLI `eval --json`, `render` | Golden-Werte (Volumen, Hülle, Kantenanzahl) und Referenzbilder pro Datei |
| OpenSCAD-Korpus | CLI | Anteil parst / wertet aus / erzeugt Geometrie |
| UI | Vitest, Playwright | Editor ↔ Ansicht, Ziehen, Undo, Snappen |
| Leistung | Browser-Benchmarks | Ladezeit, Auswertung der Beispiele, Ziehen mit ≥ 30 fps |

CI: GitHub Actions; SMLib-Wasm und SMLib-nativ als gecachte Build-Artefakte. Umgesetzt in `.github/workflows/`, jeder Schritt ist ein Skript in `scripts/ci/` und lokal genauso aufrufbar:

| Workflow | Inhalt | Cache |
| --- | --- | --- |
| `rust` | `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` (nativ und `cad-wasm` für wasm32), `cargo test --workspace`, `cargo check -p cad-wasm --target wasm32-unknown-unknown` | SMLib nativ (`smlib/build-native/lib`), Cargo |
| `web` | emsdk 6.0.13, `smlib/build-wasm.sh`, `web/scripts/build-wasm.sh`, `pnpm install --frozen-lockfile`, `typecheck`, Vitest, Node-Tests (smlib.wasm, Engine, Transport), `pnpm build`, Playwright; Artefakte `web/dist` und bei Fehlern der Playwright-Bericht | emsdk, smlib.js/.wasm, Cargo, pnpm |
| `compat` | OpenSCAD 2021.01 (apt): Referenz neu erzeugen und mit `reference.json` vergleichen, Selbsttest von `compare.py`, `include`/`use` gegen OpenSCAD, `compare.py --eval` mit der Release-CLI; STEP-Prüfung mit `cadquery-ocp==8.0.1.1.0` | SMLib nativ, Cargo, venv |
| `pages` | manuell (`workflow_dispatch`): `web/dist` auf GitHub Pages (keine COOP/COEP-Header nötig) | wie `web` |

Cache-Schlüssel der SMLib-Builds (`scripts/ci/smlib-key.sh`): Fork-Commit, Inhalt der Patches, `smlib/CMakeLists.txt`, Build-Skript und Compiler-Version (nativ `c++`, Wasm `emcc`, dazu CMake), bei Wasm zusätzlich die Fassadenquellen.

---

## 12. Klärungsbedarf in der Spec

Vorschläge, wie der Plan sie bis zur Klärung behandelt:

1. **`g.name` ist Zahl oder Ebene.** In `mirror(g.deckel)` eine Ebene, in Ausdrücken eine Zahl. Vorschlag: Typ `PlaneRef`, der sich nach dem erwarteten Argumenttyp umwandelt.
2. **Benannte Instanzen in Schleifen.** `k: gehaeuse();` innerhalb von `for` ist nicht definiert. Umgesetzt: `k: …` in `for` ist ein Fehler; `k[i]: …` benennt die Instanzen mit ganzzahligem Index (`k[2].anker`, Snap schreibt `k[2].ecke`).
3. **`$fn` aus OpenSCAD.** In OpenSCAD erzeugt `cylinder(r = 5, $fn = 6)` ein Sechseck, das wird oft für Muttern genutzt. Ein globales `$fn = 100` ist dagegen nur für glatte Darstellung gedacht. Vorschlag: `$fn` zwischen 3 und 12 am Aufruf erzeugt ein Prisma wie in OpenSCAD; sonst exakte Geometrie und ein Hinweis.
4. **Länge von `zone(linie, d)`.** Eine Rasterlinie ist unendlich. Vorschlag: auf die Hülle des Körpers plus `d` begrenzt.
5. **Ziehen an Standardparametern.** Kommt ein Wert aus einem Standardparameter (`w = 40`) und der Aufruf überschreibt ihn nicht, ändert Ziehen den Standardwert und damit alle Aufrufe. Vorschlag: zulassen, Griff anders einfärben.
6. **Fillets unter `scale`.** Fillet im lokalen System, danach Transformation. Passt zur Architektur.
7. **2D-Formen mit z-Abweichung.** Warnung laut Spec; projiziert wird in die xy-Ebene des aktuellen Systems.
8. **Alt-Schrittweite** (Offener Punkt 1): einstellbar, Standard 1 mm und 15°.
9. **Mehrere Bauteile** (Offener Punkt 2): umgesetzt nach dem Vorschlag – benannte Instanzen auf oberster Ebene der Hauptdatei sind Bauteile und bestimmen die Exportnamen (Export pro Bauteil als STL/3MF/STEP, mehrere als ZIP oder als 3MF mit einem Objekt je Bauteil; CLI `export --part`, `--all-parts`). Baugruppen bleiben offen.

---

## 13. Meilensteine

Jeder Meilenstein endet mit einem lauffähigen Stand im Browser und in der CLI.

| # | Inhalt | Ergebnis | Status |
| --- | --- | --- | --- |
| M0 | **Spike:** SMLib mit Emscripten bauen; Fassade mit Quader, Zylinder, Boolean, Fillet, Klassifizieren, Tessellierung, Attribut-Weitergabe; Aufruf aus Rust im Worker und nativ; Anzeige in three.js | Größe, Ladezeit, Aufrufkosten, Stabilität gemessen; Entscheidung SMLib oder OCCT | erledigt (SMLib, B1) |
| M1 | Sprachkern: Parser, Variablen, Funktionen, Module, `for`/`if`/`let`, 3D-Primitive, Transformationen, Booleans; Editor mit Diagnosen und Inline-Werten; CLI `check`, `eval` | OpenSCAD-ähnliche Teile modellierbar | erledigt (B1) |
| M2 | Szene und Zuordnung: Provenienz, Cursor/Hover in beide Richtungen, Dimmen, Modifier, Cache, schrittweise Auswertung, Watchdog, Modulkontext, Pfadleiste; CLI `inspect`, `render`, `export` (STL) | Code ↔ Ansicht vollständig; Claude Code kann Modelle prüfen und ansehen | erledigt (B1, B2) |
| M3 | Raster und Anker: `grid`, Zugriffe, Platzierung, Sichtbarkeit, `anchor`, benannte Instanzen, `rotate(to = …)`, Darstellung | Raster nutzbar | erledigt (B1, B2) |
| M4 | Griffe und Rückschreiben: Herkunft der Werte, Griffe für Primitive, Transformationen und Rasterebenen, Snappen, Undo, Parameterleiste | Bearbeiten in der Ansicht | erledigt (B3) |
| M5 | Verrundung und Fase: `$fillet`, `fillet =` an Booleans, Zonenblöcke, `select`, Fehlerdarstellung | Fillets wie in der Spec | erledigt (B4, B7) |
| M6 | 2D und Profiloperationen: Profile, `linear_extrude`, `rotate_extrude`, `loft`, `sweep`, `surface`, `thicken`, `shell` | Freiformen | erledigt (B4, B7) |
| M7 | Dateien, 3MF, OpenSCAD-Korpus, Beispiele, Leistung | erste vollständige Version | erledigt (B1, B6; Leistung in B1–B4) |
| M8 | Umstieg auf solidcore: `kernel-solidcore`, Vergleich über `diff-kernels`, SMLib-Wasm entfernen | bereit zur Veröffentlichung | offen: wartet auf solidcore (heute bricht jedes Primitiv ab, B5); `kernel-solidcore` und `diff-kernels` vorhanden |
| später | STEP-Export, variabler Radius, mehrere Bauteile, `include`/`use` | | erledigt (B5, B6, B7) |
| später | Tauri-Version (`TauriTransport`) | | offen (optional) |
| – | CI (GitHub Actions) | | erledigt (Workflows lokal geprüft; Lauf auf GitHub steht aus) |

Abhängigkeiten: M1 → M2 → M3 → M4 → M5. M6 hängt nur an M2. M8 hängt am Fortschritt von solidcore und kann jederzeit nach M2 teilweise beginnen.

---

## 14. Risiken

| Risiko | Gegenmaßnahme |
| --- | --- |
| SMLib lässt sich nicht mit Emscripten bauen | eingetreten nicht: der Build gelingt (B1); Rückfall OCCT nicht nötig |
| Kernel hängt oder verhält sich undefiniert bei bestimmten Eingaben | Watchdog mit Neustart und Sperre (7.3); Fork mit Speicherfehler-Korrekturen und Patch 0001; geprüfte Booleans in der Fassade; gefundene Fälle an solidcore melden |
| solidcore ist zur geplanten Veröffentlichung nicht fertig | Veröffentlichung mit SMLib-C++ ist lizenzrechtlich möglich (Apache-2.0); Umstieg danach |
| Attribute gehen bei einzelnen Operationen verloren (Provenienz) | Rückfall über geometrische Zuordnung; Tests pro Operation |
| Fillets scheitern bei Grenzfällen | klare Fehlermeldung mit Kante, Körper ohne Fillet anzeigen (Spec-konform) |
| Wasm-Stack ohne Guard-Page (solidcore) | Stack auf 8 MiB setzen, wie im solidcore-README beschrieben |
| Speicher (wasm32, 4 GB) | LRU-Cache mit Speicherschätzung, Handles konsequent freigeben |

---

## 15. Bekannte Grenzen (aus den Testberichten 1–7)

Stand Bericht 7; behobene Punkte früherer Berichte sind weggelassen (z. B. F1–F4 aus B4, behoben in B7).

**Kernel (SMLib)**

- Booleans in Sonderlagen unzuverlässig; die Fassade erkennt falsche Ergebnisse nur über Stichproben (ein Fehler zwischen den Probepunkten bleibt unentdeckt). Die Prüfung nutzt SMLibs Punktklassifizierung, die bei gedrehten Splinekörpern selbst irrt; besteht keine Variante, bleibt SMLibs Ergebnis. Im Verschiebe-Rückfall weicht der zweite Operand um bis zu 10⁻⁴ der Modellgröße ab. Die Prüfung kostet Zeit (+38 % über alle Beispiele). (B1)
- Berühren sich Körper nur an einer Kante, ist die Vereinigung kein geschlossener Körper; `cad` meldet Volumen 0 (STL in Ordnung). (B1)
- Speicherfehler in SMLib können Ergebnisse vom Heap abhängig machen; die Shell-Vorprüfung deckt nur ebene, gegenüberliegende Wände ab. Patch 0001 beseitigt die Abhängigkeit von der Adressreihenfolge der Zeigertabellen. (B1, B7)
- `shell` aus Booleans nur für ebene geöffnete Flächen; bei gekrümmten geöffneten Flächen bleibt SMLibs Ergebnis bzw. dessen Fehlermeldung. Wanddicke ≥ Rundungsradius scheitert weiter (SMLibs Versatz). Die Booleans kosten Zeit (`dose_shell` ≈ 1,9 s). (B7)
- Variabler Radius: SMLibs Löser ist empfindlich (steile Verläufe an Taschenecken, Kreiskanten mit veränderlichem Radius scheitern); Sprünge werden über etwa drei Stützpunktabstände geglättet; kein variabler Radius für `chamfer` und `$fillet`; Gesetze nur aus dem reinen Ausdruckskern (keine Texte, Listenausdrücke oder Rekursion über `p`). (B7)
- Glatte Kanten nach SMLibs Kriterium (2°): fast tangentiale Kanten werden nie gewählt; die Prüfung „alte Kante eines Operanden“ ist bei gekrümmten Kanten nur so genau wie deren Abtastung. (B7)
- Tessellierung von Freiformflächen verfeinert global, ebene Flächen desselben Körpers werden mit feiner (z. B. 15 730 Dreiecke bei `12_linear_extrude_twist`). Twist-Extrusion ist eine kubische Loft-Annäherung (Schnitte alle 7,5°); Kreisprofile mit ungleichmäßigem `scale` und `scale = 0` werden nicht unterstützt. (B1)
- SMLib integriert Flächen nur auf etwa 10⁻⁴ genau. (B5)
- Vorgebaute Bibliotheken (`SMLIB_PREBUILT`) müssen nach einer Patch-Änderung neu gebaut werden (Header-Änderung); ein Symlink `third_party/smlib` wird von `fetch.sh` nur geprüft. (B7)

**Sprache und Engine**

- OpenSCAD facettiert, wir rechnen exakt: Volumen runder Teile weichen um 0,01–0,3 % ab, bei `twist` liegt OpenSCAD selbst um ~1 % daneben. (B1)
- `include`/`use`: kein Suchpfad (`OPENSCADPATH`); Variablen einer Bibliothek werden einmal je Auswertung ausgewertet; doppelt eingebundene Dateien werden zweimal geladen; höchstens 255 Dateien zu je 16 MiB; Snap-Sichtbarkeit eines in einer eingebundenen Datei benannten Rasters endet am Ende dieser Datei. (B6)
- Bauteile nur im Dateikontext; ein Bauteil innerhalb einer Booleschen Operation lässt sich nicht getrennt ausblenden; STEP mehrerer Bauteile ist ein Produkt ohne Baugruppenstruktur; Baugruppen sind offen. (B6)
- Die Tag-Tabelle der Engine wächst nur; Kanten zwischen Flächen zweier inhaltsgleicher Kinder gelten als alt (gleicher Tag). (`cad-engine`, Modulkommentar)

**STEP**

- Keine p-Kurven (Leser projizieren die 3D-Kanten); Kantenkurven von Booleans sind B-Splines, Kreise/Linien werden erkannt; Flächen ohne B-Spline-Form würden approximiert (in den Beispielen 0). Farben als `STYLED_ITEM`, von `verify.py` nicht geprüft. 2D-Ergebnisse werden abgelehnt; im Browser ist der Zeitstempel fest. (B5)

**UI**

- Hover über Kanten geht über die angrenzende Fläche; bei Mehrdeutigkeit (gleicher Inhalt an mehreren Stellen) wird über die Lage in den Knotenhüllen getrennt, sonst werden alle Zeilen markiert; Rasterausdrücke in Schleifen zeigen den Punkt des ersten Durchlaufs. (B2)
- Snap von 1D-Griffen nur auf Ebenen, auf denen der Griff nach dem Schreiben exakt landet (nicht unter verschobenen Teilbäumen oder bei `center = true`/`d =`). Punktgriffe bewegen sich in der Ebene senkrecht zur Blickrichtung (achsgebunden mit Umschalt oder X/Y/Z, B4). Inline-Regler nur für Zahl-/Bool-Literale von Zuweisungen. Parameter ohne Standardwert werden beim Reset 0/false/[0, 0, 0]. (B3)
- Zonen-Kanten und geöffnete Flächen bestimmt die Ansicht an tessellierten Zonen-Meshes (die Engine exakt); in Grenzfällen kann die Anzeige abweichen. Die Anzeige des `sweep`-Pfads interpoliert wie die Fassade und weicht ab, falls sich das Verfahren im Kernel ändert. (B4)
- Watchdog-Sperre gilt für den Inhaltsschlüssel (gleiche Knoten an anderer Stelle sind mitgesperrt) und bis zum Neuladen der Seite; der Test blockiert den Worker über eine Testnachricht, ein echter hängender Kernel-Aufruf wurde nicht nachgestellt. (B2, B4)
- Messwerte stammen aus headless Chromium mit SwiftShader (ohne GPU) und sind nur relativ aussagekräftig. (B2–B4)

**solidcore (M8)**

- Jedes Primitiv bricht in solidcore mit `Unported: SmSurface::DropCurve` ab; nur 2D-Knoten (in Rust berechnet) stimmen überein (101 von 855). `kernel-solidcore` deckt nur die heute vorhandenen API-Einstiege ab. (B5)
