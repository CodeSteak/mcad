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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Select {
    #[default]
    Inside,
    Touch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Prim {
    Box { min: DVec3, max: DVec3 },
    Sphere { r: f64 },
    Cylinder { h: f64, r1: f64, r2: f64 },
    Torus { big_r: f64, r: f64 },
    Polyhedron { points: Vec<DVec3>, faces: Vec<Vec<u32>> },
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
            },
            Op::Profile(p) => match p {
                Profile::Rect { .. } => "rect".into(),
                Profile::Circle { .. } => "circle".into(),
                Profile::Polygon { .. } => "polygon".into(),
                Profile::Spline { .. } => "spline".into(),
            },
            Op::Transform { kind, .. } => format!("{kind:?}").to_lowercase(),
            Op::Boolean { op, .. } => format!("{op:?}").to_lowercase(),
            Op::Blend { kind, .. } => format!("{kind:?}").to_lowercase(),
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
            Op::Empty => "empty".into(),
        }
    }
}

impl Tree {
    pub fn node(&self, i: NodeIdx) -> &Node {
        &self.nodes[i as usize]
    }

    /// Dimension of a node's result: 2D if it consists of profiles only.
    pub fn dim(&self, i: NodeIdx) -> Option<Dim> {
        let n = self.node(i);
        match &n.op {
            Op::Profile(_) => Some(Dim::Two),
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
