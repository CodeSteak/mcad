// Generated from crates/cad-session/src/protocol.rs (ts-rs). Do not edit.
// Regenerate: UPDATE_PROTOCOL_TS=1 cargo test -p cad-session --test protocol_ts

export const PROTOCOL_VERSION = 3;

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
 * `end_line`/`end_col`: 1-based position of `to`. Columns count UTF-16 units. All are local
 * to the range's file: `file` = id in `Scene.files`, omitted for the main file.
 */
export type SrcRange = { from: number, to: number, line: number, col: number, end_line: number, end_col: number, file?: number, };

/**
 * A language value: `null` = undef, numbers, bools, strings, lists, ranges; grids,
 * instances and functions as `{ opaque: "<type>" }`.
 */
export type Value = null | boolean | number | string | Array<Value> | { start: number, step: number, end: number, } | { opaque: string, };

export type FileRole = "main" | "include" | "use";

/**
 * A file of the evaluated project (`SrcRange.file` indexes this list). A file included twice
 * appears twice (same path).
 */
export type FileInfo = { id: number, path: string, role: FileRole, };

/**
 * An `include <…>`/`use <…>` statement: `range` covers `<path>` (Ctrl+click opens `target`).
 */
export type IncludeRef = { range: SrcRange, 
/**
 * Resolved project path.
 */
target: string, is_use: boolean, 
/**
 * The file was found.
 */
found: boolean, };

/**
 * A part (Plan 12.9): named instance at the top level of the main file.
 */
export type Part = { 
/**
 * `deckel`, `k[2]`: export name.
 */
name: string, 
/**
 * `ModuleCall` node.
 */
node: number, inst: number, range: SrcRange, 
/**
 * Bounds in context coordinates (complete scene).
 */
bounds: Bounds | null, 
/**
 * Volume in mm³ (complete scene, 3D parts).
 */
volume: number | null, };

/**
 * Message from the UI to the engine.
 */
export type ToEngine = { "type": "SetSource", text: string, version: number, } | { "type": "SetFiles", files: { [key in string]: string }, main?: string, } | { "type": "SetFile", path: string, text?: string, } | { "type": "SetContext", module?: string, overrides: { [key in string]: Value }, } | { "type": "RequestMeshes", nodes: Array<number>, coarse?: boolean, version?: number, } | { "type": "DragStart", handle: number, } | { "type": "Drag", handle: number, target: DragTarget, } | { "type": "DragEnd", handle: number, target?: DragTarget, } | { "type": "SnapTargets", handle: number, } | { "type": "SetDragOptions", step_mm: number, step_deg: number, } | { "type": "SetLang", lang: string, } | { "type": "SetDefault", module: string, param: string, value: Value, } | { "type": "Export", format: ExportFormat, parts?: Array<string>, zip?: boolean, colors?: boolean, } | { "type": "RequestColorRegions", meshes: boolean, coarse?: boolean, version?: number, } | { "type": "SetFilament", color: string, filament?: number, } | { "type": "SetRender", changes: Array<RenderChange>, } | { "type": "Cancel" } | { "type": "BlockKeys", keys: Array<string>, } | { "type": "Sketch" } & SketchRequest | { "type": "Measure" } & MeasureRequest;

/**
 * Tools of the sketch editor.
 */
export type SketchTool = "select" | "line" | "arc_through" | "arc_center" | "arc_tangent" | "bezier" | "rect" | "circle" | "corner" | "close" | "delete";

/**
 * Grid point of a picked point (`z` omitted: implicit plane, written `g[x, y]`).
 */
export type SketchGridSel = { grid: number, x: string, y: string, z?: string, };

/**
 * A point a tool has picked: position in the sketch plane, the expression written for it and
 * what it snapped to (`grid`, `anchor`, `start`, `end`, `center`, `intersection`, `mid`,
 * `vertex`, `tangent`, `horizontal`, `vertical`, `grid_line`, `step`).
 */
export type SketchPoint = { p: [number, number], text: string, kind: string, grid?: SketchGridSel, };

/**
 * A ray in context coordinates.
 */
export type Ray = { origin: Vec3, dir: Vec3, };

export type SketchRequest = { 
/**
 * Echoed in the answer.
 */
id: number, 
/**
 * Editor cursor in the main file (UTF-16): picks the sketch and where new code goes.
 */
cursor: number, tool: SketchTool, 
/**
 * Points picked so far (from the previous answer's `points`).
 */
points: Array<SketchPoint>, 
/**
 * Mouse ray; without it only plane, prompt (and with `info` the raster) are answered.
 */
ray?: Ray, 
/**
 * Size of a pixel at distance 1 from the camera (2·tan(fov/2)/height).
 */
pixel: number, 
/**
 * Snap radius in pixels.
 */
snap_px: number, 
/**
 * Alt: no snapping, numbers rounded to the step.
 */
free: boolean, 
/**
 * Mouse click: adds the point; a complete action answers `edits`.
 */
click: boolean, 
/**
 * Arc around a centre: sweep of the previous answer (degrees), so the arc can go on
 * beyond 180°.
 */
angle?: number, 
/**
 * Also answer the raster.
 */
info: boolean, };

/**
 * Where the mouse snapped (context coordinates; `local` in the sketch plane).
 */
export type SketchSnap = { point: Vec3, local: [number, number], 
/**
 * Expression that would be written (`g[a, b]`, `k.ecke`, `[12, 5]`).
 */
text: string, kind: string, 
/**
 * Display text.
 */
label: string, };

/**
 * Drawing raster in the sketch plane (context coordinates): lines of the visible grids, grid
 * points, and the extent (sketch plane) for the step raster.
 */
export type SketchRaster = { lines: Array<[Vec3, Vec3]>, points: Array<Vec3>, min: [number, number], max: [number, number], };

export type SketchAnswer = { id: number, 
/**
 * Source version the answer (and its edits) belong to.
 */
version: number, 
/**
 * The sketch statement being edited (`null`: none yet, the first action creates one).
 */
sketch: SrcRange | null, 
/**
 * Its node.
 */
node: number | null, 
/**
 * Sketch plane → context coordinates (camera, drawing).
 */
plane: Mat4, 
/**
 * End of the open contour at the insertion point (context coordinates).
 */
current: Vec3 | null, snap: SketchSnap | null, 
/**
 * Help lines of the snap (horizontal/vertical, tangent).
 */
guides: Array<Array<Vec3>>, 
/**
 * What the tool would draw (context coordinates).
 */
preview: Array<Array<Vec3>>, 
/**
 * Tool state after this message: the points picked so far.
 */
points: Array<SketchPoint>, 
/**
 * Code of a completed action (one undo step).
 */
edits: Array<Edit>, 
/**
 * Cursor after applying `edits`: index of the edit and UTF-16 offset in its text.
 */
select: [number, number] | null, 
/**
 * What the tool asks for next.
 */
prompt: string, 
/**
 * Why an action did not apply.
 */
message: string | null, 
/**
 * Select tool: the segment statement under the mouse.
 */
segment: SrcRange | null, 
/**
 * Arc around a centre: current sweep in degrees (send back as `angle`).
 */
angle: number | null, raster: SketchRaster | null, };

/**
 * What to measure (Plan 18).
 */
export type MeasureKind = "distance" | "radius" | "angle" | "length" | "area" | "body" | "wall";

/**
 * Section plane: through `point`, `normal` (unit) points into the removed half (convention
 * of `mcad render --section`).
 */
export type SectionPlaneRef = { point: Vec3, normal: Vec3, };

/**
 * The face under the mouse (UI raycast on the displayed mesh).
 */
export type MeasureHit = { 
/**
 * Node whose mesh was hit.
 */
node: number, 
/**
 * `MeshData.tri_faces` of the hit triangle.
 */
face: number, 
/**
 * Hit point, context coordinates.
 */
point: Vec3, };

/**
 * A picked element. `kind`: `vertex`, `mid` (edge midpoint), `center` (circle centre),
 * `edge`, `face`, `body`; in a section `section_vertex`, `section_mid`, `section_center`,
 * `section_edge`, `region`; `point` (a free point: anchor, grid point, coordinate).
 */
export type MeasurePick = { kind: string, 
/**
 * Node the element belongs to.
 */
node: number, 
/**
 * Body (shape) of the node.
 */
body: number, 
/**
 * B-rep edge or face of the body; in a section: contour and segment, or region.
 */
edge?: number, face?: number, contour?: number, segment?: number, region?: number, 
/**
 * Snapped point (context coordinates).
 */
point: Vec3, 
/**
 * Display text (`Kreismitte · Kreis R 3`).
 */
label: string, 
/**
 * Reference for `mcad measure` (`center@[10, 10, 6]`, `face@[…]`, `k.ecke`).
 */
reference: string, 
/**
 * Provenance: code lines, named instances, grid planes/points and anchors the element
 * lies on.
 */
origin: Array<string>, 
/**
 * Highlight while hovering (polylines, context coordinates).
 */
lines: Array<Array<Vec3>>, };

export type MeasureRequest = { 
/**
 * Echoed in the answer.
 */
id: number, kind: MeasureKind, 
/**
 * Elements picked so far (from the previous answer's `picks`).
 */
picks: Array<MeasurePick>, 
/**
 * Mouse ray; without it only the prompt (and `section` info) is answered.
 */
ray?: Ray, 
/**
 * Face under the mouse (raycast of the UI).
 */
hit?: MeasureHit, 
/**
 * Size of a pixel at distance 1 from the camera (2·tan(fov/2)/height).
 */
pixel: number, 
/**
 * Snap radius in pixels.
 */
snap_px: number, 
/**
 * Alt: no snapping to points, the exact point on the face/edge.
 */
free: boolean, 
/**
 * Mouse click: adds the pick; a complete measurement answers `result`.
 */
click: boolean, 
/**
 * Measure in this section plane (contours of plane ∩ B-rep).
 */
section?: SectionPlaneRef, 
/**
 * The section view is on (elements in the removed half are not offered).
 */
clip?: SectionPlaneRef, 
/**
 * Node to measure when nothing is hit (default: the root).
 */
node?: number, };

/**
 * One value of a measurement.
 */
export type MeasureValue = { 
/**
 * `distance`, `dx`, `radius`, `diameter`, `angle`, `length`, `area`, `volume`, …
 */
key: string, label: string, value: number, 
/**
 * `mm`, `mm²`, `mm³`, `°`.
 */
unit: string, };

/**
 * A completed measurement (UI: drawn in the view and listed; CLI: `measurements`).
 */
export type Measurement = { kind: MeasureKind, 
/**
 * `Abstand`, `Radius`, `Winkel`, `Länge`, `Fläche`, `Körper`, `Wandstärke`.
 */
title: string, 
/**
 * The main value (`values[0]`).
 */
value: number, unit: string, 
/**
 * Short text in the view (`12.5 mm`, `R 3 · ⌀ 6`, `45°`).
 */
label: string, values: Array<MeasureValue>, 
/**
 * Dimension lines (context coordinates).
 */
lines: Array<Array<Vec3>>, 
/**
 * Help lines (axes, extension lines), drawn dashed.
 */
guides: Array<Array<Vec3>>, 
/**
 * Markers (foot points, centres).
 */
points: Array<Vec3>, 
/**
 * Where the label goes.
 */
at: Vec3, 
/**
 * Further lines of text (foot points, Δ, axis, provenance).
 */
details: Array<string>, picks: Array<MeasurePick>, 
/**
 * From the exact geometry in closed form; else `tol` is the accuracy.
 */
exact: boolean, tol?: number, 
/**
 * Measured in this section plane.
 */
section?: SectionPlaneRef, 
/**
 * The measurement as text (copy, code comment).
 */
text: string, 
/**
 * Arguments of `mcad measure` after the file name that repeat it.
 */
cli: Array<string>, };

/**
 * The section measured in (answer to requests with `section`).
 */
export type MeasureSectionInfo = { plane: SectionPlaneRef, regions: number, contours: number, 
/**
 * mm²
 */
area: number, 
/**
 * All contour segments in closed form (lines, circles, ellipses from analytic surfaces).
 */
exact: boolean, 
/**
 * Largest deviation of the kernel's intersection curves from the closed forms (mm).
 */
max_dev: number, 
/**
 * Why there is no section (kernel boolean failed, plane misses the model).
 */
message?: string, };

export type MeasureAnswer = { id: number, 
/**
 * Scene version the picks belong to.
 */
version: number, 
/**
 * Element under the mouse.
 */
hover: MeasurePick | null, 
/**
 * Picks after this message.
 */
picks: Array<MeasurePick>, 
/**
 * Completed measurement (after a click).
 */
result: Measurement | null, 
/**
 * What to pick next.
 */
prompt: string, 
/**
 * Why a pick was not accepted.
 */
message: string | null, section: MeasureSectionInfo | null, };

/**
 * Where a dragged handle goes.
 */
export type DragTarget = { "kind": "point", pos: Vec3, } | { "kind": "value", value: number, } | { "kind": "snap", snap: SnapRef, };

export type SnapRef = { "kind": "grid", grid: number, planes: [string, string, string], } | { "kind": "anchor", inst: number, name: string, } | { "kind": "plane", grid: number, plane: string, };

export type ExportFormat = "stl" | "3mf" | "step";

/**
 * Message from the engine to the UI.
 */
export type FromEngine = { "type": "Diagnostics", version: number, items: Array<Diagnostic>, } | { "type": "Scene" } & Scene | { "type": "Progress", version: number, done: number, total: number, current: NodeRef | null, } | { "type": "Meshes", version: number, items: Array<MeshData>, } | { "type": "Edits", version: number, edits: Array<Edit>, } | { "type": "SnapTargets", version: number, handle: number, targets: Array<SnapTarget>, } | { "type": "ExportResult", format: ExportFormat, bytes: Uint8Array, triangles: number, 
/**
 * Parts exported (`null`: the root).
 */
parts?: Array<string>, 
/**
 * A ZIP package with one file per part.
 */
zip?: boolean, 
/**
 * Multi-colour export (3MF with volumes per colour, ZIP with one STL per colour).
 */
colors?: boolean, 
/**
 * Notes for the user (multi-colour 3MF: objects, material outside parts, slicers).
 */
notes?: Array<string>, } | { "type": "ColorRegions", version: number, items: Array<RegionSet>, } | { "type": "Error", message: string, version: number | null, node: number | null, } | { "type": "Sketch" } & SketchAnswer | { "type": "Measure" } & MeasureAnswer | { "type": "Asserts", version: number, items: Array<AssertResult>, };

export type Severity = "error" | "warning" | "info";

export type DiagSource = "syntax" | "eval" | "kernel";

export type Diagnostic = { severity: Severity, message: string, range: SrcRange, source: DiagSource, 
/**
 * Node of a kernel diagnostic.
 */
node: number | null, 
/**
 * Path of the file the range is in (omitted: the main file).
 */
file?: string, 
/**
 * Where the problem is (context coordinates), e.g. where the operands of a Boolean touch
 * tangentially; the UI frames it on a click, `mcad render` marks it.
 */
at?: Vec3, 
/**
 * Nodes the message names (the touching operands).
 */
nodes?: Array<number>, };

export type Edit = { 
/**
 * UTF-16 offsets.
 */
from: number, to: number, text: string, 
/**
 * Path of the file to change (omitted: the main file).
 */
file?: string, };

export type NodeRef = { node: number, stmt: number | null, range: SrcRange, 
/**
 * Cache key of the node (hex; in `Progress.current`): what `BlockKeys` takes.
 */
key?: string, };

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

/**
 * Colour regions of one node (a part or the root), in the node's system (place meshes with
 * `SceneNode.display_world`).
 */
export type RegionSet = { node: number, 
/**
 * Part name (`null`: the root); `"no part"` for the material outside the parts
 * (`cad_session::NO_PART`, fixed in every language).
 */
part: string | null, 
/**
 * Volume of the node's result in mm³ (= sum of the region volumes).
 */
total: number, regions: Array<Region>, 
/**
 * The region volumes do not add up to `total` (kernel boolean incomplete).
 */
warning: string | null, };

export type Region = { 
/**
 * `ColorInfo.key`.
 */
color: string, filament: number, 
/**
 * mm³
 */
volume: number, 
/**
 * Requested with `meshes`.
 */
mesh: MeshData | null, };

/**
 * How a colour was given.
 */
export type ColorSource = "default" | "palette" | "random" | "exact";

/**
 * A colour of the material with its filament slot (Plan 16.3). Order: first appearance.
 */
export type ColorInfo = { 
/**
 * Identity, `#rrggbb` or `#rrggbbaa` (what `SetFilament` takes).
 */
key: string, 
/**
 * Exact value (exports).
 */
rgba: Rgba, 
/**
 * Palette name, the unknown name as written, hex text, or `default` (material without
 * `color()`; a fixed value, the UI shows it translated).
 */
label: string, source: ColorSource, 
/**
 * Filament slot (1-based).
 */
filament: number, 
/**
 * The slot is written in the code (`filament = n`).
 */
explicit: boolean, 
/**
 * The `color()` statements of this colour (distinct, in order of appearance).
 */
ranges: Array<SrcRange>, 
/**
 * `color()` nodes.
 */
nodes: Array<number>, };

/**
 * Kind of a geometric assertion.
 */
export type AssertKind = "keepout" | "disjoint" | "intersect" | "touches";

/**
 * One checked statement instance of `assert_keepout`, `assert_disjoint`, `assert_intersect`
 * or `assert_touches`.
 */
export type AssertResult = { 
/**
 * The assertion's node (a group of the checked children).
 */
node: number, kind: AssertKind, range: SrcRange, 
/**
 * No violation (also `true` while unchecked).
 */
ok: boolean, 
/**
 * The check ran (`false`: skipped while dragging, or the node was not evaluated).
 */
checked: boolean, 
/**
 * Why nothing was checked, or a remark (`Netz: nur ja/nein`).
 */
note: string | null, 
/**
 * `assert_keepout(clearance)` in mm.
 */
clearance: number, 
/**
 * `assert_touches(tol)` in mm.
 */
tol: number, 
/**
 * `assert_touches(contact)`: `face` or `any`.
 */
contact: string, 
/**
 * Volume below which an intersection counts as none (mm³, relative to the size).
 */
eps_volume: number, 
/**
 * Bodies checked: real bodies of the model (keepout) or children.
 */
bodies: number, 
/**
 * Pairs: keepout only those with a violation, the others every pair of children.
 */
pairs: Array<AssertPair>, 
/**
 * Time of the check in ms (0 when every pair came from the cache).
 */
ms: number, 
/**
 * Every pair came from the engine cache.
 */
cached: boolean, };

/**
 * A body of an assertion pair: a child of the assertion or a real body of the model.
 */
export type AssertBody = { 
/**
 * Node whose result holds the body.
 */
node: number, 
/**
 * `deckel`, `line 7: cube`, `keep-out volume` (in the engine language, `SetLang`).
 */
label: string, 
/**
 * Code lines and instances of the faces involved (like `mcad measure`).
 */
origin: Array<string>, };

/**
 * Check of two bodies.
 */
export type AssertPair = { a: AssertBody, b: AssertBody, ok: boolean, 
/**
 * `overlap`, `too_close`, `apart`, `gap`, `line_contact`, `point_contact`,
 * `unchecked` (`null` when ok).
 */
problem: string | null, 
/**
 * Text of the result (engine language), as in the diagnostic.
 */
message: string, 
/**
 * Intersection volume in mm³ (`null`: not computed or only known as yes/no for meshes).
 */
volume: number | null, 
/**
 * Intersection decided on meshes (yes/no) instead of an exact boolean.
 */
mesh_based: boolean, 
/**
 * Smallest distance of the tessellations in mm (0 when they touch or overlap).
 */
distance: number | null, 
/**
 * Accuracy of `distance` (± mm, sum of the chordal tolerances; 0 for planar faces).
 */
accuracy: number, 
/**
 * Foot points of `distance` (a, b), context coordinates.
 */
points: Array<Vec3>, 
/**
 * `assert_touches`: `face`, `line`, `point`, `mesh` (freeform, mesh-based) or `null`.
 */
contact: string | null, 
/**
 * Contact area in mm² (`face`).
 */
contact_area: number | null, 
/**
 * Contact area exact (planar faces, `region2`) or estimated on the mesh.
 */
contact_exact: boolean, 
/**
 * Intersection body (red), context coordinates.
 */
overlap: AssertMesh | null, 
/**
 * Contact faces (green), context coordinates.
 */
contact_mesh: AssertMesh | null, 
/**
 * Contact lines and outlines of contact faces, context coordinates.
 */
contact_lines: Array<Array<Vec3>>, 
/**
 * Where to look: bounds of the intersection, the contact or the gap.
 */
focus: Bounds | null, };

/**
 * Triangles in context coordinates.
 */
export type AssertMesh = { positions: Array<number>, indices: Array<number>, 
/**
 * Feature edges as segment pairs (xyz xyz), for the outline.
 */
edges: Array<number>, };

export type Scene = { version: number, 
/**
 * Files of the project, index = id (`SrcRange.file`); file 0 is the main file.
 */
files: Array<FileInfo>, 
/**
 * `include`/`use` statements of all files.
 */
includes: Array<IncludeRef>, 
/**
 * Parts (file context only).
 */
parts: Array<Part>, 
/**
 * Colours of the material and their filament slots (volumes: `RequestColorRegions`).
 */
colors: Array<ColorInfo>, 
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
instances: Array<Instance>, 
/**
 * Editable handles of the context module.
 */
handles: Array<Handle>, 
/**
 * Handles of computed values (`locked = true`): shown, not editable.
 */
locked_handles: Array<Handle>, 
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
aborted: boolean, stats: Stats, 
/**
 * Results of the geometric assertions (empty until checked, see `FromEngine.Asserts`).
 */
asserts: Array<AssertResult>, 
/**
 * The main file's `render { … }` block (studio look of the file; `None`: none).
 */
render?: RenderBlock, };

/**
 * `render { … }` of the main file: the values given (`None`: not set, the default applies).
 */
export type RenderBlock = { range: SrcRange, 
/**
 * `studio` or `standard`.
 */
style: string | null, edges: boolean | null, 
/**
 * `studio`, `overcast` or `none`.
 */
environment: string | null, ambient: number | null, light_intensity: number | null, shadows: boolean | null, shadow_softness: number | null, ao: boolean | null, ao_radius: number | null, ao_strength: number | null, ground: boolean | null, 
/**
 * `#rrggbb`.
 */
ground_color: string | null, 
/**
 * Mirror image of the model in the ground, 0..1.
 */
ground_reflection: number | null, 
/**
 * Blur of the ground reflection, 0..1.
 */
ground_roughness: number | null, 
/**
 * Ray-traced reflections of the model in itself (CLI renderer).
 */
reflections: boolean | null, 
/**
 * One or two (top, bottom) `#rrggbb`.
 */
background: Array<string> | null, lights: Array<RenderLight>, materials: Array<RenderMaterial>, };

/**
 * One change of the render block (`SetRender`); `value: null` removes the setting.
 */
export type RenderChange = { "kind": "set", key: string, value: Value, } | { "kind": "light", name: string, key: string, value: Value, } | { "kind": "remove_light", name: string, } | { "kind": "material", target: RenderTarget, key: string, value: Value, } | { "kind": "remove_material", target: RenderTarget, };

export type RenderLight = { name: string, azimuth: number, elevation: number, intensity: number, 
/**
 * `#rrggbb`.
 */
color: string, range: SrcRange, };

/**
 * What a material entry applies to: a colour as written (`red`, `#rrggbb`, `default` =
 * uncoloured material) or a part.
 */
export type RenderTarget = { color: string, } | { part: string, };

export type RenderMaterial = { target: RenderTarget, preset: string | null, metalness: number | null, roughness: number | null, clearcoat: number | null, anisotropy: number | null, range: SrcRange, };

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
zone: boolean, 
/**
 * `assert_keepout`: keep-out volume, not in the result (also `background`), shown
 * transparent with its outline in its own colour.
 */
keepout: boolean, };

/**
 * Edge/face selection of a zone block (`select`).
 */
export type SelectMode = "inside" | "touch";

/**
 * Resolved parameters of operations the view draws specially (zone blocks, profile
 * operations, surfaces). Points are in the node system.
 */
export type OpInfo = { "kind": "blend", chamfer: boolean, r: number, select: SelectMode, } | { "kind": "shell", t: number, select: SelectMode, } | { "kind": "zone", line: boolean, point: Vec3, dir: Vec3, d: number, } | { "kind": "linear_extrude", h: number, center: boolean, twist: number, scale: [number, number], } | { "kind": "rotate_extrude", angle: number, } | { "kind": "loft", ruled: boolean, } | { "kind": "sweep", path: Array<Vec3>, spline: boolean, } | { "kind": "surface", rows: Array<Array<Vec3>>, } | { "kind": "thicken", t: number, } | { "kind": "blend2d", chamfer: boolean, r: number, select: SelectMode, } | { "kind": "offset", round: boolean, d: number, chamfer: boolean, } | { "kind": "stroke", w: number, cap: string, join: string, } | { "kind": "sketch", closed: boolean, contours: Array<SketchContour>, 
/**
 * The sketch plane → context coordinates (like `display_world`, but for the profile of
 * a `rotate_extrude` the xz-plane and for a centred `linear_extrude` its bottom: where
 * the profile lies in the body; Plan 17.12).
 */
plane: Mat4, };

/**
 * One contour of a sketch (node system, z = 0).
 */
export type SketchContour = { 
/**
 * In a `hole { … }` block.
 */
hole: boolean, closed: boolean, 
/**
 * Start point (`start(p)`).
 */
start: Vec3, 
/**
 * The `start(…)` statement.
 */
start_range: SrcRange | null, segments: Array<SketchSegment>, };

export type SketchSegment = { 
/**
 * `line`, `arc`, `bezier`, `spline`.
 */
kind: string, 
/**
 * Statement that made the segment.
 */
stmt: number, range: SrcRange, 
/**
 * Polyline from start to end (display tolerance).
 */
points: Array<Vec3>, 
/**
 * Help lines: control polygon of a Bézier curve (start, control points, end); centre of an
 * arc (one point).
 */
ctrl: Array<Vec3>, 
/**
 * Closing line added for a contour without `close()` (error).
 */
implicit: boolean, 
/**
 * An error was reported for this segment.
 */
error: boolean, };

export type GuideLine = { kind: string, points: Array<Vec3>, };

export type Bounds = { min: Vec3, max: Vec3, };

export type NodeMessage = { severity: Severity, message: string, 
/**
 * Affected edges (polylines, node system), e.g. the edge a fillet failed on, the edges
 * along which the operands of a Boolean touch tangentially.
 */
edges: Array<Array<Vec3>>, 
/**
 * Place of the problem (node system), e.g. where the operands touch.
 */
at?: Vec3, };

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
 * Inline hints of the node, e.g. "4 edges".
 */
hints: Array<string>, 
/**
 * Edges selected by fillet/chamfer (polylines, node system); the new corner pieces of
 * `fillet2d`/`chamfer2d`.
 */
selected_edges: Array<Array<Vec3>>, 
/**
 * Guide lines of 2D operations (node system): `source` (input contour of `offset`),
 * `result` (offset contour, drawn dashed), `centerline` (path of `stroke`).
 */
guides: Array<GuideLine>, opened_faces: number, 
/**
 * Draft mode while dragging: this node's blend was left out (it took longer than the
 * draft limit); the full evaluation after the drag restores it.
 */
blend_suspended: boolean, 
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
tol: number | null, 
/**
 * Parameters of zone blocks, profile operations and surfaces (special display).
 */
op_info: OpInfo | null, };

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
 * Computed value: shown, not editable (no literal, no reference).
 */
locked: boolean, 
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
eval_steps: number, 
/**
 * Blends left out in draft mode (while dragging).
 */
suspended_blends: number, };
