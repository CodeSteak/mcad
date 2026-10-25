//! [`cad_kernel::Kernel`] over [solidcore](https://github.com/tangency-labs/solidcore), the Rust
//! port of SMLib (plan section 2.3, milestone M8).
//!
//! solidcore is not usable end to end yet (its README: "no primitives, booleans or fillets"):
//! its `SM_API`-level entry points exist, but most stop at a unit that is not ported and return
//! `SolidError::Unported`. This crate calls the public API where it exists and maps
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
//! So `mcad diff-kernels` can report which operations already work ("coverage"); at d4940b6:
//! `make_box`, plain `extrude` of a polygon with an identity frame, `faces`, `edges`, `bounds`,
//! `mass_props`, `is_solid`; cylinders, cones, spheres, tori and `revolve` answer with the faces
//! and edges of SMLib, but volumes about 1e-5 off. Faces carry the tag of the operation that made the shape
//! (solidcore's attribute plumbing for provenance is not wired here yet).
//!
//! Feature `solidcore` (opt-in): link solidcore (git dependency pinned to a commit). Without
//! it the crate compiles without solidcore and every operation is `Unsupported`.

// without solidcore the operations ignore their arguments
#![cfg_attr(not(feature = "solidcore"), allow(unused_variables))]

use cad_kernel::{
    BoolOp, Class, EdgeInfo, FaceId, FaceInfo, KResult, Kernel, KernelError, Loop, MassProps, Mesh, PathInterp,
    PlacedLoop, Tag,
};
use glam::{DMat4, DVec2, DVec3};
use std::rc::Rc;

/// One solidcore B-rep and the tag of the operation that made it.
#[derive(Debug)]
pub struct ShapeData {
    #[cfg(feature = "solidcore")]
    pub brep: solidcore::Brep,
    pub tag: Tag,
}

pub type Shape = Rc<ShapeData>;

/// The commit of solidcore this crate is built against.
pub const SOLIDCORE_REV: &str = "d4940b63e9816c77a32a421bd9dfa89c7e63fa12";

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
    use solidcore::primitive::api_curves as ac;
    use solidcore::primitive::api_primitives as ap;
    use solidcore::query::api_queries as q;
    use solidcore::query::brep::{BrepClassifier, BrepIdentity};
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
    }

    impl Inner {
        pub fn new() -> Self {
            Inner { k: solidcore::Kernel::default(), classify_check: None }
        }

        fn wrap(r: Result<solidcore::Brep, SolidError>, op: &str, tag: Tag) -> KResult<Shape> {
            r.map(|brep| Rc::new(ShapeData { brep, tag })).map_err(|e| err(op, e))
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
            let mut b = s.brep.clone();
            if b.face_list().is_empty() {
                return Ok(Rc::new(ShapeData { brep: b, tag: s.tag }));
            }
            let before = self.mass_props(s).map_err(|e| check_err("transform", e))?;
            transform_independent_object(&mut self.k, Some(TransformObject::Brep(&mut b)), &pl, scale)
                .map_err(|e| err("transform", e))?;
            let t = Rc::new(ShapeData { brep: b, tag: s.tag });
            // solidcore @ c9de85d returns success but moves only the vertices: the curves and
            // surfaces of a primitive are not owned under the keys its edit looks for. Hand out
            // only a result whose volume and centroid moved with the matrix.
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
                Loop::Spline(_) => Err(unsupported(&format!(
                    "{op}: spline profile (SmApiCreateCurveInterpPoints is outside solidcore's scope)"
                ))),
                Loop::Composite(_) => Err(unsupported(&format!("{op}: sketch profile (lines, arcs, B-splines)"))),
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
            let s = Rc::new(ShapeData { brep: b, tag });
            self.transform(&s, &p.frame)
        }

        pub fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
            if !angle.is_finite() || angle == 0.0 {
                return Err(KernelError::InvalidInput("solidcore: revolve: angle must be non-zero".into()));
            }
            let deg = angle.to_degrees().clamp(-360.0, 360.0);
            let curves = self.loop_curves(&p.lp, &p.frame, "revolve")?;
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
            Ok(Rc::new(ShapeData { brep: b, tag }))
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
            if s.brep.face_list().is_empty() {
                return Ok(vec![Class::Outside; pts.len()]);
            }
            let tol = zone_tol_3d(&self.k, SmallTopoType::NormalSize);
            let mut c = BrepClassifier::new(&s.brep, BrepIdentity::default());
            let mut out = Vec::with_capacity(pts.len());
            for p in pts {
                let pc = c.point_3d_classify(&self.k, pt(*p), tol, true).map_err(|e| err("classify", e))?;
                out.push(match pc.point_class() {
                    // as the SMLib facade: inside a region that is not void (`IsVoid` 0)
                    PointClassType::Region => match pc.object() {
                        Some(EntityRef::Region(r)) => {
                            let void = c.brep().region(r).map_err(|e| err("classify", e))?.is_void();
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
            let mut acc = first.brep.clone();
            for s in it {
                let out = solidcore::boolean::api_brep::boolean(&mut self.k, acc, s.brep.clone(), op_sc)
                    .map_err(|e| err("boolean", e))?;
                acc = out.result;
            }
            Ok(Rc::new(ShapeData { brep: acc, tag }))
        }

        /// Sample points: an interior point (`SmApiFaceInternalPoint`) and the vertices of the
        /// boundary loops; only the vertices while the interior point is not ported.
        pub fn faces(&mut self, s: &Shape) -> KResult<Vec<FaceInfo>> {
            let fs = q::get_faces(&s.brep).map_err(|e| err("faces", e))?;
            let mut out = Vec::with_capacity(fs.len());
            for (i, f) in fs.into_iter().enumerate() {
                let mut points = Vec::new();
                match q::face_internal_point(&self.k, &s.brep, f) {
                    Ok(p) => points.push(dv(p)),
                    Err(e) if e.is_unported() => {}
                    Err(e) => return Err(err("faces", e)),
                }
                let loops = q::face_get_boundary_loops(&s.brep, f).map_err(|e| err("faces", e))?;
                for e in loops.edges {
                    for v in [edge::start_vertex(&s.brep, e), edge::end_vertex(&s.brep, e)] {
                        if let Some(v) = v.map_err(|e| err("faces", e))? {
                            let p = dv(q::vertex_get_point(&s.brep, v).map_err(|e| err("faces", e))?);
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
            let es = q::get_edges(&s.brep).map_err(|e| err("edges", e))?;
            let mut out = Vec::with_capacity(es.len());
            for (i, e) in es.into_iter().enumerate() {
                let ed = s.brep.edge(e).map_err(|e| err("edges", e))?;
                let curve =
                    ed.curve.as_ref().ok_or_else(|| KernelError::failed("solidcore: edges: edge without curve"))?;
                let (t0, t1) = (ed.interval.min(), ed.interval.max());
                let at = |t: f64| curve.evaluate_point(&self.k, t).map(dv).map_err(|e| err("edges", e));
                let end = |v: Result<Option<solidcore::VertexId>, SolidError>, t: f64| -> KResult<DVec3> {
                    match v.map_err(|e| err("edges", e))? {
                        Some(v) => Ok(dv(q::vertex_get_point(&s.brep, v).map_err(|e| err("edges", e))?)),
                        None => at(t),
                    }
                };
                let mut points = vec![end(edge::start_vertex(&s.brep, e), t0)?];
                for j in 1..=samples {
                    points.push(at(t0 + (t1 - t0) * f64::from(j) / f64::from(samples + 1))?);
                }
                points.push(end(edge::end_vertex(&s.brep, e), t1)?);
                out.push(EdgeInfo { id: i as u32, points, face_tags: vec![s.tag, s.tag], smooth: false });
            }
            Ok(out)
        }

        pub fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
            let store = solidcore::geom::attribute::AttributeStore::new();
            let key_of = |_e: solidcore::EntityRef| solidcore::geom::attribute::ObjectKey(0);
            let attrs = mp::PropertyAttributes { store: &store, key_of: &key_of };
            let mut b = s.brep.clone();
            let m = mp::brep_mass_properties(&self.k, &mut b, 1e-6, 1.0, pt(DVec3::ZERO), &attrs)
                .map_err(|e| err("mass_props", e))?;
            let area = mp::brep_area(&self.k, &mut b, 1e-6).map_err(|e| err("mass_props (area)", e))?;
            let c = m.centroid;
            Ok(MassProps { volume: m.properties.volume, area, centroid: DVec3::new(c.x, c.y, c.z) })
        }

        pub fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
            if s.brep.face_list().is_empty() {
                return Ok(None);
            }
            let b = q::brep_bounding_box(&self.k, &s.brep, true).map_err(|e| err("bounds", e))?;
            Ok(Some((
                DVec3::new(b.min_at(0), b.min_at(1), b.min_at(2)),
                DVec3::new(b.max_at(0), b.max_at(1), b.max_at(2)),
            )))
        }

        pub fn is_solid(&mut self, s: &Shape) -> bool {
            !s.brep.face_list().is_empty() && q::brep_is_manifold_solid(&s.brep).unwrap_or(false)
        }

        pub fn is_empty(&mut self, s: &Shape) -> bool {
            s.brep.face_list().is_empty()
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
        "solidcore-d4940b6"
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

    fn sweep(&mut self, _p: &Loop, _path: &[DVec3], _interp: PathInterp, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("sweep (SmApiCreatePipeSweep exists, not wired in kernel-solidcore)"))
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
