//! Booleans with the SMLib facade's result checks (`checked_boolean`, `boolean_plausible`,
//! `interiors_overlap` in `smlib/facade/sc_facade.cpp`): solidcore's Boolean can return a body
//! that is no closed solid where the operands touch tangentially, or a wrong body where one
//! operand is tiny against the other (the whole box for a box ∩ a sphere of 1e-5 of its size).
//! Per step `a op b` of closed solids:
//!
//! * the result must be a closed solid (or empty); a difference or intersection of bodies whose
//!   interiors do not overlap is `a` or empty;
//! * its volume must lie within the bounds of the operation (3 % slack: gross errors only);
//! * points just inside and just outside the faces of each operand whose box meets the box of a
//!   face of the other (one per face, at most 24 faces; the facade takes 5 on every face that
//!   reaches the other operand's box, solidcore's point classification is too slow for that)
//!   must be inside or outside the result as the operation says, confirmed at a second, shifted
//!   point and by the side of the result's nearest face (solidcore's ray classification can be
//!   wrong at several nearby points);
//! * else the step runs again with the operands swapped (union, intersection), then with the
//!   second operand moved by 1e-6, 1e-5, 1e-4 of the size; if nothing passes, `Failed` with the
//!   facade's wording.
//!
//! A union of bodies that only touch is one solid with a shell per body (`SmBrep::MergeBrep`, the
//! facade's `touching_union`). Not ported: the facade's union of bodies whose boxes are apart
//! (`disjoint_union`, solidcore unites those correctly) and its reordering of the operands of a
//! chain when a step fails. In draft
//! mode a step's result is taken unchecked when it is closed or empty, as the facade.

use cad_kernel::{BoolOp, Class, KResult, KernelError, Tag};
use glam::{DMat4, DVec3};
use solidcore::geom::a_object::BooleanOperation;
use solidcore::query::api_queries as q;

use crate::imp::{Inner, err};
use crate::{Shape, ShapeData};

/// What the tries of a step found: the first reason a result was refused, the first error,
/// whether a result was wrong (then the moved operand is tried).
#[derive(Default)]
struct Attempts {
    why: Option<String>,
    first_err: Option<KernelError>,
    wrong: bool,
    /// A union gave no closed solid and the interiors do not overlap: the bodies only touch.
    touching: bool,
}

/// How a step's result compares with its operands (the facade's `Plausible`).
enum Plausible {
    Ok,
    /// Two closed solids gave a body that is not closed.
    Open,
    Wrong(String),
}

fn boxes_overlap(a: (DVec3, DVec3), b: (DVec3, DVec3), eps: f64) -> bool {
    a.0.cmple(b.1 + DVec3::splat(eps)).all() && b.0.cmple(a.1 + DVec3::splat(eps)).all()
}

/// Membership of `q` in `a op b` from its classes in `a`, `b`; `None` where undecidable.
fn expected(op: BoolOp, ca: Option<Class>, cb: Option<Class>) -> Option<bool> {
    let inside = |c: Option<Class>| match c? {
        Class::Inside => Some(true),
        Class::Outside => Some(false),
        Class::On => None,
    };
    let (a, b) = (inside(ca)?, inside(cb)?);
    Some(match op {
        BoolOp::Union => a || b,
        BoolOp::Difference => a && !b,
        BoolOp::Intersection => a && b,
    })
}

impl Inner {
    /// `shapes[0] op shapes[1] op …`, each step checked (module doc).
    pub fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
        let first = shapes.first().ok_or_else(|| KernelError::InvalidInput("boolean without operands".into()))?;
        let mut acc = first.clone();
        for s in &shapes[1..] {
            acc = self.checked_boolean(op, &acc, s, tag)?;
        }
        let mut b = if shapes.len() == 1 || std::rc::Rc::ptr_eq(&acc, first) {
            self.copy(&acc.brep.borrow())?
        } else {
            std::mem::replace(&mut *acc.brep.borrow_mut(), solidcore::Brep::new())
        };
        // solidcore carries the tags (split pieces keep theirs); a face without one gets
        // the tag of the operand face it lies on, else the Boolean's
        self.resolve_tags(&mut b, shapes, tag, false)?;
        Ok(ShapeData::new(b, tag))
    }

    /// One Boolean of solidcore on copies of the operands (`b` moved by `shift`).
    fn raw_boolean(&mut self, op: BoolOp, a: &Shape, b: &Shape, tag: Tag) -> KResult<Shape> {
        let op_sc = match op {
            BoolOp::Union => BooleanOperation::Union,
            BoolOp::Difference => BooleanOperation::Difference,
            BoolOp::Intersection => BooleanOperation::Intersection,
        };
        // the Boolean consumes its operands: copies, the shapes stay as they are
        let x = self.copy(&a.brep.borrow())?;
        let y = self.copy(&b.brep.borrow())?;
        let out = solidcore::boolean::api_brep::boolean(&mut self.k, x, y, op_sc).map_err(|e| err("boolean", e))?;
        Ok(ShapeData::new(out.result, tag))
    }

    fn checked_boolean(&mut self, op: BoolOp, a: &Shape, b: &Shape, tag: Tag) -> KResult<Shape> {
        let closed = |k: &mut Inner, s: &Shape| !k.is_empty(s) && k.is_solid(s);
        if !(closed(self, a) && closed(self, b)) {
            // only closed solids can be checked
            return self.raw_boolean(op, a, b, tag);
        }
        if self.draft {
            let r = self.raw_boolean(op, a, b, tag)?;
            if self.is_empty(&r) || self.is_solid(&r) {
                return Ok(r);
            }
        }
        let commutative = op != BoolOp::Difference;
        let mut st = Attempts::default();
        if let Some(r) = self.attempt(op, a, b, tag, &mut st)? {
            return Ok(r);
        }
        if commutative && let Some(r) = self.attempt(op, b, a, tag, &mut st)? {
            return Ok(r);
        }
        if st.wrong {
            let size = self.pair_box(a, b).map_or(1.0, |(lo, hi)| (hi - lo).length().max(1e-9));
            let dir = DVec3::new(0.2672612, 0.5345225, 0.8017837).normalize();
            for rel in [1e-6, 1e-5, 1e-4] {
                let moved = self.transform(b, &DMat4::from_translation(dir * (rel * size)))?;
                if let Some(r) = self.attempt(op, a, &moved, tag, &mut st)? {
                    return Ok(r);
                }
                if commutative && let Some(r) = self.attempt(op, &moved, a, tag, &mut st)? {
                    return Ok(r);
                }
            }
        }
        if st.touching {
            let mut m = self.copy(&a.brep.borrow())?;
            if self.merge_into(&mut m, &b.brep.borrow()).is_ok() {
                let t = ShapeData::new(m, tag);
                if self.is_solid(&t) {
                    return Ok(t);
                }
            }
        }
        match (st.why, st.first_err) {
            (Some(w), _) => Err(KernelError::failed(format!("solidcore: boolean: {w}"))),
            (None, Some(e)) => Err(e),
            (None, None) => Err(KernelError::failed("solidcore: boolean: no result")),
        }
    }

    /// One try of `x op y`: the result if it passes the checks; else `None`, with the reason in
    /// `st`.
    fn attempt(&mut self, op: BoolOp, x: &Shape, y: &Shape, tag: Tag, st: &mut Attempts) -> KResult<Option<Shape>> {
        let r = match self.raw_boolean(op, x, y, tag) {
            Ok(r) => r,
            Err(e @ KernelError::Unsupported(_)) => return Err(e),
            Err(e) => {
                st.wrong = true;
                st.first_err.get_or_insert(e);
                return Ok(None);
            }
        };
        let pl = self.plausible(op, x, y, &r)?;
        match pl {
            Plausible::Ok => Ok(Some(r)),
            Plausible::Open => {
                // bodies that only touch: x ∪ y one solid with a shell per body (the facade's
                // `touching_union`), x − y = x, x ∩ y = empty
                if op == BoolOp::Union && !st.touching && !self.interiors_overlap(x, y, tag)? {
                    // (after the other tries: the swapped union may be one closed solid)
                    st.touching = true;
                }
                if op != BoolOp::Union && !self.interiors_overlap(x, y, tag)? {
                    return Ok(Some(if op == BoolOp::Difference {
                        ShapeData::new(self.copy(&x.brep.borrow())?, tag)
                    } else {
                        ShapeData::new(solidcore::Brep::new(), tag)
                    }));
                }
                st.why.get_or_insert_with(|| {
                    "the result is not a closed solid: the operands touch tangentially or along an edge there \
                     (let them overlap a little or keep a gap)"
                        .into()
                });
                Ok(None)
            }
            Plausible::Wrong(w) => {
                st.wrong = true;
                st.why.get_or_insert_with(|| format!("solidcore returned a wrong result ({w})"));
                Ok(None)
            }
        }
    }

    /// The volume for the plausibility check, coarse as the facade's `solid_volume` (accuracy
    /// 1e-2; the exact mass properties where they are known already).
    fn coarse_volume(&mut self, s: &Shape) -> Option<f64> {
        if let Some(m) = s.mass.get() {
            return Some(m.volume);
        }
        let store = solidcore::geom::attribute::AttributeStore::new();
        let key_of = |_e: solidcore::EntityRef| solidcore::geom::attribute::ObjectKey(0);
        let attrs = solidcore::mass::integrator::properties::PropertyAttributes { store: &store, key_of: &key_of };
        solidcore::mass::integrator::properties::brep_volume(&self.k, &mut s.brep.borrow_mut(), 1e-2, &attrs)
            .ok()
            .filter(|v| v.is_finite())
    }

    /// The box of both shapes.
    fn pair_box(&mut self, a: &Shape, b: &Shape) -> Option<(DVec3, DVec3)> {
        let (x, y) = (self.bounds(a).ok()??, self.bounds(b).ok()??);
        Some((x.0.min(y.0), x.1.max(y.1)))
    }

    /// The boxes of the faces of `s`.
    fn face_boxes(&mut self, s: &Shape) -> Vec<(solidcore::FaceId, (DVec3, DVec3))> {
        let b = s.brep.borrow();
        b.face_list()
            .iter()
            .filter_map(|&f| {
                let x = q::face_bounding_box(&self.k, &b, f, false).ok()?;
                Some((
                    f,
                    (
                        DVec3::new(x.min_at(0), x.min_at(1), x.min_at(2)),
                        DVec3::new(x.max_at(0), x.max_at(1), x.max_at(2)),
                    ),
                ))
            })
            .collect()
    }

    /// A point of each face of `s` whose box meets the box of a face of `other` (the facade takes
    /// 5 on each face that reaches the other operand's box; only faces near the other's faces can
    /// be cut), with the outward normal there.
    fn near_samples(&mut self, s: &Shape, other: &Shape, eps: f64) -> Vec<(DVec3, DVec3)> {
        let theirs = self.face_boxes(other);
        let mine = self.face_boxes(s);
        let near: Vec<solidcore::FaceId> = mine
            .into_iter()
            .filter(|(_, fb)| theirs.iter().any(|(_, ob)| boxes_overlap(*fb, *ob, eps)))
            .map(|(f, _)| f)
            .collect();
        // solidcore's point classification is slow on large bodies (a box with 600 faces of text
        // on it: 200 s with the facade's 5 samples on every face): one sample per face, every
        // k-th face beyond MAX_FACES
        const MAX_FACES: usize = 24;
        let step = near.len().div_ceil(MAX_FACES).max(1);
        let mut b = s.brep.borrow_mut();
        let mut out = Vec::new();
        for f in near.into_iter().step_by(step) {
            if let Some(x) = crate::brep::face_sample(&self.k, &mut b, f) {
                out.push(x);
            }
        }
        out
    }

    fn class(&mut self, s: &Shape, p: DVec3) -> Option<Class> {
        if self.is_empty(s) {
            return Some(Class::Outside);
        }
        self.classify(s, &[p]).ok().map(|c| c[0])
    }

    /// The facade's `boolean_plausible` (module doc).
    fn plausible(&mut self, op: BoolOp, a: &Shape, b: &Shape, r: &Shape) -> KResult<Plausible> {
        let r_empty = self.is_empty(r);
        if !r_empty && !self.is_solid(r) {
            return Ok(Plausible::Open);
        }
        let (Some(ba), Some(bb)) = (self.bounds(a)?, self.bounds(b)?) else { return Ok(Plausible::Ok) };
        let size = (ba.0.min(bb.0) - ba.1.max(bb.1)).length().max(1e-9);
        let eps = 1e-4 * size;
        if !boxes_overlap(ba, bb, eps) {
            return Ok(Plausible::Ok);
        }
        let bounds_hold = |va: f64, vb: f64, vr: f64| {
            let s = 3e-2 * (va + vb) + 1e-12;
            match op {
                BoolOp::Union => vr >= va.max(vb) - s && vr <= va + vb + s,
                BoolOp::Difference => vr >= va - vb - s && vr <= va + s,
                BoolOp::Intersection => vr >= -s && vr <= va.min(vb) + s,
            }
        };
        // coarse volumes first (the facade's 1e-2); before a refusal the accurate ones
        for accurate in [false, true] {
            let vol = |k: &mut Inner, s: &Shape| {
                if accurate {
                    k.mass_props(s).ok().map(|m| m.volume).filter(|v| v.is_finite())
                } else {
                    k.coarse_volume(s)
                }
            };
            let (va, vb) = (vol(self, a), vol(self, b));
            let vr = if r_empty { Some(0.0) } else { vol(self, r) };
            let (Some(va), Some(vb), Some(vr)) = (va, vb, vr) else { break };
            if bounds_hold(va, vb, vr) {
                break;
            }
            if accurate {
                return Ok(Plausible::Wrong(format!("volume {vr} impossible for operands {va}, {vb}")));
            }
        }
        for side in 0..2 {
            let (own, other) = if side == 0 { (a, b) } else { (b, a) };
            for (p, n) in self.near_samples(own, other, eps) {
                for sign in [-1.0, 1.0] {
                    let q = p + n * (sign * eps);
                    let cs = Some(if sign < 0.0 { Class::Inside } else { Class::Outside });
                    let co = self.class(other, q);
                    let (ca, cb) = if side == 0 { (cs, co) } else { (co, cs) };
                    let Some(want) = expected(op, ca, cb) else { continue };
                    let got = match self.class(r, q) {
                        Some(Class::Inside) => true,
                        Some(Class::Outside) => false,
                        _ => continue,
                    };
                    if got == want {
                        continue;
                    }
                    // confirm at a second point, everything classified again
                    let t = n
                        .cross(DVec3::new(0.48, 0.6, 0.64))
                        .try_normalize()
                        .unwrap_or_else(|| n.any_orthonormal_vector());
                    let q2 = q + t * (0.37 * eps);
                    let (ca2, cb2) = (self.class(a, q2), self.class(b, q2));
                    let Some(want2) = expected(op, ca2, cb2) else { continue };
                    let got2 = match self.class(r, q2) {
                        Some(Class::Inside) => true,
                        Some(Class::Outside) => false,
                        _ => continue,
                    };
                    if got2 == want2 {
                        continue;
                    }
                    // solidcore's ray classification can be wrong at several nearby points (a tube
                    // wall in `14_pipe_bend`): confirmed by the side of the nearest face of the
                    // result, where that can be found
                    if self.local_inside(r, q).is_some_and(|inside| inside == want) {
                        continue;
                    }
                    let (is, must) = if got { ("inside", "outside") } else { ("outside", "inside") };
                    return Ok(Plausible::Wrong(format!(
                        "point ({:.6}, {:.6}, {:.6}) is {is} the result but must be {must}",
                        q.x, q.y, q.z
                    )));
                }
            }
        }
        Ok(Plausible::Ok)
    }

    /// Whether `p` lies inside `s` by the side of the nearest face of `s` (its foot on the face's
    /// surface inside the face, the outward normal there); `None` if no face has a foot.
    fn local_inside(&mut self, s: &Shape, p: DVec3) -> Option<bool> {
        let mut b = s.brep.borrow_mut();
        let mut best: Option<(f64, DVec3, DVec3)> = None;
        for f in b.face_list().to_vec() {
            let Ok(Some(srf)) = solidcore::topology::face::p2::surface(&b, f) else { continue };
            let Ok(c) = q::surface_closest_point(&self.k, srf, crate::imp::pt(p)) else { continue };
            if best.is_some_and(|x| x.0 <= c.closest.distance) {
                continue;
            }
            let in_face = c.uvs.iter().any(|&uv| {
                q::face_classify_uv(&self.k, &mut b, f, uv)
                    .is_ok_and(|x| x == solidcore::intersect::point_class::PointClassType::Face)
            });
            if !in_face {
                continue;
            }
            let foot = crate::imp::dv(c.closest.point);
            if let Some(n) = crate::brep::outward_normal(&self.k, &b, f, foot) {
                best = Some((c.closest.distance, foot, n));
            }
        }
        best.map(|(_, foot, n)| (p - foot).dot(n) < 0.0)
    }

    /// Do the interiors of `a` and `b` overlap (the facade's `interiors_overlap`)? A point just
    /// inside a face of one inside the other; else their intersection's volume.
    fn interiors_overlap(&mut self, a: &Shape, b: &Shape, tag: Tag) -> KResult<bool> {
        let (Some(ba), Some(bb)) = (self.bounds(a)?, self.bounds(b)?) else { return Ok(false) };
        let eps = 1e-4 * (ba.0.min(bb.0) - ba.1.max(bb.1)).length().max(1e-9);
        for side in 0..2 {
            let (own, other) = if side == 0 { (a, b) } else { (b, a) };
            for (p, n) in self.near_samples(own, other, eps) {
                if self.class(other, p - n * eps) == Some(Class::Inside) {
                    return Ok(true);
                }
            }
        }
        let Ok(x) = self.raw_boolean(BoolOp::Intersection, a, b, tag) else { return Ok(true) };
        if self.is_empty(&x) {
            return Ok(false);
        }
        let vx = self.mass_props(&x).map(|m| m.volume);
        let vmin = self.mass_props(a).map(|m| m.volume)?.min(self.mass_props(b).map(|m| m.volume)?);
        Ok(!matches!(vx, Ok(v) if v.abs() <= 1e-6 * vmin))
    }
}
