//! Geometry tree produced by the evaluator and consumed by the engine.

use cad_kernel::{BoolOp, PathInterp};
use cad_syntax::Span;
use cad_syntax::ast::StmtId;
use glam::{DMat4, DVec2, DVec3};
use serde::{Deserialize, Serialize};

pub mod law;
pub use law::{Law, LawExpr, LawFn, LawOp, LawVal};

pub type NodeIdx = u32;

/// `Node::stmt` of nodes without a statement (the synthetic root of a file context).
pub const NO_STMT: StmtId = u32::MAX;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: Option<NodeIdx>,
    /// `filament(c, n)` statements at the top level of the main file (Bericht 12, F2): the
    /// slot of a colour for the whole model, before any `color(…, filament = n)`.
    #[serde(default)]
    pub filaments: Vec<FilamentDecl>,
    /// `assert_keepout`, `assert_disjoint`, `assert_intersect`, `assert_touches` (Plan 21 C):
    /// the checks the session runs after the kernel job, one per evaluated statement instance.
    #[serde(default)]
    pub asserts: Vec<AssertDecl>,
    /// The `render { … }` block at the top level of the main file (studio look of the file).
    #[serde(default)]
    pub render: Option<RenderDecl>,
}

/// `render { … }`, evaluated: the values given (`None`: not set, the default applies).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RenderDecl {
    pub stmt: StmtId,
    pub span: Span,
    /// `"studio"` or `"standard"`.
    pub style: Option<String>,
    pub edges: Option<bool>,
    /// `"studio"`, `"overcast"` or `"none"`.
    pub environment: Option<String>,
    pub ambient: Option<f64>,
    pub light_intensity: Option<f64>,
    pub shadows: Option<bool>,
    pub shadow_softness: Option<f64>,
    pub ao: Option<bool>,
    pub ao_radius: Option<f64>,
    pub ao_strength: Option<f64>,
    pub ground: Option<bool>,
    pub ground_color: Option<[f32; 4]>,
    /// Mirror image in the ground, 0..1.
    #[serde(default)]
    pub ground_reflection: Option<f64>,
    /// Blur of the ground reflection, 0..1.
    #[serde(default)]
    pub ground_roughness: Option<f64>,
    /// Ray-traced reflections of the model (CLI renderer).
    #[serde(default)]
    pub reflections: Option<bool>,
    /// One colour, or two for a vertical gradient (top, bottom).
    pub background: Option<Vec<[f32; 4]>>,
    /// `light NAME { … }` in order (missing values: the defaults of the key light).
    pub lights: Vec<RenderLight>,
    pub materials: Vec<RenderMaterial>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLight {
    pub name: String,
    pub azimuth: f64,
    pub elevation: f64,
    pub intensity: f64,
    pub color: [f32; 4],
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RenderTarget {
    /// The colour as written (`red`, `#rrggbb`, `default`) and its RGBA (`None` for
    /// `default`, the uncoloured material).
    Color {
        label: String,
        rgba: Option<[f32; 4]>,
    },
    Part(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderMaterial {
    pub target: RenderTarget,
    /// Preset name (checked).
    pub preset: Option<String>,
    pub metalness: Option<f64>,
    pub roughness: Option<f64>,
    pub clearcoat: Option<f64>,
    #[serde(default)]
    pub anisotropy: Option<f64>,
    pub span: Span,
}

/// A geometric assertion. Its node is a group of the checked children; the node of
/// `assert_keepout` also has [`Flags::keepout`] (and `background`: not part of the result).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertDecl {
    pub node: NodeIdx,
    pub kind: AssertKind,
    /// `assert_keepout(clearance)`: required distance of every real body in mm (0: may touch).
    pub clearance: f64,
    /// `assert_touches(tol)`: largest gap that still counts as contact in mm.
    pub tol: f64,
    /// `assert_touches(contact)`.
    pub contact: Contact,
    pub stmt: StmtId,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssertKind {
    Keepout,
    Disjoint,
    Intersect,
    Touches,
}

impl AssertKind {
    /// Name of the module (`assert_keepout`, …).
    pub fn name(self) -> &'static str {
        match self {
            AssertKind::Keepout => "assert_keepout",
            AssertKind::Disjoint => "assert_disjoint",
            AssertKind::Intersect => "assert_intersect",
            AssertKind::Touches => "assert_touches",
        }
    }
}

/// Kind of contact `assert_touches` requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Contact {
    /// Contact over an area (default): line and point contact are warned about.
    Face,
    /// Any contact.
    Any,
}

/// `filament("red", 4);`: colour `rgba` prints with slot `slot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentDecl {
    pub rgba: [f32; 4],
    /// Palette name or the unknown name as written (`None` for hex and vectors).
    pub name: Option<String>,
    pub slot: u32,
    /// The statement.
    pub stmt: StmtId,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Flags {
    /// `#`
    pub highlight: bool,
    /// `%`: not part of the result, shown grey.
    pub background: bool,
    /// `*`: not part of the result, not shown.
    pub disabled: bool,
    /// Zone of a fillet/chamfer/shell block: not part of the result.
    pub zone: bool,
    /// `assert_keepout`: keep-out volume, not part of the result (set together with
    /// `background`), shown as such.
    #[serde(default)]
    pub keepout: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Statement that produced the node (editor line).
    pub stmt: StmtId,
    pub span: Span,
    pub op: Op,
    pub children: Vec<NodeIdx>,
    pub parent: Option<NodeIdx>,
    /// Local system of this node relative to its parent's local system.
    pub local: DMat4,
    /// Local system relative to the tree root (the context module).
    pub world: DMat4,
    pub flags: Flags,
    /// Index of the module instance this node belongs to (0 = context root).
    pub module_inst: u32,
    /// `$fillet` / `$chamfer` in effect at a primitive.
    pub blend: Option<(BlendKind, f64)>,
    /// `$tol` in effect (display tessellation tolerance in mm).
    pub tol: Option<f64>,
    /// Display colour from `color()`.
    pub color: Option<[f32; 4]>,
    /// Stable identity across evaluations: hash of the statement and instance path
    /// (call sites and loop indices). Below 2^53 (exact in JavaScript numbers).
    pub inst_key: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dim {
    Two,
    Three,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    /// Implicit union (block, `for`, `if`, module call body, `children()`).
    Group,
    /// Module instantiation; children are the body's results.
    ModuleCall {
        name: String,
        instance: Option<String>,
    },
    /// Primitive, placed by `place` within the node's system.
    Prim {
        prim: Prim,
        place: DMat4,
    },
    Profile(Profile),
    /// Transform applied to the children (the matrix is also in the children's `local`).
    Transform {
        kind: TransformKind,
        m: DMat4,
    },
    /// `fillet_law`: `fillet = function (p) …` (variable radius; `fillet` is then `None`).
    Boolean {
        op: BoolOp,
        fillet: Option<f64>,
        chamfer: Option<f64>,
        fillet_law: Option<Box<Law>>,
    },
    /// `fillet(r) { body; zone; … }` / `chamfer`. Child 0 is the body, others zones.
    /// `law`: `fillet(r = function (p) …)`, a variable radius (`r` is then 0).
    Blend {
        kind: BlendKind,
        r: f64,
        select: Select,
        law: Option<Box<Law>>,
    },
    /// `fillet2d(r) { profile; zone; … }` / `chamfer2d(d)` (Plan 17.3): rounds (cuts) the
    /// corners of a 2D profile, convex and concave; child 0 is the profile, the others are 2D
    /// zones selecting corners.
    Blend2 {
        kind: BlendKind,
        r: f64,
        select: Select,
    },
    /// `offset(r = …)` (`round`) / `offset(delta = …, chamfer)` of the 2D children
    /// (OpenSCAD-compatible).
    Offset {
        round: bool,
        d: f64,
        chamfer: bool,
    },
    /// `stroke(w, cap, join)`: the area a pen of width `w` covers along the paths of the
    /// children (open sketches; boundaries of 2D profiles).
    Stroke {
        w: f64,
        cap: StrokeCap,
        join: StrokeJoin,
    },
    /// `shell(t) { body; zone; … }`.
    Shell {
        t: f64,
        select: Select,
    },
    /// Explicit zone around a grid line or plane, in the node's local system.
    Zone(ZoneShape),
    LinearExtrude {
        h: f64,
        twist: f64,
        scale: DVec2,
        center: bool,
    },
    RotateExtrude {
        angle: f64,
    },
    Loft {
        ruled: bool,
    },
    Sweep {
        path: Vec<DVec3>,
        interp: PathInterp,
    },
    Surface {
        rows: Vec<Vec<DVec3>>,
    },
    Thicken {
        t: f64,
    },
    /// `resize`: scale children so their bounds get the given size (0 = keep / auto).
    Resize {
        size: DVec3,
        auto: [bool; 3],
    },
    /// `color(c, alpha, filament)`. `name`: palette name or the unknown name as written
    /// (`None` for hex and vectors). Only `rgba` matters for geometry; the engine's cache key
    /// of this node ignores all three (a new colour or slot recomputes nothing).
    Color {
        rgba: [f32; 4],
        #[serde(default)]
        name: Option<String>,
        /// Filament slot (1-based) from `filament = n`.
        #[serde(default)]
        filament: Option<u32>,
    },
    /// `hull() { … }`: convex hull of the children (2D or 3D, like the children). The engine
    /// builds the exact special cases; otherwise it warns and shows the children unchanged.
    Hull,
    /// `minkowski() { … }`: Minkowski sum of the children (exact special cases like `Hull`).
    Minkowski,
    /// Unsupported or failed statement that still has children to show.
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformKind {
    Translate,
    Rotate,
    Scale,
    Mirror,
    Matrix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendKind {
    Fillet,
    Chamfer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeCap {
    Round,
    Flat,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeJoin {
    Round,
    Miter,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Select {
    #[default]
    Inside,
    Touch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Prim {
    Box {
        min: DVec3,
        max: DVec3,
    },
    Sphere {
        r: f64,
    },
    Cylinder {
        h: f64,
        r1: f64,
        r2: f64,
    },
    Torus {
        big_r: f64,
        r: f64,
    },
    Polyhedron {
        points: Vec<DVec3>,
        faces: Vec<Vec<u32>>,
    },
    /// Imported triangle mesh (`import("x.stl")`): with `mode` [`MeshMode::Mesh`] a body
    /// without B-rep, otherwise a polyhedron of its planar faces (a mesh again if the kernel
    /// fails).
    Mesh {
        mesh: MeshRef,
        mode: MeshMode,
    },
}

/// How an imported mesh enters the model (`import(…, as = "solid" | "mesh")`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MeshMode {
    /// B-rep, chosen automatically (closed, at most `cad_stl::AUTO_SOLID_FACES` faces).
    AutoSolid,
    /// B-rep, `as = "solid"`.
    Solid,
    /// Mesh without B-rep (`as = "mesh"` or automatically).
    Mesh,
}

/// Shared mesh data of an import with the hash that stands for it: equality, `Debug` and
/// serialisation (the engine's cache keys) use only the hash of the file content and the
/// import options. Not deserialisable.
#[derive(Clone)]
pub struct MeshRef {
    pub hash: [u8; 32],
    pub mesh: std::sync::Arc<cad_kernel::trimesh::TriMesh>,
}

impl PartialEq for MeshRef {
    fn eq(&self, o: &Self) -> bool {
        self.hash == o.hash
    }
}

impl std::fmt::Debug for MeshRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let h: String = self.hash[..6].iter().map(|b| format!("{b:02x}")).collect();
        write!(f, "MeshRef({h}…, {} Dreiecke, {} Flächen)", self.mesh.triangle_count(), self.mesh.face_count())
    }
}

impl Serialize for MeshRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.hash)
    }
}

impl<'de> Deserialize<'de> for MeshRef {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("imported meshes cannot be deserialised"))
    }
}

/// 2D shapes in the xy-plane of the node's local system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Profile {
    Rect {
        min: DVec2,
        max: DVec2,
    },
    Circle {
        r: f64,
    },
    /// Outer and hole loops (OpenSCAD `polygon(points, paths)`).
    Polygon {
        loops: Vec<Vec<DVec2>>,
    },
    Spline {
        points: Vec<DVec2>,
        closed: bool,
    },
    /// `sketch { … }` (Plan 17.1): contours of exact segments. With `closed` every contour is
    /// closed and the filled region follows the nonzero winding rule, `hole` contours are always
    /// subtracted; otherwise the contours are open paths (a `sweep` path). Statement ids of
    /// the segments are in the evaluation output (`EvalOutput::sketches`), not here, so the
    /// cache key does not depend on source positions.
    Sketch {
        contours: Vec<Contour>,
        closed: bool,
    },
    /// `slot(p1, p2, r)`: stadium of radius `r` around the segment `a`–`b` (two half circles
    /// joined by two lines, exact); `a == b` is a circle around `a`.
    Slot {
        a: DVec2,
        b: DVec2,
        r: f64,
    },
}

/// The contour of [`Profile::Slot`], counter-clockwise: line on the right of `a`→`b`, half
/// circle around `b`, line back, half circle around `a` (only the two half circles if `a == b`).
pub fn slot_contour(a: DVec2, b: DVec2, r: f64) -> Contour {
    let d = b - a;
    let len = d.length();
    let u = if len > 0.0 { d / len } else { DVec2::X };
    let n = u.perp();
    let t = u.y.atan2(u.x);
    let h = std::f64::consts::FRAC_PI_2;
    let mut segs = Vec::with_capacity(4);
    if len > 0.0 {
        segs.push(Seg::Line { a: a - n * r, b: b - n * r });
    }
    segs.push(Seg::Arc { center: b, r, a0: t - h, a1: t + h, ccw: true });
    if len > 0.0 {
        segs.push(Seg::Line { a: b + n * r, b: a + n * r });
    }
    segs.push(Seg::Arc { center: a, r, a0: t + h, a1: t + 3.0 * h, ccw: true });
    Contour { hole: false, closed: true, segs }
}

/// One contour of a sketch: connected segments from `start()` to the next `start()` / `close()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    /// Inside a `hole { … }` block.
    pub hole: bool,
    pub closed: bool,
    pub segs: Vec<Seg>,
}

/// A sketch segment in the profile's xy-plane. Angles in radians.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Seg {
    Line {
        a: DVec2,
        b: DVec2,
    },
    /// Circular arc: `ccw` gives the direction from angle `a0` to `a1` (`a1 - a0` is the signed
    /// sweep, positive for `ccw`).
    Arc {
        center: DVec2,
        r: f64,
        a0: f64,
        a1: f64,
        ccw: bool,
    },
    /// Quadratic (3 points) or cubic (4 points) Bézier curve; start and end included.
    Bezier {
        points: Vec<DVec2>,
    },
    /// Interpolating spline through `points` (start included); optional end tangent
    /// directions.
    Spline {
        points: Vec<DVec2>,
        t0: Option<DVec2>,
        t1: Option<DVec2>,
    },
    /// Piece of a B-spline (a spline shortened by `corner()`, results of `fillet2d`), `z = 0`.
    Nurbs(cad_kernel::brep::BSplineCurve),
}

impl Seg {
    pub fn kind(&self) -> &'static str {
        match self {
            Seg::Line { .. } => "line",
            Seg::Arc { .. } => "arc",
            Seg::Bezier { .. } => "bezier",
            Seg::Spline { .. } => "spline",
            Seg::Nurbs(_) => "nurbs",
        }
    }

    /// Kernel segment (exact; splines interpolated).
    pub fn to_kernel(&self) -> Option<cad_kernel::Segment> {
        Some(match self {
            Seg::Line { a, b } => cad_kernel::Segment::Line { a: *a, b: *b },
            Seg::Arc { center, r, a0, a1, .. } => cad_kernel::Segment::Arc { center: *center, r: *r, a0: *a0, a1: *a1 },
            Seg::Bezier { points } => cad_kernel::Segment::bezier(points),
            Seg::Spline { points, t0, t1 } => {
                cad_kernel::Segment::Nurbs(cad_kernel::curve2::interpolate(points, *t0, *t1)?)
            }
            Seg::Nurbs(c) => cad_kernel::Segment::Nurbs(c.clone()),
        })
    }

    /// IR form of a kernel segment.
    pub fn from_kernel(s: &cad_kernel::Segment) -> Seg {
        match s {
            cad_kernel::Segment::Line { a, b } => Seg::Line { a: *a, b: *b },
            cad_kernel::Segment::Arc { center, r, a0, a1 } => {
                Seg::Arc { center: *center, r: *r, a0: *a0, a1: *a1, ccw: a1 >= a0 }
            }
            cad_kernel::Segment::Nurbs(c) => Seg::Nurbs(c.clone()),
        }
    }

    pub fn start(&self) -> DVec2 {
        match self {
            Seg::Line { a, .. } => *a,
            Seg::Arc { center, r, a0, .. } => *center + DVec2::new(a0.cos(), a0.sin()) * *r,
            Seg::Bezier { points } | Seg::Spline { points, .. } => points[0],
            Seg::Nurbs(c) => c.eval(c.domain().0).truncate(),
        }
    }

    pub fn end(&self) -> DVec2 {
        match self {
            Seg::Line { b, .. } => *b,
            Seg::Arc { center, r, a1, .. } => *center + DVec2::new(a1.cos(), a1.sin()) * *r,
            Seg::Bezier { points } | Seg::Spline { points, .. } => *points.last().unwrap_or(&DVec2::ZERO),
            Seg::Nurbs(c) => c.eval(c.domain().1).truncate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ZoneShape {
    /// Infinite line through `point` along `dir`, diameter `d`.
    Line { point: DVec3, dir: DVec3, d: f64 },
    /// Plane through `point` with `normal`, thickness `d`.
    Plane { point: DVec3, normal: DVec3, d: f64 },
}

impl Op {
    /// Short name for display and diagnostics.
    pub fn label(&self) -> String {
        match self {
            Op::Group => "group".into(),
            Op::ModuleCall { name, .. } => name.clone(),
            Op::Prim { prim: p, .. } => match p {
                Prim::Box { .. } => "box".into(),
                Prim::Sphere { .. } => "sphere".into(),
                Prim::Cylinder { .. } => "cylinder".into(),
                Prim::Torus { .. } => "torus".into(),
                Prim::Polyhedron { .. } => "polyhedron".into(),
                Prim::Mesh { .. } => "mesh".into(),
            },
            Op::Profile(p) => match p {
                Profile::Rect { .. } => "rect".into(),
                Profile::Circle { .. } => "circle".into(),
                Profile::Polygon { .. } => "polygon".into(),
                Profile::Spline { .. } => "spline".into(),
                Profile::Sketch { .. } => "sketch".into(),
                Profile::Slot { .. } => "slot".into(),
            },
            Op::Transform { kind, .. } => format!("{kind:?}").to_lowercase(),
            Op::Boolean { op, .. } => format!("{op:?}").to_lowercase(),
            Op::Blend { kind, .. } => format!("{kind:?}").to_lowercase(),
            Op::Blend2 { kind, .. } => format!("{}2d", format!("{kind:?}").to_lowercase()),
            Op::Offset { .. } => "offset".into(),
            Op::Stroke { .. } => "stroke".into(),
            Op::Shell { .. } => "shell".into(),
            Op::Zone(_) => "zone".into(),
            Op::LinearExtrude { .. } => "linear_extrude".into(),
            Op::RotateExtrude { .. } => "rotate_extrude".into(),
            Op::Loft { .. } => "loft".into(),
            Op::Sweep { .. } => "sweep".into(),
            Op::Surface { .. } => "surface".into(),
            Op::Thicken { .. } => "thicken".into(),
            Op::Resize { .. } => "resize".into(),
            Op::Color { .. } => "color".into(),
            Op::Hull => "hull".into(),
            Op::Minkowski => "minkowski".into(),
            Op::Empty => "empty".into(),
        }
    }
}

impl Tree {
    pub fn node(&self, i: NodeIdx) -> &Node {
        &self.nodes[i as usize]
    }

    /// The assertion whose node `i` is.
    pub fn assert_of(&self, i: NodeIdx) -> Option<&AssertDecl> {
        self.asserts.iter().find(|a| a.node == i)
    }

    /// Is `i` inside a keep-out volume (the node itself or an ancestor has [`Flags::keepout`])?
    pub fn in_keepout(&self, i: NodeIdx) -> bool {
        let mut cur = Some(i);
        while let Some(c) = cur {
            let n = self.node(c);
            if n.flags.keepout {
                return true;
            }
            cur = n.parent;
        }
        false
    }

    /// Dimension of a node's result: 2D if it consists of profiles only.
    pub fn dim(&self, i: NodeIdx) -> Option<Dim> {
        let n = self.node(i);
        match &n.op {
            Op::Profile(_) | Op::Blend2 { .. } | Op::Offset { .. } | Op::Stroke { .. } => Some(Dim::Two),
            Op::Prim { .. }
            | Op::LinearExtrude { .. }
            | Op::RotateExtrude { .. }
            | Op::Loft { .. }
            | Op::Sweep { .. }
            | Op::Surface { .. }
            | Op::Thicken { .. }
            | Op::Zone(_) => Some(Dim::Three),
            Op::Blend { .. } | Op::Shell { .. } => n.children.first().and_then(|&c| self.dim(c)),
            _ => {
                let mut d = None;
                for &c in &n.children {
                    let cn = self.node(c);
                    if cn.flags.disabled || cn.flags.background || cn.flags.zone {
                        continue;
                    }
                    match (d, self.dim(c)) {
                        (None, x) => d = x,
                        (Some(Dim::Two), Some(Dim::Three)) => d = Some(Dim::Three),
                        _ => {}
                    }
                }
                d
            }
        }
    }

    /// Children that contribute to the result (not disabled, background or zones).
    pub fn solid_children(&self, i: NodeIdx) -> impl Iterator<Item = NodeIdx> + '_ {
        self.node(i).children.iter().copied().filter(|&c| {
            let f = self.node(c).flags;
            !(f.disabled || f.background || f.zone)
        })
    }
}
