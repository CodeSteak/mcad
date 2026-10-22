//! Bounding-box estimate of an evaluated geometry tree (no kernel needed).
//!
//! union → union of boxes, intersection → intersection of boxes, difference → box of
//! the first child. Primitives, profiles and extrusions are bounded exactly
//! (spheres/discs as ellipsoids/ellipses under the affine frame; revolutions and twists
//! sampled finely).

use cad_ir::{Op, Prim, Profile, Tree};
use cad_kernel::BoolOp;
use glam::{DMat3, DMat4, DVec2, DVec3};

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub lo: DVec3,
    pub hi: DVec3,
}

impl Aabb {
    pub fn empty() -> Self {
        Aabb { lo: DVec3::splat(f64::INFINITY), hi: DVec3::splat(f64::NEG_INFINITY) }
    }
    pub fn is_empty(&self) -> bool {
        self.lo.x > self.hi.x
    }
    fn add(&mut self, p: DVec3) {
        self.lo = self.lo.min(p);
        self.hi = self.hi.max(p);
    }
    /// Ellipse `c + e1 cos t + e2 sin t` (filled).
    fn add_ellipse(&mut self, c: DVec3, e1: DVec3, e2: DVec3) {
        let ext = DVec3::new(e1.x.hypot(e2.x), e1.y.hypot(e2.y), e1.z.hypot(e2.z));
        self.add(c - ext);
        self.add(c + ext);
    }
    /// Ellipsoid `c + A u`, |u| ≤ 1.
    fn add_ellipsoid(&mut self, c: DVec3, a: DMat3) {
        let t = a.transpose();
        let ext = DVec3::new(t.x_axis.length(), t.y_axis.length(), t.z_axis.length());
        self.add(c - ext);
        self.add(c + ext);
    }
    fn union(&mut self, o: &Aabb) {
        if !o.is_empty() {
            self.add(o.lo);
            self.add(o.hi);
        }
    }
    fn intersect(&self, o: &Aabb) -> Aabb {
        let r = Aabb { lo: self.lo.max(o.lo), hi: self.hi.min(o.hi) };
        if r.lo.cmpgt(r.hi).any() { Aabb::empty() } else { r }
    }
}

/// 2D pieces in the system of the enclosing extrusion.
#[derive(Clone, Debug)]
enum P2 {
    Pt(DVec2),
    Ellipse(DVec2, DVec2, DVec2),
    /// Box from a 2D intersection.
    Rect(DVec2, DVec2),
}

fn lin3(m: &DMat4) -> DMat3 {
    DMat3::from_mat4(*m)
}

fn prim_box(prim: &Prim, f: DMat4, b: &mut Aabb) {
    let l = lin3(&f);
    match prim {
        Prim::Box { min, max } => {
            for i in 0..8 {
                let p = DVec3::new(
                    if i & 1 == 0 { min.x } else { max.x },
                    if i & 2 == 0 { min.y } else { max.y },
                    if i & 4 == 0 { min.z } else { max.z },
                );
                b.add(f.transform_point3(p));
            }
        }
        Prim::Sphere { r } => b.add_ellipsoid(f.transform_point3(DVec3::ZERO), l * *r),
        Prim::Cylinder { h, r1, r2 } => {
            for (z, r) in [(0.0, *r1), (*h, *r2)] {
                b.add_ellipse(f.transform_point3(DVec3::new(0.0, 0.0, z)), l * DVec3::X * r, l * DVec3::Y * r);
            }
        }
        Prim::Torus { big_r, r } => {
            for k in 0..=1440 {
                let a = (k as f64 / 4.0).to_radians();
                let c = DVec3::new(big_r * a.cos(), big_r * a.sin(), 0.0);
                b.add_ellipsoid(f.transform_point3(c), l * *r);
            }
        }
        Prim::Polyhedron { points, .. } => {
            for p in points {
                b.add(f.transform_point3(*p));
            }
        }
    }
}

/// Bounding box of node `i` whose system maps to the target system by `f`.
pub fn bounds3(t: &Tree, i: u32, f: DMat4) -> Result<Aabb, String> {
    let n = t.node(i);
    let kids = |b: &mut Aabb| -> Result<(), String> {
        for c in t.solid_children(i) {
            b.union(&bounds3(t, c, f * t.node(c).local)?);
        }
        Ok(())
    };
    let mut b = Aabb::empty();
    match &n.op {
        Op::Prim { prim, place } => prim_box(prim, f * *place, &mut b),
        // 2D at top level is not part of OpenSCAD's 3D export.
        Op::Profile(_) => {}
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty => kids(&mut b)?,
        Op::Boolean { op, .. } => {
            let mut first = true;
            for c in t.solid_children(i) {
                let cb = bounds3(t, c, f * t.node(c).local)?;
                match op {
                    BoolOp::Union => b.union(&cb),
                    BoolOp::Difference => {
                        if first {
                            b = cb;
                        }
                    }
                    BoolOp::Intersection => b = if first { cb } else { b.intersect(&cb) },
                }
                first = false;
            }
        }
        Op::Blend { .. } | Op::Shell { .. } => {
            if let Some(c) = t.solid_children(i).next() {
                b = bounds3(t, c, f * t.node(c).local)?;
            }
        }
        Op::LinearExtrude { h, twist, scale, center } => {
            let p2 = profile_children(t, i)?;
            let z0 = if *center { -h / 2.0 } else { 0.0 };
            let steps = if *twist == 0.0 { 1 } else { 2880 };
            for k in 0..=steps {
                let s = k as f64 / steps as f64;
                // OpenSCAD: slice = scale · rotate(-twist) · p (positive twist turns
                // clockwise seen from +z, the scale acts on the rotated profile).
                let rot = glam::DMat2::from_angle((-twist * s).to_radians());
                let sc = DVec2::ONE + (*scale - DVec2::ONE) * s;
                let lift = |p: DVec2| f.transform_point3(((rot * p) * sc).extend(z0 + h * s));
                let lvec = |v: DVec2| f.transform_vector3(((rot * v) * sc).extend(0.0));
                add_p2(&p2, &mut b, lift, lvec);
            }
        }
        Op::RotateExtrude { angle } => {
            let p2 = profile_children(t, i)?;
            let steps = (angle.abs() * 8.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let a = (angle * k as f64 / steps as f64).to_radians();
                let (s, c) = a.sin_cos();
                let lift = |p: DVec2| f.transform_point3(DVec3::new(p.x * c, p.x * s, p.y));
                let lvec = |v: DVec2| f.transform_vector3(DVec3::new(v.x * c, v.x * s, v.y));
                add_p2(&p2, &mut b, lift, lvec);
            }
        }
        Op::Loft { .. } => {
            for c in t.solid_children(i) {
                let mut p2 = Vec::new();
                pieces2(t, c, DMat4::IDENTITY, &mut p2)?;
                let fc = f * t.node(c).local;
                add_p2(&p2, &mut b, |p| fc.transform_point3(p.extend(0.0)), |v| fc.transform_vector3(v.extend(0.0)));
            }
        }
        other => return Err(format!("no bounds estimate for `{}`", other.label())),
    }
    Ok(b)
}

fn add_p2(p2: &[P2], b: &mut Aabb, lift: impl Fn(DVec2) -> DVec3, lvec: impl Fn(DVec2) -> DVec3) {
    for p in p2 {
        match p {
            P2::Pt(q) => b.add(lift(*q)),
            P2::Ellipse(c, e1, e2) => b.add_ellipse(lift(*c), lvec(*e1), lvec(*e2)),
            P2::Rect(lo, hi) => {
                for q in [*lo, DVec2::new(hi.x, lo.y), *hi, DVec2::new(lo.x, hi.y)] {
                    b.add(lift(q));
                }
            }
        }
    }
}

fn profile_children(t: &Tree, i: u32) -> Result<Vec<P2>, String> {
    let mut p2 = Vec::new();
    for c in t.solid_children(i) {
        pieces2(t, c, t.node(c).local, &mut p2)?;
    }
    Ok(p2)
}

fn rect_of(p2: &[P2]) -> Option<(DVec2, DVec2)> {
    let mut b = Aabb::empty();
    add_p2(p2, &mut b, |p| p.extend(0.0), |v| v.extend(0.0));
    (!b.is_empty()).then(|| (b.lo.truncate(), b.hi.truncate()))
}

/// 2D pieces of a profile subtree; `m` maps the node's system to the extrusion's.
fn pieces2(t: &Tree, i: u32, m: DMat4, out: &mut Vec<P2>) -> Result<(), String> {
    let n = t.node(i);
    let pt = |p: DVec2| m.transform_point3(p.extend(0.0)).truncate();
    let vc = |v: DVec2| m.transform_vector3(v.extend(0.0)).truncate();
    match &n.op {
        Op::Profile(p) => match p {
            Profile::Rect { min, max } => {
                for q in [*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)] {
                    out.push(P2::Pt(pt(q)));
                }
            }
            Profile::Circle { r } => out.push(P2::Ellipse(pt(DVec2::ZERO), vc(DVec2::X * *r), vc(DVec2::Y * *r))),
            Profile::Polygon { loops } => out.extend(loops.iter().flatten().map(|q| P2::Pt(pt(*q)))),
            Profile::Spline { points, .. } => out.extend(points.iter().map(|q| P2::Pt(pt(*q)))),
        },
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty => {
            for c in t.solid_children(i) {
                pieces2(t, c, m * t.node(c).local, out)?;
            }
        }
        Op::Boolean { op, .. } => {
            let mut acc: Option<(DVec2, DVec2)> = None;
            for (k, c) in t.solid_children(i).enumerate() {
                let mut cp = Vec::new();
                pieces2(t, c, m * t.node(c).local, &mut cp)?;
                match op {
                    BoolOp::Union => out.extend(cp),
                    BoolOp::Difference => {
                        if k == 0 {
                            out.extend(cp)
                        }
                    }
                    BoolOp::Intersection => {
                        let r = rect_of(&cp);
                        acc = if k == 0 {
                            r
                        } else {
                            match (acc, r) {
                                (Some(a), Some(r)) => Some((a.0.max(r.0), a.1.min(r.1))),
                                _ => None,
                            }
                        };
                    }
                }
            }
            if let Some((lo, hi)) = acc {
                out.push(P2::Rect(lo, hi));
            }
        }
        other => return Err(format!("no 2D bounds estimate for `{}`", other.label())),
    }
    Ok(())
}
