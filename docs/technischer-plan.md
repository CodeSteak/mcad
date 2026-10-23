# Technischer Plan: Parametrische CAD-Sprache mit Rastern

Stand: 2026-10-22 · Grundlage: `Spec: Parametrische CAD-Sprache mit Rastern` (22.10.2026)

Ziel: Die Anwendung läuft vollständig im Browser, nur auf dem Desktop, für kleine Hobbyprojekte. Sprache, Auswertung, Cache und Rückschreiben sind in Rust (Wasm), die UI in TypeScript/React. Später kommen eine CLI (auch zur Nutzung durch Claude Code) und eventuell eine Tauri-Version dazu.

Festgelegte Rahmenbedingungen:

| Thema | Entscheidung |
| --- | --- |
| Kernel | Bis zur Veröffentlichung: SMLib (C++, Upstream von solidcore) als Wasm. Vor der Veröffentlichung: Umstieg auf solidcore. |
| Plattform | Desktop-Browser; keine Touch-Bedienung |
| Speicherung | Nur im Browser; kein Backend |
| Header | Keine Cross-Origin-Isolation (COOP/COEP), solange nicht nötig. Damit auch kein `SharedArrayBuffer` und keine Threads. |
| OpenSCAD | Weitgehend kompatibel, nicht vollständig (Abschnitt 5) |
| Priorität | Raster vor Fillets |

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

Bis solidcore so weit ist, nutzt die Anwendung den C++-Upstream direkt, auf demselben Stand, gegen den solidcore prüft (`usd-brep` @ `c9c5979`), seit Bericht 4 mit den Speicherfehler-Korrekturen aus dem Fork `tangency-labs/smlib` (Zweig `capi-tess` @ `7eba8aa`, `smlib/fetch.sh`):

| Kriterium | SMLib (C++) als Übergang | OCCT als Übergang |
| --- | --- | --- |
| Ergebnisse beim Umstieg auf solidcore | gleiche Algorithmen, gleiche Topologie | andere Fillets, andere Kantenaufteilung, andere Fehlerfälle |
| API-Konzepte | wie solidcore (`SM_API`) | eigenes Modell, Übersetzung nötig |
| Lizenz | Apache-2.0 | LGPL 2.1 mit Ausnahme |
| Reife | Produktivkernel, aber bekannte UB/Hänger (solidcore: `plan/upstream-issues.md`) | sehr verbreitet |
| STEP-Export | nicht enthalten (Upstream nutzt dafür OCCT) | enthalten |
| Wasm-Build | unbekannt, im Spike prüfen | bekannt machbar |

Empfehlung: SMLib. OCCT bleibt die Rückfallebene, falls SMLib sich im Spike nicht mit Emscripten bauen lässt oder bei einfachen Teilen scheitert.

### 2.2 Anbindung im Browser

| Teil | Technik |
| --- | --- |
| SMLib | Emscripten-Build der Bibliotheken `SMLib` und `SM_API` (CMake, `-O3 -fwasm-exceptions`, `ALLOW_MEMORY_GROWTH`, ohne Threads) |
| Fassade | Eigene C++-Schicht (~1–2 kLOC) mit flacher C-ABI: Handles (`u32`) statt Objekte, Fehlercodes statt Exceptions |
| Rust → SMLib | `kernel-smlib` ruft die Fassade über eine dünne JS-Brücke (`#[wasm_bindgen(module = "/smlib-bridge.js")]`) |
| Daten | Kleine Daten (Punkte, Kanten-IDs) werden kopiert; Meshes gehen aus dem C++-Heap direkt als `ArrayBuffer` an den Main Thread |

Zwei Wasm-Module statt einem: `wasm-bindgen` unterstützt das Emscripten-Target nicht. Ein gemeinsames Modul (Rust auf `wasm32-unknown-emscripten`) wäre möglich, die Toolchain aber fragiler. Die Brücke kostet Mikrosekunden pro Aufruf; Kernel-Operationen dauern Millisekunden. Massenaufrufe (Klassifizieren vieler Punkte) werden gebündelt.

**Nativ:** Dieselbe Fassade wird für CLI und Tests nativ gebaut und per FFI gelinkt. solidcore bringt dafür bereits ein Build-Skript für die C++-Bibliotheken mit (`harness/cpp/build.sh`, Ausgabe `libSMLib.a`, `libSM_API.a`).

### 2.3 Umstieg auf solidcore

- `kernel-solidcore` implementiert denselben Trait direkt in Rust, im selben Wasm-Modul. Die JS-Brücke und `smlib.wasm` entfallen.
- solidcore baut für `wasm32-unknown-unknown`. Laut solidcore-README braucht der Konsument einen größeren Stack (`-zstack-size=8388608`), weil Wasm keine Guard-Page hat.
- Abnahme: Die CLI wertet alle Beispiele mit beiden Kerneln aus und vergleicht Volumen, Flächen- und Kantenanzahl und ausgewählte Kanten (Abschnitt 8). Erst wenn alle übereinstimmen, wird umgestellt.
- Bis dahin kann `kernel-solidcore` als zweite Implementierung mitlaufen und meldet, welche Operationen schon gehen.

### 2.4 Fassade: Kernpunkte

- **Handle-Tabelle** mit Referenzzählung. Rust hält `Shape(u32)`, `Drop` gibt frei.
- **Provenienz:** Jede Fläche trägt einen Tag als SMLib-Attribut. SMLib reicht Attribute bei Booleans weiter (`SmAttribute::Split/Merge`). Im Spike wird geprüft, wie sie sich bei Fillets, Fasen und Sweeps verhalten. Rückfall: Tag über die nächstgelegene Fläche der Eingaben bestimmen.
- **Neue Kanten eines Booleans** werden kernelunabhängig bestimmt: Eine Kante ist neu, wenn ihre beiden angrenzenden Flächen von verschiedenen Kindern dieser Operation stammen. Das entspricht der Definition der Spec und braucht nur die Flächen-Tags.
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
| `PlaneRef` (`g.name`) | Raster-ID + Ebenenname | wird kontextabhängig zu Zahl oder Ebene (Abschnitt 11) |
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
| Mit Abweichung | `color()` nur als Anzeigefarbe; `$fn/$fa/$fs` siehe Abschnitt 11; `polyhedron` nur für kleine Körper mit ebenen Flächen; `resize` als `scale` mit denselben Regeln |
| Voll (Projekte) | `include`/`use` mit Projekten aus mehreren Dateien (Pfade relativ zur einbindenden Datei; ohne Suchpfad `OPENSCADPATH`) |
| Später | `text()` (Schriften), `offset()` in 2D, `hull()` |
| Nicht vorgesehen | `minkowski`, `projection`, `import` von Meshes, `surface()` als Höhenkarte (der Name ist in der Spec für NURBS-Flächen vergeben) |

Ein Korpus von `.scad`-Dateien (Lizenzen vorher prüfen) läuft in CI über die CLI. Gemessen wird pro Datei: parst, wertet aus, erzeugt Geometrie.

---

## 6. Engine (`cad-engine`)

### 6.1 Ablauf einer Auswertung

1. Parsen. Bei Syntaxfehler: Diagnosen senden, letzte Szene bleibt.
2. Sprachauswertung → Geometriebaum.
3. Szenenbeschreibung sofort senden (Knoten, Raster, Anker, Griffe, Hinweise). Raster und Griffe sind damit schon sichtbar.
4. Kernel-Auswertung von unten nach oben, in Schritten (7.3); pro Knoten Cache-Lookup über `CacheKey`.
5. Tessellierung der Wurzel und der von der UI angeforderten Knoten; Meshes als Transferables.

### 6.2 Cache

- `CacheKey`: stabiler Hash (z. B. `blake3`) über Operation, Parameter (Floats mit kanonisiertem Bitmuster), Kinder-Schlüssel und Kernel-Version.
- Eintrag: Kernel-Shape, Tag-Tabelle, optional Meshes pro Toleranz. LRU nach geschätztem Speicher; Freigabe gibt Kernel-Handles frei.
- **Transformationen werden für die Anzeige nicht in die Shapes gebacken.** Ein Mesh wird im lokalen System tesselliert und in three.js per Matrix platziert. Gleiche Teile in Schleifen teilen sich ein Mesh.

### 6.3 Provenienz (Fläche/Kante → Zeile)

- Kernel-Tags sind lokal zum gecachten Teilbaum: Index in die Liste der erzeugenden Knoten dieses Teilbaums.
- Beim Kombinieren verschiebt die Fassade die Tags der Kinder in den Bereich des Elternknotens.
- Nach der Auswertung kennt die Engine für jeden Knoten die Liste `lokaler Tag → InstanceId`. Ein gecachtes Ergebnis wird dadurch an anderer Stelle wiederverwendet, ohne die Geometrie anzufassen.
- Die UI bekommt pro Mesh ein Attribut `tag` pro Dreieck und Kantensegment plus die Tabelle `tag → InstanceId → Zeile`.

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

- Während eines Ziehvorgangs: grobe Toleranz (z. B. 10 × `$tol`), nur die Wurzel.
- Bei `DragEnd`: feine Tessellierung. Meshes werden pro Toleranz gecacht.

### 7.3 Abbruch ohne `SharedArrayBuffer`

- Die Engine rechnet in Schritten: `engine.step(budget_ms)` bearbeitet Knoten, bis das Zeitbudget (z. B. 20 ms) erreicht ist, und kehrt zurück.
- Die Worker-Schleife gibt zwischen zwei Schritten die Kontrolle an die Event-Loop ab (`MessageChannel`), verarbeitet neue Nachrichten und bricht bei `SetSource` die laufende Auswertung ab. Fertige Knoten bleiben im Cache.
- Eine einzelne lange Kernel-Operation ist nicht unterbrechbar. Bei Hobbyprojekten ist das selten ein Problem.
- **Watchdog:** Der Main Thread beendet den Worker, wenn ein Schritt länger als z. B. 15 s dauert (Schutz vor Hängern im Kernel), und startet ihn neu. Der Cache geht dabei verloren. Der Schlüssel des betroffenen Knotens wird gemerkt und bis zur nächsten Parameteränderung nicht erneut gerechnet; die Zeile wird rot markiert.

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
| `export <datei> -o teil.stl\|.3mf [-D …]` | Exportdatei |
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
| 3D | three.js über react-three-fiber; `three-mesh-bvh` für Picking; `Line2` für Kanten und Rasterlinien |
| Picking | Raycast → Dreieck → `tag` → `InstanceId` → Zeile; Hover und Klick ohne Engine-Aufruf |
| Darstellungszustände | Material-Varianten pro Knoten: voll, gedimmt, halbtransparent, rot halbtransparent (`#`, abgezogen), grau (`%`), Umriss |
| Griffe | Eigene Gizmos (Punkt, Pfeil, Drehring mit 15°-Rasten, Ebenengriff); Snap-Suche im Bildschirmraum gegen Rasterpunkte und Anker |
| Raster | Ebenen als Linien am Rand der Bauteilhülle; durchgezogen (Literal) oder gestrichelt (Ausdruck); standardmäßig nur die von der aktuellen Zeile verwendeten |
| Parameterleiste | aus der Modulsignatur; Werte pro Modul in der Sitzung; „Als Standard übernehmen“ erzeugt einen `TextEdit` |
| Pfadleiste | aus dem Modulkontext; Doppelklick auf Instanz springt in die Definition |
| Dateien | Autospeichern in IndexedDB; Öffnen/Speichern über File System Access API (Chromium), sonst Hoch- und Herunterladen |

Eine Zeile kann mehrere Instanzen haben (Schleifen, mehrfache Modulaufrufe). Die UI indiziert die Szene nach `stmt` und hebt alle hervor.

---

## 10. Repository-Struktur

```
crates/
  cad-syntax/       Lexer, Parser, AST, Spans
  cad-eval/         Namensauflösung, Werte mit Herkunft, Raster, Anker, Auswertung
  cad-ir/           Geometriebaum, CacheKey
  cad-kernel/       trait Kernel, gemeinsame Typen (P3, Mat4, Mesh, Tag …)
  kernel-mock/      Testkernel in Rust
  kernel-smlib/     Kernel über SMLib-Fassade (Features: wasm | native)
  kernel-solidcore/ Kernel über solidcore (ab Umstieg)
  cad-engine/       Cache, Provenienz, Zonen, Griffe, Rückschreiben, Export STL/3MF
  cad-render/       Software-Rasterizer für die CLI
  cad-wasm/         wasm-bindgen-Einstieg für den Worker
  cad-cli/          CLI
smlib/
  facade/           C++-Fassade + CMake
  build/            Emscripten- und native Builds (tangency-labs/smlib @ 7eba8aa = usd-brep c9c5979 + Korrekturen)
web/
  src/editor/  src/viewport/  src/engine/  src/panels/  src/state/
examples/           Beispieldateien (zugleich Testfälle)
docs/
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

CI: GitHub Actions; SMLib-Wasm und SMLib-nativ als gecachte Build-Artefakte.

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

| # | Inhalt | Ergebnis |
| --- | --- | --- |
| M0 | **Spike:** SMLib mit Emscripten bauen; Fassade mit Quader, Zylinder, Boolean, Fillet, Klassifizieren, Tessellierung, Attribut-Weitergabe; Aufruf aus Rust im Worker und nativ; Anzeige in three.js | Größe, Ladezeit, Aufrufkosten, Stabilität gemessen; Entscheidung SMLib oder OCCT |
| M1 | Sprachkern: Parser, Variablen, Funktionen, Module, `for`/`if`/`let`, 3D-Primitive, Transformationen, Booleans; Editor mit Diagnosen und Inline-Werten; CLI `check`, `eval` | OpenSCAD-ähnliche Teile modellierbar |
| M2 | Szene und Zuordnung: Provenienz, Cursor/Hover in beide Richtungen, Dimmen, Modifier, Cache, schrittweise Auswertung, Watchdog, Modulkontext, Pfadleiste; CLI `inspect`, `render`, `export` (STL) | Code ↔ Ansicht vollständig; Claude Code kann Modelle prüfen und ansehen |
| M3 | Raster und Anker: `grid`, Zugriffe, Platzierung, Sichtbarkeit, `anchor`, benannte Instanzen, `rotate(to = …)`, Darstellung | Raster nutzbar |
| M4 | Griffe und Rückschreiben: Herkunft der Werte, Griffe für Primitive, Transformationen und Rasterebenen, Snappen, Undo, Parameterleiste | Bearbeiten in der Ansicht |
| M5 | Verrundung und Fase: `$fillet`, `fillet =` an Booleans, Zonenblöcke, `select`, Fehlerdarstellung | Fillets wie in der Spec |
| M6 | 2D und Profiloperationen: Profile, `linear_extrude`, `rotate_extrude`, `loft`, `sweep`, `surface`, `thicken`, `shell` | Freiformen |
| M7 | Dateien, 3MF, OpenSCAD-Korpus, Beispiele, Leistung | erste vollständige Version |
| M8 | Umstieg auf solidcore: `kernel-solidcore`, Vergleich über `diff-kernels`, SMLib-Wasm entfernen | bereit zur Veröffentlichung |
| später | STEP-Export, Tauri, variabler Radius, mehrere Bauteile, `include`/`use` | |

Abhängigkeiten: M1 → M2 → M3 → M4 → M5. M6 hängt nur an M2. M8 hängt am Fortschritt von solidcore und kann jederzeit nach M2 teilweise beginnen.

---

## 14. Risiken

| Risiko | Gegenmaßnahme |
| --- | --- |
| SMLib lässt sich nicht mit Emscripten bauen | im Spike klären; Rückfall OCCT (gleicher Trait, andere Fassade) |
| Kernel hängt oder verhält sich undefiniert bei bestimmten Eingaben | Watchdog mit Neustart (7.3); gefundene Fälle an solidcore melden |
| solidcore ist zur geplanten Veröffentlichung nicht fertig | Veröffentlichung mit SMLib-C++ ist lizenzrechtlich möglich (Apache-2.0); Umstieg danach |
| Attribute gehen bei einzelnen Operationen verloren (Provenienz) | Rückfall über geometrische Zuordnung; Tests pro Operation |
| Fillets scheitern bei Grenzfällen | klare Fehlermeldung mit Kante, Körper ohne Fillet anzeigen (Spec-konform) |
| Wasm-Stack ohne Guard-Page (solidcore) | Stack auf 8 MiB setzen, wie im solidcore-README beschrieben |
| Speicher (wasm32, 4 GB) | LRU-Cache mit Speicherschätzung, Handles konsequent freigeben |
