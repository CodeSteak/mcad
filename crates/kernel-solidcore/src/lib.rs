//! [`cad_kernel::Kernel`] over [solidcore](https://github.com/tangency-labs/solidcore), the Rust
//! port of SMLib (plan section 2.3, milestone M8).
//!
//! solidcore has every unit ported, but is still experimental: some entry points stop at a call
//! that is not wired and return `SolidError::Unported`. This crate calls the public API where it
//! exists and maps
//!
//! * `SolidError::Unported` → [`KernelError::Unsupported`]`("solidcore: <op>: …")` (the message
//!   names the upstream function and the solidcore unit that blocks it),
//! * every operation solidcore has no entry point for, or that is not wired here →
//!   `Unsupported("solidcore: <op> (…)")`,
//! * entry points that answer but give wrong results at the pinned commit (transform, point
//!   classification) → `Unsupported` too: `transform` checks volume and centroid of its result,
//!   `classify` runs a self-test on a box once,
//! * other solidcore errors → [`KernelError::Failed`] / [`KernelError::InvalidInput`].
//!
//! So `mcad diff-kernels` can report which operations already work ("coverage"); at bba8d87:
//! primitives, rigid transforms, `extrude` and `revolve` of polygons, circles, closed
//! interpolating splines and sketch profiles (lines, arcs and B-splines; all curves built as
//! the SMLib facade builds them), `classify`,
//! `faces`, `edges`, `bounds`, `mass_props`, `is_solid`; booleans mostly (some results lose faces);
//! curved solids have volumes closer to the exact value than SMLib's. Queries work on the
//! shape's own body ([`ShapeData`]); before an operation changes a body it is copied with
//! `SmApiBrepCopy` ([`Inner::copy`]), never with `Brep::clone`, whose geometry has no owner.
//! Faces carry the tag
//! of the operation that made the shape (solidcore's attribute plumbing for provenance is not
//! wired here yet).
//!
//! Feature `solidcore` (opt-in): link solidcore (git dependency pinned to a commit). Without
//! it the crate compiles without solidcore and every operation is `Unsupported`.

// without solidcore the operations ignore their arguments
#![cfg_attr(not(feature = "solidcore"), allow(unused_variables))]

use cad_kernel::{
    BoolOp, Class, EdgeInfo, FaceId, FaceInfo, KResult, Kernel, KernelError, Loop, MassProps, Mesh, PathInterp,
    PlacedLoop, PlacedPath, Tag,
};
use glam::{DMat4, DVec2, DVec3};
use std::rc::Rc;

/// One solidcore B-rep and the tag of the operation that made it.
///
/// The body is owned by the shape and queried in place: solidcore's read-only queries
/// (`face_internal_point`, mass properties, classification) take `&mut Brep` only for their
/// caches, hence the `RefCell`. Operations that change a body (transform, boolean operands)
/// work on a copy ([`Inner::copy`]); face and edge ids always refer to this body.
#[derive(Debug)]
pub struct ShapeData {
    #[cfg(feature = "solidcore")]
    pub brep: std::cell::RefCell<solidcore::Brep>,
    /// Mass properties, once computed (the shape never changes).
    #[cfg(feature = "solidcore")]
    mass: std::cell::Cell<Option<MassProps>>,
    pub tag: Tag,
}

impl ShapeData {
    #[cfg(feature = "solidcore")]
    fn new(brep: solidcore::Brep, tag: Tag) -> Shape {
        Rc::new(ShapeData { brep: std::cell::RefCell::new(brep), mass: std::cell::Cell::new(None), tag })
    }
}

pub type Shape = Rc<ShapeData>;

/// The commit of solidcore this crate is built against.
pub const SOLIDCORE_REV: &str = "bba8d8798122e9e5adc69c9ba153b8411dca24be";

/// [`Kernel::id`] of [`SolidcoreKernel`] (part of every cache key): `solidcore-` and the short
/// [`SOLIDCORE_REV`].
pub const KERNEL_ID: &str = "solidcore-bba8d87";

fn unsupported(op: &str) -> KernelError {
    KernelError::Unsupported(format!("solidcore: {op}"))
}

#[cfg(feature = "solidcore")]
mod imp {
    use super::*;
    use solidcore::curve::curve::Curve;
    use solidcore::error::EntityRef;
    use solidcore::geom::a_object::BooleanOperation;
    use solidcore::geom::axis2_placement::Axis2Placement;
    use solidcore::geom::tol::{SmallTopoType, zone_tol_3d};
    use solidcore::intersect::point_class::PointClassType;
    use solidcore::mass::integrator::properties as mp;
    use solidcore::nurbs::b_spline_curve::BSplineCurveForm;
    use solidcore::primitive::api_curves as ac;
    use solidcore::primitive::api_primitives as ap;
    use solidcore::primitive::primitive_creation::SweepOptions;
    use solidcore::query::api_queries as q;
    use solidcore::query::brep::BrepQuery;
    use solidcore::sweep::api_brep as sw;
    use solidcore::topology::edge;
    use solidcore::topology::proto_topology::p1::api_transform::{TransformObject, transform_independent_object};
    use solidcore::{SolidError, Vec3};

    pub fn err(op: &str, e: SolidError) -> KernelError {
        if e.is_unported() {
            KernelError::Unsupported(format!("solidcore: {op}: {e}"))
        } else if matches!(e, SolidError::InvalidInput { .. }) {
            KernelError::InvalidInput(format!("solidcore: {op}: {e}"))
        } else {
            KernelError::failed(format!("solidcore: {op}: {e}"))
        }
    }

    pub fn pt(p: DVec3) -> solidcore::Point3 {
        solidcore::Point3::new(p.x, p.y, p.z)
    }

    /// An error of a check that uses another operation: unsupported stays unsupported.
    pub fn check_err(op: &str, e: KernelError) -> KernelError {
        match e {
            KernelError::Unsupported(m) => KernelError::Unsupported(format!("{m} (needed by {op})")),
            e => e,
        }
    }

    pub fn dv(p: solidcore::Point3) -> DVec3 {
        DVec3::new(p.x, p.y, p.z)
    }

    pub struct Inner {
        pub k: solidcore::Kernel,
        /// Result of the classification self-test ([`Inner::classify`]), once run.
        classify_check: Option<Result<(), String>>,
        /// Last caller key domain handed to a copy ([`Inner::copy`]); also the number of copies.
        copies: u64,
        /// Time spent in [`Inner::copy`].
        copy_time: std::time::Duration,
    }

    /// Caller identity of `e` in a B-rep copied under `domain` (bits 36 and up; the low
    /// 36 bits are kind and index as in the kernel's own topology keys).
    fn copy_key(e: solidcore::EntityRef, domain: u64) -> solidcore::geom::object::ObjectRef {
        let r = solidcore::topology::brep::p3::topology_ref(0, e);
        solidcore::geom::object::ObjectRef {
            type_code: r.type_code,
            key: solidcore::geom::attribute::ObjectKey((domain << 36) | (r.key.0 & ((1 << 36) - 1))),
        }
    }

    /// Is the linear part of `f` a similarity (rotation and uniform scale) in the profile
    /// plane? Then arcs stay circular arcs.
    fn similar_in_plane(f: &DMat4) -> bool {
        let (a, b) = (f.x_axis.truncate(), f.y_axis.truncate());
        let (la, lb) = (a.length(), b.length());
        la > 0.0 && (la - lb).abs() <= 1e-7 * la && a.dot(b).abs() <= 1e-9 * la * la
    }

    /// One segment of a composite loop as the SMLib facade hands it to SMLib
    /// (`smlib/facade/sc_sketch.cpp`): a line, or a B-spline (arcs as rational quadratic
    /// B-splines of the circular-arc form, pieces of at most 90°).
    enum SkSeg {
        Line(DVec2, DVec2),
        Spline { arc: bool, c: cad_kernel::brep::BSplineCurve },
    }

    impl SkSeg {
        fn start(&self) -> DVec2 {
            match self {
                SkSeg::Line(a, _) => *a,
                SkSeg::Spline { c, .. } => c.points[0].truncate(),
            }
        }

        fn end(&self) -> DVec2 {
            match self {
                SkSeg::Line(_, b) => *b,
                SkSeg::Spline { c, .. } => c.points[c.points.len() - 1].truncate(),
            }
        }

        fn set_start(&mut self, p: DVec2) {
            match self {
                SkSeg::Line(a, _) => *a = p,
                SkSeg::Spline { c, .. } => c.points[0] = p.extend(0.0),
            }
        }

        /// The solidcore curve, its points mapped by `frame` (exact for B-splines under any
        /// affine map; an arc under a non-uniform map is an elliptic arc).
        fn curve(&self, frame: &DMat4, op: &str) -> KResult<Curve> {
            let p3 = |p: DVec3| pt(frame.transform_point3(DVec3::new(p.x, p.y, 0.0)));
            match self {
                SkSeg::Line(a, b) => Ok(Curve::from(
                    ac::create_line_segment(p3(a.extend(0.0)), p3(b.extend(0.0))).map_err(|e| err(op, e))?,
                )),
                SkSeg::Spline { arc, c } => {
                    let form = match (arc, similar_in_plane(frame)) {
                        (false, _) => BSplineCurveForm::Unspecified,
                        (true, true) => BSplineCurveForm::CircularArc,
                        (true, false) => BSplineCurveForm::EllipticArc,
                    };
                    Ok(Curve::from(bspline(c, p3, form, op)?))
                }
            }
        }
    }

    /// A solidcore B-spline from the neutral one, control points mapped by `map`.
    fn bspline(
        c: &cad_kernel::brep::BSplineCurve,
        map: impl Fn(DVec3) -> solidcore::Point3,
        form: BSplineCurveForm,
        op: &str,
    ) -> KResult<solidcore::nurbs::b_spline_curve::BSplineCurve> {
        let points: Vec<solidcore::Point3> = c.points.iter().map(|p| map(*p)).collect();
        let (mut knots, mut mult) = (Vec::<f64>::new(), Vec::<usize>::new());
        for (i, &k) in c.knots.iter().enumerate() {
            if i > 0 && k == c.knots[i - 1] {
                *mult.last_mut().expect("knot") += 1;
            } else {
                knots.push(k);
                mult.push(1);
            }
        }
        // as the facade: rational only if the weights differ
        let weights = c.weights.as_deref().filter(|w| w.iter().any(|v| (v - w[0]).abs() > 1e-14 * w[0].abs()));
        ac::create_canonical_curve(&ac::CanonicalCurveParams {
            points: &points,
            knots: &knots,
            knot_multiplicities: &mult,
            weights,
            degree: c.degree as usize,
            form,
        })
        .map_err(|e| err(op, e))
    }

    /// The segments of a composite loop (`Loop::Composite`) or an open sketch path, checked
    /// and welded as the SMLib facade does (`sketch_segments`, `weld`): every segment starts
    /// exactly where the previous one ends (`closed`: the first where the last ends).
    fn sketch_segments(segs: &[cad_kernel::Segment], closed: bool, op: &str) -> KResult<Vec<SkSeg>> {
        use cad_kernel::Segment;
        let bad = |m: &str| KernelError::InvalidInput(format!("solidcore: {op}: {m}"));
        if segs.is_empty() {
            return Err(bad("sketch loop without segments"));
        }
        let mut out = Vec::with_capacity(segs.len());
        for s in segs {
            out.push(match s {
                Segment::Line { a, b } => SkSeg::Line(*a, *b),
                Segment::Arc { r, a0, a1, .. } => {
                    if !(r.is_finite() && *r > 0.0) {
                        return Err(bad("sketch arc radius must be > 0"));
                    }
                    let sweep = (a1 - a0).abs();
                    if !(1e-12..=std::f64::consts::TAU + 1e-9).contains(&sweep) {
                        return Err(bad("sketch arc sweep must be in (0, 2pi]"));
                    }
                    SkSeg::Spline { arc: true, c: s.to_nurbs() }
                }
                Segment::Nurbs(c) => {
                    let n = c.points.len();
                    if c.degree < 1
                        || c.degree > 9
                        || n < c.degree as usize + 1
                        || c.knots.len() != n + c.degree as usize + 1
                    {
                        return Err(bad("bad sketch B-spline"));
                    }
                    if c.weights
                        .as_ref()
                        .is_some_and(|w| w.len() != n || w.iter().any(|v| !(v.is_finite() && *v > 0.0)))
                    {
                        return Err(bad("sketch B-spline weights must be > 0"));
                    }
                    if c.knots.windows(2).any(|w| w[1] < w[0]) {
                        return Err(bad("sketch B-spline knots must not decrease"));
                    }
                    SkSeg::Spline { arc: false, c: c.clone() }
                }
            });
        }
        let ext = out.iter().map(|s| s.start().length().max(s.end().length())).fold(1.0, f64::max);
        let n = out.len();
        for k in 0..n {
            if k == 0 && !closed {
                continue;
            }
            let e = out[(k + n - 1) % n].end();
            if e.distance(out[k].start()) > 1e-6 * ext {
                return Err(bad(&format!("sketch segments do not meet (segment {k})")));
            }
            out[k].set_start(e);
        }
        Ok(out)
    }

    /// The points of a spline loop as the SMLib facade takes them (`parse_loops`): without
    /// repeated consecutive points and without a last point equal to the first; at least 3.
    fn spline_points(ps: &[DVec2], op: &str) -> KResult<Vec<DVec2>> {
        let mut out: Vec<DVec2> = Vec::with_capacity(ps.len());
        for p in ps {
            if !p.is_finite() {
                return Err(KernelError::InvalidInput(format!("solidcore: {op}: non-finite profile point")));
            }
            if out.last().is_none_or(|q| q.distance(*p) >= 1e-12) {
                out.push(*p);
            }
        }
        while out.len() > 1 && out[0].distance(out[out.len() - 1]) < 1e-12 {
            out.pop();
        }
        if out.len() < 3 {
            return Err(KernelError::InvalidInput(format!(
                "solidcore: {op}: profile needs at least 3 distinct points"
            )));
        }
        Ok(out)
    }

    /// Degree-1 B-spline through `pts` of the polyline form, clamped, chord-length knots
    /// (the facade's `polyline_curve`).
    fn polyline_curve(pts: &[DVec3], closed: bool, op: &str) -> KResult<Curve> {
        let mut p: Vec<solidcore::Point3> = pts.iter().map(|q| pt(*q)).collect();
        if closed {
            p.push(p[0]);
        }
        let mut knots = Vec::with_capacity(p.len());
        let mut t = 0.0;
        for i in 0..p.len() {
            if i > 0 {
                t += dv(p[i]).distance(dv(p[i - 1])).max(1e-12);
            }
            knots.push(t);
        }
        let mut mult = vec![1; p.len()];
        mult[0] = 2;
        mult[p.len() - 1] = 2;
        let c = ac::create_canonical_curve(&ac::CanonicalCurveParams {
            points: &p,
            knots: &knots,
            knot_multiplicities: &mult,
            weights: None,
            degree: 1,
            form: BSplineCurveForm::PolylineForm,
        })
        .map_err(|e| err(op, e))?;
        Ok(Curve::from(c))
    }

    /// Cubic B-spline interpolating `pts` with chord-length parameters, as the SMLib facade
    /// makes it (`interp_curve`): `SmApiCreateCurveInterpPoints` is outside solidcore's scope,
    /// but the `SmBSplineCurve::CreateInterpolatingCurve` it calls is ported, so the curve is
    /// SMLib's (control points and knots agree with SMLib's and with
    /// `cad_kernel::curve2::interpolate` to rounding). `closed`: through the first point again
    /// (a closed clamped curve, C0 there). An open run of 3 points gets two more on the
    /// parabola through them, fewer than 4 points give a polyline.
    fn interp_curve(pts: &[DVec3], closed: bool, op: &str) -> KResult<Curve> {
        let mut p = pts.to_vec();
        if closed {
            p.push(pts[0]);
        }
        if !closed && p.len() == 3 {
            let t1 = p[1].distance(p[0]);
            let t2 = t1 + p[2].distance(p[1]);
            let q = |t: f64| {
                let l0 = (t - t1) * (t - t2) / (t1 * t2);
                let l1 = t * (t - t2) / (t1 * (t1 - t2));
                let l2 = t * (t - t1) / (t2 * (t2 - t1));
                p[0] * l0 + p[1] * l1 + p[2] * l2
            };
            p = vec![p[0], q(t1 / 2.0), p[1], q((t1 + t2) / 2.0), p[2]];
        }
        if p.len() < 4 {
            return polyline_curve(pts, closed, op);
        }
        Ok(Curve::from(interp_spline(&p, op)?))
    }

    /// `SmApiCreateCurveInterpPoints(points, SM_CP_CHORDLENGTH)`: the cubic through `p` (at
    /// least 4 distinct consecutive points).
    pub(crate) fn interp_spline(p: &[DVec3], op: &str) -> KResult<solidcore::nurbs::b_spline_curve::BSplineCurve> {
        if p.windows(2).any(|w| w[0].distance_squared(w[1]) <= 1e-24) {
            return Err(KernelError::InvalidInput(format!("solidcore: {op}: coincident spline points")));
        }
        let p: Vec<solidcore::Point3> = p.iter().map(|q| pt(*q)).collect();
        solidcore::nurbs::b_spline_curve::BSplineCurve::create_interpolating_curve(
            solidcore::nurbs::crv_fit::p1::ParamMethod::ChordLength,
            3,
            3,
            &p,
            &[],
            None,
            false,
        )
        .map_err(|e| err(op, e))
    }

    impl Inner {
        pub fn new() -> Self {
            Inner {
                k: solidcore::Kernel::default(),
                classify_check: None,
                copies: 0,
                copy_time: std::time::Duration::ZERO,
            }
        }

        /// An independent copy (`SmApiBrepCopy`): `Brep::clone` leaves the curves and surfaces
        /// without owner, so transforms and face queries of the clone miss its geometry.
        pub fn copy(&mut self, b: &solidcore::Brep) -> KResult<solidcore::Brep> {
            use solidcore::topology::brep::{p2, p3};
            let t0 = std::time::Instant::now();
            self.copies += 1;
            let domain = self.copies;
            let src = b.key().0;
            let src_domain = if solidcore::geom::context::is_reserved_key(b.key()) { 0 } else { src >> 36 };
            let mut state = p2::ArenaNotifyState::new(&solidcore::Brep::new());
            let mut env = state.env(None);
            let key_of = |e| copy_key(e, domain);
            env.key_of = &key_of;
            let r = p3::api_brep_copy(&mut self.k, &mut env, Some(b), &|e| copy_key(e, src_domain))
                .map_err(|e| err("copy", e));
            self.copy_time += t0.elapsed();
            r
        }

        pub fn copy_stats(&self) -> (u64, std::time::Duration) {
            (self.copies, self.copy_time)
        }

        fn wrap(r: Result<solidcore::Brep, SolidError>, op: &str, tag: Tag) -> KResult<Shape> {
            r.map(|brep| ShapeData::new(brep, tag)).map_err(|e| err(op, e))
        }

        pub fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Shape> {
            let d = max - min;
            Self::wrap(ap::create_box(&mut self.k, pt(min), d.x, d.y, d.z), "make_box", tag)
        }

        pub fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Shape> {
            Self::wrap(ap::create_sphere(&mut self.k, pt(DVec3::ZERO), r), "make_sphere", tag)
        }

        pub fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Shape> {
            if (r1 - r2).abs() <= 1e-12 * r1.max(r2) {
                Self::wrap(ap::create_cylinder(&mut self.k, pt(DVec3::ZERO), r1, h), "make_cylinder", tag)
            } else if r1 > 0.0 {
                Self::wrap(ap::create_cone(&mut self.k, pt(DVec3::ZERO), r1, r2, h), "make_cylinder (cone)", tag)
            } else {
                // as the SMLib facade: the cone with its tip on top, turned over
                let s =
                    Self::wrap(ap::create_cone(&mut self.k, pt(DVec3::ZERO), r2, r1, h), "make_cylinder (cone)", tag)?;
                let m = DMat4::from_translation(DVec3::new(0.0, 0.0, h)) * DMat4::from_rotation_x(std::f64::consts::PI);
                self.transform(&s, &m)
            }
        }

        pub fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Shape> {
            Self::wrap(ap::create_torus(&mut self.k, pt(DVec3::ZERO), big_r, r), "make_torus", tag)
        }

        /// `m` as solidcore takes it: a scale along the body axes, then a rigid placement
        /// (`SmBrep::Transform(const SmAxis2Placement&, scale)`); `None` for the identity. A
        /// mirror is a negative scale along x. Shear has no counterpart (the 4x4 matrix
        /// overload of `SmBrep::Transform` is not ported).
        fn placement(m: &DMat4) -> KResult<Option<(Axis2Placement, Option<Vec3>)>> {
            let a = m.to_cols_array();
            if a.iter().any(|v| !v.is_finite()) {
                return Err(KernelError::InvalidInput("solidcore: transform: non-finite matrix".into()));
            }
            if a[3].abs() > 1e-15 || a[7].abs() > 1e-15 || a[11].abs() > 1e-15 || (a[15] - 1.0).abs() > 1e-15 {
                return Err(unsupported("transform: projective matrix"));
            }
            if m.abs_diff_eq(DMat4::IDENTITY, 1e-15) {
                return Ok(None);
            }
            let c = [m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate()];
            let mut s = c.map(|v| v.length());
            if s.iter().any(|&l| l < 1e-12) || m.determinant().abs() < 1e-18 {
                return Err(KernelError::InvalidInput("solidcore: transform: singular matrix".into()));
            }
            for (i, j) in [(0, 1), (0, 2), (1, 2)] {
                if c[i].dot(c[j]).abs() > 1e-9 * s[i] * s[j] {
                    return Err(unsupported(
                        "transform: shear (SmBrep::Transform with a 4x4 matrix is not ported, \
                         only a rigid placement after a scale along the body axes)",
                    ));
                }
            }
            let mut x = c[0] / s[0];
            let y = c[1] / s[1];
            if m.determinant() < 0.0 {
                x = -x;
                s[0] = -s[0];
            }
            let t = m.w_axis.truncate();
            let pl = Axis2Placement::new(pt(t), Vec3::new(x.x, x.y, x.z), Vec3::new(y.x, y.y, y.z));
            let scale =
                if s.iter().all(|v| (v - 1.0).abs() <= 1e-12) { None } else { Some(Vec3::new(s[0], s[1], s[2])) };
            Ok(Some((pl, scale)))
        }

        pub fn transform(&mut self, s: &Shape, m: &DMat4) -> KResult<Shape> {
            let Some((pl, scale)) = Self::placement(m)? else {
                return Ok(s.clone());
            };
            let mut b = self.copy(&s.brep.borrow())?;
            if b.face_list().is_empty() {
                return Ok(ShapeData::new(b, s.tag));
            }
            let before = self.mass_props(s).map_err(|e| check_err("transform", e))?;
            transform_independent_object(&mut self.k, Some(TransformObject::Brep(&mut b)), &pl, scale)
                .map_err(|e| err("transform", e))?;
            let t = ShapeData::new(b, s.tag);
            // solidcore @ c9de85d returned success but moved only the vertices (on a
            // `Brep::clone`, whose geometry has no owner, it still does). Hand out only a result
            // whose volume and centroid moved with the matrix.
            let after = self.mass_props(&t).map_err(|e| match e {
                KernelError::Unsupported(_) => check_err("transform", e),
                e => unsupported(&format!("transform: the result fails the check ({e})")),
            })?;
            let want_c = m.transform_point3(before.centroid);
            let want_v = before.volume * m.determinant().abs();
            let tol = 1e-6 * (1.0 + want_c.length() + want_v.abs().cbrt());
            if (after.centroid - want_c).length() > tol
                || (after.volume - want_v).abs() > tol * tol.max(want_v.abs().cbrt())
            {
                return Err(unsupported(&format!(
                    "transform: SmBrep::Transform (unit topology.brep_p5) moved the vertices but not the \
                     curves and surfaces (centroid {:.6?}, expected {:.6?})",
                    after.centroid, want_c
                )));
            }
            Ok(t)
        }

        /// The closed loop as solidcore curves, its points mapped by `frame` (a circle is
        /// transformed as a curve).
        fn loop_curves(&mut self, lp: &Loop, frame: &DMat4, op: &str) -> KResult<Vec<Curve>> {
            let p3 = |p: DVec2| pt(frame.transform_point3(DVec3::new(p.x, p.y, 0.0)));
            match lp {
                Loop::Polygon(ps) => {
                    if ps.len() < 3 {
                        return Err(KernelError::InvalidInput(format!("solidcore: {op}: polygon with < 3 points")));
                    }
                    let mut out = Vec::with_capacity(ps.len());
                    for i in 0..ps.len() {
                        let (a, b) = (ps[i], ps[(i + 1) % ps.len()]);
                        if a == b {
                            continue;
                        }
                        out.push(Curve::from(ac::create_line_segment(p3(a), p3(b)).map_err(|e| err(op, e))?));
                    }
                    Ok(out)
                }
                Loop::Circle { center, r } => {
                    let c = ac::create_analytic_circle(&self.k, pt(DVec3::new(center.x, center.y, 0.0)), *r)
                        .map_err(|e| err(op, e))?;
                    let mut c = Curve::from(c);
                    if let Some((pl, scale)) = Self::placement(frame)? {
                        transform_independent_object(&mut self.k, Some(TransformObject::Curve(&mut c)), &pl, scale)
                            .map_err(|e| err(op, e))?;
                    }
                    Ok(vec![c])
                }
                Loop::Spline(ps) => {
                    let pts: Vec<DVec3> = spline_points(ps, op)?.iter().map(|p| dv(p3(*p))).collect();
                    Ok(vec![interp_curve(&pts, true, op)?])
                }
                Loop::Composite(segs) => {
                    let segs = sketch_segments(segs, true, op)?;
                    segs.iter().map(|s| s.curve(frame, op)).collect()
                }
            }
        }

        pub fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Shape> {
            if !h.is_finite() || h == 0.0 {
                return Err(KernelError::InvalidInput("solidcore: extrude: height must be non-zero".into()));
            }
            if twist.abs() >= 1e-12 || (scale - DVec2::ONE).abs().max_element() >= 1e-12 {
                return Err(unsupported(
                    "extrude: twist or scale (a skin through sections, SmApiCreateSkin; not wired in kernel-solidcore)",
                ));
            }
            // as the SMLib facade: sweep in the profile plane, then place the solid
            let curves = self.loop_curves(&p.lp, &DMat4::IDENTITY, "extrude")?;
            let refs: Vec<Option<&Curve>> = curves.iter().map(Some).collect();
            let b = sw::create_linear_sweep(&mut self.k, &refs, Vec3::new(0.0, 0.0, h.signum()), h.abs(), true)
                .map_err(|e| err("extrude", e))?;
            let s = ShapeData::new(b, tag);
            self.transform(&s, &p.frame)
        }

        /// The profile frame of a sweep, as the SMLib facade (`sc_sweep`): origin at the path
        /// start `p0`, z along the start tangent `z`, x the global X projected into the normal
        /// plane (global Y if the tangent is nearly parallel to X).
        fn sweep_frame(p0: DVec3, z: DVec3) -> DMat4 {
            let r = if z.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            let x = (r - z * r.dot(z)).normalize();
            DMat4::from_cols(x.extend(0.0), z.cross(x).extend(0.0), z.extend(0.0), p0.extend(1.0))
        }

        /// Is the centre of the profile curves' bounding box at `p0`? SMLib's mitered sweep
        /// along a planar path moves that centre along the path (the curve sweep the frame
        /// origin, smlib/patches/0003, here `SweepOptions::profile_at_path_start`).
        fn profile_centred(&self, prof: &[Curve], p0: DVec3) -> KResult<bool> {
            use solidcore::nurbs::b_spline_curve::BoundingBoxRequest;
            let req = BoundingBoxRequest { normal: true, pseudo: false, polar: false };
            let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
            for c in prof {
                let b = c
                    .calculate_bounding_box(&self.k, &c.natural_interval(), req, true)
                    .map_err(|e| err("sweep", e))?
                    .normal
                    .ok_or_else(|| KernelError::failed("solidcore: sweep: profile curve without a box"))?;
                lo = lo.min(DVec3::new(b.min_at(0), b.min_at(1), b.min_at(2)));
                hi = hi.max(DVec3::new(b.max_at(0), b.max_at(1), b.max_at(2)));
            }
            Ok(((lo + hi) / 2.0).distance(p0) <= 1e-9 * hi.distance(lo).max(1.0))
        }

        /// The sweep itself, as the SMLib facade: `segments` (a path with corners, planar):
        /// mitered joints (`SmApiCreateSweepAlongPlanarPath`); otherwise the curve sweep
        /// along `path` (`SmApiCreateCurveSweep`) with the profile where it lies.
        fn sweep_curves(
            &mut self,
            prof: &[Curve],
            path: &Curve,
            segments: Option<&[Curve]>,
            p0: DVec3,
            polygon: bool,
            tag: Tag,
        ) -> KResult<Shape> {
            const OP: &str = "sweep";
            if segments.is_some() && !self.profile_centred(prof, p0)? {
                return Err(unsupported(
                    "sweep: profile not centred on its origin along a path with corners (SMLib's mitered \
                     sweep moves the profile's box centre along the path; use a spline path)",
                ));
            }
            let b = match segments {
                Some(segs) => {
                    let pr: Vec<&Curve> = prof.iter().collect();
                    let sg: Vec<&Curve> = segs.iter().collect();
                    sw::create_sweep_along_planar_path(&mut self.k, &pr, &sg, true, 3).map_err(|e| err(OP, e))?
                }
                None => {
                    // smlib/patches/0003 (bUseProfileRefPoint with the frame origin, which lies
                    // at the path start): the profile is swept where it lies
                    let opt = SweepOptions {
                        cap_ends: true,
                        move_profile_center_to_path: true,
                        profile_at_path_start: true,
                        ..SweepOptions::default()
                    };
                    let pr: Vec<Option<&Curve>> = prof.iter().map(Some).collect();
                    sw::create_curve_sweep(&mut self.k, &pr, Some(path), None, None, Some(&opt))
                        .map_err(|e| err(OP, e))?
                        .brep
                }
            };
            let s = ShapeData::new(b, tag);
            // as the facade's require_solid: a closed solid of positive volume
            if self.is_empty(&s) || !self.is_solid(&s) {
                let what = if polygon { " (polygon profile: open seams between the side faces?)" } else { "" };
                return Err(KernelError::failed(format!("solidcore: sweep did not produce a closed solid{what}")));
            }
            let v = self.mass_props(&s).map_err(|e| check_err(OP, e))?.volume;
            if v.is_nan() || v <= 0.0 {
                return Err(KernelError::failed(format!("solidcore: sweep produced an invalid solid (volume {v})")));
            }
            Ok(s)
        }

        /// `sweep` along a polyline or an interpolating spline through `path`, as the SMLib
        /// facade (`sc_sweep`): a collinear path is one straight segment; a polyline with
        /// corners must be planar.
        pub fn sweep(&mut self, lp: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Shape> {
            const OP: &str = "sweep";
            let mut pp: Vec<DVec3> = Vec::with_capacity(path.len());
            for p in path {
                if !p.is_finite() {
                    return Err(KernelError::InvalidInput("solidcore: sweep: bad path point".into()));
                }
                if pp.last().is_none_or(|q| q.distance(*p) >= 1e-12) {
                    pp.push(*p);
                }
            }
            if pp.len() < 2 {
                return Err(KernelError::InvalidInput(
                    "solidcore: sweep: path needs at least 2 distinct points".into(),
                ));
            }
            let plen: f64 = pp.windows(2).map(|w| w[0].distance(w[1])).sum();
            let ptol = 1e-9 * plen.max(1.0);
            let d = (pp[pp.len() - 1] - pp[0]).normalize_or_zero();
            let straight = pp.iter().all(|p| {
                let q = *p - pp[0];
                (q - d * q.dot(d)).length() <= ptol
            });
            if straight {
                pp = vec![pp[0], pp[pp.len() - 1]];
            }
            let mut planar = true;
            if !straight {
                let n = (1..pp.len() - 1)
                    .map(|i| (pp[i] - pp[0]).cross(pp[i + 1] - pp[0]))
                    .find(|c| c.length() > ptol * plen)
                    .map_or(DVec3::ZERO, |c| c.normalize());
                planar = pp.iter().all(|p| (*p - pp[0]).dot(n).abs() <= 1e-7 * plen.max(1.0));
            }
            let spline = interp == PathInterp::Spline && pp.len() >= 3 && !straight;
            if !spline && !straight && !planar {
                return Err(unsupported("sweep: along a non-planar polyline (as SMLib; use a spline path)"));
            }
            let path_curve = if spline { interp_curve(&pp, false, OP)? } else { polyline_curve(&pp, false, OP)? };
            // start tangent: towards the first control point after the start (clamped curve)
            let z = match path_curve.kind() {
                solidcore::curve::curve::CurveKind::BSpline(c) => {
                    let (cp, _) = c.control_polygon().map_err(|e| err(OP, e))?;
                    cp.iter().map(|q| dv(*q) - pp[0]).find(|v| v.length() > 1e-12).map(|v| v.normalize())
                }
                _ => None,
            }
            .unwrap_or_else(|| (pp[1] - pp[0]).normalize());
            let prof = self.loop_curves(lp, &Self::sweep_frame(pp[0], z), OP)?;
            let segments = if straight || spline {
                None
            } else {
                let mut segs = Vec::with_capacity(pp.len() - 1);
                for w in pp.windows(2) {
                    segs.push(Curve::from(ac::create_line_segment(pt(w[0]), pt(w[1])).map_err(|e| err(OP, e))?));
                }
                Some(segs)
            };
            let polygon = matches!(lp, Loop::Polygon(_)) && !straight;
            self.sweep_curves(&prof, &path_curve, segments.as_deref(), pp[0], polygon, tag)
        }

        /// `sweep` along an open sketch path, as kernel-smlib and the facade
        /// (`sc_sketch_sweep`): a tangent-continuous path is one B-spline (curve sweep), a
        /// path with corners is swept segment by segment with mitered joints.
        pub fn sweep_path(&mut self, lp: &Loop, path: &PlacedPath, tag: Tag) -> KResult<Shape> {
            const OP: &str = "sweep";
            if path.segs.is_empty() {
                return Err(KernelError::InvalidInput("solidcore: sweep path without segments".into()));
            }
            let smooth = path.segs.windows(2).all(|w| w[0].end_tangent().dot(w[1].start_tangent()) > 1.0 - 1e-9);
            let (segs, mitered) = if path.segs.len() == 1 {
                (path.segs.clone(), false)
            } else if smooth {
                (vec![cad_kernel::Segment::Nurbs(cad_kernel::curve2::join(&path.segs))], false)
            } else {
                (path.segs.clone(), true)
            };
            let sk = sketch_segments(&segs, false, OP)?;
            let curves: Vec<Curve> = sk.iter().map(|s| s.curve(&path.frame, OP)).collect::<KResult<_>>()?;
            let p0 = path.frame.transform_point3(sk[0].start().extend(0.0));
            // start tangent of the first segment (B-splines: towards the first distinct
            // control point)
            let t = match &sk[0] {
                SkSeg::Line(a, b) => *b - *a,
                SkSeg::Spline { c, .. } => c
                    .points
                    .iter()
                    .map(|q| q.truncate() - c.points[0].truncate())
                    .find(|v| v.length() > 1e-12)
                    .unwrap_or_default(),
            };
            let z = path.frame.transform_vector3(t.extend(0.0)).normalize_or_zero();
            if z == DVec3::ZERO {
                return Err(KernelError::InvalidInput("solidcore: sweep path starts with a zero tangent".into()));
            }
            let prof = self.loop_curves(lp, &Self::sweep_frame(p0, z), OP)?;
            let polygon = matches!(lp, Loop::Polygon(_));
            if mitered {
                self.sweep_curves(&prof, &curves[0], Some(&curves), p0, polygon, tag)
            } else {
                self.sweep_curves(&prof, &curves[0], None, p0, polygon, tag)
            }
        }

        pub fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
            if !angle.is_finite() || angle == 0.0 {
                return Err(KernelError::InvalidInput("solidcore: revolve: angle must be non-zero".into()));
            }
            let deg = angle.to_degrees().clamp(-360.0, 360.0);
            let curves = self.revolve_curves(p)?;
            self.check_revolve_profile(&curves)?;
            let refs: Vec<Option<&Curve>> = curves.iter().map(Some).collect();
            let b = sw::create_rotational_sweep(
                &mut self.k,
                &refs,
                pt(DVec3::ZERO),
                Vec3::new(0.0, 0.0, deg.signum()),
                deg.abs(),
                true,
            )
            .map_err(|e| err("revolve", e))?;
            Ok(ShapeData::new(b, tag))
        }

        /// Profile curves of a revolve, as the SMLib facade (`revolve_profile_curves`): a
        /// closed spline cannot pass through two points on the axis without crossing it, so
        /// two spline points that follow each other on the axis are joined by a line along
        /// the axis and the spline runs open between such lines.
        fn revolve_curves(&mut self, p: &PlacedLoop) -> KResult<Vec<Curve>> {
            const OP: &str = "revolve";
            let Loop::Spline(ps) = &p.lp else { return self.loop_curves(&p.lp, &p.frame, OP) };
            let pts: Vec<DVec3> =
                spline_points(ps, OP)?.iter().map(|q| p.frame.transform_point3(q.extend(0.0))).collect();
            let n = pts.len();
            let atol = 1e-9 * pts.iter().map(|q| q.length()).fold(1.0, f64::max);
            let on_axis = |i: usize| pts[i].x.hypot(pts[i].y) <= atol;
            let cuts: Vec<usize> = (0..n).filter(|&i| on_axis(i) && on_axis((i + 1) % n)).collect();
            if cuts.is_empty() {
                return self.loop_curves(&p.lp, &p.frame, OP);
            }
            let mut out = Vec::new();
            for (c, &a) in cuts.iter().enumerate() {
                let b = (a + 1) % n;
                out.push(Curve::from(ac::create_line_segment(pt(pts[a]), pt(pts[b])).map_err(|e| err(OP, e))?));
                // open run from b to the start of the next axis segment
                let e = cuts[(c + 1) % cuts.len()];
                let mut run = vec![pts[b]];
                let mut i = b;
                while i != e {
                    i = (i + 1) % n;
                    run.push(pts[i]);
                }
                if run.len() >= 2 {
                    out.push(interp_curve(&run, false, OP)?);
                }
            }
            Ok(out)
        }

        /// As the SMLib facade (`revolve_profile_curves`): a profile curve that reaches x < 0
        /// (257 samples per curve) crosses the axis and is `InvalidInput`.
        fn check_revolve_profile(&self, curves: &[Curve]) -> KResult<()> {
            const K: u32 = 256;
            let mut pts = Vec::with_capacity(curves.len() * (K as usize + 1));
            for c in curves {
                let iv = c.natural_interval();
                let (t0, t1) = (iv.min(), iv.max());
                for k in 0..=K {
                    if let Ok(p) = c.evaluate_point(&self.k, t0 + (t1 - t0) * f64::from(k) / f64::from(K)) {
                        pts.push(dv(p));
                    }
                }
            }
            let ext = pts.iter().map(|p| p.length()).fold(1.0, f64::max);
            match pts.iter().map(|p| p.x).reduce(f64::min) {
                Some(x) if x < -1e-7 * ext => Err(KernelError::InvalidInput(format!(
                    "solidcore: revolve: the profile curve crosses the axis (reaches x = {x} after its frame)"
                ))),
                _ => Ok(()),
            }
        }

        /// Point classification, once a self-test on a box passed: at c9de85d
        /// `SmBrep::Point3DClassify` puts points inside and on the faces of a box into the
        /// infinite region.
        pub fn classify(&mut self, s: &Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
            if self.classify_check.is_none() {
                let probe = [DVec3::splat(0.5), DVec3::new(2.0, 0.5, 0.5), DVec3::new(1.0, 0.5, 0.5)];
                let want = [Class::Inside, Class::Outside, Class::On];
                let r = self
                    .make_box(DVec3::ZERO, DVec3::ONE, 0)
                    .and_then(|b| self.classify_unchecked(&b, &probe))
                    .map_err(|e| e.to_string())
                    .and_then(|got| {
                        if got == want {
                            Ok(())
                        } else {
                            Err(format!(
                                "SmBrep::Point3DClassify (unit query.brep) gives {got:?} for points inside, \
                                 outside and on a unit box (self-test)"
                            ))
                        }
                    });
                self.classify_check = Some(r);
            }
            if let Some(Err(e)) = &self.classify_check {
                return Err(unsupported(&format!("classify: {e}")));
            }
            self.classify_unchecked(s, pts)
        }

        fn classify_unchecked(&mut self, s: &Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
            let mut b = s.brep.borrow_mut();
            let b = &mut *b;
            if b.face_list().is_empty() {
                return Ok(vec![Class::Outside; pts.len()]);
            }
            let tol = zone_tol_3d(&self.k, SmallTopoType::NormalSize);
            let mut out = Vec::with_capacity(pts.len());
            for p in pts {
                let pc = BrepQuery::of_brep(b)
                    .point_3d_classify(&self.k, b, pt(*p), tol, true)
                    .map_err(|e| err("classify", e))?;
                out.push(match pc.point_class() {
                    // as the SMLib facade: inside a region that is not void (`IsVoid` 0)
                    PointClassType::Region => match pc.object() {
                        Some(EntityRef::Region(r)) => {
                            let void = b.region(r).map_err(|e| err("classify", e))?.is_void();
                            if void == 0 { Class::Inside } else { Class::Outside }
                        }
                        _ => Class::Outside,
                    },
                    PointClassType::Face | PointClassType::Edge | PointClassType::Vertex => Class::On,
                    _ => Class::Outside,
                });
            }
            Ok(out)
        }

        pub fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
            let op_sc = match op {
                BoolOp::Union => BooleanOperation::Union,
                BoolOp::Difference => BooleanOperation::Difference,
                BoolOp::Intersection => BooleanOperation::Intersection,
            };
            let mut it = shapes.iter();
            let first = it.next().ok_or_else(|| KernelError::InvalidInput("boolean without operands".into()))?;
            // the boolean consumes its operands: copies, the shapes stay as they are
            let mut acc = self.copy(&first.brep.borrow())?;
            for s in it {
                let other = self.copy(&s.brep.borrow())?;
                let out = solidcore::boolean::api_brep::boolean(&mut self.k, acc, other, op_sc)
                    .map_err(|e| err("boolean", e))?;
                acc = out.result;
            }
            Ok(ShapeData::new(acc, tag))
        }

        /// Sample points: an interior point (`SmApiFaceInternalPoint`) and the vertices of the
        /// boundary loops; only the vertices while the interior point is not ported.
        pub fn faces(&mut self, s: &Shape) -> KResult<Vec<FaceInfo>> {
            let mut b = s.brep.borrow_mut();
            let b = &mut *b;
            let fs = q::get_faces(b).map_err(|e| err("faces", e))?;
            let mut out = Vec::with_capacity(fs.len());
            for (i, f) in fs.into_iter().enumerate() {
                let mut points = Vec::new();
                match q::face_internal_point(&self.k, b, f) {
                    Ok(p) => points.push(dv(p)),
                    Err(e) if e.is_unported() => {}
                    Err(e) => return Err(err("faces", e)),
                }
                let loops = q::face_get_boundary_loops(b, f).map_err(|e| err("faces", e))?;
                for e in loops.edges {
                    for v in [edge::start_vertex(b, e), edge::end_vertex(b, e)] {
                        if let Some(v) = v.map_err(|e| err("faces", e))? {
                            let p = dv(q::vertex_get_point(b, v).map_err(|e| err("faces", e))?);
                            if !points.contains(&p) {
                                points.push(p);
                            }
                        }
                    }
                }
                if points.is_empty() {
                    return Err(unsupported("faces: no sample point (SmApiFaceInternalPoint not ported)"));
                }
                out.push(FaceInfo { id: i as FaceId, tag: s.tag, points });
            }
            Ok(out)
        }

        /// Start point, `samples` interior points of the curve, end point.
        pub fn edges(&mut self, s: &Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
            let b = s.brep.borrow();
            let b = &*b;
            let es = q::get_edges(b).map_err(|e| err("edges", e))?;
            let mut out = Vec::with_capacity(es.len());
            for (i, e) in es.into_iter().enumerate() {
                let ed = b.edge(e).map_err(|e| err("edges", e))?;
                let curve =
                    ed.curve.as_ref().ok_or_else(|| KernelError::failed("solidcore: edges: edge without curve"))?;
                let (t0, t1) = (ed.interval.min(), ed.interval.max());
                let at = |t: f64| curve.evaluate_point(&self.k, t).map(dv).map_err(|e| err("edges", e));
                let end = |v: Result<Option<solidcore::VertexId>, SolidError>, t: f64| -> KResult<DVec3> {
                    match v.map_err(|e| err("edges", e))? {
                        Some(v) => Ok(dv(q::vertex_get_point(b, v).map_err(|e| err("edges", e))?)),
                        None => at(t),
                    }
                };
                let mut points = vec![end(edge::start_vertex(b, e), t0)?];
                for j in 1..=samples {
                    points.push(at(t0 + (t1 - t0) * f64::from(j) / f64::from(samples + 1))?);
                }
                points.push(end(edge::end_vertex(b, e), t1)?);
                out.push(EdgeInfo { id: i as u32, points, face_tags: vec![s.tag, s.tag], smooth: false });
            }
            Ok(out)
        }

        pub fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
            if let Some(m) = s.mass.get() {
                return Ok(m);
            }
            let store = solidcore::geom::attribute::AttributeStore::new();
            let key_of = |_e: solidcore::EntityRef| solidcore::geom::attribute::ObjectKey(0);
            let attrs = mp::PropertyAttributes { store: &store, key_of: &key_of };
            let mut b = s.brep.borrow_mut();
            let m = mp::brep_mass_properties(&self.k, &mut b, 1e-6, 1.0, pt(DVec3::ZERO), &attrs)
                .map_err(|e| err("mass_props", e))?;
            let area = mp::brep_area(&self.k, &mut b, 1e-6).map_err(|e| err("mass_props (area)", e))?;
            let c = m.centroid;
            let m = MassProps { volume: m.properties.volume, area, centroid: DVec3::new(c.x, c.y, c.z) };
            s.mass.set(Some(m));
            Ok(m)
        }

        pub fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
            let b = s.brep.borrow();
            if b.face_list().is_empty() {
                return Ok(None);
            }
            let b = q::brep_bounding_box(&self.k, &b, true).map_err(|e| err("bounds", e))?;
            Ok(Some((
                DVec3::new(b.min_at(0), b.min_at(1), b.min_at(2)),
                DVec3::new(b.max_at(0), b.max_at(1), b.max_at(2)),
            )))
        }

        pub fn is_solid(&mut self, s: &Shape) -> bool {
            let b = s.brep.borrow();
            !b.face_list().is_empty() && q::brep_is_manifold_solid(&b).unwrap_or(false)
        }

        pub fn is_empty(&mut self, s: &Shape) -> bool {
            s.brep.borrow().face_list().is_empty()
        }
    }
}

/// Kernel over solidcore.
pub struct SolidcoreKernel {
    #[cfg(feature = "solidcore")]
    inner: imp::Inner,
}

impl std::fmt::Debug for SolidcoreKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SolidcoreKernel")
    }
}

impl Default for SolidcoreKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl SolidcoreKernel {
    pub fn new() -> Self {
        SolidcoreKernel {
            #[cfg(feature = "solidcore")]
            inner: imp::Inner::new(),
        }
    }

    /// `true` if built with solidcore (feature `solidcore`).
    pub fn available() -> bool {
        cfg!(feature = "solidcore")
    }

    /// Number of B-rep copies (`SmApiBrepCopy`) so far and the time they took (performance
    /// measurements of `mcad diff-kernels`).
    pub fn copy_stats(&self) -> (u64, std::time::Duration) {
        #[cfg(feature = "solidcore")]
        {
            self.inner.copy_stats()
        }
        #[cfg(not(feature = "solidcore"))]
        {
            (0, std::time::Duration::ZERO)
        }
    }
}

/// Dispatches to `imp` with feature `solidcore`, else `Unsupported`.
macro_rules! with_sc {
    ($self:ident, $op:literal, |$i:ident| $body:expr) => {{
        #[cfg(feature = "solidcore")]
        {
            let $i = &mut $self.inner;
            $body
        }
        #[cfg(not(feature = "solidcore"))]
        {
            Err(unsupported(concat!($op, " (built without feature `solidcore`)")))
        }
    }};
}

impl Kernel for SolidcoreKernel {
    type Shape = Shape;

    fn id(&self) -> &'static str {
        KERNEL_ID
    }

    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_box", |i| i.make_box(min, max, tag))
    }

    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_sphere", |i| i.make_sphere(r, tag))
    }

    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_cylinder", |i| i.make_cylinder(h, r1, r2, tag))
    }

    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_torus", |i| i.make_torus(big_r, r, tag))
    }

    fn make_polyhedron(&mut self, _points: &[DVec3], _faces: &[Vec<u32>], _tag: Tag) -> KResult<Shape> {
        Err(unsupported("make_polyhedron (poly B-reps, SmApiPoly*, are outside solidcore's scope: plan/closure.md)"))
    }

    fn transform(&mut self, s: &Shape, m: &DMat4) -> KResult<Shape> {
        with_sc!(self, "transform", |i| i.transform(s, m))
    }

    fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
        with_sc!(self, "boolean", |i| i.boolean(op, shapes, tag))
    }

    fn fillet(&mut self, _s: &Shape, _edges: &[u32], _r: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("fillet (no public entry point: SmApiFilletEdges is not ported)"))
    }

    fn chamfer(&mut self, _s: &Shape, _edges: &[u32], _d: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("chamfer (no public entry point: SmApiChamferFillet is not ported)"))
    }

    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "extrude", |i| i.extrude(p, h, twist, scale, tag))
    }

    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "revolve", |i| i.revolve(p, angle, tag))
    }

    fn loft(&mut self, _sections: &[PlacedLoop], _ruled: bool, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("loft (SmApiCreateSkin exists, not wired in kernel-solidcore)"))
    }

    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "sweep", |i| i.sweep(p, path, interp, tag))
    }

    fn sweep_path(&mut self, p: &Loop, path: &PlacedPath, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "sweep_path", |i| i.sweep_path(p, path, tag))
    }

    fn nurbs_surface(&mut self, _rows: &[Vec<DVec3>], _tag: Tag) -> KResult<Shape> {
        Err(unsupported("nurbs_surface (not wired in kernel-solidcore)"))
    }

    fn thicken(&mut self, _s: &Shape, _t: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("thicken (SmApiOffsetBrepFull exists, not wired in kernel-solidcore)"))
    }

    fn shell(&mut self, _s: &Shape, _t: f64, _open: &[FaceId], _tag: Tag) -> KResult<Shape> {
        Err(unsupported("shell (SmApiShellBrepFull stops at SmBrep::SmBrep, unit topology.brep_p3, at c9de85d)"))
    }

    fn offset(&mut self, _s: &Shape, _d: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("offset (not wired in kernel-solidcore)"))
    }

    fn edges(&mut self, s: &Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
        let _ = (s, samples);
        with_sc!(self, "edges", |i| i.edges(s, samples))
    }

    fn faces(&mut self, s: &Shape, _samples: u32) -> KResult<Vec<FaceInfo>> {
        let _ = s;
        with_sc!(self, "faces", |i| i.faces(s))
    }

    fn classify(&mut self, s: &Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
        with_sc!(self, "classify", |i| i.classify(s, pts))
    }

    fn tessellate(&mut self, _s: &Shape, _tol: f64) -> KResult<Mesh> {
        Err(unsupported("tessellate (outside solidcore's scope: SmApiTessellate, plan/closure.md)"))
    }

    fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
        let _ = s;
        with_sc!(self, "mass_props", |i| i.mass_props(s))
    }

    fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
        let _ = s;
        with_sc!(self, "bounds", |i| i.bounds(s))
    }

    fn is_empty(&mut self, s: &Shape) -> bool {
        #[cfg(feature = "solidcore")]
        {
            self.inner.is_empty(s)
        }
        #[cfg(not(feature = "solidcore"))]
        {
            let _ = s;
            true
        }
    }

    fn is_solid(&mut self, s: &Shape) -> bool {
        #[cfg(feature = "solidcore")]
        {
            self.inner.is_solid(s)
        }
        #[cfg(not(feature = "solidcore"))]
        {
            let _ = s;
            false
        }
    }
}

#[cfg(all(test, feature = "solidcore"))]
mod tests {
    use super::imp::*;
    use glam::{DVec2, DVec3};

    /// The interpolating spline of a closed spline profile is SMLib's: the same as
    /// `cad_kernel::curve2::interpolate` through the points and the first again (which agrees
    /// with SMLib's `SmApiCreateCurveInterpPoints` to 4e-15, checked through `Kernel::brep` of
    /// kernel-smlib).
    #[test]
    fn interpolating_spline_matches_curve2() {
        for pts in [
            vec![DVec2::new(5.0, 0.0), DVec2::new(0.0, 3.0), DVec2::new(-5.0, 0.0), DVec2::new(0.0, -3.0)],
            vec![DVec2::new(1.0, 0.0), DVec2::new(2.0, 1.0), DVec2::new(1.0, 2.0)],
            vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(10.0, 1.0),
                DVec2::new(12.0, 7.0),
                DVec2::new(3.0, 9.0),
                DVec2::new(-2.0, 4.0),
            ],
        ] {
            let mut closed = pts.clone();
            closed.push(pts[0]);
            let want = cad_kernel::curve2::interpolate(&closed, None, None).unwrap();
            let p3: Vec<DVec3> = closed.iter().map(|p| p.extend(0.0)).collect();
            let got = interp_spline(&p3, "test").unwrap();
            assert_eq!(got.degree().unwrap(), want.degree as usize);
            let (cp, w) = got.control_polygon().unwrap();
            assert!(w.iter().all(|w| (w - 1.0).abs() < 1e-15), "{w:?}");
            assert_eq!(cp.len(), want.points.len());
            for (a, b) in cp.iter().zip(&want.points) {
                assert!((dv(*a) - *b).length() < 1e-12, "{a:?} {b:?}");
            }
            let (ks, ms) = got.knots(None).unwrap();
            let full: Vec<f64> = ks.iter().zip(&ms).flat_map(|(k, m)| std::iter::repeat_n(*k, *m)).collect();
            assert_eq!(full.len(), want.knots.len());
            assert!(full.iter().zip(&want.knots).all(|(a, b)| (a - b).abs() < 1e-12), "{full:?} {:?}", want.knots);
        }
    }
}
