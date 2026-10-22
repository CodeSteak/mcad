// Generated from crates/cad-session/src/protocol.rs (ts-rs). Do not edit.
// Regenerate: UPDATE_PROTOCOL_TS=1 cargo test -p cad-session --test protocol_ts

export const PROTOCOL_VERSION = 1;

/**
 * `[x, y, z]`
 */
export type Vec3 = [number, number, number];

/**
 * 4×4 matrix, column-major.
 */
export type Mat4 = [number, number, number, number, number, number, number, number, number, number, number, number, number, number, number, number];

/**
 * RGBA, 0..1.
 */
export type Rgba = [number, number, number, number];

/**
 * A source range. `from`/`to`: UTF-16 offsets (end exclusive). `line`/`col`: 1-based start,
 * `end_line`/`end_col`: 1-based position of `to`. Columns count UTF-16 units.
 */
export type SrcRange = { from: number, to: number, line: number, col: number, end_line: number, end_col: number, };

/**
 * A language value: `null` = undef, numbers, bools, strings, lists, ranges; grids,
 * instances and functions as `{ opaque: "<type>" }`.
 */
export type Value = null | boolean | number | string | Array<Value> | { start: number, step: number, end: number, } | { opaque: string, };

/**
 * Message from the UI to the engine.
 */
export type ToEngine = { "type": "SetSource", text: string, version: number, } | { "type": "SetContext", module?: string, overrides: { [key in string]: Value }, } | { "type": "RequestMeshes", nodes: Array<number>, coarse?: boolean, version?: number, } | { "type": "DragStart", handle: number, } | { "type": "Drag", handle: number, target: DragTarget, } | { "type": "DragEnd", handle: number, target?: DragTarget, } | { "type": "SnapTargets", handle: number, } | { "type": "SetDefault", module: string, param: string, value: Value, } | { "type": "Export", format: ExportFormat, } | { "type": "Cancel" };

/**
 * Where a dragged handle goes.
 */
export type DragTarget = { "kind": "point", pos: Vec3, } | { "kind": "value", value: number, } | { "kind": "snap", snap: SnapRef, };

export type SnapRef = { "kind": "grid", grid: number, planes: [string, string, string], } | { "kind": "anchor", inst: number, name: string, };

export type ExportFormat = "stl" | "3mf";

/**
 * Message from the engine to the UI.
 */
export type FromEngine = { "type": "Diagnostics", version: number, items: Array<Diagnostic>, } | { "type": "Scene" } & Scene | { "type": "Progress", version: number, done: number, total: number, current: NodeRef | null, } | { "type": "Meshes", version: number, items: Array<MeshData>, } | { "type": "Edits", version: number, edits: Array<Edit>, } | { "type": "SnapTargets", version: number, handle: number, targets: Array<SnapTarget>, } | { "type": "ExportResult", format: ExportFormat, bytes: Uint8Array, triangles: number, } | { "type": "Error", message: string, version: number | null, node: number | null, };

export type Severity = "error" | "warning" | "info";

export type DiagSource = "syntax" | "eval" | "kernel";

export type Diagnostic = { severity: Severity, message: string, range: SrcRange, source: DiagSource, 
/**
 * Node of a kernel diagnostic.
 */
node: number | null, };

export type Edit = { 
/**
 * UTF-16 offsets.
 */
from: number, to: number, text: string, };

export type NodeRef = { node: number, stmt: number | null, range: SrcRange, };

export type SnapTarget = { 
/**
 * Text written into the source, e.g. `g[rechts, hinten, deckel]` or `k.ecke`.
 */
text: string, snap: SnapRef, 
/**
 * Value of the reference in the handle's local system.
 */
local: Vec3, 
/**
 * Where the referenced point is (context coordinates).
 */
world: Vec3, 
/**
 * Where the handle ends up after snapping (context coordinates).
 */
result_world: Vec3, };

/**
 * One tessellated node. Arrays are flat: xyz per vertex, 3 indices per triangle, one tag and
 * face index per triangle, xyz xyz per edge segment, one edge id per segment.
 */
export type MeshData = { 
/**
 * First requested node with this mesh.
 */
node: number, 
/**
 * All requested nodes that share this mesh (same `key`).
 */
nodes: Array<number>, 
/**
 * Content key (equals `SceneNode.mesh_key`).
 */
key: string, coarse: boolean, 
/**
 * Chordal tolerance in mm.
 */
tol: number, positions: Float32Array, normals: Float32Array, indices: Uint32Array, 
/**
 * Face tag per triangle (→ `Scene.tag_nodes`).
 */
tri_tags: Uint32Array, tri_faces: Uint32Array, edge_segments: Float32Array, edge_ids: Uint32Array, };

export type Scene = { version: number, 
/**
 * `false`: preliminary scene before the kernel job (see `FromEngine.Scene`).
 */
complete: boolean, context: ContextInfo, root: number | null, 
/**
 * A `!` modifier made a nested node the root (status line hint); `display_world` is
 * relative to it.
 */
root_is_subtree: boolean, 
/**
 * Geometry tree, indexed by `SceneNode.idx`.
 */
nodes: Array<SceneNode>, grids: Array<Grid>, 
/**
 * Anchors of all module instances in context coordinates.
 */
anchors: Array<Anchor>, 
/**
 * Module instances; index = id, 0 = context root.
 */
instances: Array<Instance>, handles: Array<Handle>, 
/**
 * Inline hints of assignments.
 */
hints: Array<Hint>, 
/**
 * Module signatures (parameter bar, path bar).
 */
modules: Array<Module>, 
/**
 * Grid and anchor reference expressions in the context module (hover, jump).
 */
refs: Array<Ref>, 
/**
 * Mesh face tag → nodes that created those faces.
 */
tag_nodes: Array<TagNodes>, echoes: Array<Echo>, 
/**
 * The language evaluation was aborted (step budget, recursion, failed assertion).
 */
aborted: boolean, stats: Stats, };

export type NodeStatus = "ok" | "warning" | "error" | "skipped" | "pending";

export type Dim = "2d" | "3d";

export type NodeFlags = { 
/**
 * `#`: additionally shown red transparent.
 */
highlight: boolean, 
/**
 * `%`: not in the result, shown grey transparent.
 */
background: boolean, 
/**
 * `*`: not in the result, not shown.
 */
disabled: boolean, 
/**
 * Zone of a fillet/chamfer/shell block: not in the result, shown blue transparent.
 */
zone: boolean, };

export type Bounds = { min: Vec3, max: Vec3, };

export type NodeMessage = { severity: Severity, message: string, 
/**
 * Affected edges (polylines, node system), e.g. the edge a fillet failed on.
 */
edges: Array<Array<Vec3>>, };

export type SceneNode = { idx: number, parent: number | null, children: Array<number>, 
/**
 * Statement id (`null` for the synthetic file root).
 */
stmt: number | null, 
/**
 * Range of the statement.
 */
range: SrcRange, 
/**
 * Operation: `cube`/`box`, `translate`, `difference`, module name, …
 */
label: string, 
/**
 * `k` of `k: modul();` (module call nodes).
 */
instance: string | null, flags: NodeFlags, 
/**
 * Non-first child of a `difference` (shown red transparent when the difference is
 * selected).
 */
subtracted: boolean, 
/**
 * `color()`.
 */
color: Rgba | null, 
/**
 * Node system → context root.
 */
world: Mat4, 
/**
 * Where to place this node's mesh (equals `world` unless `root_is_subtree`).
 */
display_world: Mat4, 
/**
 * Module instance the node belongs to (index into `Scene.instances`).
 */
module_inst: number, 
/**
 * Stable identity across versions (statement + instance path).
 */
inst_key: number, status: NodeStatus, messages: Array<NodeMessage>, 
/**
 * Inline hints of the node, e.g. "4 Kanten".
 */
hints: Array<string>, 
/**
 * Edges selected by fillet/chamfer (polylines, node system).
 */
selected_edges: Array<Array<Vec3>>, opened_faces: number, 
/**
 * Bounds in the node system.
 */
bounds: Bounds | null, dim: Dim | null, 
/**
 * Content key of the node's mesh (request it with `RequestMeshes`).
 */
mesh_key: string | null, 
/**
 * Face tag of faces this node creates.
 */
tag: number, 
/**
 * Result came from the cache.
 */
cached: boolean, 
/**
 * `$tol` in effect.
 */
tol: number | null, };

export type TagNodes = { tag: number, nodes: Array<number>, };

export type ParamValue = { name: string, value: Value, overridden: boolean, };

export type ContextInfo = { 
/**
 * `null` for the file.
 */
module: string | null, def_stmt: number | null, def_range: SrcRange | null, 
/**
 * Path bar: enclosing modules and the module itself.
 */
path: Array<string>, 
/**
 * Parameter values used for the context module.
 */
params: Array<ParamValue>, 
/**
 * Node a `!` modifier made the root.
 */
root_modifier: number | null, };

export type Plane = { 
/**
 * 0 = x, 1 = y, 2 = z.
 */
axis: number, 
/**
 * Display and reference name (`rechts`, `loch[2]`, `_`).
 */
name: string, base: string, index: number | null, 
/**
 * Coordinate in the grid system.
 */
value: number, 
/**
 * Value expression.
 */
value_range: SrcRange | null, 
/**
 * Editable literal defining the value (solid line + handle); `null` = expression (dashed,
 * locked).
 */
literal: SrcRange | null, 
/**
 * The value is (or comes from) a grid/anchor reference.
 */
is_ref: boolean, 
/**
 * Implicit plane `_` at 0 of an axis without planes.
 */
implicit: boolean, };

export type Grid = { id: number, name: string, stmt: number, range: SrcRange, name_range: SrcRange, 
/**
 * Module instance that declared the grid.
 */
module_inst: number, 
/**
 * Declared in the context module (shown, editable).
 */
in_context: boolean, 
/**
 * Grid system → context root.
 */
frame: Mat4, 
/**
 * Grid system → its module's system.
 */
frame_module: Mat4, 
/**
 * Sorted by axis, then value.
 */
planes: Array<Plane>, 
/**
 * Where the grid can be referenced.
 */
visible: SrcRange, };

export type LocalAnchor = { name: string, stmt: number, range: SrcRange, 
/**
 * Instance coordinates.
 */
point: Vec3, dir: Vec3 | null, };

export type Anchor = { inst: number, 
/**
 * Index in `Instance.anchors`.
 */
index: number, name: string, 
/**
 * Label of the named instance (`k`), if any.
 */
instance: string | null, 
/**
 * Reference text (`k.ecke`) if the instance is named.
 */
text: string | null, stmt: number, range: SrcRange, 
/**
 * Context coordinates.
 */
point: Vec3, dir: Vec3 | null, };

export type Instance = { id: number, 
/**
 * Module name (empty for the file).
 */
module: string, label: string | null, call_stmt: number | null, call_range: SrcRange | null, def_stmt: number | null, def_range: SrcRange | null, 
/**
 * `ModuleCall` node.
 */
node: number | null, parent: number | null, 
/**
 * Instance system → parent module system.
 */
matrix: Mat4, 
/**
 * Instance system → context root.
 */
world: Mat4, anchors: Array<LocalAnchor>, 
/**
 * Named instances: where `k` can be referenced.
 */
visible: SrcRange | null, depth: number, };

export type HandleKind = { "kind": "point" } | { "kind": "radius" } | { "kind": "height" } | { "kind": "angle", axis: number, } | { "kind": "plane", axis: number, } | { "kind": "scale", axis: number | null, };

export type Comp = { 
/**
 * Literal encoding this component: `literal = value * scale`.
 */
range: SrcRange, scale: number, };

export type Handle = { 
/**
 * Stable across versions.
 */
id: number, stmt: number, range: SrcRange, kind: HandleKind, index: number, 
/**
 * Handle's local system → context root.
 */
frame: Mat4, 
/**
 * Position in the local system.
 */
pos: Vec3, 
/**
 * Position in context coordinates.
 */
world_pos: Vec3, 
/**
 * 1D handles: direction (`pos = base + axis * value[0]`); angles: rotation axis.
 */
axis: Vec3 | null, base: Vec3 | null, value: Array<number>, 
/**
 * Literal per component, `null` = locked.
 */
comps: Array<Comp | null>, 
/**
 * Some component comes from a grid or anchor reference.
 */
has_ref: boolean, expr_range: SrcRange, def_range: SrcRange | null, 
/**
 * Edited through a variable (all uses move).
 */
via_var: boolean, 
/**
 * Edited through a module parameter default (all calls change).
 */
via_default: boolean, 
/**
 * Grids / named instances visible at the statement (snap targets).
 */
grids: Array<number>, instances: Array<number>, node: number | null, module_inst: number, };

export type Hint = { stmt: number, range: SrcRange, text: string, value: Value, 
/**
 * Literal defining the value (slider / switch).
 */
literal: SrcRange | null, in_context: boolean, };

export type ParamType = "number" | "bool" | "vector" | "string" | "list" | "unknown";

export type Param = { name: string, range: SrcRange, default_text: string | null, default_range: SrcRange | null, default_value: Value | null, ty: ParamType, };

export type Module = { name: string, stmt: number, range: SrcRange, name_range: SrcRange, body_range: SrcRange, 
/**
 * Enclosing modules, outermost first.
 */
path: Array<string>, params: Array<Param>, };

export type Sel = { "kind": "plane", plane: number, } | { "kind": "wild" } | { "kind": "coord", value: number, };

export type RefTarget = { "kind": "grid", grid: number, sel: [Sel, Sel, Sel], } | { "kind": "plane", grid: number, plane: number, } | { "kind": "anchor", inst: number, anchor: number, };

export type Ref = { range: SrcRange, stmt: number | null, target: RefTarget, };

export type Echo = { stmt: number | null, range: SrcRange, text: string, in_context: boolean, };

export type Stats = { 
/**
 * Parse + language evaluation.
 */
eval_ms: number, 
/**
 * Kernel job (sum of step times).
 */
kernel_ms: number, nodes: number, cache_hits: number, kernel_calls: number, steps: number, 
/**
 * Language evaluation steps.
 */
eval_steps: number, };
