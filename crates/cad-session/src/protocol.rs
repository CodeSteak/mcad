//! Messages between the UI and the engine (Plan 7.1). Plain serde types; the TypeScript
//! declarations in `web/src/engine/protocol.ts` are generated from them ([`typescript`]).
//!
//! Conventions:
//! * Source positions are **UTF-16 code unit offsets** (CodeMirror) plus 1-based line and
//!   column (column in UTF-16 units), see [`SrcRange`].
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
pub const PROTOCOL_VERSION: u32 = 1;

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
/// `end_line`/`end_col`: 1-based position of `to`. Columns count UTF-16 units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct SrcRange {
    pub from: u32,
    pub to: u32,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
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
    /// What to show: `module = null` for the whole file, otherwise a module name (or
    /// `outer::inner`) evaluated with its defaults and `overrides` (parameter bar).
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
    /// Handle moved: answers `Edits` (nothing if the text would not change).
    Drag { handle: u64, target: DragTarget },
    /// Drag finished; `target` = final position (optional). Back to fine tessellation.
    DragEnd {
        handle: u64,
        #[ts(optional)]
        target: Option<DragTarget>,
    },
    /// Grid points and anchors a point handle can snap to. Answers `SnapTargets`.
    SnapTargets { handle: u64 },
    /// "Als Standard übernehmen": write `value` as the default of `param` of `module`.
    /// Answers `Edits`.
    SetDefault { module: String, param: String, value: Value },
    /// Export the root of the current evaluation (after the running job has finished).
    Export { format: ExportFormat },
    /// Abort the running job and drop queued mesh/export work. The last scene stays.
    Cancel,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ExportFormat {
    #[serde(rename = "stl")]
    Stl,
    #[serde(rename = "3mf")]
    ThreeMf,
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
    },
    Error {
        message: String,
        version: Option<u64>,
        node: Option<u32>,
    },
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Edit {
    /// UTF-16 offsets.
    pub from: u32,
    pub to: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NodeRef {
    pub node: u32,
    pub stmt: Option<u32>,
    pub range: SrcRange,
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

// ------------------------------------------------------------------------------------------
// scene
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Scene {
    pub version: u64,
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
    pub handles: Vec<Handle>,
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
    /// Affected edges (polylines, node system), e.g. the edge a fillet failed on.
    pub edges: Vec<Vec<Vec3>>,
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
    /// Inline hints of the node, e.g. "4 Kanten".
    pub hints: Vec<String>,
    /// Edges selected by fillet/chamfer (polylines, node system).
    pub selected_edges: Vec<Vec<Vec3>>,
    pub opened_faces: u32,
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
        ToEngine,
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
        Scene,
        NodeStatus,
        Dim,
        NodeFlags,
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
