//! Hand-built trees for tests and tools (the evaluator builds the real ones).
//!
//! `local` of a new node is the parent's matrix if the parent is a `Transform`, identity
//! otherwise; `world = parent.world * local`. `stmt` is the node index, `inst_key` is
//! `1000 + index`.

use cad_ir::{BlendKind, Flags, Node, NodeIdx, Op, Prim, Profile, Select, TransformKind, Tree, ZoneShape};
use cad_kernel::{BoolOp, PathInterp};
use cad_syntax::Span;
use glam::{DMat4, DVec2, DVec3};

#[derive(Default)]
pub struct TreeBuilder {
    pub tree: Tree,
}

impl TreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node; the first node without parent becomes the root.
    pub fn add(&mut self, parent: Option<NodeIdx>, op: Op) -> NodeIdx {
        let idx = self.tree.nodes.len() as NodeIdx;
        let (local, pworld) = match parent {
            Some(p) => {
                let pn = self.tree.node(p);
                let l = match &pn.op {
                    Op::Transform { m, .. } => *m,
                    _ => DMat4::IDENTITY,
                };
                (l, pn.world)
            }
            None => (DMat4::IDENTITY, DMat4::IDENTITY),
        };
        self.tree.nodes.push(Node {
            stmt: idx,
            span: Span::new(idx as usize * 10, idx as usize * 10 + 5),
            op,
            children: Vec::new(),
            parent,
            local,
            world: pworld * local,
            flags: Flags::default(),
            module_inst: 0,
            blend: None,
            tol: None,
            color: None,
            inst_key: 1000 + idx as u64,
        });
        match parent {
            Some(p) => self.tree.nodes[p as usize].children.push(idx),
            None => {
                if self.tree.root.is_none() {
                    self.tree.root = Some(idx);
                }
            }
        }
        idx
    }

    pub fn root(&mut self, op: Op) -> NodeIdx {
        self.add(None, op)
    }

    pub fn child(&mut self, parent: NodeIdx, op: Op) -> NodeIdx {
        self.add(Some(parent), op)
    }

    pub fn flags(&mut self, n: NodeIdx) -> &mut Flags {
        &mut self.tree.nodes[n as usize].flags
    }

    pub fn set_blend(&mut self, n: NodeIdx, kind: BlendKind, r: f64) {
        self.tree.nodes[n as usize].blend = Some((kind, r));
    }

    pub fn finish(self) -> Tree {
        self.tree
    }
}

// ---- op constructors

pub fn group() -> Op {
    Op::Group
}

pub fn cube(s: f64) -> Op {
    boxed(DVec3::ZERO, DVec3::splat(s))
}

pub fn boxed(min: DVec3, max: DVec3) -> Op {
    Op::Prim { prim: Prim::Box { min, max }, place: DMat4::IDENTITY }
}

pub fn sphere(r: f64) -> Op {
    Op::Prim { prim: Prim::Sphere { r }, place: DMat4::IDENTITY }
}

pub fn cylinder(h: f64, r: f64) -> Op {
    Op::Prim { prim: Prim::Cylinder { h, r1: r, r2: r }, place: DMat4::IDENTITY }
}

pub fn translate(v: DVec3) -> Op {
    Op::Transform { kind: TransformKind::Translate, m: DMat4::from_translation(v) }
}

pub fn rotate_z(deg: f64) -> Op {
    Op::Transform { kind: TransformKind::Rotate, m: DMat4::from_rotation_z(deg.to_radians()) }
}

pub fn rotate_x(deg: f64) -> Op {
    Op::Transform { kind: TransformKind::Rotate, m: DMat4::from_rotation_x(deg.to_radians()) }
}

pub fn scale(s: DVec3) -> Op {
    Op::Transform { kind: TransformKind::Scale, m: DMat4::from_scale(s) }
}

pub fn boolean(op: BoolOp) -> Op {
    Op::Boolean { op, fillet: None, chamfer: None }
}

pub fn difference() -> Op {
    boolean(BoolOp::Difference)
}

pub fn union() -> Op {
    boolean(BoolOp::Union)
}

pub fn intersection() -> Op {
    boolean(BoolOp::Intersection)
}

pub fn difference_fillet(r: f64) -> Op {
    Op::Boolean { op: BoolOp::Difference, fillet: Some(r), chamfer: None }
}

pub fn fillet(r: f64, select: Select) -> Op {
    Op::Blend { kind: BlendKind::Fillet, r, select }
}

pub fn chamfer(r: f64, select: Select) -> Op {
    Op::Blend { kind: BlendKind::Chamfer, r, select }
}

pub fn shell(t: f64, select: Select) -> Op {
    Op::Shell { t, select }
}

pub fn zone_line(point: DVec3, dir: DVec3, d: f64) -> Op {
    Op::Zone(ZoneShape::Line { point, dir, d })
}

pub fn zone_plane(point: DVec3, normal: DVec3, d: f64) -> Op {
    Op::Zone(ZoneShape::Plane { point, normal, d })
}

pub fn square(s: f64) -> Op {
    rect(DVec2::ZERO, DVec2::splat(s))
}

pub fn rect(min: DVec2, max: DVec2) -> Op {
    Op::Profile(Profile::Rect { min, max })
}

pub fn circle(r: f64) -> Op {
    Op::Profile(Profile::Circle { r })
}

pub fn polygon(loops: Vec<Vec<DVec2>>) -> Op {
    Op::Profile(Profile::Polygon { loops })
}

pub fn linear_extrude(h: f64) -> Op {
    Op::LinearExtrude { h, twist: 0.0, scale: DVec2::ONE, center: false }
}

pub fn rotate_extrude(angle: f64) -> Op {
    Op::RotateExtrude { angle }
}

pub fn loft(ruled: bool) -> Op {
    Op::Loft { ruled }
}

pub fn sweep(path: Vec<DVec3>, interp: PathInterp) -> Op {
    Op::Sweep { path, interp }
}
