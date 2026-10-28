//! Messages between the UI and the engine (Plan 7.1). Plain serde types; the TypeScript
//! declarations in `web/src/engine/protocol.ts` are generated from them ([`typescript`]).
//!
//! Conventions:
//! * Source positions are **UTF-16 code unit offsets** (CodeMirror) plus 1-based line and
//!   column (column in UTF-16 units), see [`SrcRange`]. They are local to their file:
//!   `SrcRange.file` is the file id in `Scene.files` (omitted = main file), `Diagnostic.file`
//!   and `Edit.file` name the file by its path (omitted = main file).
//! * Projects (several files): `SetFiles` gives the other files (path → text), `SetFile`
//!   changes one; the main file's text still comes with `SetSource`. Paths are `/`-separated,
//!   relative to the project root; `include <p>`/`use <p>` resolve relative to the including
//!   file.
//! * Matrices are 4×4, column-major (`Matrix4.fromArray` in three.js), mapping the local
//!   system of the thing they belong to into the system of the current context (the context
//!   module or the file).
//! * Node indices (`u32`) are only valid for the scene `version` they came with. Mesh keys
//!   (`mesh_key`, `MeshData.key`) are content hashes and stay valid across versions: equal key,
//!   equal mesh.
//! * In the browser, mesh arrays and export bytes arrive as typed arrays; in JSON (CLI) they
//!   are plain number arrays.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Version of this protocol (also the `schema_version` of the CLI's JSON output).
pub const PROTOCOL_VERSION: u32 = 3;

/// `[x, y, z]`
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Vec3(pub [f64; 3]);

/// 4×4 matrix, column-major.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Mat4(pub [f64; 16]);

/// RGBA, 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Rgba(pub [f32; 4]);

/// A source range. `from`/`to`: UTF-16 offsets (end exclusive). `line`/`col`: 1-based start,
/// `end_line`/`end_col`: 1-based position of `to`. Columns count UTF-16 units. All are local
/// to the range's file: `file` = id in `Scene.files`, omitted for the main file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct SrcRange {
    pub from: u32,
    pub to: u32,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<u32>,
}

/// A language value: `null` = undef, numbers, bools, strings, lists, ranges; grids,
/// instances and functions as `{ opaque: "<type>" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Value {
    Undef,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Value>),
    Range { start: f64, step: f64, end: f64 },
    Opaque { opaque: String },
}

// ------------------------------------------------------------------------------------------
// UI → engine
// ------------------------------------------------------------------------------------------

/// Message from the UI to the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum ToEngine {
    /// New source text. Parses and evaluates at once (answers `Diagnostics`, and unless there
    /// are syntax errors a preliminary `Scene` and `Progress`), aborts a running kernel job
    /// (finished nodes stay cached) and starts a new one; `step` then finishes it.
    SetSource { text: String, version: u64 },
    /// Project files besides the main file (path → text), replacing earlier ones; `main` is the
    /// main file's path (default `main.mcad`; includes resolve relative to it). Answers
    /// nothing and does not evaluate: send `SetSource` next.
    SetFiles {
        files: BTreeMap<String, String>,
        #[ts(optional)]
        main: Option<String>,
    },
    /// Changes one project file (`text = null`: removes it). Answers nothing and does not
    /// evaluate: send `SetSource` next.
    SetFile {
        path: String,
        #[ts(optional)]
        text: Option<String>,
    },
    /// What to show: `module = null` for the whole file, otherwise a module name (or
    /// `outer::inner`) evaluated with its defaults and `overrides` (parameter bar). Without a
    /// module, `overrides` replace top-level assignments of the main file (CLI `-D`).
    /// Re-evaluates the current source.
    SetContext {
        #[ts(optional)]
        module: Option<String>,
        #[serde(default)]
        overrides: BTreeMap<String, Value>,
    },
    /// Meshes of nodes of the current evaluation. `coarse`: coarse tolerance (default: while a
    /// drag is active). `version`: drop the request if the evaluation has another version.
    RequestMeshes {
        nodes: Vec<u32>,
        #[ts(optional)]
        coarse: Option<bool>,
        #[ts(optional)]
        version: Option<u64>,
    },
    /// A handle drag starts: coarse tessellation until `DragEnd`. Answers `SnapTargets`.
    DragStart { handle: u64 },
    /// Handle moved: always answers `Edits` (an empty list if the text would not change), so
    /// the UI can keep at most one drag message in flight.
    Drag { handle: u64, target: DragTarget },
    /// Drag finished; `target` = final position (optional). Back to fine tessellation.
    DragEnd {
        handle: u64,
        #[ts(optional)]
        target: Option<DragTarget>,
    },
    /// Grid points and anchors a point handle can snap to; grid planes (`g.name`) for height,
    /// radius and plane handles. Answers `SnapTargets`.
    SnapTargets { handle: u64 },
    /// Rounding steps of free drags (Alt): lengths in mm, angles in degrees (0 = no rounding).
    /// Answers nothing.
    SetDragOptions { step_mm: f64, step_deg: f64 },
    /// Language of the engine's texts (diagnostics, hints, measure and sketch prompts,
    /// errors): `"en"` or `"de"` (also `de-DE` …; anything else: English). The UI sends it at
    /// start and on every change of its language, then its source again so the texts switch.
    /// Answers nothing.
    SetLang { lang: String },
    /// "Als Standard übernehmen": write `value` as the default of `param` of `module`.
    /// Answers `Edits`.
    SetDefault { module: String, param: String, value: Value },
    /// Export the root of the current evaluation (after the running job has finished), or the
    /// named `parts` (`Scene.parts`): STL = all in one file, 3MF = one object per part, STEP =
    /// the kernel's export. `zip`: one file per part in a ZIP package. Requests are answered in
    /// order.
    ///
    /// `colors` (Plan 16.4): 3MF with one volume per colour and filament metadata for Bambu
    /// Studio / OrcaSlicer (default for 3MF: `true`; `false`: single colour); STL with
    /// `colors = true`: a ZIP with one STL per colour (and part). Ignored for STEP.
    Export {
        format: ExportFormat,
        #[ts(optional)]
        #[serde(default)]
        parts: Option<Vec<String>>,
        #[ts(optional)]
        #[serde(default)]
        zip: Option<bool>,
        #[ts(optional)]
        #[serde(default)]
        colors: Option<bool>,
    },
    /// Colour regions of the current evaluation (after the running job has finished): per
    /// part (file context with parts) or for the root, the volume per colour and, with
    /// `meshes`, a mesh per region. Answers `ColorRegions` (or `Error`). `version`: drop the
    /// request if the evaluation has another version.
    RequestColorRegions {
        #[serde(default)]
        meshes: bool,
        #[ts(optional)]
        coarse: Option<bool>,
        #[ts(optional)]
        version: Option<u64>,
    },
    /// Writes the filament slot of colour `color` (`ColorInfo.key`) into the code: every
    /// `color()` of that colour with `filament = …` gets the new value, otherwise the first one
    /// gets `filament = n`; `filament = null` removes the argument (automatic slot). Answers
    /// `Edits`.
    SetFilament {
        color: String,
        #[ts(optional)]
        filament: Option<u32>,
    },
    /// Writes studio settings into the main file's `render { … }` block (created at the end of
    /// the file if missing): changed values in place, new settings after their kind, `null`
    /// removes. Always answers `Edits` (empty when nothing changes; plus `Error` for a bad
    /// name).
    SetRender { changes: Vec<RenderChange> },
    /// Abort the running job and drop queued mesh/export work. The last scene stays.
    Cancel,
    /// Watchdog: nodes with these cache keys (`NodeRef.key` of `Progress.current`, hex) hung
    /// the kernel. Jobs started from now on do not compute them and report the error
    /// "Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung"; a changed parameter
    /// gives a new key, which is computed again. Answers nothing (`Error` for a bad key).
    BlockKeys { keys: Vec<String> },
    /// Sketch editor (Plan 17.12): snaps the mouse ray in the plane of the sketch at `cursor`,
    /// previews the tool, and on `click` adds the point or completes the action (code as
    /// `edits`). Answers `Sketch`.
    Sketch(SketchRequest),
    /// Measure mode (Plan 18): snaps the mouse ray (vertex, edge midpoint, circle centre,
    /// edge, face; in a section the contour), and on `click` adds the pick or completes the
    /// measurement. Answers `Measure`.
    Measure(Box<MeasureRequest>),
}

/// What to measure (Plan 18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MeasureKind {
    /// Two elements: minimum distance with foot points (parallel planes: normal distance).
    Distance,
    /// Circular edge or round face: radius, diameter, centre, axis.
    Radius,
    /// Two edges or faces.
    Angle,
    /// Length of an edge (section: of a contour segment).
    Length,
    /// Area of a face (section: of a region).
    Area,
    /// Body or part of a face: bounds, volume, area.
    Body,
    /// Section only: wall thickness across the contour at a point.
    Wall,
}

/// Section plane: through `point`, `normal` (unit) points into the removed half (convention
/// of `mcad render --section`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct SectionPlaneRef {
    pub point: Vec3,
    pub normal: Vec3,
}

/// The face under the mouse (UI raycast on the displayed mesh).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct MeasureHit {
    /// Node whose mesh was hit.
    pub node: u32,
    /// `MeshData.tri_faces` of the hit triangle.
    pub face: u32,
    /// Hit point, context coordinates.
    pub point: Vec3,
}

/// A picked element. `kind`: `vertex`, `mid` (edge midpoint), `center` (circle centre),
/// `edge`, `face`, `body`; in a section `section_vertex`, `section_mid`, `section_center`,
/// `section_edge`, `region`; `point` (a free point: anchor, grid point, coordinate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MeasurePick {
    pub kind: String,
    /// Node the element belongs to.
    pub node: u32,
    /// Body (shape) of the node.
    pub body: u32,
    /// B-rep edge or face of the body; in a section: contour and segment, or region.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<u32>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<u32>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contour: Option<u32>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<u32>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<u32>,
    /// Snapped point (context coordinates).
    pub point: Vec3,
    /// Display text (`Kreismitte · Kreis R 3`).
    pub label: String,
    /// Reference for `mcad measure` (`center@[10, 10, 6]`, `face@[…]`, `k.ecke`).
    pub reference: String,
    /// Provenance: code lines, named instances, grid planes/points and anchors the element
    /// lies on.
    pub origin: Vec<String>,
    /// Highlight while hovering (polylines, context coordinates).
    pub lines: Vec<Vec<Vec3>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MeasureRequest {
    /// Echoed in the answer.
    pub id: u32,
    pub kind: MeasureKind,
    /// Elements picked so far (from the previous answer's `picks`).
    #[serde(default)]
    pub picks: Vec<MeasurePick>,
    /// Mouse ray; without it only the prompt (and `section` info) is answered.
    #[ts(optional)]
    #[serde(default)]
    pub ray: Option<Ray>,
    /// Face under the mouse (raycast of the UI).
    #[ts(optional)]
    #[serde(default)]
    pub hit: Option<MeasureHit>,
    /// Size of a pixel at distance 1 from the camera (2·tan(fov/2)/height).
    #[serde(default)]
    pub pixel: f64,
    /// Snap radius in pixels.
    #[serde(default)]
    pub snap_px: f64,
    /// Alt: no snapping to points, the exact point on the face/edge.
    #[serde(default)]
    pub free: bool,
    /// Mouse click: adds the pick; a complete measurement answers `result`.
    #[serde(default)]
    pub click: bool,
    /// Measure in this section plane (contours of plane ∩ B-rep).
    #[ts(optional)]
    #[serde(default)]
    pub section: Option<SectionPlaneRef>,
    /// The section view is on (elements in the removed half are not offered).
    #[ts(optional)]
    #[serde(default)]
    pub clip: Option<SectionPlaneRef>,
    /// Node to measure when nothing is hit (default: the root).
    #[ts(optional)]
    #[serde(default)]
    pub node: Option<u32>,
}

/// One value of a measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MeasureValue {
    /// `distance`, `dx`, `radius`, `diameter`, `angle`, `length`, `area`, `volume`, …
    pub key: String,
    pub label: String,
    pub value: f64,
    /// `mm`, `mm²`, `mm³`, `°`.
    pub unit: String,
}

/// A completed measurement (UI: drawn in the view and listed; CLI: `measurements`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Measurement {
    pub kind: MeasureKind,
    /// `Abstand`, `Radius`, `Winkel`, `Länge`, `Fläche`, `Körper`, `Wandstärke`.
    pub title: String,
    /// The main value (`values[0]`).
    pub value: f64,
    pub unit: String,
    /// Short text in the view (`12.5 mm`, `R 3 · ⌀ 6`, `45°`).
    pub label: String,
    pub values: Vec<MeasureValue>,
    /// Dimension lines (context coordinates).
    pub lines: Vec<Vec<Vec3>>,
    /// Help lines (axes, extension lines), drawn dashed.
    pub guides: Vec<Vec<Vec3>>,
    /// Markers (foot points, centres).
    pub points: Vec<Vec3>,
    /// Where the label goes.
    pub at: Vec3,
    /// Further lines of text (foot points, Δ, axis, provenance).
    pub details: Vec<String>,
    pub picks: Vec<MeasurePick>,
    /// From the exact geometry in closed form; else `tol` is the accuracy.
    pub exact: bool,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tol: Option<f64>,
    /// Measured in this section plane.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<SectionPlaneRef>,
    /// The measurement as text (copy, code comment).
    pub text: String,
    /// Arguments of `mcad measure` after the file name that repeat it.
    pub cli: Vec<String>,
}

/// The section measured in (answer to requests with `section`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MeasureSectionInfo {
    pub plane: SectionPlaneRef,
    pub regions: u32,
    pub contours: u32,
    /// mm²
    pub area: f64,
    /// All contour segments in closed form (lines, circles, ellipses from analytic surfaces).
    pub exact: bool,
    /// Largest deviation of the kernel's intersection curves from the closed forms (mm).
    pub max_dev: f64,
    /// Why there is no section (kernel boolean failed, plane misses the model).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MeasureAnswer {
    pub id: u32,
    /// Scene version the picks belong to.
    pub version: u64,
    /// Element under the mouse.
    pub hover: Option<MeasurePick>,
    /// Picks after this message.
    pub picks: Vec<MeasurePick>,
    /// Completed measurement (after a click).
    pub result: Option<Measurement>,
    /// What to pick next.
    pub prompt: String,
    /// Why a pick was not accepted.
    pub message: Option<String>,
    pub section: Option<MeasureSectionInfo>,
}

/// Tools of the sketch editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SketchTool {
    /// Hover picks a segment (`SketchAnswer.segment`).
    Select,
    Line,
    /// Arc through three points.
    ArcThrough,
    /// Arc around a centre (`angle`, or `to` on a snapped point).
    ArcCenter,
    /// Tangent arc continuing the contour.
    ArcTangent,
    /// Cubic Bézier curve (start, c1, c2, end).
    Bezier,
    Rect,
    Circle,
    /// `corner(r = …)`: vertex, then radius.
    Corner,
    /// `close();` at the insertion point (no ray needed).
    Close,
    /// Removes the segment statement at `cursor` (no ray needed).
    Delete,
}

/// Grid point of a picked point (`z` omitted: implicit plane, written `g[x, y]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchGridSel {
    pub grid: u32,
    pub x: String,
    pub y: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<String>,
}

/// A point a tool has picked: position in the sketch plane, the expression written for it and
/// what it snapped to (`grid`, `anchor`, `start`, `end`, `center`, `intersection`, `mid`,
/// `vertex`, `tangent`, `horizontal`, `vertical`, `grid_line`, `step`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchPoint {
    pub p: [f64; 2],
    pub text: String,
    pub kind: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<SketchGridSel>,
}

/// A ray in context coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchRequest {
    /// Echoed in the answer.
    pub id: u32,
    /// Editor cursor in the main file (UTF-16): picks the sketch and where new code goes.
    pub cursor: u32,
    pub tool: SketchTool,
    /// Points picked so far (from the previous answer's `points`).
    #[serde(default)]
    pub points: Vec<SketchPoint>,
    /// Mouse ray; without it only plane, prompt (and with `info` the raster) are answered.
    #[ts(optional)]
    #[serde(default)]
    pub ray: Option<Ray>,
    /// Size of a pixel at distance 1 from the camera (2·tan(fov/2)/height).
    #[serde(default)]
    pub pixel: f64,
    /// Snap radius in pixels.
    #[serde(default)]
    pub snap_px: f64,
    /// Alt: no snapping, numbers rounded to the step.
    #[serde(default)]
    pub free: bool,
    /// Mouse click: adds the point; a complete action answers `edits`.
    #[serde(default)]
    pub click: bool,
    /// Arc around a centre: sweep of the previous answer (degrees), so the arc can go on
    /// beyond 180°.
    #[ts(optional)]
    #[serde(default)]
    pub angle: Option<f64>,
    /// Also answer the raster.
    #[serde(default)]
    pub info: bool,
}

/// Where the mouse snapped (context coordinates; `local` in the sketch plane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchSnap {
    pub point: Vec3,
    pub local: [f64; 2],
    /// Expression that would be written (`g[a, b]`, `k.ecke`, `[12, 5]`).
    pub text: String,
    pub kind: String,
    /// Display text.
    pub label: String,
}

/// Drawing raster in the sketch plane (context coordinates): lines of the visible grids, grid
/// points, and the extent (sketch plane) for the step raster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchRaster {
    pub lines: Vec<[Vec3; 2]>,
    pub points: Vec<Vec3>,
    pub min: [f64; 2],
    pub max: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchAnswer {
    pub id: u32,
    /// Source version the answer (and its edits) belong to.
    pub version: u64,
    /// The sketch statement being edited (`null`: none yet, the first action creates one).
    pub sketch: Option<SrcRange>,
    /// Its node.
    pub node: Option<u32>,
    /// Sketch plane → context coordinates (camera, drawing).
    pub plane: Mat4,
    /// End of the open contour at the insertion point (context coordinates).
    pub current: Option<Vec3>,
    pub snap: Option<SketchSnap>,
    /// Help lines of the snap (horizontal/vertical, tangent).
    pub guides: Vec<Vec<Vec3>>,
    /// What the tool would draw (context coordinates).
    pub preview: Vec<Vec<Vec3>>,
    /// Tool state after this message: the points picked so far.
    pub points: Vec<SketchPoint>,
    /// Code of a completed action (one undo step).
    pub edits: Vec<Edit>,
    /// Cursor after applying `edits`: index of the edit and UTF-16 offset in its text.
    pub select: Option<[u32; 2]>,
    /// What the tool asks for next.
    pub prompt: String,
    /// Why an action did not apply.
    pub message: Option<String>,
    /// Select tool: the segment statement under the mouse.
    pub segment: Option<SrcRange>,
    /// Arc around a centre: current sweep in degrees (send back as `angle`).
    pub angle: Option<f64>,
    pub raster: Option<SketchRaster>,
}

/// Where a dragged handle goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DragTarget {
    /// Free drag (Alt): new position in the handle's local system (`Handle.frame`); for 1D
    /// handles projected onto the handle's axis. Numbers are rounded to the step.
    Point { pos: Vec3 },
    /// Free drag of a 1D handle: new value (radius, height, angle in degrees, plane, scale).
    Value { value: f64 },
    /// Dropped on a grid point or anchor: a reference is written.
    Snap { snap: SnapRef },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapRef {
    /// Grid point by plane names per axis (`_` for the implicit plane of an empty axis).
    Grid { grid: u32, planes: [String; 3] },
    /// Anchor `name` of the named instance `inst`.
    Anchor { inst: u32, name: String },
    /// Grid plane `g.name` as a number (height, radius and plane handles).
    Plane { grid: u32, plane: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ExportFormat {
    #[serde(rename = "stl")]
    Stl,
    #[serde(rename = "3mf")]
    ThreeMf,
    /// STEP AP242 with the exact B-rep (`ExportResult.triangles` is 0).
    #[serde(rename = "step")]
    Step,
}

// ------------------------------------------------------------------------------------------
// engine → UI
// ------------------------------------------------------------------------------------------

/// Message from the engine to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum FromEngine {
    /// All diagnostics of `version` (syntax, evaluation and, once the kernel job is done,
    /// kernel). Replaces earlier lists.
    Diagnostics {
        version: u64,
        items: Vec<Diagnostic>,
    },
    /// Scene description. Sent twice per evaluation: right after the language evaluation
    /// (`complete = false`: node status `pending`, no bounds/selected edges) and when the
    /// kernel job is done (`complete = true`). Not sent for sources with syntax errors (the
    /// last scene stays valid).
    Scene(Box<Scene>),
    /// Kernel job progress. `current`: node evaluated next (for the watchdog).
    Progress {
        version: u64,
        done: u32,
        total: u32,
        current: Option<NodeRef>,
    },
    /// Meshes in the node's own system; place them with `SceneNode.display_world`.
    Meshes {
        version: u64,
        items: Vec<MeshData>,
    },
    /// Text edits for the editor, valid for source `version` (UTF-16, non-overlapping).
    Edits {
        version: u64,
        edits: Vec<Edit>,
    },
    SnapTargets {
        version: u64,
        handle: u64,
        targets: Vec<SnapTarget>,
    },
    ExportResult {
        format: ExportFormat,
        #[ts(type = "Uint8Array")]
        bytes: Vec<u8>,
        triangles: u32,
        /// Parts exported (`null`: the root).
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parts: Option<Vec<String>>,
        /// A ZIP package with one file per part.
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        zip: Option<bool>,
        /// Multi-colour export (3MF with volumes per colour, ZIP with one STL per colour).
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        colors: Option<bool>,
        /// Notes for the user (multi-colour 3MF: objects, material outside parts, slicers).
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<Vec<String>>,
    },
    /// Answer to `RequestColorRegions`.
    ColorRegions {
        version: u64,
        items: Vec<RegionSet>,
    },
    Error {
        message: String,
        version: Option<u64>,
        node: Option<u32>,
    },
    /// Answer to `Sketch`.
    Sketch(Box<SketchAnswer>),
    /// Answer to `Measure`.
    Measure(Box<MeasureAnswer>),
    /// Results of the geometric assertions of `version` (Plan 21 C), sent after the kernel
    /// job (and its root mesh) together with the updated `Diagnostics`; also in
    /// `Scene.asserts` of later scenes of the same evaluation. While a handle is dragged the
    /// checks are skipped (`checked: false`).
    Asserts {
        version: u64,
        items: Vec<AssertResult>,
    },
}

/// Kind of a geometric assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AssertKind {
    Keepout,
    Disjoint,
    Intersect,
    Touches,
}

/// One checked statement instance of `assert_keepout`, `assert_disjoint`, `assert_intersect`
/// or `assert_touches`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AssertResult {
    /// The assertion's node (a group of the checked children).
    pub node: u32,
    pub kind: AssertKind,
    pub range: SrcRange,
    /// No violation (also `true` while unchecked).
    pub ok: bool,
    /// The check ran (`false`: skipped while dragging, or the node was not evaluated).
    pub checked: bool,
    /// Why nothing was checked, or a remark (`Netz: nur ja/nein`).
    pub note: Option<String>,
    /// `assert_keepout(clearance)` in mm.
    pub clearance: f64,
    /// `assert_touches(tol)` in mm.
    pub tol: f64,
    /// `assert_touches(contact)`: `face` or `any`.
    pub contact: String,
    /// Volume below which an intersection counts as none (mm³, relative to the size).
    pub eps_volume: f64,
    /// Bodies checked: real bodies of the model (keepout) or children.
    pub bodies: u32,
    /// Pairs: keepout only those with a violation, the others every pair of children.
    pub pairs: Vec<AssertPair>,
    /// Time of the check in ms (0 when every pair came from the cache).
    pub ms: f64,
    /// Every pair came from the engine cache.
    pub cached: bool,
}

/// A body of an assertion pair: a child of the assertion or a real body of the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AssertBody {
    /// Node whose result holds the body.
    pub node: u32,
    /// `deckel`, `line 7: cube`, `keep-out volume` (in the engine language, `SetLang`).
    pub label: String,
    /// Code lines and instances of the faces involved (like `mcad measure`).
    pub origin: Vec<String>,
}

/// Check of two bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AssertPair {
    pub a: AssertBody,
    pub b: AssertBody,
    pub ok: bool,
    /// `overlap`, `too_close`, `apart`, `gap`, `line_contact`, `point_contact`,
    /// `unchecked` (`null` when ok).
    pub problem: Option<String>,
    /// Text of the result (engine language), as in the diagnostic.
    pub message: String,
    /// Intersection volume in mm³ (`null`: not computed or only known as yes/no for meshes).
    pub volume: Option<f64>,
    /// Intersection decided on meshes (yes/no) instead of an exact boolean.
    pub mesh_based: bool,
    /// Smallest distance of the tessellations in mm (0 when they touch or overlap).
    pub distance: Option<f64>,
    /// Accuracy of `distance` (± mm, sum of the chordal tolerances; 0 for planar faces).
    pub accuracy: f64,
    /// Foot points of `distance` (a, b), context coordinates.
    pub points: Vec<Vec3>,
    /// `assert_touches`: `face`, `line`, `point`, `mesh` (freeform, mesh-based) or `null`.
    pub contact: Option<String>,
    /// Contact area in mm² (`face`).
    pub contact_area: Option<f64>,
    /// Contact area exact (planar faces, `region2`) or estimated on the mesh.
    pub contact_exact: bool,
    /// Intersection body (red), context coordinates.
    pub overlap: Option<AssertMesh>,
    /// Contact faces (green), context coordinates.
    pub contact_mesh: Option<AssertMesh>,
    /// Contact lines and outlines of contact faces, context coordinates.
    pub contact_lines: Vec<Vec<Vec3>>,
    /// Where to look: bounds of the intersection, the contact or the gap.
    pub focus: Option<Bounds>,
}

/// Triangles in context coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AssertMesh {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    /// Feature edges as segment pairs (xyz xyz), for the outline.
    pub edges: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum DiagSource {
    Syntax,
    Eval,
    Kernel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub range: SrcRange,
    pub source: DiagSource,
    /// Node of a kernel diagnostic.
    pub node: Option<u32>,
    /// Path of the file the range is in (omitted: the main file).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Where the problem is (context coordinates), e.g. where the operands of a Boolean touch
    /// tangentially; the UI frames it on a click, `mcad render` marks it.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec3>,
    /// Nodes the message names (the touching operands).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Edit {
    /// UTF-16 offsets.
    pub from: u32,
    pub to: u32,
    pub text: String,
    /// Path of the file to change (omitted: the main file).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NodeRef {
    pub node: u32,
    pub stmt: Option<u32>,
    pub range: SrcRange,
    /// Cache key of the node (hex; in `Progress.current`): what `BlockKeys` takes.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SnapTarget {
    /// Text written into the source, e.g. `g[rechts, hinten, deckel]` or `k.ecke`.
    pub text: String,
    pub snap: SnapRef,
    /// Value of the reference in the handle's local system.
    pub local: Vec3,
    /// Where the referenced point is (context coordinates).
    pub world: Vec3,
    /// Where the handle ends up after snapping (context coordinates).
    pub result_world: Vec3,
}

/// One tessellated node. Arrays are flat: xyz per vertex, 3 indices per triangle, one tag and
/// face index per triangle, xyz xyz per edge segment, one edge id per segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MeshData {
    /// First requested node with this mesh.
    pub node: u32,
    /// All requested nodes that share this mesh (same `key`).
    pub nodes: Vec<u32>,
    /// Content key (equals `SceneNode.mesh_key`).
    pub key: String,
    pub coarse: bool,
    /// Chordal tolerance in mm.
    pub tol: f64,
    #[ts(type = "Float32Array")]
    pub positions: Vec<f32>,
    #[ts(type = "Float32Array")]
    pub normals: Vec<f32>,
    #[ts(type = "Uint32Array")]
    pub indices: Vec<u32>,
    /// Face tag per triangle (→ `Scene.tag_nodes`).
    #[ts(type = "Uint32Array")]
    pub tri_tags: Vec<u32>,
    #[ts(type = "Uint32Array")]
    pub tri_faces: Vec<u32>,
    #[ts(type = "Float32Array")]
    pub edge_segments: Vec<f32>,
    #[ts(type = "Uint32Array")]
    pub edge_ids: Vec<u32>,
}

/// Colour regions of one node (a part or the root), in the node's system (place meshes with
/// `SceneNode.display_world`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RegionSet {
    pub node: u32,
    /// Part name (`null`: the root); `"no part"` for the material outside the parts
    /// (`cad_session::NO_PART`, fixed in every language).
    pub part: Option<String>,
    /// Volume of the node's result in mm³ (= sum of the region volumes).
    pub total: f64,
    pub regions: Vec<Region>,
    /// The region volumes do not add up to `total` (kernel boolean incomplete).
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Region {
    /// `ColorInfo.key`.
    pub color: String,
    pub filament: u32,
    /// mm³
    pub volume: f64,
    /// Requested with `meshes`.
    pub mesh: Option<MeshData>,
}

/// How a colour was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ColorSource {
    /// Material without `color()` (label `default`, filament 1).
    Default,
    /// Palette name.
    Palette,
    /// Unknown name: stable random colour (with a warning).
    Random,
    /// `#…` or `[r, g, b, a]`.
    Exact,
}

/// A colour of the material with its filament slot (Plan 16.3). Order: first appearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ColorInfo {
    /// Identity, `#rrggbb` or `#rrggbbaa` (what `SetFilament` takes).
    pub key: String,
    /// Exact value (exports).
    pub rgba: Rgba,
    /// Palette name, the unknown name as written, hex text, or `default` (material without
    /// `color()`; a fixed value, the UI shows it translated).
    pub label: String,
    pub source: ColorSource,
    /// Filament slot (1-based).
    pub filament: u32,
    /// The slot is written in the code (`filament = n`).
    pub explicit: bool,
    /// The `color()` statements of this colour (distinct, in order of appearance).
    pub ranges: Vec<SrcRange>,
    /// `color()` nodes.
    pub nodes: Vec<u32>,
}

// ------------------------------------------------------------------------------------------
// scene
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum FileRole {
    Main,
    /// Inserted by `include`.
    Include,
    /// Loaded by `use`.
    Use,
}

/// A file of the evaluated project (`SrcRange.file` indexes this list). A file included twice
/// appears twice (same path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct FileInfo {
    pub id: u32,
    pub path: String,
    pub role: FileRole,
}

/// An `include <…>`/`use <…>` statement: `range` covers `<path>` (Ctrl+click opens `target`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct IncludeRef {
    pub range: SrcRange,
    /// Resolved project path.
    pub target: String,
    pub is_use: bool,
    /// The file was found.
    pub found: bool,
}

/// A part (Plan 12.9): named instance at the top level of the main file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Part {
    /// `deckel`, `k[2]`: export name.
    pub name: String,
    /// `ModuleCall` node.
    pub node: u32,
    pub inst: u32,
    pub range: SrcRange,
    /// Bounds in context coordinates (complete scene).
    pub bounds: Option<Bounds>,
    /// Volume in mm³ (complete scene, 3D parts).
    pub volume: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Scene {
    pub version: u64,
    /// Files of the project, index = id (`SrcRange.file`); file 0 is the main file.
    #[serde(default)]
    pub files: Vec<FileInfo>,
    /// `include`/`use` statements of all files.
    #[serde(default)]
    pub includes: Vec<IncludeRef>,
    /// Parts (file context only).
    #[serde(default)]
    pub parts: Vec<Part>,
    /// Colours of the material and their filament slots (volumes: `RequestColorRegions`).
    #[serde(default)]
    pub colors: Vec<ColorInfo>,
    /// `false`: preliminary scene before the kernel job (see `FromEngine.Scene`).
    pub complete: bool,
    pub context: ContextInfo,
    pub root: Option<u32>,
    /// A `!` modifier made a nested node the root (status line hint); `display_world` is
    /// relative to it.
    pub root_is_subtree: bool,
    /// Geometry tree, indexed by `SceneNode.idx`.
    pub nodes: Vec<SceneNode>,
    pub grids: Vec<Grid>,
    /// Anchors of all module instances in context coordinates.
    pub anchors: Vec<Anchor>,
    /// Module instances; index = id, 0 = context root.
    pub instances: Vec<Instance>,
    /// Editable handles of the context module.
    pub handles: Vec<Handle>,
    /// Handles of computed values (`locked = true`): shown, not editable.
    pub locked_handles: Vec<Handle>,
    /// Inline hints of assignments.
    pub hints: Vec<Hint>,
    /// Module signatures (parameter bar, path bar).
    pub modules: Vec<Module>,
    /// Grid and anchor reference expressions in the context module (hover, jump).
    pub refs: Vec<Ref>,
    /// Mesh face tag → nodes that created those faces.
    pub tag_nodes: Vec<TagNodes>,
    pub echoes: Vec<Echo>,
    /// The language evaluation was aborted (step budget, recursion, failed assertion).
    pub aborted: bool,
    pub stats: Stats,
    /// Results of the geometric assertions (empty until checked, see `FromEngine.Asserts`).
    #[serde(default)]
    pub asserts: Vec<AssertResult>,
    /// The main file's `render { … }` block (studio look of the file; `None`: none).
    #[serde(default)]
    #[ts(optional)]
    pub render: Option<RenderBlock>,
}

/// One change of the render block (`SetRender`); `value: null` removes the setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderChange {
    /// A top-level setting (`edges`, `ambient`, …).
    Set {
        key: String,
        value: Value,
    },
    /// A value of `light NAME { … }` (the entry is created if missing).
    Light {
        name: String,
        key: String,
        value: Value,
    },
    RemoveLight {
        name: String,
    },
    /// A value of `material SEL { … }` (created if missing).
    Material {
        target: RenderTarget,
        key: String,
        value: Value,
    },
    RemoveMaterial {
        target: RenderTarget,
    },
}

/// `render { … }` of the main file: the values given (`None`: not set, the default applies).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct RenderBlock {
    pub range: SrcRange,
    /// `studio` or `standard`.
    pub style: Option<String>,
    pub edges: Option<bool>,
    /// `studio`, `overcast` or `none`.
    pub environment: Option<String>,
    pub ambient: Option<f64>,
    pub light_intensity: Option<f64>,
    pub shadows: Option<bool>,
    pub shadow_softness: Option<f64>,
    pub ao: Option<bool>,
    pub ao_radius: Option<f64>,
    pub ao_strength: Option<f64>,
    pub ground: Option<bool>,
    /// `#rrggbb`.
    pub ground_color: Option<String>,
    /// One or two (top, bottom) `#rrggbb`.
    pub background: Option<Vec<String>>,
    pub lights: Vec<RenderLight>,
    pub materials: Vec<RenderMaterial>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RenderLight {
    pub name: String,
    pub azimuth: f64,
    pub elevation: f64,
    pub intensity: f64,
    /// `#rrggbb`.
    pub color: String,
    pub range: SrcRange,
}

/// What a material entry applies to: a colour as written (`red`, `#rrggbb`, `default` =
/// uncoloured material) or a part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum RenderTarget {
    Color { color: String },
    Part { part: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RenderMaterial {
    pub target: RenderTarget,
    pub preset: Option<String>,
    pub metalness: Option<f64>,
    pub roughness: Option<f64>,
    pub clearcoat: Option<f64>,
    pub range: SrcRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Ok,
    Warning,
    Error,
    /// Disabled (`*`) or not reachable from the root.
    Skipped,
    /// Kernel job not finished yet.
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Dim {
    #[serde(rename = "2d")]
    Two,
    #[serde(rename = "3d")]
    Three,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct NodeFlags {
    /// `#`: additionally shown red transparent.
    pub highlight: bool,
    /// `%`: not in the result, shown grey transparent.
    pub background: bool,
    /// `*`: not in the result, not shown.
    pub disabled: bool,
    /// Zone of a fillet/chamfer/shell block: not in the result, shown blue transparent.
    pub zone: bool,
    /// `assert_keepout`: keep-out volume, not in the result (also `background`), shown
    /// transparent with its outline in its own colour.
    #[serde(default)]
    pub keepout: bool,
}

/// Edge/face selection of a zone block (`select`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SelectMode {
    Inside,
    Touch,
}

/// Resolved parameters of operations the view draws specially (zone blocks, profile
/// operations, surfaces). Points are in the node system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpInfo {
    /// `fillet(r) { … }` / `chamfer(d) { … }`.
    Blend {
        chamfer: bool,
        r: f64,
        select: SelectMode,
    },
    /// `shell(t) { … }`.
    Shell {
        t: f64,
        select: SelectMode,
    },
    /// `zone(linie, d)` / `zone(ebene, d)`.
    Zone {
        line: bool,
        point: Vec3,
        dir: Vec3,
        d: f64,
    },
    LinearExtrude {
        h: f64,
        center: bool,
        twist: f64,
        scale: [f64; 2],
    },
    /// `angle` in degrees.
    RotateExtrude {
        angle: f64,
    },
    Loft {
        ruled: bool,
    },
    Sweep {
        path: Vec<Vec3>,
        spline: bool,
    },
    /// Control points, rows of equal length.
    Surface {
        rows: Vec<Vec<Vec3>>,
    },
    Thicken {
        t: f64,
    },
    /// `fillet2d(r) { … }` / `chamfer2d(d) { … }` (Plan 17.3).
    Blend2d {
        chamfer: bool,
        r: f64,
        select: SelectMode,
    },
    /// `offset(r = …)` (`round`) / `offset(delta = …, chamfer)`.
    Offset {
        round: bool,
        d: f64,
        chamfer: bool,
    },
    /// `stroke(w, cap, join)`.
    Stroke {
        w: f64,
        cap: String,
        join: String,
    },
    /// `sketch { … }` (Plan 17.1): contours with their segments, for drawing the sketch in its
    /// plane (outline, fill, the segment of the cursor line).
    Sketch {
        closed: bool,
        contours: Vec<SketchContour>,
        /// The sketch plane → context coordinates (like `display_world`, but for the profile of
        /// a `rotate_extrude` the xz-plane and for a centred `linear_extrude` its bottom: where
        /// the profile lies in the body; Plan 17.12).
        plane: Mat4,
    },
}

/// One contour of a sketch (node system, z = 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchContour {
    /// In a `hole { … }` block.
    pub hole: bool,
    pub closed: bool,
    /// Start point (`start(p)`).
    pub start: Vec3,
    /// The `start(…)` statement.
    pub start_range: Option<SrcRange>,
    pub segments: Vec<SketchSegment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SketchSegment {
    /// `line`, `arc`, `bezier`, `spline`.
    pub kind: String,
    /// Statement that made the segment.
    pub stmt: u32,
    pub range: SrcRange,
    /// Polyline from start to end (display tolerance).
    pub points: Vec<Vec3>,
    /// Help lines: control polygon of a Bézier curve (start, control points, end); centre of an
    /// arc (one point).
    pub ctrl: Vec<Vec3>,
    /// Closing line added for a contour without `close()` (error).
    pub implicit: bool,
    /// An error was reported for this segment.
    pub error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GuideLine {
    pub kind: String,
    pub points: Vec<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NodeMessage {
    pub severity: Severity,
    pub message: String,
    /// Affected edges (polylines, node system), e.g. the edge a fillet failed on, the edges
    /// along which the operands of a Boolean touch tangentially.
    pub edges: Vec<Vec<Vec3>>,
    /// Place of the problem (node system), e.g. where the operands touch.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SceneNode {
    pub idx: u32,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    /// Statement id (`null` for the synthetic file root).
    pub stmt: Option<u32>,
    /// Range of the statement.
    pub range: SrcRange,
    /// Operation: `cube`/`box`, `translate`, `difference`, module name, …
    pub label: String,
    /// `k` of `k: modul();` (module call nodes).
    pub instance: Option<String>,
    pub flags: NodeFlags,
    /// Non-first child of a `difference` (shown red transparent when the difference is
    /// selected).
    pub subtracted: bool,
    /// `color()`.
    pub color: Option<Rgba>,
    /// Node system → context root.
    pub world: Mat4,
    /// Where to place this node's mesh (equals `world` unless `root_is_subtree`).
    pub display_world: Mat4,
    /// Module instance the node belongs to (index into `Scene.instances`).
    pub module_inst: u32,
    /// Stable identity across versions (statement + instance path).
    pub inst_key: u64,
    pub status: NodeStatus,
    pub messages: Vec<NodeMessage>,
    /// Inline hints of the node, e.g. "4 edges".
    pub hints: Vec<String>,
    /// Edges selected by fillet/chamfer (polylines, node system); the new corner pieces of
    /// `fillet2d`/`chamfer2d`.
    pub selected_edges: Vec<Vec<Vec3>>,
    /// Guide lines of 2D operations (node system): `source` (input contour of `offset`),
    /// `result` (offset contour, drawn dashed), `centerline` (path of `stroke`).
    pub guides: Vec<GuideLine>,
    pub opened_faces: u32,
    /// Draft mode while dragging: this node's blend was left out (it took longer than the
    /// draft limit); the full evaluation after the drag restores it.
    pub blend_suspended: bool,
    /// Bounds in the node system.
    pub bounds: Option<Bounds>,
    pub dim: Option<Dim>,
    /// Content key of the node's mesh (request it with `RequestMeshes`).
    pub mesh_key: Option<String>,
    /// Face tag of faces this node creates.
    pub tag: u32,
    /// Result came from the cache.
    pub cached: bool,
    /// `$tol` in effect.
    pub tol: Option<f64>,
    /// Parameters of zone blocks, profile operations and surfaces (special display).
    pub op_info: Option<OpInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TagNodes {
    pub tag: u32,
    pub nodes: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ParamValue {
    pub name: String,
    pub value: Value,
    pub overridden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ContextInfo {
    /// `null` for the file.
    pub module: Option<String>,
    pub def_stmt: Option<u32>,
    pub def_range: Option<SrcRange>,
    /// Path bar: enclosing modules and the module itself.
    pub path: Vec<String>,
    /// Parameter values used for the context module.
    pub params: Vec<ParamValue>,
    /// Node a `!` modifier made the root.
    pub root_modifier: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Plane {
    /// 0 = x, 1 = y, 2 = z.
    pub axis: u8,
    /// Display and reference name (`rechts`, `loch[2]`, `_`).
    pub name: String,
    pub base: String,
    pub index: Option<u32>,
    /// Coordinate in the grid system.
    pub value: f64,
    /// Value expression.
    pub value_range: Option<SrcRange>,
    /// Editable literal defining the value (solid line + handle); `null` = expression (dashed,
    /// locked).
    pub literal: Option<SrcRange>,
    /// The value is (or comes from) a grid/anchor reference.
    pub is_ref: bool,
    /// Implicit plane `_` at 0 of an axis without planes.
    pub implicit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Grid {
    pub id: u32,
    pub name: String,
    pub stmt: u32,
    pub range: SrcRange,
    pub name_range: SrcRange,
    /// Module instance that declared the grid.
    pub module_inst: u32,
    /// Declared in the context module (shown, editable).
    pub in_context: bool,
    /// Grid system → context root.
    pub frame: Mat4,
    /// Grid system → its module's system.
    pub frame_module: Mat4,
    /// Sorted by axis, then value.
    pub planes: Vec<Plane>,
    /// Where the grid can be referenced.
    pub visible: SrcRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LocalAnchor {
    pub name: String,
    pub stmt: u32,
    pub range: SrcRange,
    /// Instance coordinates.
    pub point: Vec3,
    pub dir: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Anchor {
    pub inst: u32,
    /// Index in `Instance.anchors`.
    pub index: u32,
    pub name: String,
    /// Label of the named instance (`k`), if any.
    pub instance: Option<String>,
    /// Reference text (`k.ecke`) if the instance is named.
    pub text: Option<String>,
    pub stmt: u32,
    pub range: SrcRange,
    /// Context coordinates.
    pub point: Vec3,
    pub dir: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Instance {
    pub id: u32,
    /// Module name (empty for the file).
    pub module: String,
    pub label: Option<String>,
    pub call_stmt: Option<u32>,
    pub call_range: Option<SrcRange>,
    pub def_stmt: Option<u32>,
    pub def_range: Option<SrcRange>,
    /// `ModuleCall` node.
    pub node: Option<u32>,
    pub parent: Option<u32>,
    /// Instance system → parent module system.
    pub matrix: Mat4,
    /// Instance system → context root.
    pub world: Mat4,
    pub anchors: Vec<LocalAnchor>,
    /// Named instances: where `k` can be referenced.
    pub visible: Option<SrcRange>,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HandleKind {
    Point,
    Radius,
    Height,
    /// Rotation about axis 0..2 (x, y, z of `frame`) or 3 (`Handle.axis`).
    Angle {
        axis: u8,
    },
    /// Grid plane along an axis of the grid system.
    Plane {
        axis: u8,
    },
    /// Scale factor (`axis = null`: uniform).
    Scale {
        axis: Option<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Comp {
    /// Literal encoding this component: `literal = value * scale`.
    pub range: SrcRange,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Handle {
    /// Stable across versions.
    pub id: u64,
    pub stmt: u32,
    pub range: SrcRange,
    pub kind: HandleKind,
    pub index: u32,
    /// Handle's local system → context root.
    pub frame: Mat4,
    /// Position in the local system.
    pub pos: Vec3,
    /// Position in context coordinates.
    pub world_pos: Vec3,
    /// 1D handles: direction (`pos = base + axis * value[0]`); angles: rotation axis.
    pub axis: Option<Vec3>,
    pub base: Option<Vec3>,
    pub value: Vec<f64>,
    /// Literal per component, `null` = locked.
    pub comps: Vec<Option<Comp>>,
    /// Some component comes from a grid or anchor reference.
    pub has_ref: bool,
    pub expr_range: SrcRange,
    pub def_range: Option<SrcRange>,
    /// Edited through a variable (all uses move).
    pub via_var: bool,
    /// Edited through a module parameter default (all calls change).
    pub via_default: bool,
    /// Computed value: shown, not editable (no literal, no reference).
    pub locked: bool,
    /// Grids / named instances visible at the statement (snap targets).
    pub grids: Vec<u32>,
    pub instances: Vec<u32>,
    pub node: Option<u32>,
    pub module_inst: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Hint {
    pub stmt: u32,
    pub range: SrcRange,
    pub text: String,
    pub value: Value,
    /// Literal defining the value (slider / switch).
    pub literal: Option<SrcRange>,
    pub in_context: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    Number,
    Bool,
    Vector,
    String,
    List,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Param {
    pub name: String,
    pub range: SrcRange,
    pub default_text: Option<String>,
    pub default_range: Option<SrcRange>,
    pub default_value: Option<Value>,
    pub ty: ParamType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Module {
    pub name: String,
    pub stmt: u32,
    pub range: SrcRange,
    pub name_range: SrcRange,
    pub body_range: SrcRange,
    /// Enclosing modules, outermost first.
    pub path: Vec<String>,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sel {
    Plane { plane: u32 },
    Wild,
    Coord { value: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefTarget {
    /// Grid point / line / plane (`plane` indexes `Grid.planes`).
    Grid {
        grid: u32,
        sel: [Sel; 3],
    },
    /// `g.name`.
    Plane {
        grid: u32,
        plane: u32,
    },
    Anchor {
        inst: u32,
        anchor: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Ref {
    pub range: SrcRange,
    pub stmt: Option<u32>,
    pub target: RefTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Echo {
    pub stmt: Option<u32>,
    pub range: SrcRange,
    pub text: String,
    pub in_context: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct Stats {
    /// Parse + language evaluation.
    pub eval_ms: f64,
    /// Kernel job (sum of step times).
    pub kernel_ms: f64,
    pub nodes: u32,
    pub cache_hits: u32,
    pub kernel_calls: u32,
    pub steps: u32,
    /// Language evaluation steps.
    pub eval_steps: u32,
    /// Blends left out in draft mode (while dragging).
    pub suspended_blends: u32,
}

/// TypeScript declarations of all protocol types (content of `web/src/engine/protocol.ts`).
pub fn typescript() -> String {
    let cfg = ts_rs::Config::new().with_large_int("number");
    let mut out = String::from(
        "// Generated from crates/cad-session/src/protocol.rs (ts-rs). Do not edit.\n\
         // Regenerate: UPDATE_PROTOCOL_TS=1 cargo test -p cad-session --test protocol_ts\n\n",
    );
    out.push_str(&format!("export const PROTOCOL_VERSION = {PROTOCOL_VERSION};\n"));
    macro_rules! decl {
        ($($t:ty),* $(,)?) => {$(
            out.push('\n');
            if let Some(d) = <$t as TS>::docs() {
                out.push_str(&d);
            }
            out.push_str("export ");
            out.push_str(&<$t as TS>::decl(&cfg));
            out.push('\n');
        )*};
    }
    decl!(
        Vec3,
        Mat4,
        Rgba,
        SrcRange,
        Value,
        FileRole,
        FileInfo,
        IncludeRef,
        Part,
        ToEngine,
        SketchTool,
        SketchGridSel,
        SketchPoint,
        Ray,
        SketchRequest,
        SketchSnap,
        SketchRaster,
        SketchAnswer,
        MeasureKind,
        SectionPlaneRef,
        MeasureHit,
        MeasurePick,
        MeasureRequest,
        MeasureValue,
        Measurement,
        MeasureSectionInfo,
        MeasureAnswer,
        DragTarget,
        SnapRef,
        ExportFormat,
        FromEngine,
        Severity,
        DiagSource,
        Diagnostic,
        Edit,
        NodeRef,
        SnapTarget,
        MeshData,
        RegionSet,
        Region,
        ColorSource,
        ColorInfo,
        AssertKind,
        AssertResult,
        AssertBody,
        AssertPair,
        AssertMesh,
        Scene,
        RenderBlock,
        RenderChange,
        RenderLight,
        RenderTarget,
        RenderMaterial,
        NodeStatus,
        Dim,
        NodeFlags,
        SelectMode,
        OpInfo,
        SketchContour,
        SketchSegment,
        GuideLine,
        Bounds,
        NodeMessage,
        SceneNode,
        TagNodes,
        ParamValue,
        ContextInfo,
        Plane,
        Grid,
        LocalAnchor,
        Anchor,
        Instance,
        HandleKind,
        Comp,
        Handle,
        Hint,
        ParamType,
        Param,
        Module,
        Sel,
        RefTarget,
        Ref,
        Echo,
        Stats,
    );
    out
}
