# Technischer Plan: Parametrische CAD-Sprache mit Rastern

Stand: 2026-10-23, Ist-Stand nach Testbericht 11 (erste Fassung 2026-10-22) · Grundlage: `Spec: Parametrische CAD-Sprache mit Rastern` (22.10.2026)

Abschnitt 0 fasst zusammen, was sich gegenüber der ersten Fassung geändert hat; die Abschnitte 1–14 beschreiben den umgesetzten Stand, Abschnitt 13 den Status der Meilensteine, Abschnitt 15 die bekannten Grenzen, Abschnitt 16 Farben, Filamente und Thema, Abschnitt 17 Skizzen, Text und Skizzen-Editor, Abschnitt 18 das Messen. Testberichte: `docs/reports/` (B1 … B11 = Bericht 1 … 11, Übersicht in `docs/reports/README.md`).

Ziel: Die Anwendung läuft vollständig im Browser, nur auf dem Desktop, für kleine Hobbyprojekte. Sprache, Auswertung, Cache und Rückschreiben sind in Rust (Wasm), die UI in TypeScript/React. Dazu gibt es eine CLI (auch zur Nutzung durch Claude Code) und eine Desktop-Version (Tauri 2, B9) mit nativem Kernel, echten Dateien, Dateibeobachter und Git-Markierungen.

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
| Watchdog-Sperre | Schlüssel merken, nicht erneut rechnen | 20 min (`WorkerTransport`, vorher 15 s; große Modelle rechnen minutenlang); gesperrt wird der Inhaltsschlüssel des Knotens aus dem letzten `Progress.current`; der neue Worker bekommt `BlockKeys` vor `SetSource`; die Sperre endet mit einer Änderung des Knotens (neuer Schlüssel) oder dem Neuladen der Seite | B2 (S17), B4 (C19) |
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
| solidcore | `kernel-solidcore` ab Umstieg | `kernel-solidcore` und `mcad diff-kernels` vorhanden, opt-in (Feature `solidcore`), Pin `c9de85d`: Quader und einfache Extrusion gleich wie SMLib, übrige Primitive, Transformation, Booleans noch nicht; Tessellierung nicht im Umfang von solidcore (eigener Tessellierer nötig), M8 wartet | B5, B6, `docs/solidcore-umstieg.md` |
| UI-Bibliotheken | react-three-fiber, `three-mesh-bvh`, `Line2` | three.js direkt (Raycaster, Linien als `LineSegments`), ohne r3f und BVH; TS-Protokolltypen aus Rust mit `ts-rs` | B2 |
| CI | GitHub Actions, gecachte SMLib-Builds | Workflows `rust`, `web`, `desktop`, `compat`, `pages` (manuell) in `.github/workflows/`, Schritte in `scripts/ci/` (Abschnitt 11) | B9 |
| Tauri-Version | „später, optional“ | `crates/cad-app` (Tauri 2) im Binary `mcad` (Feature `gui`, Standard): `mcad` / `mcad datei.mcad` / `mcad ordner/` startet die UI. Engine in einem eigenen Prozess (`mcad engine-host`, Engine-Thread mit 256 MiB Stack, schrittweise wie im Worker), weil SMLib globalen Zustand hat und ein hängender Aufruf nur mit dem Prozess beendet werden kann; Antworten binär über einen Tauri-Kanal (`cad-desktop/src/wire.rs`). Dateien auf der Festplatte, Dateibeobachter (`notify`), Git-Rand gegen `HEAD` (`git`-Aufruf + `similar`), Steuerung durch `mcad ui status/show/screenshot` über lokale IPC (`cad-ipc`) | B9 |
| Schnittansicht | – | GPU-Schnitt in Browser und Tauri: Clipping-Ebene, Stencil-Kappe (Paritäts-Pass je Körper, schraffiert wie `mcad render --section`), Kontur, Griff mit Rasten auf Rasterebenen und Anker; „Als `mcad render …` kopieren“ | B9 |
| `color()` | CSS-Namen wie OpenSCAD | eigene Palette aus 16 gedeckten Farben (Abschnitt 16.1), unbekannte Namen: Warnung und stabile Zufallsfarbe; `filament = n` | B11 |
| Mehrfarbdruck | – | Farbregionen über Kernel-Booleans, Filament-Slots, 3MF mit einem Teil je Farbe und Bambu/Orca-Metadaten, ein STL je Farbe (Abschnitt 16) | B11 |
| Messen | – | Messmodus in UI und `mcad measure` mit gemeinsamer Logik in Rust (`cad-measure`, `cad_session::measure`), exakt an der B-Rep, im Schnitt über Ebene ∩ B-Rep aus dem Kernel (Abschnitt 18) | B19 |
| Thema | Hell/Dunkel umschaltbar | „System“ (Standard, folgt `prefers-color-scheme`, auch in Tauri), Hell, Dunkel; Token-Satz mit WCAG-Prüfung; `mcad render --theme dark` | B11 |

---

## 1. Architektur

```
┌──────────────────────────── Main Thread (React/TS) ────────────────────────────┐
│  Editor (CodeMirror 6)      3D-Ansicht (three.js)      Parameterleiste, Pfadleiste │
│        │  ▲                        │  ▲                                          │
│        ▼  │                        ▼  │                                          │
│  ─────────────────────── EngineTransport (TS-Interface) ──────────────────────── │
│        Browser: Web Worker + postMessage      Tauri: Kanal + Engine-Prozess      │
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
      Einstiege:  cad-wasm (Browser-Worker) · cad-cli (nativ) · cad-app/cad-desktop (Tauri, nativ)

Desktop (Tauri 2, B9):
  mcad (App-Prozess): Tauri-Hauptthread + Webview (dieselbe React-App), Supervisor mit Watchdog,
                     Dateibeobachter, Git-Basis, Steuer-Socket ui-<pid> (cad-ipc)
     │ stdin: Rahmen `post` [ToEngine…]            ▲ stdout: binäre Antwort-Batches (wire.rs)
     ▼                                              │
  mcad engine-host (Engine-Prozess): Engine-Thread 256 MiB, Session::post/step(16 ms), SMLib nativ
  mcad ui status|show|screenshot (CLI) ──── Unix-Socket im Benutzerverzeichnis ────▶ App
```

Grundsätze:

- **Die Engine kennt weder Browser noch Transport.** Dieselben Crates laufen im Worker (Wasm), in der CLI (nativ) und im Engine-Prozess der Desktop-App (nativ). Das Protokoll ist ein Satz `serde`-Typen.
- **Die Engine besitzt den Zustand** (Quelltext-Version, Cache, Kernel-Shapes). Die UI hält nur Darstellungsdaten.
- **Cursor und Hover brauchen keine Neuberechnung.** Die Engine liefert pro Auswertung eine vollständige Szenenbeschreibung (alle Knoten mit Quellbereich, Transformation, Art, Eltern). Die UI entscheidet selbst über Hervorheben, Dimmen und Ausblenden. Meshes werden pro Knoten bei Bedarf nachgeladen.
- **Volle Neuauswertung der Sprache bei jeder Änderung, Kernel-Operationen aus dem Cache.** Die Sprachauswertung dauert Millisekunden. Teuer sind nur Kernel-Aufrufe; sie werden über inhaltsbasierte Schlüssel wiederverwendet. Damit folgt „nur der Pfad zur Wurzel wird ungültig“ automatisch.

---

## 2. Kernel

### 2.1 Übergang über SMLib (C++)

solidcore ist ein Rust-Port von SMLib aus NVIDIAs `usd-brep` (Apache-2.0) mit dem Ziel gleicher Topologie und numerischer Abweichung ≤ 1e-9. Stand `c9de85d` (2026-10-23): 238 von 261 Port-Einheiten portiert (89,8 % der Zeilen); end to end laufen der Quader, lineare Extrusion, Abfragen und Masseneigenschaften, Primitive mit gekrümmten Flächen, Transformation, Booleans und Fillets noch nicht.

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
- Tessellierung und Polyeder sind ausdrücklich nicht im Umfang von solidcore (`plan/closure.md`). Für M8 braucht es einen eigenen Tessellierer in Rust (Kanten einmal diskretisieren, ebene Flächen als Polygon mit Löchern, gekrümmte im Parameterraum mit Trimmen und Kantenabgleich, Sehnenfehler-Steuerung; Skizze in `docs/solidcore-umstieg.md`), sonst bleibt SMLib oder OCCT allein dafür im Build.
- Fillet und Fase haben in solidcore noch keine öffentliche API; öffentliche Einstiege für Transformation, Sweeps (linear, Rotation, Pipe, Skin), Offset/Shell und Klassifizierung existieren (Stand und Ergebnisse in `docs/solidcore-umstieg.md`).

### 2.4 Fassade: Kernpunkte

- **Handle-Tabelle** mit Referenzzählung. Rust hält `Shape(u32)`, `Drop` gibt frei.
- **Provenienz:** Jede Fläche trägt einen Tag als SMLib-Attribut. SMLib reicht Attribute bei Booleans weiter (`SmAttribute::Split/Merge`); neu erzeugte Flächen (Rundungen, Deckel) bekommen den Tag der Operation. Die Tags vergibt die Engine (6.3).
- **Neue Kanten eines Booleans** werden kernelunabhängig bestimmt: Eine Kante ist neu, wenn ihre beiden angrenzenden Flächen von verschiedenen Kindern dieser Operation stammen und sie nicht auf einer Kante eines Operanden liegt (B7, F4). Glatte Kanten (`sc_edges`-Flag) zählen nie.
- **Fehler:** Jede Funktion fängt Exceptions, gibt einen Code zurück und legt Meldung sowie betroffene Kanten ab (für „Kante rot“).
- **Hänger:** Upstream hat bekannte Endlosschleifen bei bestimmten Eingaben. Siehe Watchdog in 7.3.

### 2.5 Export

- STL und 3MF aus der Tessellierung, in Rust, kernelunabhängig. 3MF mehrfarbig (Standard) mit einem Teil je Farbe und Filament-Metadaten für Bambu Studio/OrcaSlicer, „ein STL je Farbe“ als ZIP (Abschnitt 16.4).
- STEP: weder SMLib noch solidcore haben einen Writer. Umgesetzt (Testbericht 5): neutrale B-Rep-Beschreibung `cad_kernel::brep` (`Kernel::brep`, bei SMLib über die Fassadenabfrage `sc_brep`), Writer `cad-step` (AP242, wahlweise AP214; Ebene, Zylinder, Kegel, Kugel, Torus, Rotations-/Extrusionsfläche, B-Spline; Linie, Kreis, Ellipse, B-Spline), `Engine::export_step`, `mcad export -o teil.step`, `Export { format: "step" }`. Prüfung mit OpenCASCADE: `tests/step/verify.py`.

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
| Voll | Syntax, Variablen, Funktionen und Funktionsliterale, Module, `children()`, `for`, `if`, `let`, `each`, Listenausdrücke, Strings, Bereiche, `echo`, `assert`, Mathefunktionen, `cube`, `sphere`, `cylinder`, `square`, `circle`, `polygon`, Transformationen, Booleans, Modifier, `linear_extrude`, `rotate_extrude`, `offset` (exakte Bögen statt Facetten, 17.9) |
| Mit Abweichung | `color()` nur als Anzeigefarbe; `$fn/$fa/$fs` siehe 12.3; `polyhedron` nur für kleine Körper mit ebenen Flächen; `resize` als `scale` mit denselben Regeln |
| Voll (Projekte) | `include`/`use` mit Projekten aus mehreren Dateien (Pfade relativ zur einbindenden Datei; ohne Suchpfad `OPENSCADPATH`) |
| Mit Abweichung (Bericht 16) | `text()` mit den mitgelieferten Schriften oder Schriftdateien des Projekts (17.10), `import()` von SVG (17.10; Abweichungen bei Einheiten ohne `viewBox`, Strichen und Füllregel) |
| Exakte Sonderfälle (Plan 21) | `hull()` (2D immer; 3D: ebene Körper, Kugeln/Zylinder/Kegel auf einer Achse, parallele Prismen gleicher Höhe, gleich große Kugeln) und `minkowski()` (Körper + Kugel, Profil + Kreis, Extrusion + Zylinder, zwei konvexe ebene Körper); sonst Warnung, Kinder unverändert (Abschnitt 20) |
| Nicht vorgesehen | `projection`, `import` von Meshes, `surface()` als Höhenkarte (der Name ist in der Spec für NURBS-Flächen vergeben) |

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
- Eintrag: Kernel-Shape, Tag-Tabelle, optional Meshes pro Toleranz. Ein LRU für Knotenergebnisse und Meshes nach geschätztem Speicher (`cache_bytes`, Standard 512 MiB) und Anzahl (`cache_entries`); Größe eines Shapes über `Kernel::shape_bytes` (SMLib: aus Flächen/Kanten, an den Beispielen kalibriert) oder, im CLI-Daemon, gemessen am Heap während der Auswertung des Knotens (Bericht 10). Freigabe gibt Kernel-Handles frei.
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
| `Sketch { … }` | Skizzen-Editor (17.12) |
| `Measure { kind, picks, ray, hit, section, clip, click }` | Messmodus (Abschnitt 18): Element unter der Maus, Messung nach dem letzten Klick |

Engine → UI: `Diagnostics`, `Scene { version, nodes, grids, anchors, handles, hints }`, `Meshes`, `TextEdits`, `ExportResult`, `Progress`.

Übergabe mit `serde` + `serde-wasm-bindgen`, Meshes als `Float32Array`/`Uint32Array`-Transferables. TS-Typen werden aus Rust generiert (`tsify` oder `ts-rs`).

`EngineTransport` in TS kapselt den Weg: `WorkerTransport` (Browser) oder `TauriTransport` (Desktop, `web/src/engine/tauriTransport.ts`). Die UI kennt nur das Interface.

`TauriTransport`: Nachrichten an die Engine gehen gebündelt pro Microtask als ein Befehl `engine_post` (Reihenfolge bleibt); die Antworten kommen über einen Tauri-2-Kanal als Binärrahmen `[u32 Kopflänge][u32 Datenlänge][Kopf-JSON, auf 4 Bytes aufgefüllt][Daten]` (Format von `cad-ipc`). Der Kopf enthält die `FromEngine`-Nachrichten als JSON, die Mesh-Arrays und Exportbytes sind durch `{"$b": [offset, länge, "f32"|"u32"|"u8"]}` ersetzt; der Webview legt `Float32Array`/`Uint32Array` direkt auf den empfangenen `ArrayBuffer` (keine JSON-Zahlenarrays, keine Kopie). Die Desktop-Oberfläche wird ohne Worker und ohne Wasm gebaut (`pnpm build:tauri`, `vite --mode tauri` ersetzt `workerFactory.ts`).

### 7.2 Feine und grobe Tessellierung

- Während eines Ziehvorgangs: grobe Toleranz (10 × Toleranz), nur die Wurzel. Entwurfsmodus: Verrundungen über 25 ms (letzte Messung, nicht im Cache) werden ausgesetzt (B4, C20).
- Bei `DragEnd`: feine Tessellierung. Meshes werden pro Toleranz gecacht.

### 7.3 Abbruch ohne `SharedArrayBuffer`

- Die Engine rechnet in Schritten: `engine.step(budget_ms)` bearbeitet Knoten, bis das Zeitbudget (z. B. 20 ms) erreicht ist, und kehrt zurück.
- Die Worker-Schleife gibt zwischen zwei Schritten die Kontrolle an die Event-Loop ab (`MessageChannel`), verarbeitet neue Nachrichten und bricht bei `SetSource` die laufende Auswertung ab. Fertige Knoten bleiben im Cache.
- Eine einzelne lange Kernel-Operation ist nicht unterbrechbar. Bei Hobbyprojekten ist das selten ein Problem.
- **Watchdog:** Der Main Thread beendet den Worker, wenn 20 min keine Antwort kommt (Schutz vor Hängern im Kernel), und startet ihn neu. In der Desktop-App übernimmt der Supervisor im App-Prozess dasselbe (`cad-desktop/src/engine.rs`): er zählt gesendete und von der Engine übernommene Nachrichten, beendet bei 20 min ohne Antwort den Engine-Prozess (`kill`) und startet ihn neu, mit denselben `BlockKeys`/`SetFiles`/`SetContext`/`SetSource` wie im Browser; ein Thread allein ließe sich nicht beenden und teilte den globalen SMLib-Zustand mit dem hängenden Aufruf. Der Cache geht dabei verloren. Der Inhaltsschlüssel des Knotens aus dem letzten `Progress.current` wird gesperrt; der neue Worker bekommt `BlockKeys` vor `SetSource`. Der Knoten meldet „Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung“, die Zeile wird rot (B4, C19).

### 7.4 Versionen

Die Szene trägt die Quelltext-Version. Bis eine neue Szene da ist, bildet die UI Quellpositionen alter Szenen über die zwischenzeitlichen Editor-Änderungen ab (`ChangeSet.mapPos`), damit Hervorhebungen beim Tippen nicht springen.

---

## 8. CLI (`cad-cli`)

Ein natives Programm `mcad` (Crate `cad-cli`; bis 0.2 hieß das Binary `cad`) auf denselben Crates wie der Browser, mit nativ gelinktem Kernel. Zweck: Nutzung durch Claude Code und andere Agenten (Modelle ganz ohne UI entwickeln und prüfen, auch parallel zu einem Menschen in der UI), Tests in CI, Kernel-Vergleich vor dem Umstieg auf solidcore.

| Befehl | Ausgabe |
| --- | --- |
| `check <datei>` | Diagnosen mit Zeile und Spalte |
| `outline <datei>` | Übersicht in einem Aufruf: Module (Datei:Zeile, Parameter mit Standardwert und Typ, Raster und Anker mit Standardwerten), Funktionen, Variablen mit Werten, benannte Instanzen, Bauteile (Hülle, Volumen), `include`/`use`, Diagnosen |
| `eval <datei> [--module m] [-D name=wert]` | Baum, Werte, Inline-Hinweise, Raster, Anker |
| `inspect <datei> --line n` | Was die Zeile erzeugt: Knoten, Hülle, Volumen, ausgewählte Kanten, Warnungen |
| `render <datei> -o bild.png` | Bild: `--view`, Kontaktbogen `--views iso,front,top,right\|all` (gleiche Skalierung), freie Kamera (`--azimuth/--elevation/--zoom/--target/--fit`, `--perspective`), Auswahl (`--part`, `--line` mehrfach, `--node`, `--only`), Überlagerungen `--show grids,anchors,handles,edges,labels,bbox,dims`, Schnitt `--section x\|y\|z=wert`/`--section-plane p,n`/`--section-flip`, `--xray` |
| `measure <datei>` | `--between A B` (Anker, Rasterpunkt, Koordinate), `--bounds/--volume/--area ZIEL`, `--interference` (Kernel-Schnitt aller Bauteilpaare), `--clearance A B` (Abstand über die Tessellierung); an der exakten B-Rep (Abschnitt 18) `--distance`, `--radius`, `--angle`, `--length`, `--area face@P`, `--body`, `--at`, im Schnitt `--section …` (Konturen, Kreise, kleinste Wand) und `--wall P` |
| `watch <datei> [--render bild.png …]` | Beobachtet Datei und Projektdateien (`notify`), wertet in derselben Sitzung neu aus (Cache bleibt), eine JSON-Zeile je Lauf |
| `set <datei> --param m.p=wert \| --literal [datei:]z:s=wert` | Code über die Engine ändern (Standardwert, Literal), prüft vor dem Schreiben, atomar, `--dry-run` |
| `export <datei> -o teil.stl\|.3mf\|.step [-D …] [--schema ap214] [--part name] [--all-parts]` | Exportdatei (STEP: exakte B-Rep, AP242), einzelne oder alle Bauteile |
| `diff-kernels <datei…>` | Vergleich SMLib ↔ solidcore (nur Entwicklung, `docs/solidcore-umstieg.md`) |

- Jeder Befehl hat `--json` mit festem, versioniertem Schema (`schema_version`) und definierte Exit-Codes (0 ok, 1 Modellfehler, 2 Aufruf, 3 Kernel). Ohne `--json` gibt es lesbaren Text.
- `mcad` ohne Argumente startet die Desktop-UI, `mcad datei.mcad` bzw. `mcad ordner/` mit dieser Datei bzw. diesem Ordner als Projekt (Feature `gui`, Standard; `--no-default-features` baut eine CLI ohne UI). Ohne grafische Sitzung (kein `DISPLAY`/`WAYLAND_DISPLAY`) oder ohne `gui`: Meldung mit den headless Befehlen, Exit 2. `mcad --help` erklärt beides.
- `mcad ui status [--json]` / `mcad ui show datei [--line N] [--module m]` / `mcad ui screenshot -o x.png` sprechen über lokale IPC (`cad-ipc`, Unix-Socket `ui-<pid>` im Benutzerverzeichnis, nur der eigene Benutzer) mit laufenden Fenstern; ohne laufende UI Exit 3. Für den Ablauf „Nutzer in der UI, Agent editiert und prüft“ siehe Skill.
- `render` nutzt einen kleinen Software-Rasterizer in Rust (`cad-render`: Z-Buffer, Schattierung, Kantenlinien, Ortho/Perspektive, Marker, Beschriftung mit eingebetteter Bitmap-Schrift). Er braucht keine GPU und läuft damit auch in Containern und CI.
- Schnitt: Clipping im Renderer mit gefüllter Kappe nach dem Stencil-Prinzip (Kappe, wo der Sichtstrahl bis zur Ebene die Oberfläche ungerade oft kreuzt), kein Kernel-Boolean. Gleiches Verfahren und gleiche Konvention wie die spätere GPU-Schnittansicht der UI: Ebene durch `p` mit Normale `n` entfernt die Hälfte, in die `n` zeigt (es bleibt `(q − p) · n ≤ 0`).
- **Daemon** (Bericht 10): Kernel-Ergebnisse sind Handles der nativen Bibliothek und nicht serialisierbar; statt eines Cache auf der Platte hält ein Hintergrundprozess den Engine-Cache. `check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure` und `set` verbinden sich über `cad-ipc` (Endpunkt `daemon`, Socket-Ordner 0700, Socket 0600, Peer-UID) mit `mcad daemon serve` oder starten ihn abgekoppelt (neue Sitzung, Ausgabe in `daemon.log`); genau ein Daemon über eine `flock`-Sperre. Der Client schickt Argumente, Arbeitsverzeichnis und stdin; der Daemon führt dieselbe Befehlsfunktion aus (Sitzung auf seiner Engine, Wechsel ins Arbeitsverzeichnis, Dateischreiben wird aufgezeichnet) und liefert stdout, stderr, Exit-Code und die Dateien, die der Client schreibt. Ausgabe wie ohne Daemon, auch die Flächen-Tags: die Sitzung nummeriert sie in der Reihenfolge, in der eine frische Engine sie vergäbe. Eine Engine für alle Projekte (inhaltsbasierte Schlüssel); Grenzen 512 MiB (Knotenergebnisse + Netze; Kosten eines Knotens im Daemon am Heap gemessen, sonst aus der Topologie geschätzt), 16384 Einträge, 8 Projekte, Cache leeren bei RSS über Start + 1,5 × Grenze; Ende nach 15 min ohne Anfrage; Watchdog 30 min (Daemon endet, der Befehl läuft ohne Daemon und meldet das); andere Build-ID ersetzt den Daemon. Aus mit `--no-daemon`, `MCAD_NO_DAEMON=1`, `[daemon] enabled = false` (`$XDG_CONFIG_HOME/mcad/config.toml`); ohne Daemon (Windows, Fehler) rechnet der Befehl still selbst. `mcad daemon status|stop|serve`. `mcad watch` behält seinen eigenen Cache im Prozess.
- **Umbenennung `cad` → `mcad` (0.3.0):** Binary, Programmname, App-ID `dev.mcad.desktop` (Fenstertitel „… – mcad“, Desktop-Datei `dev.mcad.desktop.desktop`, Dateitypen `text/x-mcad` und `application/x-openscad` in `packaging/linux/mcad.xml`), Arch-Paket `mcad` (ersetzt `controller-cad`), Konfiguration `$XDG_CONFIG_HOME/mcad/config.toml`, Socket-Ordner `$XDG_RUNTIME_DIR/mcad` bzw. `/tmp/mcad-<uid>`, Umgebung `MCAD_*`. Rückfall: `CAD_*` wird gelesen, wenn `MCAD_*` fehlt (`cad_ipc::env_var`), die alte `controller-cad/config.toml`, wenn es die neue nicht gibt; die Liste zuletzt geöffneter Dateien der Desktop-App wird einmal aus dem Ordner der alten App-ID übernommen. Neue Dateien heißen `.mcad` (UI „Neu“, „Speichern unter“, neuer Ordner `main.mcad`, Beispiele, Testdaten); `.scad` öffnet überall (CLI, `include`/`use`, Datei-Dialoge, Ordner-Projekte, Upload, ZIP), gemeinsame Prüfung `cad_eval::is_model_file` bzw. `isModelFile` in der UI. `.scad` bleiben: `tests/models/openscad-compat/`, `tests/openscad-compat/` (Vergleich mit OpenSCAD), `tests/farben-pruefung/cases/` (Momentaufnahme einer unabhängigen Prüfung mit Bildern und Bericht) und die Fallstudie von Bericht 8. Unverändert: Crate-Namen `cad-*`, Repository `controller-cad`, Schriftname „cad stencil mono“ (steht in Modellen), IndexedDB-Name der Browser-App (gespeicherte Projekte).
- Dazu eine Anleitung für Claude Code im Repo (`.claude/skills/cad-cli/SKILL.md`): outline → editieren → check → render (Kontaktbogen mit Rastern/Ankern) → inspect/measure → export.

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
| Parameterleiste | aus der Modulsignatur; Werte pro Modul in der Sitzung; „Als Standard übernehmen“ erzeugt einen `TextEdit`; seit der UI-Überarbeitung (Abschnitt 19) Reiter „Parameter“ des Seitenpanels |
| Pfadleiste | aus dem Modulkontext, mittig in der Kopfleiste beim Dateinamen (Abschnitt 19); Doppelklick auf Instanz springt in die Definition |
| Dateien | Browser: Autospeichern des Projekts in IndexedDB; Projekte aus mehreren Dateien als Tabs; Ordner öffnen/speichern über File System Access API (Chromium), sonst Hoch- und Herunterladen (Datei oder ZIP). Desktop: echtes Dateisystem (Öffnen, Speichern atomar, Speichern unter, Ordner als Projekt, zuletzt geöffnet), ungespeicherte Dateien mit ●, „Automatisch speichern“ (Standard aus, Verzögerung einstellbar), kein IndexedDB |
| Dateibeobachter (Desktop) | `notify` auf die Ordner der Projektdateien (atomares Speichern ersetzt die Datei); bekannte Inhalte als Hash, eigene Speichervorgänge kommen nicht zurück; saubere Datei: still neu laden als kleinste Zeilenänderungen (Cursor/Scroll bleiben, eigener Undo-Schritt); ungespeicherte Änderungen: Leiste „Neu laden / Meine behalten / Vergleichen“ (Diff-Ansicht); gelöscht/umbenannt: Leiste, Speichern legt neu an |
| Git (Desktop) | Randmarkierung gegen `HEAD`: neu grün, geändert blau (3 px, Deckkraft 0,5–0,55), gelöscht rotes Dreieck zwischen den Zeilen; Hover zeigt die alten Zeilen; entprellt 300 ms beim Tippen, beim Speichern, bei externen Änderungen und Fensterfokus; unversionierte Dateien ohne Markierung |
| Messen | Modus „Messen“ (Abschnitt 18): Kontextleiste mit Messart und „Im Schnitt“, Snap auf Eckpunkte, Kantenmitten, Kreismitten, Kanten, Flächen bzw. die Schnittkontur, Maßlinien und Beschriftungen in der Ansicht, Liste „Messungen“ mit Kopieren als Text, Kommentar und `mcad measure` |
| Schnittansicht | Werkzeug „Schnitt“: Ebene x/y/z oder frei (Normale aus Blickrichtung oder Eingabe), Griff ziehen (rastet auf Rasterebenen des Kontextmoduls und Anker, Alt frei), Umkehren, Kappe (Stencil, schraffiert, je Körper wechselnde Richtung, transparente Körper ohne Kappe), Kontur; Statuszeile und „Als `mcad render …` kopieren“ mit derselben Konvention wie die CLI |
| Bauteile | Reiter „Bauteile“ des Seitenpanels: Sichtbarkeit, Volumen, Export je Bauteil im ⋯-Menü, „ohne Bauteil“, Summe |
| Farben & Filamente | Reiter „Farben“ des Seitenpanels (Farbfeld, Name, Zeilen, Slot-Auswahl, Volumen, Masse PLA), „Farbregionen zeigen“, Exporte „3MF mehrfarbig“, „STL je Farbe (ZIP)“, „3MF einfarbig“; Farbe je Fläche in der Ansicht (Vertex-Farben aus `tri_tags` → `tag_nodes` → `color`) |
| Thema | Symbolknopf mit Menü System/Hell/Dunkel in der Kopfleiste (localStorage `cad-theme`); alle Farben als CSS-Variablen (`styles.css`), 3D-Ansicht liest sie beim Wechsel neu; Palette-Farben im Thema angepasst (`logic/palette.ts`) |

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
  cad-measure/      Messen an der exakten B-Rep (Abstand, Radius, Winkel, Länge, Fläche, Schnitt, Wandstärke)
  cad-render/       Software-Rasterizer für die CLI
  cad-session/      Protokoll und Sitzung (Worker und CLI), TS-Typen (ts-rs)
  cad-wasm/         wasm-bindgen-Einstieg für den Worker
  cad-ipc/          lokale IPC (Unix-Socket je Benutzer, versionierte Rahmen, Anfrage/Antwort, Ereignisse)
  cad-desktop/      Desktop-Backend ohne Tauri: Engine-Prozess + Supervisor, Binärformat, Dateien, Beobachter, Git
  cad-app/          Tauri-2-App (Befehle, Kanal, Fenster, Steuer-Socket)
  cad-cli/          CLI (startet ohne Befehl die UI)
  compat-tests/     OpenSCAD-Vergleich Stufe 1
smlib/
  fetch.sh          Quellen: tangency-labs/smlib @ 7eba8aa (= usd-brep c9c5979 + Korrekturen) + patches/
  facade/           C++-Fassade; CMakeLists.txt, build-native.sh, build-wasm.sh
third_party/smlib/  SMLib-Quellen (nicht eingecheckt)
web/
  src/app/  src/editor/  src/view/  src/engine/  src/logic/  src/files/  src/state/  src/platform/  e2e/  e2e-tauri/  scripts/
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
| `rust` | Tauri-Systempakete (apt, `scripts/ci/desktop.sh deps`: der Workspace enthält `cad-app`), `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` (nativ und `cad-wasm` für wasm32), `cargo test --workspace`, `cargo check -p cad-wasm --target wasm32-unknown-unknown` | SMLib nativ (`smlib/build-native/lib`), Cargo |
| `web` | emsdk 6.0.13, `smlib/build-wasm.sh`, `web/scripts/build-wasm.sh`, `pnpm install --frozen-lockfile`, `typecheck`, Vitest, Node-Tests (smlib.wasm, Engine, Transport), `pnpm build`, Playwright; Artefakte `web/dist` und bei Fehlern der Playwright-Bericht | emsdk, smlib.js/.wasm, Cargo, pnpm |
| `compat` | OpenSCAD 2021.01 (apt): Referenz neu erzeugen und mit `reference.json` vergleichen, Selbsttest von `compare.py`, `include`/`use` gegen OpenSCAD, `compare.py --eval` mit der Release-CLI; STEP-Prüfung mit `cadquery-ocp==8.0.1.1.0` | SMLib nativ, Cargo, venv |
| `desktop` | Tauri-Pakete (apt), SMLib nativ, `pnpm build:tauri`, `cargo build --release -p cad-cli`, Tauri-E2E mit `tauri-driver` + WebKitWebDriver unter Xvfb (`web/e2e-tauri/desktop.e2e.mjs`) | SMLib nativ, Cargo, pnpm |
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
| M8 | Umstieg auf solidcore: `kernel-solidcore`, Vergleich über `diff-kernels`, eigener Tessellierer (nicht im Umfang von solidcore), SMLib-Wasm entfernen | bereit zur Veröffentlichung | offen: wartet auf solidcore (Pin `c9de85d`: 241 von 942 Knoten gleich, 0 abweichend; Quader und einfache Extrusion gehen, übrige Primitive, Transformation, Booleans, Fillets nicht); Schätzung der Recherche (unsicher): alle Primitive 1–2 Wochen, Booleans 2–4, Fillets/Sweeps/Shell 6–10 Wochen; Tessellierer eigene Arbeit; `kernel-solidcore` und `diff-kernels` vorhanden |
| später | STEP-Export, variabler Radius, mehrere Bauteile, `include`/`use` | | erledigt (B5, B6, B7) |
| später | Tauri-Version (`TauriTransport`), Dateibeobachter, Git-Rand, `mcad ui`, Schnittansicht | | erledigt (B9) |
| später | CLI-Daemon | | erledigt (B10) |
| später | Mehrfarbigkeit, Multi-Filament-3MF, dunkles Thema (Abschnitt 16) | | erledigt (B11) |
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

## 15. Bekannte Grenzen (aus den Testberichten 1–14)

Stand Bericht 7; behobene Punkte früherer Berichte sind weggelassen (z. B. F1–F4 aus B4, behoben in B7).

**Kernel (SMLib)**

- Booleans in Sonderlagen unzuverlässig; die Fassade erkennt falsche Ergebnisse nur über Stichproben (ein Fehler zwischen den Probepunkten bleibt unentdeckt). Die Prüfung nutzt SMLibs Punktklassifizierung, die bei gedrehten Splinekörpern selbst irrt; besteht keine Variante, bleibt SMLibs Ergebnis. Im Verschiebe-Rückfall weicht der zweite Operand um bis zu 10⁻⁴ der Modellgröße ab. Die Prüfung kostet Zeit (+38 % über alle Beispiele). (B1)
- Berühren sich Körper nur an einer Kante, ist die Vereinigung kein geschlossener Körper; `mcad` meldet Volumen 0 (STL in Ordnung). (B1)
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
- Snap von 1D-Griffen nur auf Ebenen, auf denen der Griff nach dem Schreiben exakt landet (nicht unter verschobenen Teilbäumen oder bei `center = true`/`d =`). Punktgriffe bewegen sich in der Ebene senkrecht zur Blickrichtung (achsgebunden mit Umschalt oder X/Y/Z, B4). Inline-Schalter nur für Bool-Literale von Zuweisungen (Zahl-Literale ohne Inline-Schieberegler, er störte; Werte über die Parameterleiste). Parameter ohne Standardwert werden beim Reset 0/false/[0, 0, 0]. (B3)
- Zonen-Kanten und geöffnete Flächen bestimmt die Ansicht an tessellierten Zonen-Meshes (die Engine exakt); in Grenzfällen kann die Anzeige abweichen. Die Anzeige des `sweep`-Pfads interpoliert wie die Fassade und weicht ab, falls sich das Verfahren im Kernel ändert. (B4)
- Watchdog-Sperre gilt für den Inhaltsschlüssel (gleiche Knoten an anderer Stelle sind mitgesperrt) und bis zum Neuladen der Seite; der Test blockiert den Worker über eine Testnachricht, ein echter hängender Kernel-Aufruf wurde nicht nachgestellt. (B2, B4)
- Messwerte stammen aus headless Chromium mit SwiftShader (ohne GPU) und sind nur relativ aussagekräftig. (B2–B4)

**Desktop, Schnittansicht (B9)**

- Steuer-IPC nur unter Unix (Linux, macOS); unter Windows liefern `mcad ui …` und der Steuer-Socket „nicht umgesetzt“ (die App selbst läuft). Ein Fenster je App-Prozess; `mcad datei.mcad` startet einen weiteren Prozess, statt das laufende Fenster zu nutzen.
- Der hängende Engine-Prozess wird beendet; die Watchdog-Tests blockieren ihn über eine Testnachricht (`__debugHang`), ein echter hängender Kernel-Aufruf wurde nicht nachgestellt.
- Der Dateibeobachter vergleicht Inhalte (Hash); eine externe Änderung, die genau den Inhalt der UI schreibt, bleibt unbemerkt (gewollt). Umbenennen wird als Löschen erkannt, nicht als neuer Name.
- Git-Markierungen nur gegen `HEAD` (nicht gegen den Index), über den `git`-Befehl; ohne `git` im `PATH` keine Markierungen.
- „Speichern unter“ und die Öffnen-Dialoge sind native Dialoge und in den E2E-Tests nicht automatisiert (der Weg über die Kommandozeile und `mcad ui show` schon).
- Schnitt: eine Ebene; Kappen nur für undurchsichtige Körper (wie die CLI); die Kontur wird je Lage auf der CPU berechnet (bei sehr großen Meshes sinkt die Bildrate); Picking ignoriert den entfernten Teil, Griffe und Raster werden nicht abgeschnitten. Die kopierte CLI-Kamera ist orthografisch (UI perspektivisch) – das Bild stimmt im Schnitt, nicht pixelgenau.
- Messwerte unter Xvfb ohne GPU (WebKitGTK mit Software-GL, Chromium mit SwiftShader) sind nur relativ aussagekräftig.

**CLI-Daemon (B10)**

- Nur Unix (Linux, macOS; Peer-UID-Prüfung nur unter Linux, sonst nur der 0700-Ordner); unter Windows rechnet jeder Befehl selbst.
- Ein Arbeitsthread: gleichzeitige CLI-Aufrufe warten aufeinander (ohne Daemon liefen sie parallel in eigenen Prozessen).
- Der Watchdog (30 min) trifft auch einen legitim langen Befehl: der Daemon endet, der Befehl wird ohne Daemon wiederholt (doppelte Zeit); einstellbar. Ein Absturz im Kernel (Signal) beendet den Daemon ebenso, die Wiederholung kann lokal genauso abstürzen.
- Speichermessung pro Knoten nur mit glibc (`mallinfo2`, Allokationen anderer Threads zählen mit); sonst die Schätzung aus Flächen/Kanten, die je Modell um den Faktor 2–10 danebenliegen kann (Freiformflächen). RSS wird nach jeder Anfrage geprüft, nicht währenddessen.
- Panik im Daemon: Exit-Code und Meldung wie ohne Daemon, die Panikmeldung des Laufzeitsystems steht aber im Log statt auf stderr. Die Zähler `cache_hits`, `kernel_calls`, `cached` und Zeitfelder zeigen, was der Daemon tatsächlich tat.
- Build-ID aus Größe und Änderungszeit des Programms: wechseln sich zwei `mcad`-Binaries ab (Debug/Release), ersetzen sie einander den Daemon.
- Die Tag-Tabelle der Engine wächst weiter; nach 2²⁰ Tags wird die Engine (mit Cache) ersetzt.

**Farben, Multi-Filament, Thema (B11)**

- Farbregionen hängen an Kernel-Booleans (Schnittmenge, Differenz, Vereinigung je `color()`): wo SMLib Material verliert (z. B. Vereinigung mit einem Torus, B1), stimmen die Regionen nicht; die Engine meldet dann „Farbregionen ungenau“ (Summe ≠ Volumen). Mehrere Booleans je Farbe kosten Zeit; die UI fragt sie nur bei offener Farbliste an (gecacht, Farb- und Slot-Änderungen rechnen keine Knoten neu).
- `color()` innerhalb eines 2D-Teilbaums (unter `linear_extrude` usw.) färbt Flächen, teilt aber kein Volumen; die Extrusion ist eine Region. `resize` wird beim Platzieren berücksichtigt. `hull`/`minkowski` erzeugen eigenes Material: was über die Kinder hinausgeht, ist ungefärbt (Standard-Eintrag), außer `color()` steht außen.
- Material ohne `color()` in einem sonst gefärbten Bauteil (z. B. Verrundung zwischen zwei Farben) gehört zur Farbe des ersten gefärbten Teilbaums; mit eigenem Standard-Eintrag (Slot 1) nur, wenn das Bauteil ungefärbte Primitive hat.
- Farbliste und Mehrfarbexport betrachten die Bauteile (benannte Instanzen); Material außerhalb von Bauteilen fehlt dann in der Liste (der Export der Wurzel enthält es als ein Objekt).
- Bambu Studio liest `project_settings.config` nur aus eigenen Dateien; die Filamentfarben setzt dort der Farbdialog für 3MF mit Farbgruppe (neuere Versionen) oder der Nutzer. OrcaSlicer übernimmt die Slots, nicht die Farben. Nicht im Container geprüft (kein Slicer mit Bambu-Importer verfügbar): die Zuordnung folgt dem Quellcode (`bbs_3mf.cpp`), geprüft mit einem Modell des Importers, lib3mf, trimesh und PrusaSlicer 2.7 (lädt sechs Objekte an der richtigen Stelle, ohne Slots).
- Farbfehlsichtigkeit: 16 Farben lassen sich nicht für alle Formen gut trennen; kleinste ΔE_OK bei Deuteranopie 0,070 (red/green), Protanopie 0,052 (orange/green), Tritanopie 0,031 (teal/blue). Die UI zeigt Namen und Slot zu jeder Farbe.
- Weiß auf hellem und Schwarz auf dunklem Hintergrund unterscheiden sich in der flachen Farbe kaum; Schattierung und Kantenlinien machen die Körper sichtbar, die Anzeige-Varianten (Weiß #e9e8e5 hell, Schwarz #3b3d41 dunkel) helfen etwas.

**Korrekturen Farben und Daemon (B12)** – ersetzt die B11-Punkte zu 2D-Farben, Material außerhalb von Bauteilen und Farbdialog:

- Die Ansicht zeigt die exakten Farbregionen erst, wenn sie gerechnet sind (Booleans je Farbknoten, gecacht; bei Modellen mit Farben immer angefragt); davor, beim Ziehen und für den hervorgehobenen Teilbaum gilt die genäherte Farbe je Fläche (Hüllen-Test bei Knoten gleichen Inhalts).
- STEP teilt keine Flächen an Farbgrenzen; eine über die Grenze verschmolzene Fläche bekommt die Mehrheitsfarbe ihrer Abtastpunkte.
- Gemischte Farben in einem 2D-Profil unter einer Extrusion werden nicht in Körper je Farbe geteilt (Warnung, Standard); eine einheitliche 2D-Farbe färbt die Extrusion.
- Zufallsfarben, die im Modell ausweichen, hängen von den unbekannten Namen davor ab.
- Ohne Farbgruppe zeigen Programme, die nur 3MF-Farben lesen, das Modell einfarbig (`--color-group` für sie; Bambu öffnet dann den Farbdialog). Bambu Studio/OrcaSlicer nicht am Programm geprüft (Quellcode, Modell des Importers).
- Der Ersatz-Socket des Daemons liegt fest unter `/tmp/mcad-<uid>/`.

**Skizzen und Namensräume (B14)**

- Transformationen innerhalb einer Skizze gibt es nicht (ein in der Skizze aufgerufenes Modul
  zeichnet in Skizzenkoordinaten).
- Die Prüfung auf Selbstüberschneidung arbeitet auf abgetasteten Polylinien (Toleranz relativ zur
  Skizzengröße); Überschneidungen *zwischen* Konturen sind erlaubt (Vereinigung nach nonzero).
  Die Verschachtelung von Löchern und Inseln setzt sich nicht schneidende Konturen voraus.
- Sweep entlang einer Skizze: Pfade mit Ecken nur als Gehrung (eben) und dann mit um den Ursprung
  zentriertem Profil; ein geschlossener Pfad wird nicht unterstützt.
- STEP: Rotationsflächen von Bögen bleiben `SURFACE_OF_REVOLUTION` der exakten Kurve (Kugel und
  Torus werden nicht als solche geschrieben).
- Loft verschiedener Segmentzahlen verbindet jeden Abschnitt zu einer Kurve (C0 an den Stößen);
  die Flächen folgen dann nicht mehr den Segmenten.
- Namensräume: die Konstanten eines Moduls werden je Zugriff in einem neuen Namensraum
  ausgewertet (keine Zwischenspeicherung); `use`/`include` liefern Namensräume wie lokale Module.
  Ein Modul mit gleichem Namen auf verschiedenen Pfaden wählt im Kontext über den Pfad, `--module
  name` ohne Pfad nimmt das erste.

**Ecken, Offset, Stroke (B15)**

- `offset(delta)` an Bögen verlängert die Tangenten (Gehrung der Tangentenlinien); OpenSCAD kennt
  dort nur Polygone. B-Splines gehen in `offset`, `stroke` und 2D-Booleans als Biarcs (Toleranz
  1 µm) ein – im Ergebnis stehen Bögen statt der Spline; `fillet2d` einer einzelnen Kontur und
  `corner` behalten die Spline (gekürzt).
- `fillet2d`/`chamfer2d` haben keinen Radius-Griff (nur `corner`); Zonen sind 2D-Formen und
  `zone(linie | ebene, d)`, 3D-Zonen werden mit Warnung ignoriert.
- Die exakte Region klassifiziert jedes Teilstück über Testpunkte im Abstand ~10⁻⁶ der
  Modellgröße: Ränder, die sich näher kommen ohne zu schneiden oder zu überlappen, können falsch
  eingeordnet werden. Kandidatenpaare über x-Sortierung (quadratisch im schlechtesten Fall).
- 2D-Booleans *zwischen* Ergebnissen (`difference() { stroke(…); stroke(…); }`) bleiben
  3D-Booleans der extrudierten Teile; exakt in 2D rechnen nur `offset`, `fillet2d`, `stroke`.
- `stroke`: Gehrungen länger als 4 Halbbreiten werden abgeschrägt (fest); ein Bogen mit Radius
  unter der halben Breite wird als Sektor plus gespiegeltem Sektor überstrichen.

**solidcore (M8)**

- Pin `c9de85d`: `diff-kernels` über 48 Beispiele, 942 Knoten: 241 gleich (Quader, lineare Extrusion eines Polygons, 2D-Knoten), 288 unsupported, 413 blockiert, 0 abweichend (vorher `3a6a50b`: 101/267/487/0 von 855). Zylinder/Kegel (`surface.trim_srf_cache`), Kugel/Torus (`topology.face_p2`), Booleans (`topology.edge`, `query.face_p1`) brechen ab; `transform` und `classify` antworten, rechnen aber falsch und werden von `kernel-solidcore` geprüft und als `Unsupported` gemeldet. Fillet/Fase ohne öffentliche API. (B5, `docs/solidcore-umstieg.md`)
- Tessellierung und Polyeder sind nicht im Umfang von solidcore: ohne eigenen Tessellierer kein Umstieg (Anzeige, STL/3MF).

---

## 16. Farben, Filamente, dunkles Thema (B11, Korrekturen B12)

### 16.1 Palette

Eine Quelle: `crates/cad-eval/src/palette.rs`; daraus erzeugt `web/src/logic/palette.ts`
(Test `cad-eval --test palette`), die CLI nutzt die Rust-Funktionen. 16 Farben, Werte aus
OKLCH-Zielen (gedeckt, Chroma ≤ 0,18), optimiert auf Abstand in OKLab für normales Sehen und
simulierte Deuteranopie/Protanopie (Machado 2009): `red #c74240, orange #dc813b, yellow
#f7d974, lime #91ca55, green #4ca167, teal #217274, cyan #6eccec, blue #2e65b9, purple
#693d8c, magenta #cd67b7, pink #f7b8c6, brown #6e4223, black #202225, gray #7b7d81, silver
#abaeb2, white #f4f3f0`; `grey`, `aqua`, `fuchsia` als CSS-Schreibweisen. Auswahl: die
häufigsten Namen in OpenSCAD-Dateien (red, green, blue, yellow, cyan, gray, black, white,
magenta, orange, silver, lime, purple) plus teal, pink, brown für typische Filamente;
`indigo`/`slate` fallen weg (zu nah an blue/purple bzw. gray). Anzeige je Thema: Schwarz im
Dunkeln auf L 0,36, dunkle Töne +0,04; Weiß im Hellen auf L 0,93; Exporte nutzen die
Grundwerte.

**Unbekannte Namen** (B12, F5): Warnung am Argument; Farbe aus einer Folge von Kandidaten je
Name (FNV-1a 64 des kleingeschriebenen Namens, weitere Kandidaten über SplitMix64): Farbton,
Helligkeit L 0,45–0,80 und Chroma 0,06–0,16 aus dem Hash (Chroma verringert bis sRGB). Gewählt
wird der erste Kandidat mit ΔE_OK ≥ 0,07 zu allen Palettenfarben und zum Standard und – im
Modell, in Reihenfolge des ersten Auftretens – ≥ 0,06 zu den Zufallsfarben anderer Namen
(höchstens 256 Kandidaten, sonst der mit dem größten Abstand). Derselbe Name (Groß/Klein egal)
hat im Modell immer dieselbe Farbe; ohne Kollision im Modell ist sie über Modelle, Sitzungen,
CLI und UI gleich. Verschiedene Namen fallen nie zufällig auf einen Wert.

**Werte** (B12, F7): Hex und Vektoren exakt; Vektorwerte außerhalb 0–1 und `alpha` außerhalb
0–1 werden mit Warnung geklemmt (bei 0–255-Werten mit Hinweis auf `"#rrggbb"`). Die Identität
einer Druckfarbe ist ihr **RGB-Wert**: `alpha` wirkt nur in der Ansicht (Transparenz), Einträge,
Slots, Konfliktprüfung und Regionen sehen über Alpha hinweg. `color("transparent")`,
`color("")` und `color()` färben nicht (Warnung, das Material bleibt Standard/Slot 1).

### 16.2 Farbregionen

`cad_engine::colors`: der Körper eines gefärbten Teilbaums (`color()`-Knoten `C`, Ergebnis in
das System des Bauteils gelegt, mit `resize`-Faktoren) gehört dieser Farbe; Überlappung →
die spätere Anweisung (Pre-Order: inneres `color()` nach dem äußeren, späteres Geschwister
nach früherem) gewinnt; Abgezogenes gilt für alle (alles wird mit dem Ergebnis geschnitten).
`Stück(C) = R ∩ M(C) − ⋃ M(später, andere Farbe)` – ein gleichfarbiger Nachbar wird nicht
abgezogen (B12, F6: sonst Splitter an Berührflächen), Stücke einer Farbe werden vereinigt –,
Rest `R − ⋃ M` → Farbe am Bauteil, sonst Standard (wenn das Bauteil ungefärbte Primitive hat),
sonst die erste Farbe. Gecacht unter einem Schlüssel aus Bauteil, Farbknoten, Lagen und Farben;
Farbe und Slot sind nicht Teil der Knotenschlüssel. Prüfung Summe = Volumen (sonst Warnung).
Ohne Farbknoten unter dem Bauteil gibt es keinen Boolean: eine Region aus den Körpern des
Bauteils, Volumen = Bauteilvolumen (gecacht).

`color()` im 2D-Profil einer Extrusion ohne eigene Farbe (B12, F3): haben alle 2D-Blätter eine
Farbe, ist die Extrusion ein Körper dieser Farbe (wie `color()` um die Extrusion); gemischte
oder teils ungefärbte 2D-Inhalte werden nicht getrennt (ein Körper, Standard, Warnung
„Farben im 2D-Profil … werden nicht getrennt“).

Material außerhalb der Bauteile (B12, F4): die obersten Knoten, die weder Bauteil sind noch
eines enthalten, bilden den Eintrag „ohne Bauteil“ der Regionen (Farbliste, Export).

### 16.3 Filamente

Slot einer Farbe, in dieser Reihenfolge:
1. `filament("red", 4);` auf oberster Ebene der Hauptdatei (B12, F2): Slot-Tabelle des Modells,
   vor allen anderen Anweisungen ausgewertet (eigener Durchlauf); erste Angabe einer Farbe
   gilt; ein widersprechendes `filament = m` an `color()` warnt und wird ignoriert;
   `filament()` an anderer Stelle warnt („nur auf oberster Ebene“); ein eigenes Modul
   `filament` des Nutzers hat Vorrang.
2. `filament = n` (1–16) an einem `color()` (erste Angabe gilt, Widerspruch → Warnung).
3. Ungefärbtes Material Slot 1; sonst niedrigster freier Slot in Reihenfolge des ersten
   Auftretens.

Hinweise der Farbtabelle (B12, F8): automatische Slots über 16 → Warnung; zwei Farben auf
demselben festen Slot und ein fester Slot 1 neben Standard → Hinweis (Info).

Die UI schreibt den Slot als Text-Edit (`SetFilament`, `cad_eval::filament_set_edits`): gibt es
ein `filament()` der Farbe, wird dessen Slot geändert („automatisch“ entfernt die Zeile);
nennen alle `color()`-Anweisungen der Farbe sie als Literal (Text oder Zahlenvektor), kommt
`filament = n` dorthin; sonst (Parameter, Schleifenwert, Variable – `filament = n` gälte dort
für alle Farben der Anweisung) fügt sie `filament("<name>", n);` ein (nach vorhandenen, sonst
vor der ersten Anweisung, die kein `use`/`include`, keine Definition und keine Zuweisung ist).
Begründung: der Code bleibt die einzige Quelle (CLI-Export und UI-Export stimmen überein, Undo
und Git sehen die Änderung, der Agent liest dieselbe Datei); die Tabelle braucht keine neue
Syntax (ein Modulaufruf, in OpenSCAD eine harmlose Warnung) und gilt genau für eine Farbe;
`$filaments = [...]` hätte `$`-Gültigkeitsbereiche und wäre an jeder Stelle änderbar.

### 16.4 Export

3MF (`cad_engine::color_export`): je Bauteil ein Objekt (Material außerhalb der Bauteile: ein
Objekt „ohne Bauteil“; ohne Bauteile die Wurzel) mit `components` ohne Transformation, ein Netz
je (Objekt, Farbe), `Metadata/model_settings.config` mit `extruder` je Teil und Objekt
(Annahmen aus `bbs_3mf.cpp` von Bambu Studio und OrcaSlicer, Bericht 11). **Keine
`m:colorgroup`** (B12): Bambu Studio öffnet bei fremden 3MF genau dann den Farbdialog, wenn
eine Farbgruppe und Dreiecksfarben da sind (`Plater.cpp`), und dessen OK überschreibt
`extruder`; ohne Farbgruppe übernimmt es die Slots. OrcaSlicer nutzt Farbgruppen nur für
Objekte ohne `model_settings.config`. `--color-group` (CLI) schreibt sie für andere Programme
(nur bei mehr als einer Farbe; Meldung: im Dialog „Abbrechen“ wählen). Kein
`project_settings.config`, kein `Application = BambuStudio-…`. CLI und UI rufen denselben
Export (`Session::export_colors`), die Meldung nennt die Objekte (`color_export_notes`).
Standard für 3MF (`--single-color` / `colors: false` für einfarbig); `--per-color` /
`Export { format: stl, colors: true }`: ZIP mit `F<slot>_<farbe>.stl` (CLI: Endung `.zip`
Pflicht; `--part` beschränkt auch die Farbliste).

STEP: Flächenfarben aus den Farbregionen ohne Boolean (`Engine::face_region_colors`): je Fläche
die Farbe des spätesten Farbknotens, dessen Material ihre Abtastpunkte enthält (Mehrheit),
sonst die Restfarbe; Material ohne `color()` ohne Farbe. Flächen werden nicht geteilt.

### 16.5 Ansicht (B12, F1/F3)

Ein Modell mit Farben zeigt in der vollen Ansicht die **Farbregionen** (die Körper des 3MF),
sobald sie gerechnet sind (UI: „Farbregionen zeigen“, Standard an; CLI `mcad render` ohne
`--show faces`). Davor und für hervorgehobene Teilbäume: Farbe je Fläche aus dem Knoten, der die
Fläche erzeugt hat – nur Knoten unterhalb des gezeichneten Knotens, und bei mehreren mit
gleichem Inhalt (geteilter Cache-Eintrag, ein Flächen-Tag) der, dessen Hülle das Dreieck
enthält (bei mehreren der späteste). Die Cache-Wiederverwendung bleibt; Grenze: Farbe je Fläche
ist bei Überlappung und Verrundung an Farbgrenzen nur angenähert, die Regionen sind exakt.

### 16.6 Thema

CSS-Token je Thema (`:root[data-theme=…]`), `contrast.test.ts` prüft Text ≥ 4,5:1 und
UI-Teile ≥ 3:1; CodeMirror-Tooltips/Suchleiste über Token; 3D-Farben aus Token
(`readPalette`); System folgt `matchMedia('(prefers-color-scheme: dark)')` mit Listener.
CLI: `mcad render --theme dark`.

---

## 17. Skizzen, Text, SVG-Import, Skizzen-Editor

Stand: Plan, abgestimmt mit dem Nutzer am 2026-10-23. **Schritt 1 umgesetzt** (Skizzen mit
gemischten Konturen, Module als Namensräume; Testbericht 14, Entscheidungen in 17.8), **Schritt 2
umgesetzt** (`corner`, `fillet2d`/`chamfer2d`, `offset`, `stroke`; Testbericht 15, Entscheidungen
in 17.9); **Schritt 3 umgesetzt:** `text()` mit den mitgelieferten OFL-Schriften und
SVG-Import (Testbericht 16, Entscheidungen in 17.10), die eigene Schablonenschrift
„cad stencil mono“ als Standardschrift (Schritt 3b, Testbericht 17, Entscheidungen in 17.11).
**Schritt 4 umgesetzt:** Skizzen-Editor, Zeichenmodus mit Raster in Browser und Tauri
(Testbericht 18, Entscheidungen in 17.12).

### 17.1 Skizzen

Eine Skizze ist ein 2D-Block aus Segmenten; jede Zeile ist ein Segment mit eigener
Darstellung und eigenen Griffen. Anonym als Kind einer Profiloperation, benannt über ein
Modul:

```
linear_extrude(5) sketch {
    start([0, 0]);
    line(to = [40, 0]);
    arc(to = [50, 10], r = 10);          // auch: center + angle, through, tangent
    line(by = [0, 20]);                  // relativ; auch length + angle
    bezier(c1 = [45, 40], to = [20, 35]);
    corner(r = 3);                       // rundet die Ecke zum nächsten Segment
    close();
}

module profil(b = 40) sketch { … }       // benannte Skizze = Modul
rotate_extrude() profil(b = 30);
```

- Segmente: `start`, `line` (`to` / `by` / `length` + `angle`), `arc` (Endpunkt + Radius,
  Mittelpunkt + Winkel, durch einen Punkt, tangential), `bezier` (quadratisch, kubisch),
  `spline` (durch Punkte), `corner(r = …)` / `corner(chamfer = …)` zwischen zwei Segmenten,
  `close()`. Variablen, `let`, `for` und `if` sind im Block erlaubt (z. B. Zahnprofile).
- Punkte dürfen Rasterpunkte und Anker sein; ein Raster darf in der Skizze stehen
  (`grid s { x: …; y: …; }`), Snappen schreibt Referenzen.
- Fehler je Segmentzeile: Kontur nicht geschlossen, Selbstüberschneidung, unmöglicher Bogen.
- Löcher: `hole { … }` in der Skizze oder 2D-`difference`; offene Pfade als `sweep`-Pfad
  oder per `stroke(w)` als Fläche.
- Kernel: gemischte Konturen aus exakten Kanten (Gerade, Kreisbogen, NURBS) für Extrude,
  Revolve, Sweep, Loft; STEP bleibt exakt.

### 17.2 Module als Namensräume

- `module name { … }`: Klammern dürfen fehlen, wenn das Modul keine Parameter hat.
- Verschachtelte Module sind von außen über den Punkt erreichbar: `teile.schraube(d = 3);`,
  `linear_extrude(2) profile.stern();`. Mehrstufig: `a.b.c();`. Das innere Modul sieht die
  Parameter des äußeren mit deren Standardwerten; Raster und Instanzen des äußeren sieht es
  nicht (wie bisher).
- Kontextmodul und Pfadleiste unterstützen den Pfad (`teile › schraube`), `mcad --module`
  nimmt `teile.schraube`.

### 17.3 2D-Operationen

`offset(r = …)` / `offset(delta = …, chamfer = …)` exakt für Geraden und Bögen
(OpenSCAD-kompatibel), `fillet2d(r)` / `chamfer2d(d)` für alle Ecken eines Profils, optional mit
Zonen wie in 3D, `stroke(w, cap = "round" | "flat" | "square", join = "round" | "miter" |
"bevel")` für offene Pfade. Umgesetzt in Schritt 2 (17.9).

### 17.4 Text

- `text(t, size, font, halign, valign, spacing, direction)`, kompatibel zu OpenSCAD.
- **Standardschrift:** eine eigene, sehr einfache monospaced Schablonenschrift
  (`"cad stencil mono"`), aus Geraden und Bögen konstruiert, mit Stegen, sodass keine
  Inseln herausfallen (Innenräume von O, A, B, D, P, R, 0, 4, 6, 8, 9 usw. sind verbunden).
  Umfang: A–Z, a–z, 0–9, ÄÖÜäöüß, übliche Satz- und Sonderzeichen. Parameter: Strichstärke,
  Stegbreite (`text(…, stroke_width = …, bridge_width = …)` in mm; umgesetzt in 17.11).
- **Mitgelieferte freie Schriften (OFL):** Inter (Sans), JetBrains Mono (Mono),
  Allerta Stencil (proportionale Schablone). Eigene Schriften per UI-Upload oder Pfad (CLI).
- Umrisse exakt als Bézier/NURBS-Konturen, Kerning über Shaping, Löcher nach Windungsregel.
- **Pflicht-Zeichensatz** (`cad_font::REQUIRED_CHARS`, Test je Schrift; gilt auch für die
  eigene Schablonenschrift): A–Z a–z 0–9, Ä Ö Ü ä ö ü ß ẞ, Leerzeichen,
  `. , ; : ! ? ' " „ “ ‚ ‘ » « ( ) [ ] { } - – — _ / \ & % # + * = < > ° ² ³ € @ § ~ ^ | · • …`,
  `± × ÷ µ Ø ø`. Fehlt ein Zeichen in einer Schrift, kommt es mit Warnung am Statement aus
  JetBrains Mono gleichen Gewichts (nicht als Kästchen oder Lücke).

### 17.5 SVG-Import (Teilmenge)

`import("logo.svg", …)` liefert ein 2D-Profil (Einheiten: px → mm über `viewBox`/`width`
bzw. Parameter `scale`/`dpi`): `path` (M, L, H, V, C, S, Q, T, A, Z, absolut und relativ),
`rect` (auch mit `rx`), `circle`, `ellipse`, `polygon`, `polyline`, `line`, `g` mit
`transform`; Füllregel `nonzero`/`evenodd`. Nicht unterstützt (Warnung): Text, Bilder,
Strichstärken als Fläche, CSS, Masken, Verläufe. Wahl einzelner Ebenen per `id`.
Kein SVG- oder DXF-Export. Umgesetzt (17.10).

### 17.6 Skizzen-Editor (Zeichenmodus)

- In der Ansicht: Werkzeuge Linie, Bogen (3 Arten), Bézier, Rechteck, Kreis, Ecke runden,
  Schließen; Zeichnen in der Skizzenebene mit Raster (Skizzenraster oder Raster des Moduls),
  Snap auf Raster- und Ankerpunkte, End-, Mittel- und Schnittpunkte, horizontal/vertikal,
  tangential; Alt = frei mit Schrittweite.
- Jede Aktion schreibt Code (`line(to = g[a, b]);` …) in den aktuellen `sketch`-Block oder
  legt einen neuen an; Undo wie beim Ziehen. Bearbeiten über Griffe, Segment löschen.
- Der Code bleibt die einzige Quelle.

### 17.7 Reihenfolge (je ein Agent, Testbericht je Schritt)

1. Sprache: `sketch`-Block, Segmente, gemischte Konturen im Kernel; Module ohne Klammern und
   verschachtelte Module als Namensräume.
2. `corner`, `fillet2d`, `offset`, `stroke`.
3. Text mit eigener Schablonenschrift und OFL-Schriften; SVG-Import.
4. Skizzen-Editor (Zeichenmodus mit Raster) in Browser und Tauri.

### 17.8 Umsetzung Schritt 1 (Testbericht 14)

**Sprache.** `sketch { … }` ist ein eingebautes 2D-Modul (`sketch(closed = false) { … }` für
offene Pfade); der Parser nimmt `sketch {` und `hole {` ohne Klammern an, `module p(…) sketch
{ … }` ist ein Modul, dessen Körper eine Skizze ist. Segmente sind eigene Anweisungen; sie
hängen an die gerade ausgewertete Skizze an, auch aus Modulen und Schleifen heraus
(`for (i = …) zahn(i);`). In einer Skizze haben Segmentnamen Vorrang vor Benutzermodulen gleichen
Namens (Bibliotheken mit `module line(…)`); außerhalb bleibt alles wie in OpenSCAD (`spline(points)`
ist dort weiter das 2D-Primitiv, andere Segmentnamen ohne Benutzermodul melden „nur innerhalb von
`sketch`“). Andere Geometrie und Transformationen in einer Skizze sind ein Fehler.

| Segment | Formen |
| --- | --- |
| `start(p)` | beginnt eine Kontur (eine offene vorherige ist ein Fehler an deren letzter Zeile) |
| `line` | `to = p` (auch positional), `by = v`, `length = l, angle = a`; `length` allein setzt die Richtung fort |
| `arc` | `to = p, r = r [, large, cw]` (wie SVG), `center = c, angle = a` (Grad, Vorzeichen = Richtung), `center = c, to = p`, `through = q, to = p`, `to = p` allein = tangential (auch `tangent = true`) |
| `bezier` | `c1, to` (quadratisch), `c1, c2, to` (kubisch), auch positional |
| `spline` | `points` nach dem aktuellen Punkt; `tangent = true` (Richtung des Vorgängers) oder Vektor, `end_tangent = v`; kubische globale Interpolation (Sehnenlängen-Parameter, gemittelte Knoten) |
| `close()` | schließt mit einer Geraden zum Start (endet die Kontur schon am Start, ist sie ohne `close()` geschlossen) |
| `hole { … }` | Konturen darin sind Löcher |
| `corner(…)` | reserviert, Warnung (Schritt 2) |

Punkte dürfen Rasterpunkte und Anker sein (z ≠ 0: Warnung, projiziert), ein `grid` in der
Skizze ist erlaubt, ebenso `let`, Zuweisungen, `for`, `if`. **Löcher:** Konturen füllen nach der
Windungsregel *nonzero*, Konturen in `hole { … }` zählen dabei immer im Uhrzeigersinn; eine Kontur
ist Rand, wo die Windungszahl nur auf einer Seite ≠ 0 ist (innen ≠ 0: Material, sonst Loch);
Inseln in Löchern werden über die Verschachtelung aufgebaut (Material minus Löcher darin, Loch
minus Inseln darin). **Fehler je Segmentzeile:** nicht geschlossen (die Skizze wird mit einer
rot gestrichelten Schlusslinie weiter gezeigt), Selbstüberschneidung (an der späteren Zeile, mit
Verweis auf die andere), unmöglicher Bogen (Radius < halbe Sehne, Punkte auf einer Geraden,
tangentialer Bogen auf seiner Tangente, Winkel 0 oder > 360°), Kontur ohne Fläche, Segment vor
`start` oder nach `close()`.

**Sweep entlang einer offenen Skizze:** ohne `path` ist ein Kind von `sweep` eine offene Skizze
(auch durch Transformationen, Gruppen, Modulaufrufe): `sweep() { sketch(closed = false) { … }
circle(2); }`. Ist der Pfad tangentenstetig, wird er zu einer B-Spline verbunden (Kurven-Sweep),
sonst Segment für Segment mit Gehrung (der Pfad ist eben).

**Loft:** Haben alle Abschnitte gleich viele Segmente, werden sie Segment für Segment verbunden
(Start = `start(p)`, Richtung gegen den Uhrzeigersinn): jedes Segment wird eine eigene Fläche,
Geraden bleiben Regelflächen, und der Nutzer bestimmt die Zuordnung. Bei verschiedener Anzahl gibt
es keine natürliche Zuordnung: jeder Abschnitt wird zu einer geschlossenen B-Spline verbunden
(Grad erhöht, C0 an den Stößen) und beginnt dort, wo der +x-Strahl vom Schwerpunkt ihn schneidet –
wie Kreise und Polygone –, damit die Abschnitte nicht gegeneinander verdreht sind.

**Module als Namensräume.** `module name { … }` ohne Klammern; Aufrufe `a.b.c(…)` (der Parser
fügt den Pfad zu einem Namen `a.b.c` zusammen, auch `k: a.b();`). Der erste Teil wird lexikalisch
gesucht, jeder weitere nur direkt im Körper des vorigen Moduls; dieser Körper ist der Namensraum,
mit den Parametern des äußeren Moduls auf ihren Standardwerten. Er gehört zu keiner Instanz:
Raster, benannte Instanzen und Anker des äußeren Moduls sind nicht sichtbar, seine Geometrie
entsteht nicht. **Funktionen und Konstanten** eines Modulkörpers sind von außen erreichbar
(`teile.kopf(d)`, `teile.M3`): Bibliotheken bündeln so Hilfsfunktionen und Maße mit ihren
Modulen, ohne eigene Datei; OpenSCAD kennt keinen Punkt hinter Modulnamen, es gibt also keinen
Konflikt, und eine Variable gleichen Namens verdeckt das Modul (`v.x` bleibt Komponentenzugriff).
Kontextmodul: `mcad --module teile.schraube` (auch `teile::schraube` oder nur `schraube`),
`ContextInfo.module` ist der Pfad, die UI wählt das Modul unter dem Cursor mit Pfad, die
Pfadleiste zeigt „teile › schraube“; `mcad outline` listet Module und Funktionen mit Pfad.

**IR, Kernel, Fassade.** `Profile::Sketch { contours, closed }` mit `Seg::{Line, Arc (Mittelpunkt,
Radius, Start-/Endwinkel, Richtung), Bezier, Spline}`; die Anweisungen der Segmente stehen in
`EvalOutput::sketches` (nicht im IR: der Cache-Schlüssel bleibt unabhängig von Quellpositionen).
Kernel: `cad_kernel::Segment::{Line, Arc, Nurbs}` (`curve2.rs`: NURBS-Form, affine Abbildung –
Bögen bleiben unter Ähnlichkeiten Bögen, sonst exakte rationale Ellipsenbögen –, Abtastung,
Interpolation, Verbinden), `Loop::Composite`, `PlacedPath`, `Kernel::sweep_path`. Die Fassade
`smlib/facade/sc_sketch.cpp` wird am Ende von `sc_facade.cpp` eingebunden (nutzt dessen interne
Hilfen; allein übersetzt ist die Datei leer); Kodierung Schleifenart 3. Geraden werden `SmLine`,
Bögen rationale quadratische B-Splines (Form `SM_CF_CIRCULAR_ARC`), Bézier/Splines B-Splines; die
Stöße werden exakt verschweißt. Extrude (auch mit twist/scale), Revolve, Loft und Sweep nutzen die
bestehenden Einstiege; neu ist `sc_sketch_sweep` (glatter Pfad: Kurven-Sweep mit dem
Profil-Ursprung als Bezugspunkt, SMLib-Patch 0003; Ecken: Gehrung). STEP schreibt die Bögen als
`CIRCLE`, die Geraden als `LINE` (aus den exakten NURBS erkannt); SMLib hält extrudierte Bögen als
Extrusionsflächen der B-Spline, der Writer schreibt eine Extrusion eines Kreisbogens längs seiner
Achse als `CYLINDRICAL_SURFACE` (OpenCASCADE rechnete auf den Extrusionsflächen ohne p-Kurven bis
2,5·10⁻³ falsche Volumen). Die 2D-Darstellung tastet Bögen und Kurven
mit der Anzeigetoleranz ab (Polygon-Clipping in Rust).

**Darstellung und Griffe.** `OpInfo::Sketch` (Segmente abgetastet, Kontrollpolygone, Fehler,
Startpunkte). Steht der Cursor in einer Skizze (Segmentzeile, Schleife, `hole`, Aufruf eines
Skizzen-Moduls), zeichnet die Ansicht sie in ihrer Ebene: Netz als Umriss mit leichter Füllung
(durch den Körper sichtbar), Segmente als Linien, das Segment der Cursorzeile dick in der
Akzentfarbe, Fehler rot, Kontrollpolygone und Bogenmittelpunkte gestrichelt, Start als Ring mit
Richtungspfeil, das Ende offener Konturen als Quadrat. Griffe: End-/Startpunkte (Literal und
Raster-Snap wie bei Punkten; `line(by = …)` relativ, ohne Snap), Bogenradius, Mittelpunkt,
Durchgangspunkt, Bézier-Kontrollpunkte, Spline-Punkte, Länge von `line(length = …)`. Auf der
`sketch`-Zeile zeigt die Ansicht die Griffe aller Segmente.

### 17.9 Umsetzung Schritt 2 (Testbericht 15)

**`corner(r = …)` / `corner(chamfer = d)`** rundet bzw. fast in einer Skizze die Ecke zwischen dem
vorigen und dem nächsten Segment. Regeln: zwischen zwei Segmenten → diese Ecke (die Schlusslinie
von `close()` zählt als Segment, `corner()` vor `close()` rundet also die Ecke zur Schlusslinie);
direkt nach `start(p)` → die Startecke (zwischen letztem und erstem Segment, nur geschlossene
Konturen); nach dem letzten Segment einer Kontur, die am Start endet (keine Schlusslinie nötig) →
ebenfalls die Startecke. Fehler an der corner-Zeile: nach `close()`, vor `start`, zweimal an einer
Ecke, Start/Ende einer offenen Kontur, Wert ≤ 0, tangentiale Ecke („keine Ecke“), Spitze (180°),
Radius passt nicht („höchstens ≈ r“). Die Skizze wird ohne Ecken aufgebaut (`arc(to)` tangential,
`line(length)` beziehen sich auf die spitze Ecke); danach werden alle Ecken einer Kontur auf den
ursprünglichen Segmenten berechnet. Brauchen zwei Ecken überlappende Stücke eines Segments, scheitern
beide mit dem größten gemeinsamen Faktor („höchstens ≈ …“) – unabhängig von der Reihenfolge. Die
Verrundung (Bogen) bzw. Fase (Gerade) ist ein Segment der corner-Zeile (Darstellung, Hervorhebung);
eine Startecke steht am Ende der Kontur, der Start rückt auf das Ende der Verrundung. Fase: die
Schnittpunkte liegen im (euklidischen) Abstand `d` vom Eckpunkt. Exakt (Mittelpunkt als Schnitt der
um `r` versetzten Kurven, geschlossene Formen) für Gerade–Gerade, Gerade–Bogen und Bogen–Bogen; mit
Bézier/Spline Newton auf `a(u) + r n_a(u) = b(w) + r n_b(w)` (Startwert aus den Tangenten,
Abbruch bei Rest 10⁻¹³ der Größe, Warnung über 10⁻⁶), die Kurve wird am Parameter gekürzt
(`Seg::Nurbs`). **Griff:** Radius (Fase: Länge) auf der Mitte der Verrundung, Basis im Eckpunkt,
linear in `r` (für Gerade–Gerade exakt).

**`fillet2d(r) { profil; zone; … }` / `chamfer2d(d)`** (`Op::Blend2`): erstes Kind das Profil,
weitere Kinder Zonen (2D-Formen, auch `zone(linie | ebene, d)` über den Abstand). Ecken sind die
Stöße mit Knick (|Knickwinkel| > 10⁻⁷), konvex und konkav (die Verrundung liegt immer auf der
Innenseite des Knicks: konvex nimmt Material weg, konkav fügt hinzu). Auswahl wie in 3D:
`select = "inside"` (Standard) – der Eckpunkt liegt in einer Zone (Rand zählt mit); `"touch"` –
auch wenn nur das Stück des Profils, das die Verrundung ersetzt, eine Zone berührt. Inline-Hinweis
„n Ecken“, leere Auswahl ist eine Warnung; zu große Radien sind Fehler je Ecke („Ecke bei [x, y]:
Radius … passt nicht (höchstens ≈ …)“), die übrigen Ecken werden trotzdem gerundet. Verschachtelte
Blöcke wirken von innen nach außen; die Übergänge einer Verrundung sind tangential und keine Ecken.
Eine einzelne geschlossene Kontur behält ihre Segmente (Splines bleiben Splines), alles andere geht
durch die exakte Region.

**`offset(r = …)` / `offset(delta = …, chamfer = false)`** wie OpenSCAD 2021.01: das erste
Positionsargument ist `r`, ohne Argumente `r = 1`, `r` gewinnt vor `delta` (Warnung), `chamfer` nur
mit `delta`. `r > 0`: alle Punkte im Abstand ≤ r (konvexe Ecken werden exakte Bögen, konkave bleiben
spitz); `r < 0`: die Punkte weiter als |r| vom Äußeren (konvexe Ecken bleiben spitz, konkave werden
rund, schmale Teile verschwinden, Löcher wachsen); `delta`: Kanten wandern um `delta`, Ecken
verlängern sich bis zum Schnitt der versetzten Tangenten (ohne Gehrungsgrenze, wie OpenSCAD mit
`miter_limit` 10⁶), `chamfer = true` schneidet sie wie Clippers `jtSquare` senkrecht zur
Winkelhalbierenden im Abstand `delta` vom Eckpunkt (an OpenSCAD gemessen). Der Bogen eines Bogens
ist ein Bogen; OpenSCAD facettiert (`$fn`), wir rechnen exakt.

**`stroke(w, cap = "round" | "flat" | "square", join = "round" | "miter" | "bevel")`**
(Standard rund/rund): Kinder sind offene Skizzen (ihre Konturen als Pfade; eine am Start endende
Kontur ist geschlossen) und beliebige 2D-Profile (ihr Rand als geschlossene Pfade, `stroke(1)
circle(5)` = Ring). Bänder der halben Breite längs der Segmente (Gerade: Rechteck, Bogen:
Ringsektor), Stöße rund (Kreis), Gehrung (bis 4 Halbbreiten, sonst Fase) oder Fase, Enden rund
(Kreis), flach oder eckig (um w/2 verlängert); sich kreuzende Pfade werden vereinigt.

**Kernel: exakte 2D-Regionen** (`cad_kernel::region2`). `combine(operanden, prädikat)`: alle
Segmente (Geraden, Bögen) werden an ihren exakten Schnittpunkten geteilt (Gerade–Gerade,
Gerade–Kreis, Kreis–Kreis mit Berührung, Überlappung kollinearer Geraden und gleicher Kreise;
Punkte im Abstand 10⁻⁹ der Größe verschmelzen), doppelte Stücke entfallen, jedes Stück bleibt, wo
das Prädikat der Insideness der Operanden (Windungszahl je Füllregel: Strahl nach +x, Bögen in
y-monotone Stücke geteilt) links und rechts verschieden ist, mit der Region links. Die Stücke werden
zu Schleifen verbunden (an gemeinsamen Ecken die engste Drehung: Berührungen ergeben getrennte
Schleifen), Stücke derselben Geraden oder desselben Kreises verschmelzen. Darauf: 2D-Booleans der
Kinder (das Prädikat ist der boolesche Ausdruck der Blätter), `offset_round` = Region ∪ (Bänder
±r ∪ Kreise in den Ecken) bzw. Region ∖ (…) für r < 0 (Minkowski-Summe mit dem Kreis exakt aus
Geraden und Bögen), `offset_delta` = Region ∪ einseitige Bänder ∪ Eckstücke (Gehrung bzw. Fase) an
auseinanderlaufenden Ecken (für delta < 0 gespiegelt), `stroke` = Vereinigung von Bändern, Stößen
und Kappen, `apply_corners`/`round_corner` für `corner` und `fillet2d`. **B-Splines** gehen als
G1-Biarcs ein (zwei tangentiale Bögen je Stück, halbiert bis zur Abweichung ≤ 1 µm, höchstens
10⁻⁵ der Größe): Der Offset einer B-Spline ist keine B-Spline, der eines Bogens ein Bogen – so bleibt
der Offset exakt zur Näherung, tangentenstetig, und STEP enthält Linien und Kreise; 1 µm liegt weit
unter Druck- und Fertigungstoleranzen (≥ 10 µm) und über der Rechengenauigkeit.

**IR, Engine, Darstellung.** `Op::Offset { round, d, chamfer }`, `Op::Blend2 { kind, r, select }`,
`Op::Stroke { w, cap, join }` sind 2D-Knoten; `Seg::Nurbs` für gekürzte Splines. `profile::lift`
berechnet für sie die Region (`op2d.rs`, zwischengespeichert nach dem Inhalt des Teilbaums, 48
Einträge) und gibt sie als Skizzen-Blätter weiter: Extrude, Revolve, Loft, Sweep und STEP sehen
exakte Geraden und Bögen (STEP: `CIRCLE`, Zylinder). Fehler, Warnungen und Hinweise stehen am Knoten
selbst; `NodeReport::guides` / `SceneNode.guides` tragen Hilfslinien (`source` Ausgangskontur,
`result` Offset-Kontur, `centerline` Pfad), die neuen Eckstücke von `fillet2d` sind
`selected_edges`. `cad_engine::region_2d` liefert die exakte Region eines 2D-Knotens (Hüllen der
Kompatibilitätstests). UI: Cursor auf `offset` – Kinder als Umriss, Offset-Kontur gestrichelt in der
Akzentfarbe (`guide:offset`); auf `stroke` – Pfad als Umriss, Mittellinie gestrichelt
(`guide:centerline`); auf `fillet2d` – Zonen blau, Profil vor dem Verrunden als Umriss, neue Ecken
farbig, Hinweis an der Zeile; auf `corner` – die Verrundung als aktives Segment mit Radius-Griff.
`mcad render --line N` zeigt dieselben Linien (`--show handles` die Griffe). OpenSCAD-Vergleich:
`tests/openscad-compat` 34–36 (offset r/delta/chamfer, Löcher, Zusammenwachsen, Verschwinden),
`stl_stats.classify` zählt `offset(r)` zu den Rundungen (`$fn = 128` für die Referenz).

**Korrektur aus Schritt 1 (Bögen > 90°).** Die Fassade baut einen Bogen als eine rationale
quadratische B-Spline mit Spannen ≤ 90°; ein Bogen über 90° ist damit eine Kurve aus mehreren
Spannen. SMLib-Booleans mit Extrusionen solcher Kurven lieferten je nach Lage falsche Ergebnisse
(Ring aus zwei 180°-Bögen ∪ Quader: „volume 924.12 impossible for operands 976.79, 43.2“; mit
4 × 90° oder gedreht richtig). Die Engine teilt deshalb Bögen über 90° vor dem Kernel in gleiche
Stücke ≤ 90° (`profile::split_arcs`, für Extrude, Revolve und Sweep; nicht für Loft, das die
Abschnitte Segment für Segment verbindet): jedes Stück ist eine Spanne, ein Halbkreis wird zwei
Zylinderflächen (STEP: zwei `CYLINDRICAL_SURFACE`). Regressionstest
`ring_of_long_arcs_unites_with_a_box` (Bögen 180°/120°/90°/360°, acht Drehungen des Quaders).
Dabei gefunden und behoben: die Windungszahl der exakten Region nahm die Endpunkte der Bögen aus
ihren Winkeln (Rundung ~10⁻¹⁵), ein Strahl durch einen Eckpunkt zählte dann falsch; jetzt
gelten die gemeinsamen Eckpunkte der Schleife.


### 17.10 Umsetzung Schritt 3, Teil Text und SVG (Testbericht 16)

**Crates.** Drei neue Crates ohne Bezug zu Sprache und Kernel: `cad-outline` (neutrale Konturen:
`Seg::{Line, Arc, Quad, Cubic, Ellipse}`, affine Abbildung – Kreisbögen bleiben unter
Ähnlichkeiten Kreisbögen, sonst Ellipsenbögen –, Abtastung, Füllregel-Normalisierung),
`cad-svg` (Parser auf `roxmltree`) und `cad-font` (Schriften, Shaping mit `rustybuzz` 0.20 /
`ttf-parser` 0.25, Glyphen-Umrisse). `cad-eval` hat die Builtins `import(…)` und `text(…)`
(`import.rs`); sie erzeugen `Profile::Sketch` – ein Knoten je gefülltem SVG-Element bzw. je
Glyphe, bei mehreren unter einer Gruppe (Vereinigung: überlappende Elemente/Glyphen). Mit
`Seg::Nurbs` (aus Schritt 2) werden tangentenstetige Ketten von Bézierkurven eine B-Spline mit
C1-Knoten (eine Kernel-Fläche je Glyphenflanke statt Dutzender), Ellipsenbögen rational
quadratisch. Engine und Fassade blieben unverändert bis auf die Farbregionen (unten) und eine
Vereinigung ohne SMLib-Boolean für Körper, deren Hüllquader getrennt sind (`disjoint_union` in
`sc_facade.cpp`: beide Schalen in eine Brep): SMLib gab bei der Vereinigung getrennter
Glyphenteile (Schablonen-„c“ aus drei Stücken) ein Stück doppelt zurück, einmal verkehrt herum
(Volumen 2,0 statt 7,3 mm³, ohne Fehlermeldung); nebenbei wird Text schneller. (Die
zwischenzeitliche Umgehung – Bögen über 90° beim Import teilen – entfiel mit
`profile::split_arcs` aus Schritt 2.)

**Füllregeln.** Je Element werden die Konturen so orientiert, dass die gefüllte Fläche links
liegt (außen gegen, Löcher im Uhrzeigersinn, Inseln wieder gegen): Prüfpunkte links/rechts
der längsten Kante, Windungszahl aller Konturen des Elements nach dessen Regel; Konturen mit
Füllung auf beiden oder keiner Seite entfallen. Danach gilt nonzero wie in Skizzen.
Voraussetzung: Konturen eines Elements schneiden sich nicht (Glyphen, übliche SVGs).

**SVG.** Einheiten wie OpenSCAD, wo es konsistent ist: mit `viewBox` gilt `width`/`height`
(px = 1/96 in, mm, cm, in, pt, pc; ohne Einheit Pixel mit `dpi`, Standard **96**; OpenSCAD
2021.01: 72), ohne `width` die `viewBox`-Größe in Pixeln, `preserveAspectRatio` (Standard
`xMidYMid meet`). y wird um die Dokumenthöhe gespiegelt: Ursprung unten links (wie OpenSCAD).
Abweichungen von OpenSCAD 2021.01: ohne `viewBox` sind Nutzereinheiten Pixel (OpenSCAD: mm bei
Spiegelung um die Höhe in Pixeln), der y-Versatz der `viewBox` wird abgezogen (OpenSCAD addiert
ihn), `fill-rule` gilt (OpenSCAD: immer evenodd), Striche werden nicht zu Flächen (OpenSCAD:
ja), offene Pfade und `line`/`polyline` sind Pfade (Warnung; nur offene Pfade → offene Skizze,
als `sweep`-Pfad nutzbar). `center = true` zentriert die Hülle; `scale` multipliziert; `id`
wählt ein Element mit Inhalt (unbekannte id: Fehler). Warnungen tragen Datei und SVG-Zeile
(`logo.svg:4: …`) und erscheinen je Meldung einmal.

**Dateien.** `Project::assets`: jeder Text in den Ausdrücken der Projektdateien, der auf
`.svg/.ttf/.otf/.ttc/.otc` endet, wird beim Laden relativ zu jeder geladenen Datei gesucht und
gelesen (`FileResolver::read_bytes`; Browser/`SetFiles`: Binärdateien als
`data:…;base64,…`-Text). `import`/`font =` lösen relativ zur Datei der Anweisung auf; ein zur
Laufzeit berechneter Name, der nirgends als Text steht, ist ein Fehler („nicht geladen“). Der
Inhalt steckt in den Konturen des IR: geänderte SVGs ändern den Cache-Schlüssel; `mcad watch`
beobachtet die Datendateien, die Desktop-App liest und schreibt sie (Schriften dekodiert).
UI: „⇡ Datei laden“ (SVG, TTF/OTF, .scad), SVGs öffnen im Editor, Schrift-Tabs zeigen einen
Hinweis; ZIP und Ordner nehmen die Datendateien mit; Einzelbeispiele, die SVGs aus
`tests/models/svg` importieren, öffnen als Projekt mit diesen Dateien.

**Text.** Wie OpenSCAD 2021.01: em = `size` / 0,72; Laufweite, Versatz und die Ausrichtung
nehmen die Werte des Shapers mal 1000/1024 (OpenSCAD rechnet so; Text bleibt dadurch gleich
breit), `halign` nach der Laufweite, `valign` nach der Hülle der Glyphen, `spacing` multipliziert
die Laufweite, `direction` ltr/rtl/ttb/btt, `language`, `script` (ISO 15924 oder Namen wie
`"latin"`); ohne Angabe ermittelt der Shaper Richtung und Schrift. Hülle und Fläche stimmen mit
OpenSCAD überein (Bericht 16). Schriftwahl: `"Familie"`, `"Familie:style=Bold"`; unbekannter
Stil → Regular (Warnung), unbekannte Familie → Standardschrift (Warnung; bis Schritt 3b
JetBrains Mono), `font = "datei.ttf"` → Projektdatei. **Standardschrift:** seit Schritt 3b
`"cad stencil mono"` (17.11; vorher JetBrains Mono). **Fehlende Zeichen:** je Zeichen aus JetBrains
Mono gleichen Gewichts (Laufweite der Ersatzschrift, Warnung „Zeichen … fehlen in …“); Allerta
Stencil (unverändert mitgeliefert: der Reserved Font Name verbietet veränderte Fassungen unter
dem Namen) fehlen ẞ„‚{}_#°²³§~^·•…±µØø. Zeichen in keiner Schrift: `.notdef` und Warnung.

**Farbregionen.** Ist das Ergebnis eines Knotens die bloße Vereinigung seiner Farbknoten
(nur Gruppen, Transformationen, Farben, Vereinigungen ohne Verrundung dazwischen), braucht
eine Farbregion keine Booleans mit dem Ganzen: Material des Farbknotens minus späteres
Material anderer Farbe, dessen Hülle es überlappt (Berührung zählt nicht); Teile einer Farbe
bleiben getrennt, wenn ihre Hüllen sich nicht überlappen (`colors.rs`, `union_regions`).
Mehrfarbiger Text auf einer Platte: Sekunden statt Minuten bzw. Abbruch durch den Watchdog.

**STEP.** Extrusionen von Ellipsenbögen (rationale quadratische B-Spline, senkrecht zur Ebene
extrudiert) schreibt der Writer als `SURFACE_OF_LINEAR_EXTRUSION` einer `ELLIPSE` (`cad-step`,
`elliptic_extrusion`: Kegelschnitt durch fünf Abtastpunkte, geprüft an 65): OpenCASCADE
integrierte die Extrusion der rationalen B-Spline um 3,6·10⁻³ falsch. Offen: die Extrusion einer
quadratischen Q/T-Welle (`welle.svg`, `id = "welle"`) misst OpenCASCADE um 3 % zu klein (Kernel
652,25 mm², analytisch 652,19 mm²); `svg_logo.mcad` nutzt daher nur Ellipse und Bogen aus
`welle.svg`.

**Grenzen.** Kernel-Zeit wächst stark mit der Zahl der Glyphen auf einer gemeinsamen Fläche
(Vereinigung Platte ∪ Text in SMLib: 9 Glyphen ≈ 1,5 s, 28 ≈ 9 s nativ); Konturen, die sich
innerhalb eines Elements schneiden, werden nicht aufgelöst; keine Textpfade, kein `use`,
keine CSS-Klassen; Variable Fonts nur in ihrer Standardinstanz; `$fn` wirkt nicht (exakte
Kurven).

### 17.11 Umsetzung Schritt 3b: Schablonenschrift „cad stencil mono“ (Testbericht 17)

**Schrift.** `cad_font::stencil` (`crates/cad-font/src/stencil.rs`), keine Schriftdatei: je
Zeichen eine Zeichenfunktion über benannten Lagen (Mittellinien links/rechts/Mitte; Grund-,
x-, Versal-, Ober-, Unterlänge), die von der Strichstärke abhängen – Mittellinie = äußere Linie
∓ halber Strich, daher bleiben Versalhöhe und Glyphenbox bei jeder Strichstärke gleich.
Bausteine: Geraden, tangentiale Bögen (`arc_to`: Kreisbogen vom Punkt mit der aktuellen
Richtung zum Ziel), Bögen um einen Mittelpunkt, Stadion, Bauch, C-Bogen, Punkte (Quadrate der
Strichstärke), Kommas, Spiegelung und Drehung um 180° (d, q, 9, `)`, `]`, `}`, `»`, `>`, `\`,
„“ ‘). Die Region einer Glyphe: `region2::stroke` der Pfade (flache Enden, Gehrung, Grenze
4 halbe Striche) ∪ Füllungen ∖ Stege ∩ Clip, in einem `region2::combine` – exakte Schleifen aus
Geraden und Kreisbögen, außen gegen den Uhrzeigersinn; Cache je (Zeichen, Strich, Steg).
Konstruktionsregeln: freie Enden laufen bis zur äußeren Linie; Stöße enden auf der Mittellinie
des anderen Strichs (der Querschnitt liegt dann im anderen Band, keine Kerbe); Diagonalen
werden über Grund-/Versallinie verlängert und dort beschnitten (gerade Füße, flache Spitzen,
Gehrungsspitzen von Z, 2, 4, 7 im Clip). Umfang: `REQUIRED_CHARS` vollständig plus `’ ” $`
(`stencil::CHARS`, Glyphen-ID = Position + 1); andere Zeichen aus JetBrains Mono.

**Stege.** Ein Steg ist ein Band der Stegbreite quer über einen Strich (1,3 halbe Striche zu
jeder Seite), von der Glyphe abgezogen. Jeder Innenraum bekommt mindestens einen: B, D, P, R,
b, d, g, p, q am Stamm (Stamm und Bauch getrennt), O, Q, 0, 8, o, Ö, ö, Ø, ø oben und unten,
A offen (Beine an der Versallinie um die Stegbreite auseinander, Querstrich verbindet), a und e
am Bauch/Querstrich neben dem Stamm, 4 am Querstrich vor dem Stamm, 6/9 schräg am Bauch, ° %
& @ § an einem Ring, # am oberen Querstrich; ß und ẞ sind offen (Lücke ≥ Stegbreite zum
Stamm). **Ohne Stege** (`bridge_width = 0`) bleiben die Innenräume geschlossen (Löcher wie
eine normale Schrift, A oben geschlossen).

**Metrik.** 1000 Einheiten je em, em = `size`/0,72 wie bei den anderen Schriften; Versalhöhe
720 (= `size`), x-Höhe 530, Oberlänge 760, Unterlänge −210, Vorschub 600 (× 1000/1024 wie
OpenSCAD – gleich JetBrains Mono, Texte bleiben so breit wie mit der bisherigen
Standardschrift), Seitenrand 60. Senkrecht: Vorschub 1000, Glyphe mittig auf dem Stift;
`rtl`/`btt` kehren die Reihenfolge um. `halign`, `valign`, `spacing` wie sonst (17.10).

**Parameter.** `text(…, stroke_width = w, bridge_width = b)` in mm, nur für diese Schrift
(sonst Warnung „wirkt nur mit der Schrift „cad stencil mono““). Standard Strich 0,14·`size`
(Stil Bold: 0,2·`size`), Steg 0,1·`size`. Grenzen aus dem Test der Parameterfläche: Strich
≤ 0,2·`size` (darüber wachsen a, e, #, @ zusammen und schließen Innenräume), Steg 0 oder
0,03…0,2·`size` (schmalere Stege schließen schwere Striche wieder); außerhalb Warnung am
Argument und Begrenzung. mm statt Anteilen, weil Druck und Fräsen absolute Maße brauchen
(„Steg mindestens zwei Düsenbreiten“). Schriftnamen `"cad stencil mono"`,
`"cad stencil mono:style=Bold"`.

**Auswahl.** `cad_font::select(name)`: kein `font`, der eigene Name und unbekannte Familien
(Warnung) → Schablonenschrift; mitgelieferte TrueType-Schriften wie bisher (`builtin` liefert
nur noch diese); `Font::stencil(bold)`, `FontData::Stencil`. Der bisherige Hinweis
„Schablonenschrift folgt“ entfällt.

**OpenSCAD-Kompatibilität.** OpenSCAD kennt die Schrift nicht; ohne `font` nimmt es seine
fontconfig-Standardschrift. Die Vergleichsmodelle (`tests/models/openscad-compat`,
`tests/openscad-compat/projects`) enthalten kein `text()`, der Textvergleich (Bericht 16,
`hull_and_area_like_openscad`) nennt jede Schrift ausdrücklich – keine Abhängigkeit von der
Standardschrift, keine Änderung nötig. Bewusste Abweichung: Modelle mit `text()` ohne `font`
sehen bei uns anders aus als in OpenSCAD; für gleiche Maße eine mitgelieferte TrueType-Schrift
angeben.

**Tests.** cad-font: Abdeckung beider Stile ohne Rückfall, keine Löcher/Splitter/Überlappungen
für 8 Kombinationen von Strich und Steg, Innenräume ohne Stege, Metrik und Vorschub, jede
Glyphe in ihrer Zelle, Flächen (H exakt, o gegen Formel, gegen JetBrains Mono), Ausrichtung,
Richtungen, Rückfall; cad-eval: Standardschrift, Parameter, Warnungen; cad-engine
(`stencil_text.rs`): jede Glyphe über `region_2d` ohne Loch, Volumen = Fläche × Höhe,
Kernel-Zeit eines Schilds gegen Allerta Stencil (nativ 1,6–1,9 s gegen 3,3–3,6 s: Geraden und
Kreise statt B-Splines, weniger Kernel-Aufrufe); Beispiel `tests/models/spec/schablone.mcad`;
Playwright K01–K03 (`web/e2e/schablone.spec.ts`). Zeichentafel:
`cargo run -p cad-font --example stencil_chart -- tafel.svg [strich steg]`.

**Playwright C16 (Befund).** C16 (achsgebundenes Ziehen) scheiterte zeitabhängig, zuletzt auch
einzeln: Der Viewer baut die Griffe nach einem Cursorwechsel erst im nächsten Animationsframe
neu, `__cad.handles()` lieferte bis dahin die Griffe der vorigen Zeile (`cube([60, 30, 1])`,
ebenfalls ein Punktgriff); der Test griff die Plattenecke, das Ziehen begann nicht, die Maus
drehte die Kamera. Der Test wartet jetzt auf den Punktgriff der Zeile (`pointHandleOn`, ebenso
C20). Die App verhält sich richtig.

**Grenzen.** Eine Strichstärke je Glyphe (nur @ mit 0,72-fachem, ² ³ mit 0,75-fachem Strich),
keine optischen Korrekturen; W, w, m, @, * werden bei Bold dicht. Stege an Bögen sind gerade
Bänder (Breite auf der Mittellinie). Kombinierende Zeichen werden nicht zusammengesetzt (das
Zeichen kommt aus JetBrains Mono); Zeichen aus JetBrains Mono haben keine Stege. Glyphen
berühren sich nur bei `spacing` < 1 oder den zellenbreiten `—` und `_`.

### 17.12 Umsetzung Schritt 4: Skizzen-Editor (Testbericht 18)

**Bedienung.** Schalter „Skizze“ in der Ansichtsleiste (Browser und Tauri gleich): Zeichenmodus
für die Skizze am Cursor (die `sketch`-Anweisung enthält den Cursor oder beginnt auf seiner
Zeile). Die Kamera blickt senkrecht auf die Skizzenebene (beim Eintreten, wenn sich die Ebene
ändert, „⊥ Ansicht“); bei einer neuen Skizze passt sie auch das Modell ein. Leiste: Auswahl,
Linie, Bogen 3P (durch drei Punkte), Bogen M (Mittelpunkt + Winkel), Bogen tan (tangential),
Bézier (kubisch), Rechteck, Kreis, Ecke runden; Schließen, Löschen, Fertig. Die Statuszeile sagt,
was das Werkzeug als Nächstes braucht, wohin die Maus snappt und warum eine Aktion nicht ging.
Esc verwirft die gewählten Punkte, ein zweites Esc beendet den Modus; Entf löscht das Segment am
Cursor; Strg+Z/Strg+Umschalt+Z wirken auch mit dem Fokus in der Ansicht. Die linke Maustaste
gehört dem Zeichenwerkzeug (auch auf bestehenden Punkten: dort zeigt die Ansicht keine Griffe);
mit „Auswahl“ setzt ein Klick auf ein Segment den Cursor in seine Zeile, die Griffe aller
Segmente sind zu sehen und werden wie bisher gezogen (Literal ändern, Rasterreferenz
umschreiben, Alt: Zahlen), links ziehen auf freier Fläche dreht die Kamera.

**Werkzeuge → Code.** Jede Aktion ist ein Text-Edit der Hauptdatei und ein eigener
Verlaufsschritt; der Cursor steht danach hinter dem neuen Code, die nächste Aktion folgt dort.

| Werkzeug | Klicks | Code |
| --- | --- | --- |
| Linie | Anfang, dann je Punkt | `start(p);` (wenn der Anfang nicht das Ende der Kontur am Cursor ist) `line(to = q);` je Punkt; tangential: `line(length = l);`; Klick auf den Anfang der Kontur (≥ 2 Segmente): `close();` |
| Bogen 3P | Anfang, Durchgangspunkt, Ende | `arc(through = q, to = p);` |
| Bogen M | Anfang, Mittelpunkt, Endwinkel | `arc(center = c, angle = a);` (a auf die Winkelschrittweite gerundet; Vorzeichen = Richtung, über 180° hinaus der Mausbewegung folgend); auf einem gesnappten Punkt `arc(center = c, to = p[, cw = true]);` |
| Bogen tan | Ende | `arc(to = p);` am Ende der offenen Kontur (braucht eine Richtung) |
| Bézier | Anfang, c1, c2, Ende | `bezier(c1 = …, c2 = …, to = …);` |
| Rechteck | zwei Ecken | `start(a); line(to = [bx, ay]); line(to = b); line(to = [ax, by]); close();` – sind beide Ecken Punkte desselben Rasters, auch die anderen (`g[c, u]`, `g[b, v]`) |
| Kreis | Mittelpunkt, Punkt auf dem Kreis | `start(p); arc(center = c, angle = 360); close();` |
| Ecke runden | Ecke, Radius (Abstand zur Ecke) | `corner(r = …);` nach dem Segment, das an der Ecke endet (Startecke: nach `start`); steht dort schon ein `corner()`, ändert sich sein Radius |
| Schließen | – | `close();` für die offene Kontur am Cursor |
| Löschen | – | entfernt die Segmentanweisung am Cursor (ganze Zeile); das einzige Segment einer Kontur nimmt `start`, `close`, `corner` der Kontur mit, ein `corner()` direkt nach dem Segment ebenfalls |

Ketten: nach einer Linie, Bogen 3P/M und Bézier ist der letzte Punkt der Anfang des nächsten
Elements (kein neues `start`). Punkte werden als Referenz (`g[b, u]`, `k.ecke`) oder als Zahlen
(`fmt_num`, auf die Schrittweite gerundete freie Punkte) geschrieben; nichts Neues an der Sprache.

**Einfügestelle** (`cad_eval::sketch_edit::place`). In der Skizze am Cursor: nach der Anweisung
der Cursorzeile bzw. der letzten davor, im innersten `hole { … }`-Block, der den Cursor enthält;
auf der `sketch {`-Zeile am Ende des Blocks; leere Blöcke (auch `sketch {}` auf einer Zeile)
werden aufgeklappt, eingerückt wie die Nachbarn bzw. eine Stufe tiefer. Der Zeichenzustand dort
(offene Kontur, ihr Anfang, aktueller Punkt, Richtung) kommt aus `SketchInfo::steps`: die
Segmentanweisungen in Auswertungsreihenfolge mit dem Zustand nach jeder (auch aus Schleifen und
Modulaufrufen im Block; Geometrie vor den `corner()`). Beginnt eine Aktion eine neue Kontur
mitten in einer, die nach der Einfügestelle weiterläuft, kommt sie hinter deren letzte Anweisung.
Ohne Skizze am Cursor legt die erste Aktion einen `sketch { … }`-Block an: am Ende des
Kontextmoduls bzw. der Datei, Ebene = xy des Kontexts (eine bloße 2D-Skizze; zum Körper macht sie
z. B. `linear_extrude`). Sichtbare Raster und benannte Instanzen gelten an der Einfügestelle.

**Snap** (`snap`). Toleranz: Snap-Radius in Pixeln (Einstellungen) mal Pixelgröße am Treffpunkt
des Strahls (`pixel` = 2·tan(fov/2)/Höhe, mal Abstand). Ohne Alt der nächste Punkt im Radius:
Rasterpunkte (nur solche in der Skizzenebene, z = 0 im Skizzensystem) und Anker benannter
Instanzen schreiben ihre Referenz; Anfang der offenen Kontur (schließt), Ecken, End-, Mittel- und
Schnittpunkte (Gerade/Bogen exakt, Kurven über Abtastung) und Bogenmittelpunkte der Skizze
schreiben Zahlen; bei gleicher Lage gewinnt die Referenz. Sonst: tangential (Linie vom Ende der
Kontur entlang ihrer Richtung, Länge auf die Schrittweite), horizontal/vertikal zum vorigen Punkt
und Rasterlinien (eine Koordinate `g.name`, z. B. `[g.b, 40]`, nur für Raster ohne Drehung),
sonst die Schrittweite. Alt: frei mit Schrittweite. Die Antwort enthält Text, Art und
Beschriftung; die Ansicht zeigt eine Markierung je Art (Quadrat Raster/Anker, Ring Punkte, Raute
Richtungen, Kreuz Schritt), die Beschriftung und Hilfslinien.

**Ebene der Skizze** (`sketch_frame`): das Knotensystem; im Profil eines `rotate_extrude` die
xz-Ebene (wo das Profil im Körper liegt), beim zentrierten `linear_extrude` dessen Boden. Dieselbe
Ebene nutzen die Griffe der Segmente (`Handle.frame`), die Darstellung der Skizze
(`OpInfo::Sketch.plane`, auch außerhalb des Zeichenmodus) und der Editor (Kamera, Raster, Snap,
Vorschau). Werte bleiben Koordinaten der Skizze; Rasterpunkte werden in der Skizzenebene gezeigt,
wo die Referenz den Punkt hinlegt (wie `result_world` beim Griff-Snap).

**Protokoll** (Plan 7.1). `Sketch { id, cursor, tool, points, ray?, pixel, snap_px, free, click,
angle?, info }` → `Sketch { id, version, sketch, node, plane, current, snap, guides, preview,
points, edits, select, prompt, message, segment, angle, raster }`. Die Engine ist ohne Zustand:
die gewählten Punkte (mit Text und Art) kommen mit jeder Anfrage und gehen mit der Antwort zurück.
Höchstens eine Anfrage ist unterwegs (die letzte Mausposition gewinnt, Klicks in Reihenfolge).
Die UI wendet `edits` als eine Transaktion mit `isolateHistory` an, setzt den Cursor
(`select`: Edit und Versatz im Text) und schickt die Quelle sofort; weil die Engine Nachrichten in
Reihenfolge bearbeitet, sieht die nächste Anfrage den neuen Text. Bei Syntaxfehlern antwortet sie
nur mit einer Meldung. Gleich über `WorkerTransport` und `TauriTransport` (JSON).

**Aufteilung.** Rust (`cad-eval/src/sketch_edit.rs`, Session): Skizze und Einfügestelle,
Zeichenzustand, Snap, Code, Vorschau, Löschen, Ecken, Raster, Ebene, Strahl in der Ebene. UI
(`web/src/app/sketch.ts`, `sketchUi.tsx`, `logic/sketchEditor.ts`, Viewer): Maus und Tasten
weiterreichen, Antwort anwenden, zeichnen (Schrittraster mit Abstand ≥ 12 px, Rasterlinien und
-punkte, Vorschau, Hilfslinien, Markierung), Kamera. Helles und dunkles Thema über die
Farbwerte der Ansicht (`--view-snap`, `--view-grid`, Akzent).

**Kleinigkeiten unterwegs.** Griff-Snap: ein 2D-Punkt, der als `g[a, b]` geschrieben ist, bleibt
in dieser Form (`g[c, v]` statt `g[c, v, _]`). Die Darstellung zeichnet Skizzen in ihrer Ebene
(vorher lag eine Skizze im `rotate_extrude` beim Cursor auf der Segmentzeile in der xy-Ebene).

**Tests.** cad-eval `tests/sketch_edit.rs` (Einfügestelle, neue Skizze in Datei und Modul,
Ketten, neue Kontur hinter einer weiterlaufenden, Snap-Arten, Schnittpunkte, tangential,
Rechteck mit Rasterreferenzen, Bögen, Bézier, Kreis, Mittelpunktsbogen auf einem Rasterpunkt,
Ecken einfügen und ändern, Schließen, Löschen, `hole`- und leere Blöcke, Ebenen unter
Transformation/`rotate_extrude`/zentriertem Extrudieren, Anker, Segment unter der Maus, Griff-Snap
in Kurzform) und Unit-Tests im Modul; cad-session `tests/sketch_editor.rs` (Protokoll mit Strahl,
neue Skizze, Kette, Schließen, Löschen, Ebene unter Transformation, Syntaxfehler, Auswahl,
`OpInfo.plane`); Vitest `logic/sketchEditor.test.ts`; Playwright Z01–Z17
(`web/e2e/skizzen-editor.spec.ts`: jedes Werkzeug, Snap-Anzeige, Snap-Arten, Undo, Griff ziehen,
Segment löschen, Transformation und `rotate_extrude`, dunkles Thema, Antwortzeiten); Tauri T16
(`web/e2e-tauri/desktop.e2e.mjs`: Rechteck auf Rasterpunkten über den `TauriTransport` mit
nativem Kernel, Snap-Anzeige, Undo).

**Grenzen.** Nur die Hauptdatei: in einer anderen Projektdatei startet der Modus nicht (Hinweis),
Skizzen aus eingebundenen Dateien sind kein Ziel (die erste Aktion legt dann eine neue in der
Hauptdatei an). Anweisungen in
Schleifen oder Modulen werden weder gelöscht noch bekommen ihre Ecken ein `corner()` (Meldung);
der Zeichenzustand mitten in einer Schleife ist der nach ihrem letzten Durchlauf. Snap nur auf die
Skizze selbst, sichtbare Raster und Anker (nicht auf Kanten des Körpers oder andere Skizzen);
horizontal/vertikal nur zum vorigen Punkt. Bézier nur kubisch, kein Spline-Werkzeug, keine
erzeugten `line(by = …)`/`length + angle`-Formen außer tangential. Eine neue Skizze ist eine bloße
2D-Skizze. Die Kamera behält „z oben“ (Bahnsteuerung): in gedrehten Ebenen liegt die y-Achse der
Skizze nicht unbedingt senkrecht auf dem Bildschirm. Tauri T16 treibt die Ansicht mit
synthetischen Zeigerereignissen (WebDriver), nicht mit echter Maus.


---

## 18. Messen (Testbericht 19)

**Ziel.** Messen in der UI (Browser und Tauri) und in der CLI mit derselben Logik in Rust:
Abstand (Punkt–Punkt und kleinster Abstand zwischen Punkten, Kanten und Flächen mit Fußpunkten;
parallele Ebenen: Normalabstand), Radius/Durchmesser (Kreiskanten, Bögen, Zylinder, Kegel, Kugel,
Torus, Verrundungen) mit Achse und Mittelpunkt, Winkel zwischen Kanten und Flächen, Länge einer
Kante, Fläche einer Fläche, Hülle und Volumen eines Körpers oder Bauteils; im aktiven Schnitt Snap
auf die Konturen, Abstände in der Ebene, Radius von Schnittkreisen, Wandstärke, Fläche eines
Bereichs. Ergebnisse exakt aus der B-Rep, sonst mit angegebener Toleranz.

**Aufteilung.**

| Teil | Inhalt |
| --- | --- |
| `cad-measure` (neu, kernelunabhängig) | `curve::EdgeCurve`: Kante als beschränkte exakte Kurve (Linie, Kreisbogen, Ellipse geschlossen; B-Spline, Polylinie numerisch) mit nächstem Punkt, Länge (Gauß-Legendre), Mitte nach Bogenlänge, Hülle. `surface`: nächster Punkt auf Ebene, Zylinder, Kegel, Kugel, Torus geschlossen, auf Rotations-/Extrusionsflächen über die Erzeugende, auf B-Spline-Flächen per Gitter + Gauß-Newton. `body::Body`: B-Rep in Kontextkoordinaten mit den Dreiecken jeder Fläche (ob ein Punkt auf der getrimmten Fläche liegt, Grobsuche). `measure`: Abstand, Winkel, Radius, Fläche. `section`: Konturen, Bereiche, Wandstärke. `dist`: Dreiecksabstand mit BVH (aus `cad-cli/geom.rs`, auch `--clearance`) |
| `cad-engine` | `measure_parts` (je Teil B-Rep und Tessellierung, platziert, Massen), `section_parts` (Kernel-Boolean mit dem behaltenen Halbraum) |
| `cad-session::measure` | Messmodell je Knoten (einmal je Auswertung), Schnitte je Ebene (bis 6), Snap, Auswahl, Messungen mit Maßlinien, Texten und `mcad measure`-Argumenten, Provenienz, Referenzen für die CLI |
| CLI `mcad measure` | Referenzen auflösen, JSON/Text (`measurements`, `section`, `at`) |
| UI | `app/measure.ts` (Controller), `app/measureUi.tsx` (Kontextleiste `MeasureBar`, Liste `MeasurePanel`, Statuszeile), Viewer (Treffer, Hervorhebung, Maßlinien, Beschriftungen), `logic/measure.ts` (Texte, Kopieren) |

**Abstand.** Grobsuche über die Tessellierung (Flächen als Dreiecke, Kanten als Polylinien,
Punkte als entartete Dreiecke; BVH), dann abwechselnde Projektion auf die exakte Geometrie
(Punkt auf A nächst dem Fußpunkt auf B und umgekehrt, bis zur Ruhe). Projektion auf eine Fläche:
auf die unbeschränkte Fläche; liegt der Punkt nicht auf der getrimmten Fläche (Abstand zu ihren
Dreiecken > 2 Sehnentoleranzen), auf den nächsten Randpunkt (exakte Kantenkurven). Exakt heißt:
beide Elemente analytisch und die Iteration konvergiert; sonst gilt ± 2 × Sehnentoleranz und
das Ergebnis der Grobsuche, wenn die Verfeinerung davon wegläuft. Zwei parallele Ebenen: der
Normalabstand ist der Wert, Fußpunkte = Punkt auf A und seine Projektion auf B (Hilfslinie zum
nächsten Punkt der Fläche B), der kleinste Abstand der begrenzten Flächen steht daneben;
parallele Achsen (Zylinder, Kegel, Kreise): zusätzlich der Achsabstand. Δx/Δy/Δz der Fußpunkte.

**Radius.** Kreis und Ellipse aus der Kantenkurve; Zylinder, Kegel (Radius an der Stelle,
Halbwinkel, Spitze), Kugel, Torus aus den Flächenparametern; die Ausdehnung längs der Achse aus
Dreiecken und Rand (gezeichnete Achse). Rotationsflächen, deren Erzeugende ein Kreisbogen ist
(SMLibs Verrundung entlang einer Kreiskante), werden als Torus erkannt (Abweichung ≤ 10⁻⁹ ·
Radius: exakt); ein gerader achsparalleler Erzeuger als Zylinder; Extrusionsflächen eines
Kreisbogens als Zylinder; B-Spline-Kanten, die einem Kreis bis 10⁻⁶ folgen, mit angepasstem Kreis
(`exact = false`, Abweichung angegeben); Freiformflächen: Hauptkrümmungsradien (numerisch).
Verrundungen gerader Kanten sind in SMLib Zylinder, Ecken Kugeln.

**Winkel.** Richtung je Element: Gerade (Kante), Ebene (Fläche: äußere Normale; Kreiskante:
ihre Ebene), Achse (Zylinder, Kegel, Torus, Rotationsfläche), sonst Tangente bzw.
Tangentialebene am Klickpunkt. Zwei Ebenen: Winkel zwischen den Ebenen 0–90° (Fase: 45°), dazu
der Winkel der Normalen und der Öffnungswinkel an der Schnittgeraden zwischen den Halbebenen zu
den Klickpunkten (bei Nachbarflächen der Winkel durchs Material oder außen); der Bogen liegt an
der Schnittgeraden nächst den Klicks (beim stumpfen Öffnungswinkel zur Verlängerung der zweiten
Fläche). Zwei Kanten mit gemeinsamer Ecke: 0–180° von der Ecke weg; sonst 0–90°. Gerade–Ebene:
Winkel zur Ebene.

**Fläche, Länge, Körper.** Ebene Flächen exakt über den Rand (Green: ½∮(x dy − y dx) über die
exakten Kanten, Geraden geschlossen, sonst Gauß-Legendre). Zylinder, Kegel, Kugel, Torus: die
Dreiecke 4- und 8-fach unterteilt, Innenpunkte auf die Fläche, Randpunkte auf die Randkurven
projiziert, Richardson-Extrapolation (relativer Fehler < 10⁻⁶ in den Tests). Andere Flächen: aus
den Dreiecken mit Toleranzangabe. Länge: exakt für Geraden und Bögen, sonst Gauß-Legendre.
Körper: das Bauteil der gewählten Fläche (benannte Instanz der Datei, über Tags → Knoten →
Vorfahren), sonst der Körper (Teil) des Knotens; Volumen, Oberfläche, Schwerpunkt aus dem
Kernel (SMLib integriert auf ~10⁻⁴), Hülle aus der B-Rep (Ecken, exakte Kantenextreme,
Dreiecke, Extrempunkte von Kugeln und Tori).

**Schnitt.** Ebene ∩ B-Rep über den Kernel: je Volumenteil `Intersection` mit einem Quader
jenseits des Teils, der genau den behaltenen Halbraum `(q − p) · n ≤ 0` abdeckt (Konvention wie
`mcad render --section` und die Schnittansicht). Die Deckflächen des Ergebnisses (Ebene mit
Normale +n in der Schnittebene) begrenzen den Schnitt; ihre Schleifen laufen um Material gegen
den Uhrzeigersinn um n, um Löcher im Uhrzeigersinn, die Innennormale einer Kontur ist n ×
Tangente. Jede Kante grenzt an eine geschnittene Fläche des Körpers; ist sie Ebene, Zylinder,
Kegel, Kugel, Torus oder Rotationsfläche (die letzten drei senkrecht zur Achse geschnitten), wird
der Abschnitt in geschlossener Form gebildet (Gerade, Kreis, Ellipse; Lage und Richtung aus der
Kernel-Kurve, deren Abweichung als `dev` mitgeht); sonst bleibt die Kernel-Kurve (`exact =
false`). Bereich = Deckfläche (Umriss und Löcher) mit Fläche aus Green. Wandstärke an einem
Konturpunkt: Strahl senkrecht zur Kontur ins Material bis zur nächsten Kontur (Geraden und Kreise
exakt, sonst abgetastet und nachgesucht); `--section` allein meldet zusätzlich die kleinste
Wandstärke über 8 Stellen je Abschnitt (Schätzung). Kosten: ein Boolean je Teil und Ebene
(gecacht); beim Ziehen der Ebene im Messmodus rechnet die Engine die Lagen nacheinander.

**Snap** (Session, `snap`). Toleranz wie beim Skizzen-Editor: Snap-Radius in Pixeln × Pixelgröße
× Tiefe. Auf dem Modell: Eckpunkte, Kantenmitten (Bogenlänge), Kreismitten, dann Kanten nahe dem
Strahl, dann die getroffene Fläche. Die UI schickt neben dem Strahl den Treffer ihres Raycasts
(Knoten, `tri_faces`-ID, Punkt); die Flächen-ID wird über die Teile des Knotens zugeordnet (IDs
folgen wie in `append_offset` aufeinander) und am Treffpunkt geprüft, sonst gilt die nächste
Fläche (auch für Farbregionen, deren Netze andere IDs haben). Verdeckte Punkte (tiefer als der
Treffer) und der entfernte Halbraum der Schnittansicht werden nicht angeboten; bei gleichem
Bildabstand (koaxiale Kreise) gewinnt der nähere. Alt: kein Punktfang. Je Messart nur passende
Elemente (Radius: runde Kanten und Flächen, Winkel: keine Punkte und Kugeln, …). Im Schnitt:
Strahl ∩ Ebene, Eckpunkte, Mitten, Kreismitten der Kontur, Konturabschnitte, für „Fläche“ der
Bereich unter der Maus.

**Provenienz.** Zu jeder Auswahl: die Zeilen der Knoten der Flächen-Tags (bei Kanten beider
Flächen, bei Ecken aller anliegenden), mit benannter Instanz und Modul (`Zeile 11: box (Instanz
k: gehaeuse)`); Rasterpunkte (`g[b, h, o]`), Rasterlinien für Achsen (`g[loch[0], mitte, *]`),
Rasterebenen für ebene Flächen (`g.innen_l`) und Anker (`k.ecke`), auf denen das Element liegt
(Toleranz 10⁻⁶ relativ, alle Raster der Szene).

**Protokoll** (7.1). `Measure { id, kind, picks, ray?, hit?, pixel, snap_px, free, click,
section?, clip?, node? }` → `Measure { id, version, hover, picks, result, prompt, message,
section }`. Wie beim Skizzen-Editor ohne Zustand in der Engine: die Auswahl (Art, Knoten,
Körper, Kante/Fläche bzw. Kontur/Abschnitt/Bereich, Punkt, Text, Referenz, Herkunft,
Hervorhebung) geht mit jeder Anfrage hin und zurück; höchstens eine Anfrage unterwegs.
`Measurement { kind, title, value, unit, label, values[], lines, guides, points, at, details,
picks, exact, tol?, section?, text, cli }`: `text` ist die Kopie als Text (mit `// ` die als
Kommentar), `cli` die Argumente nach dem Dateinamen (mit `--module`/`-D` des Kontexts und
`--section …`), die UI setzt `mcad measure <datei>` davor. Gleich über `WorkerTransport` und
`TauriTransport`.

**CLI.** Elementreferenzen `vertex@P`, `mid@P`, `center@P`, `edge@P`, `face@P`, `body@P` (das
Element nächst P; bei Flächen entscheidet unter gleich nahen die exakte Fläche), mit `--section`
dieselben Präfixe für die Kontur und `region@P`; Punkte wie `--between`. `--at P` listet, was
bei P liegt (mit Referenz) – damit findet ein Agent die Referenzen. JSON: `measurements`
(Protokolltyp, Zahlen auf 12 Stellen, ohne Hervorhebungs-Polylinien), `section` (Bereiche,
Konturen mit Segmenten, Kreise, kleinste Wand), `at`.

**UI.** Schalter „Messen“ in der Ansichtsleiste; Kontextleiste `MeasureBar` (Messarten
Abstand, Radius, Winkel zuerst wie im UI-Konzept, dann Länge, Fläche, Körper, im Schnitt
Wandstärke; „Im Schnitt“ schaltet die Schnittansicht ein; Fertig); Liste `MeasurePanel`
(rechts neben der Ansicht, später Reiter „Messen“ des Seitenpanels) mit Wert, Genauigkeit,
Details, Herkunft, Kopieren (Text, `// Kommentar`, `mcad measure`), Löschen; „Alle kopieren“,
„Alle löschen“. Beide sind eigenständige Bausteine (`.contextbar`), die UI-Überarbeitung muss sie
nur einordnen (Kontextleiste links oben unter der Werkzeugleiste). In der Ansicht: Element unter
der Maus (Snap-Farbe) mit Beschriftung, gewählte Elemente (Akzent), Maßlinien, gestrichelte
Hilfslinien (Achsen, Hülle), Punkte und eine Beschriftung je Messung (DOM, jedes Bild platziert,
ohne Überdeckung; Klick wählt). Farben `--view-measure`/`--on-measure` je Thema
(`contrast.test.ts`: Schrift ≥ 4,5:1, Linie zum Hintergrund ≥ 3:1). Im Messmodus keine Griffe;
ein Klick ist Drücken ohne Bewegung (Linksziehen dreht weiter); Esc verwirft die Auswahl bzw.
beendet, Entf löscht die gewählte Messung, Buchstaben D/R/W/L/F/K/T wählen die Messart. Eine
Modelländerung verwirft offene Auswahlen; fertige Messungen bleiben (Hinweis „veraltet“).
Skizzen-Editor und Messen schließen sich aus.

**Tests.** `cad-measure`: Unit-Tests (Kurven, Flächen, Quadratur) und `tests/analytic.rs` gegen
SMLib-Körper (Zylinder: Radius, Achse, Mantel- und Deckfläche, Kreislänge, Normalabstand;
Kegel, Kugel, Torus: Radien und Flächen; Fillet-Zylinder, -Kugel, -Torus einer Kreiskante;
Fase 45°/135°; Abstände Fläche–Kugel, Punkt–Kugel, windschiefe Kanten; Schnitt durch Bohrungen
mit exakten Kreisen, Flächen, Wandstärken, schräger Schnitt als Ellipse). `cad-session`
`tests/measure.rs` (Snap mit Provenienz, Klicks, CLI-Argumente, Lochplatte mit Bohrungs- und
Verrundungsradien, Gehäuse aus `tests/models/spec` mit Wandstärke 3 mm im Schnitt, Innenabstand,
Bauteilvolumen, Fase, Schnittmodus über das Protokoll). CLI `tests/messen.rs`. Vitest
`logic/measure.test.ts`. Playwright M01–M11 (`web/e2e/messen.spec.ts`), Tauri T17.

**Grenzen.** Der Schnitt kostet einen Kernel-Boolean je Teil und Ebene; scheitert er, gibt es
keinen Schnitt (Meldung) und keine Ersatzkontur aus dem Netz. Konturen auf Freiformflächen und
auf schräg geschnittenen Kegeln, Tori und Rotationsflächen bleiben Kernel-Kurven (`exact =
false`). Die Wandstärke misst senkrecht zur Kontur an einer Stelle (nicht den kleinsten Abstand
zur Gegenseite), die kleinste Wandstärke ist eine Schätzung über Stichproben. Abstände zwischen
Freiformflächen und Fälle, in denen die abwechselnde Projektion an einem anderen lokalen Minimum
endet, gelten mit der Toleranz der Tessellierung. Die Wahl über Punkte (`face@P`) ist eindeutig,
solange P einem Element am nächsten liegt (CLI-Referenzen mit 6 Nachkommastellen). Flächen von
Freiformflächen aus den Dreiecken; Hülle von Freiformkörpern aus Dreiecken und Kanten (innen
liegende Extreme von B-Spline-Flächen bis auf die Sehnentoleranz). Volumen aus dem Kernel
(~10⁻⁴). Die Reihenfolge der CLI-Ergebnisse folgt den Optionen, nicht der Befehlszeile.
2D-Ergebnisse (Skizzen ohne Extrusion) lassen sich nicht messen. Die Liste der Messungen lebt
nur in der Sitzung (nicht gespeichert). Tauri T17 treibt die Ansicht mit synthetischen
Zeigerereignissen.

## 19. UI-Überarbeitung (Testbericht 20)

**Grundlage.** Konzept `docs/konzepte/ui-ueberarbeitung/konzept.html` (+ PDF), vom Nutzer
angenommen mit allen Vorschlägen (Option A) und Teal/Smaragdgrün statt Blau als Akzent. Keine
neue Funktion; Ordnung, Gewichtung und einheitliche Bedienelemente in Browser und Tauri.

**Bausteine** (`web/src/app/ui/`): `Button` (primär – höchstens einer je Bereich, standard,
leise, Umschalter mit `aria-pressed`), `IconButton` (26 × 26), `Segmented` (Radiogruppe, Pfeiltasten,
Pos1/Ende), `MenuButton`, `SplitButton`, `ContextMenu` (WAI-ARIA-Menü mit `menuitem`,
`menuitemradio`, Untermenüs; Popups in `<body>` mit fester Position, Schließen bei Klick außerhalb,
Esc gibt den Fokus zurück), `Toolbar`/`ToolGroup`/`ToolSep`/`ToolLabel`. Eine Höhe 26 px
(`--ctl-h`), Radius 6 px, 2 px innerhalb, Trennstrich + 12 px zwischen Gruppen; Inter 12 px aus
`crates/cad-font/fonts` (gleiche Breiten in Chromium und WebKitGTK). Symbole: `lucide-react`
(ISC), eigene Strich-Symbole für Linie, Bogen und „⊥ Ansicht“. `aria-label` ist der schlichte
Name, der Tooltip (`title`) hängt das Tastenkürzel an, `aria-keyshortcuts` trägt es.

**Aufbau.**

| Bereich | Inhalt |
| --- | --- |
| Kopfleiste (`app/header.tsx`) | App-Name, „Datei ▾“ (Neu, Öffnen …, Ordner öffnen …, Zuletzt geöffnet › nur Desktop, Speichern unter …, Datei hinzufügen …, Datei laden …, ZIP laden …, ZIP speichern), „Beispiele ▾“ mit Untermenüs je Gruppe; mittig Dateiname mit Änderungspunkt (Desktop) und Pfad in den Modulkontext; „Speichern“ primär; geteilter Export-Knopf (letztes Format in localStorage `cad-export`, Strg+E; 3MF mehrfarbig/einfarbig, STL, STL je Farbe, STEP, Je Bauteil › STL-ZIP/STEP-ZIP/3MF); Thema- und Einstellungen-Symbolknopf. Kürzel Strg+N (nur Desktop), Strg+O, Strg+S, Strg+⇧+S, Strg+E im ganzen Fenster |
| Editor | Tabs (`role=tab`, ←/→) und „+“ (Neue Datei, Datei laden, Aktionen der angezeigten Datei); Rechtsklick oder Umschalt+F10/Kontextmenütaste auf einem Tab: „Als Hauptdatei“, „Schließen“ |
| Ansicht | Werkzeugleiste oben: links Modus Modell/Schnitt/Skizze/Messen (`runtime.viewMode`/`setViewMode`, genau einer; Skizze und Messen lassen einen Schnitt stehen, danach Modus Schnitt; Farben ist kein Modus), rechts Einpassen, Ansicht ▾ (Iso, Oben, Vorne, Rechts, Hinten, Links, Unten), Alle Raster, Anzeige ▾ (Alle Raster, Farbregionen zeigen, Seitenpanel), Seitenpanel einblenden (wenn eingeklappt). Kontextleiste des Modus links oben darunter (`.ctxslot`). „Neu einpassen“ unten mittig. Ansichtswürfel unten rechts (`app/viewCube.tsx`, SVG, 3 × 3 Felder je Fläche: Fläche, Kante, Ecke; folgt der Kamera über `Viewer.onCamera`; x/y/z daneben) statt des Achsenkreuzes |
| Kontextleisten | Schnitt: X/Y/Z/Blick, Lage (`p·n` frei), Normale, Umkehren, „Als `mcad render …` kopieren“. Skizze: Auswahl \| Linie, Bogen ▾ (letzte Bogenart, `sketchArc`), Bézier, Rechteck, Kreis \| Ecke runden, Schließen, Löschen \| ⊥ Ansicht, „Fertig“ primär; Tasten V, L, A, ⇧A, T, B, R, C, F, Enter, Entf, N (`logic/sketchEditor.sketchKeyAction`). Messen: Messart als Segment (D/R/W/L/F/K/T), „Im Schnitt“, Kopieren/Löschen der gewählten bzw. letzten Messung, „Fertig“ |
| Seitenpanel (`app/sidePanel.tsx`) | rechts, einklappbar (`sideOpen`, `sideTab` in localStorage), Reiter Bauteile, Farben, Messen und im Modulkontext Parameter (wird beim Eintritt gewählt, beim Verlassen der vorige Reiter); der Messmodus wählt „Messen“. `colorsVisible()` ersetzt `colorsOpen` (Farbregionen werden für den sichtbaren Reiter angefordert) |

**Themen.** Akzent hell `#0f766e`, dunkel `#3cc4a8` (dazu `--accent-hover`, `--accent-text`,
`--accent-soft`, `--focus`, `--seg-on`, `--ctl-hover`, `--float-shadow`); 3D-Ansicht
`--view-accent`/`--view-flat` hell `#0f766e`, dunkel `#62dcc2`/`#3cc4a8`, `--hover-line` und
`--viewhover-line` teal; Würfel `--vc-face`, `--vc-edge`, Achsen `--axis-x/y/z`. Die Paare stehen in
`logic/contrast.ts` (Text ≥ 4,5:1, Bedienelemente ≥ 3:1, Akzent auf dem Modell ≥ 2:1). Die
Modellpalette bleibt.

**Wayland.** `cad-app` setzt vor GTK den GLib-Programmnamen auf den `identifier` aus
`tauri.conf.json` (`dev.mcad.desktop`, bis 0.2 `dev.controller-cad.desktop`); GTK nimmt ihn als Wayland-`app_id` und
X11-`WM_CLASS`. Die Desktop-Datei heißt danach (`packaging/linux/dev.mcad.desktop.desktop`,
`StartupWMClass` gleich), `make install` leitet den Namen aus `tauri.conf.json` ab.

**Tests.** Playwright-Selektoren über Rolle und Namen (Helfer `pickMenu`, `menuItem`, `mode`,
`sideTab`, `fileTab`, `sketchTool`, `measureKind`, `exportAs` in `web/e2e/helpers.ts`); neu
`e2e/ui-ueberarbeitung.spec.ts` U01–U09 (Leisten bei 1280/1440/1920 px hell/dunkel in allen Modi:
`scrollWidth ≤ clientWidth`, nichts ragt heraus, einzeilig, keine Überlappung; Menüs per Tastatur,
Export-Format mit Strg+E und nach Neuladen, Tabs und Kontextmenü, Modus-Exklusivität, Skizzen-Kürzel,
Ansichtswürfel, Seitenpanel, Tab-Reihenfolge, Kontrast am gerenderten Knopf); Tauri T18 (Leisten bei
drei Fensterbreiten in WebKitGTK, Menüs, Würfel). Vorher/Nachher-Bilder: `tools/report/report20-shots.mjs`.

**Grenzen.** Kein Umbenennen von Dateien; Kanten und Ecken des Würfels nur mit der Maus (Tastatur:
Menü „Ansicht“); kein Röntgen-Schalter in der UI (gab es nur in der CLI); Strg+N nur in der
Desktop-App; die Standardaufteilung Editor : Ansicht ist 33 : 67 (wie im Konzept etwa ein Drittel), schmale Ansichten zeigen Modus und
Messart als Symbole.

---

## 20. `hull` und `minkowski` (Plan 21, Phase A)

Nutzerentscheidung: nur exakte Sonderfälle; passt keiner, Warnung mit Grund und die Kinder
unverändert (keine Näherung). Die Auswertung erzeugt `Op::Hull` / `Op::Minkowski`; die Engine
entscheidet nach den Kindern.

- **Kernel-Offset** (`Kernel::offset`, Fassade `sc_offset`, Version 5): SmApiOffsetBrepFull nach
  außen mit Rundung und Selbstschnitt-Behandlung (ohne diese scheitern Quader und L-Profil).
  Spike (Bericht 21): exakt für Quader (auch gedreht), Zylinder, Kugel, L-Profil (konkave Kante
  bleibt scharf), verrundeten Quader – Volumen auf 3·10⁻⁸ an der Formel. An Ecken mit anderen als
  rechten Winkeln (Tetraeder, 45°-Keil) scheitert SMLib (Keil) oder liefert richtige Flächen mit
  angenäherten Trimmkurven (Tetraeder: bis 0,33 mm neben den Kanten, Volumen +4,5·10⁻⁴). Die
  Fassade prüft deshalb Rand-Abstand zum Körper und Trimmkurven gegen die 3D-Kanten und meldet
  sonst „offset is not exact“. Ersatz über Flächen-Offset + Fillet aller Kanten trägt nicht
  (Fillet scheitert an Quader und L-Profil).
- **2D-Hülle** (`cad_kernel::hull2`): Stützfunktion über den Normalenwinkel, Punkte und Bögen als
  Orte, Übergänge an gemeinsamen Tangenten (geschlossen), Bogen-Enden; Ergebnis Geraden + Bögen
  gegen den Uhrzeigersinn. B-Splines über die Biarc-Näherung.
- **3D** (`cad-engine/src/hull3d.rs`): Kinder als Körper (Ecken ebener Körper; Kugeln, Kegelstümpfe,
  Prismen nur unter Ähnlichkeitsabbildungen); Fälle Polyeder (Quickhull `cad_kernel::hull3`),
  Rotationskörper (2D-Hülle der Halbschnitte, Hälfte ρ ≥ 0, revolve), Prisma (2D-Hülle der
  Querschnitte, extrude), Kugeln (Polyeder + Offset). `minkowski`: Offset, Summe paralleler
  Prismen (2D-Summe: Offset mit Kreis oder konvexe Summe), konvexe Polyeder (Hülle der
  Eckensummen).
- Flächen des Ergebnisses tragen den Tag des hull-/minkowski-Knotens. Rotationskörper bestehen
  aus Rotationsflächen (STEP `SURFACE_OF_REVOLUTION`), nicht aus Kugel/Zylinder.
- Grenzen: `rotate_extrude`, Torus, `$fillet` an Primitiven, `difference` als Kinder sind kein
  Sonderfall; gleich große Kugeln in einer Ebene (abgerundete Platte) auch nicht; das Wasm-Bundle
  braucht ein neues `smlib.wasm` (Export `sc_offset`).

## 21. Kollisions-Asserts (Plan 21, Phase C)

Nutzerentscheidung: Verletzung = Warnung; `mcad export` bricht ab (Exit 4), außer mit
`--ignore-asserts`; `assert_keepout` in der UI gut sichtbar.

- **Sprache**: `assert_keepout(clearance)`, `assert_disjoint()`, `assert_intersect()`,
  `assert_touches(tol, contact)` erzeugen einen `Op::Group`-Knoten (keine neue Operation: alle
  Engine-Pfade behandeln ihn wie `group`) und einen Eintrag `Tree.asserts` (`AssertDecl`: Knoten,
  Art, Parameter, Span; einer je ausgewerteter Instanz). Das Sperrvolumen bekommt
  `Flags::keepout` und `background`: damit ist es ohne weitere Sonderfälle nicht im Ergebnis,
  nicht in Booleans, Export, Volumen, Farbregionen und Bauteilen, aber ausgewertet (Netz zum
  Zeigen). Asserts sind schachtelbar; benannte Instanzen darin bleiben Bauteile.
- **Prüfung** (`cad-session/src/asserts.rs`): nach dem Kernel-Job, im nächsten `Session::step`
  nach dem Wurzelnetz (die Szene erscheint zuerst), nicht im Entwurfsmodus beim Ziehen
  (`checked: false`). Ergebnis: Nachricht `FromEngine::Asserts`, `Scene.asserts` der fertigen
  Auswertung, Warnungen (Quelle Kernel) an der Anweisung. Körper: Kind = Menge aller Körper und
  Netze des Kindes (Kontextkoordinaten); beim Sperrvolumen jeder Körper jedes Bauteils und des
  Materials außerhalb (wie die Farbliste), sonst der Wurzel.
- **Verfahren**: Schnittvolumen exakt per Kernel-Boolean je Körperpaar mit sich treffenden
  Hüllquadern (Volumen aus den Masseeigenschaften, Netz des Schnitts mit Kanten für die
  Anzeige); mit Netzen ohne B-Rep nur ja/nein (`solids_overlap`). Abstand über die Tessellierung
  (`min_distance`, Fußpunkte, Genauigkeit ± 2·Netztoleranz). Kontakt an nahen Flächenpaaren der
  exakten B-Rep (`cad_measure::Body`): gegensinnige koplanare Ebenen → Schnitt der beiden
  Flächenumrisse mit `region2` (Geraden und Bögen exakt), Fläche in mm², Füllung für die Anzeige
  über `i_triangle`; Zylinder/Kugel auf Ebene, Zylinder auf Zylinder (parallel: Linie, windschief:
  Punkt), Kugel/Kugel → Linien-/Punktkontakt; andere Flächen nur am Netz. Ein scheiternder
  Boolean (aufliegende Flächen) ist ein Hinweis „Überlappung nicht prüfbar“, kein Fehler.
- **Toleranzen** relativ zur Größe L des Paares: ε_V = 10⁻⁹·L³, Kontaktfläche > 10⁻⁹·L²,
  Netztoleranz clamp(2·10⁻⁴·L, 0,001, 0,02) mm.
- **`assert_intersect`** verlangt eine zusammenhängende Gruppe (Union-Find über die sich
  schneidenden Paare), nicht paarweise: ein Halter, der zwei Platten verbindet, ist gültig; je
  weiterer Gruppe wird das nächstgelegene Paar gemeldet. Berühren ist kein Schnitt.
- **Cache**: Paarergebnis im Engine-LRU (neuer Slot `Aux`, `Engine::aux_get/aux_insert`) unter
  dem Schlüssel aus Knotenschlüsseln, Platzierungen, Art und Parametern: `watch`, Daemon und UI
  rechnen nur geänderte Paare neu (Beispiel: 2. Aufruf über den Daemon 0 ms).
- **CLI**: `check` (`asserts`, `violations`), `export` (Exit 4, `--ignore-asserts`), `render`
  (Sperrvolumen gelb durchscheinend mit gestricheltem Umriss, Schnitt rot mit Kanten oben,
  Kontakt grün, Spalt als Linie mit Fußpunkten, `--show asserts`), `outline`/`measure`/`inspect`.
- **UI**: Darstellungsmodus `keepout` (schraffiert über einen Shader-Zusatz, durchscheinend, durch
  das Modell sichtbar, gestrichelter Umriss, nicht wählbar/messbar), Schalter „Sperrvolumen“ in
  der Werkzeugleiste und im Menü Anzeige (gemerkt), Überlagerung der Ergebnisse mit Beschriftung,
  Statusleiste „n Assert-Verletzungen“ (Klick: nächste Verletzung, Cursor auf die Anweisung,
  Kamera rahmt den Schnittkörper ein; ebenso Klick auf die Warnung am Zeilenende), Export-Rückfrage
  „Trotzdem exportieren“ für alle Exportwege (entspricht `--ignore-asserts`).
- Grenzen: Kontaktfläche an gekrümmten koinzidenten Flächen (Bolzen in Bohrung) nur am Netz
  geschätzt; Netz-Überschneidung ohne Volumen und ohne Schnittkörper; die Prüfung blockiert den
  Worker für die Dauer der Booleans (gecacht; beim Ziehen übersprungen).
