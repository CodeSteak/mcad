//! Booleans with the SMLib facade's result checks (`checked_boolean`, `boolean_plausible`,
//! `interiors_overlap` in `smlib/facade/sc_facade.cpp`): solidcore's Boolean can return a body
//! that is no closed solid where the operands touch tangentially, or a wrong body where one
//! operand is tiny against the other (the whole box for a box ∩ a sphere of 1e-5 of its size).
//! Per step `a op b` of closed solids:
//!
//! * the result must be a closed solid (or empty); a difference or intersection of bodies whose
//!   interiors do not overlap is `a` or empty;
//! * its volume must lie within the bounds of the operation (3 % slack: gross errors only);
//! * points just inside and just outside the faces of each operand whose box meets the other
//!   operand's box, all faces of the smaller operand, and at least one face per shell (one per face, every k-th beyond 64
//!   faces; the facade takes 5 on every such face, solidcore's point classification is too slow
//!   for that) must be inside or outside the result as the operation says, confirmed at a
//!   second, shifted point and by the side of the result's nearest face (solidcore's ray
//!   classification can be wrong at several nearby points). The offset from the faces is 1e-4
//!   of the pair's size, at most 1e-3 of the smaller operand's, at least ten zone tolerances (up
//!   to 2 % of the smaller operand) (the facade: always 1e-4 of the pair's, so SMLib can lose or
//!   move an operand smaller than that);
//! * else the step runs again with the operands swapped (union, intersection), then with the
//!   second operand moved by 1e-6, 1e-5, 1e-4 of the size (at most ten times the smaller
//!   operand's); if nothing passes, `Failed` with the facade's wording.
//!
//! A union of bodies that only touch is one solid with a shell per body (`SmBrep::MergeBrep`, the
//! facade's `touching_union`); so is a union whose result repeats an operand inside out as a
//! void (more shells than both operands, two of them alike: solidcore does that with bodies
//! that do not overlap, the pieces of a stencil glyph), as the facade's `disjoint_union` did
//! for SMLib. Not ported: the facade's reordering of the operands of a chain when a step fails. In draft
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
    /// Where the first result that was no closed solid is open (its edges with one face), as
    /// the facade's places.
    places: Vec<Vec<DVec3>>,
}

/// How a step's result compares with its operands (the facade's `Plausible`).
enum Plausible {
    Ok,
    /// Two closed solids gave a body that is not closed.
    Open,
    Wrong(String),
    /// A union of bodies whose interiors do not overlap lost volume to a shell the operands do
    /// not have (a body repeated inside out as a void).
    Doubled(String),
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
            // the shift: of the pair's size, but at most 10x the smaller operand's (moving a
            // small operand by more than 1e-3 of itself would change the result)
            let size = self.pair_box(a, b).map_or(1.0, |(lo, hi)| (hi - lo).length().max(1e-9));
            let small = [a, b]
                .iter()
                .filter_map(|s| self.bounds(s).ok().flatten())
                .map(|(lo, hi)| (hi - lo).length())
                .fold(f64::INFINITY, f64::min);
            let size = size.min(10.0 * small).max(1e-9);
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
            (Some(w), _) => Err(KernelError::Failed {
                msg: format!("solidcore: boolean: {w}"),
                edges: Vec::new(),
                places: st.places,
            }),
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
            Plausible::Doubled(w) => {
                // the bodies only touch or lie apart: one solid with a shell per body
                st.touching = true;
                st.why.get_or_insert(w);
                Ok(None)
            }
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
                if st.places.is_empty()
                    && let Ok(es) = self.edges(&r, 8)
                {
                    st.places = es.into_iter().filter(|e| e.face_tags.len() == 1).map(|e| e.points).take(32).collect();
                }
                if st.places.is_empty() {
                    // every edge of an unstitched result has one face: the places where the
                    // operands touch are the edges on which both lie tangentially
                    st.places = self.tangent_places(x, y, &r);
                }
                Ok(None)
            }
            Plausible::Wrong(w) => {
                st.wrong = true;
                st.why.get_or_insert_with(|| format!("solidcore returned a wrong result ({w})"));
                Ok(None)
            }
        }
    }

    /// Shells (outer shells and voids) of the solid regions of `s`.
    fn shell_count(&self, s: &Shape) -> usize {
        let b = s.brep.borrow();
        let Ok(regions) = solidcore::topology::brep::p1::regions(&b, None) else { return 0 };
        regions
            .into_iter()
            .filter_map(|r| b.region(r).ok().filter(|x| !x.is_infinite_region(&b, r) && x.is_void() == 0))
            .map(|x| {
                x.shells
                    .iter()
                    .filter(|&&sh| b.shell(sh).is_ok_and(solidcore::topology::Shell::is_faceuse_shell))
                    .count()
            })
            .sum()
    }

    /// Faces count and box of each shell of the solid regions of `s`.
    fn shell_boxes(&self, s: &Shape) -> Vec<(usize, DVec3, DVec3)> {
        let b = s.brep.borrow();
        let Ok(regions) = solidcore::topology::brep::p1::regions(&b, None) else { return Vec::new() };
        let mut out = Vec::new();
        for r in regions {
            let Ok(x) = b.region(r) else { continue };
            if x.is_infinite_region(&b, r) || x.is_void() != 0 {
                continue;
            }
            for &sh in &x.shells {
                let Ok(shell) = b.shell(sh) else { continue };
                if !shell.is_faceuse_shell() {
                    continue;
                }
                let Ok(fus) = shell.get_faceuses() else { continue };
                let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
                for fu in &fus {
                    let Some(f) = b.faceuse(*fu).ok().and_then(|u| u.face) else { continue };
                    if let Ok(bx) = q::face_bounding_box(&self.k, &b, f, false) {
                        lo = lo.min(DVec3::new(bx.min_at(0), bx.min_at(1), bx.min_at(2)));
                        hi = hi.max(DVec3::new(bx.max_at(0), bx.max_at(1), bx.max_at(2)));
                    }
                }
                out.push((fus.len(), lo, hi));
            }
        }
        out
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

    /// A point of each face of `s` whose box meets the box of `other` (the facade's rule; it
    /// takes 5 per face), of every face if `s` is the smaller operand, and at least one per
    /// shell of `s` (a body inside the other's box but away from its faces), with the outward
    /// normal there.
    fn near_samples(&mut self, s: &Shape, other: &Shape, eps: f64) -> Vec<(DVec3, DVec3)> {
        let Ok(Some(ob)) = self.bounds(other) else { return Vec::new() };
        // the smaller operand: every face (a tiny body kept or lost whole by mistake shows on
        // the faces outside the other's box)
        let smaller =
            self.bounds(s).ok().flatten().is_some_and(|(lo, hi)| (hi - lo).length() <= (ob.1 - ob.0).length());
        let mine = self.face_boxes(s);
        let mut b = s.brep.borrow_mut();
        // the shell (pair of shells of a face's two uses) of each face
        let shell_of = |f: solidcore::FaceId| -> Option<(u32, u32)> {
            let fu = b.face(f).ok()?.faceuse?;
            let u = b.faceuse(fu).ok()?;
            let x = u.shell.map_or(u32::MAX, |s| s.index());
            let y = u.mate.and_then(|m| b.faceuse(m).ok()?.shell).map_or(u32::MAX, |s| s.index());
            Some((x.min(y), x.max(y)))
        };
        let mut near: Vec<solidcore::FaceId> = Vec::new();
        let mut shells: Vec<((u32, u32), bool)> = Vec::new();
        for (f, fb) in &mine {
            let sh = shell_of(*f);
            let hit = smaller || boxes_overlap(*fb, ob, eps);
            if hit {
                near.push(*f);
            }
            match shells.iter_mut().find(|(k, _)| Some(*k) == sh) {
                Some(e) => e.1 |= hit,
                None => {
                    if let Some(k) = sh {
                        shells.push((k, hit));
                    }
                }
            }
        }
        // solidcore's point classification is slow (~14 ms a point, a box with 600 faces of
        // text: 200 s with the facade's 5 samples on every face): one sample per face, every
        // k-th face beyond MAX_FACES
        const MAX_FACES: usize = 64;
        let step = near.len().div_ceil(MAX_FACES).max(1);
        let mut pick: Vec<solidcore::FaceId> = near.into_iter().step_by(step).collect();
        for (k, hit) in shells {
            if !hit && let Some((f, _)) = mine.iter().find(|(f, _)| shell_of(*f) == Some(k)) {
                pick.push(*f);
            }
        }
        let mut out = Vec::new();
        for f in pick {
            if let Some(x) = crate::brep::face_sample(&self.k, &mut b, f) {
                out.push(x);
            }
        }
        out
    }

    /// Offset of the sample points from the faces: 1e-4 of the pair's size, at most 1e-3 of the
    /// smaller operand's (an operand smaller than the offset would be missed), but ten times
    /// solidcore's zone tolerance (closer, the classification says "on"), up to 2 % of the
    /// smaller operand.
    fn pair_eps(&self, ba: (DVec3, DVec3), bb: (DVec3, DVec3)) -> f64 {
        use solidcore::geom::tol::{SmallTopoType, zone_tol_3d};
        let size = (ba.0.min(bb.0) - ba.1.max(bb.1)).length();
        let small = (ba.1 - ba.0).length().min((bb.1 - bb.0).length());
        let floor = (10.0 * zone_tol_3d(&self.k, SmallTopoType::NormalSize)).min(2e-2 * small);
        (1e-4 * size).min(1e-3 * small).max(floor).max(1e-9)
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
        let eps = self.pair_eps(ba, bb);
        // solidcore's union of bodies that do not overlap (the pieces of a stencil glyph, also
        // with boxes apart) can return one of them a second time, inside out, as a void: the
        // volume checks above and the point checks below pass (the void's faces face the wrong
        // way), the volume is short by that body. Seen as more shells than both operands have
        // together, two of them with the same faces count and box (a void that closes a cavity
        // of the union lies inside an outer shell, never on one).
        if op == BoolOp::Union && !r_empty && self.shell_count(r) > self.shell_count(a) + self.shell_count(b) {
            let boxes = self.shell_boxes(r);

            let same = |x: &(usize, DVec3, DVec3), y: &(usize, DVec3, DVec3)| {
                x.0 == y.0 && (x.1 - y.1).abs().max_element() <= eps && (x.2 - y.2).abs().max_element() <= eps
            };
            if boxes.iter().enumerate().any(|(i, x)| boxes[i + 1..].iter().any(|y| same(x, y))) {
                return Ok(Plausible::Doubled(
                    "solidcore returned a wrong result (a body twice, once as a void)".into(),
                ));
            }
        }
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

    /// The edges of `r` (an unclosed result of `a op b`) along which `a` and `b` touch: the
    /// edge's middle lies on the boundary of both (within the pair's offset) and their surfaces
    /// there are tangent (normals parallel within 1e-6); an edge where the operands' faces cross
    /// at an angle is an ordinary intersection edge. At most 32, sampled.
    fn tangent_places(&mut self, a: &Shape, b: &Shape, r: &Shape) -> Vec<Vec<DVec3>> {
        let (Ok(Some(ba)), Ok(Some(bb))) = (self.bounds(a), self.bounds(b)) else { return Vec::new() };
        let eps = self.pair_eps(ba, bb);
        let Ok(es) = self.edges(r, 9) else { return Vec::new() };
        let mut out = Vec::new();
        for e in es {
            let m = e.points[e.points.len() / 2];
            let (Some((da, na)), Some((db, nb))) = (self.nearest_normal(a, m), self.nearest_normal(b, m)) else {
                continue;
            };
            if da <= eps && db <= eps && na.dot(nb).abs() >= 1.0 - 1e-6 {
                out.push(e.points);
                if out.len() == 32 {
                    break;
                }
            }
        }
        out
    }

    /// The distance from `p` to the nearest face of `s` (its foot inside the face) and the
    /// outward normal there.
    fn nearest_normal(&mut self, s: &Shape, p: DVec3) -> Option<(f64, DVec3)> {
        let mut b = s.brep.borrow_mut();
        let mut best: Option<(f64, DVec3)> = None;
        for f in b.face_list().to_vec() {
            let Ok(Some(srf)) = solidcore::topology::face::p2::surface(&b, f) else { continue };
            let Ok(c) = q::surface_closest_point(&self.k, srf, crate::imp::pt(p)) else { continue };
            if best.is_some_and(|x| x.0 <= c.closest.distance) {
                continue;
            }
            let in_face = c.uvs.iter().any(|&uv| {
                q::face_classify_uv(&self.k, &mut b, f, uv).is_ok_and(|x| {
                    matches!(
                        x,
                        solidcore::intersect::point_class::PointClassType::Face
                            | solidcore::intersect::point_class::PointClassType::Edge
                    )
                })
            });
            if !in_face {
                continue;
            }
            let foot = crate::imp::dv(c.closest.point);
            if let Some(n) = crate::brep::outward_normal(&self.k, &b, f, foot) {
                best = Some((c.closest.distance, n));
            }
        }
        best
    }

    /// Do the interiors of `a` and `b` overlap (the facade's `interiors_overlap`)? A point just
    /// inside a face of one inside the other; else their intersection's volume.
    fn interiors_overlap(&mut self, a: &Shape, b: &Shape, tag: Tag) -> KResult<bool> {
        let (Some(ba), Some(bb)) = (self.bounds(a)?, self.bounds(b)?) else { return Ok(false) };
        let eps = self.pair_eps(ba, bb);
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
