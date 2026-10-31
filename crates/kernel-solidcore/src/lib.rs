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
//! * entry points that answered but gave wrong results at earlier pins (transform, point
//!   classification) → `Unsupported` if they still do: `transform` checks that vertices, curve
//!   points and surfaces followed the matrix (volume and centroid where that cannot decide),
//!   `classify` runs a self-test on a box once,
//! * other solidcore errors → [`KernelError::Failed`] / [`KernelError::InvalidInput`].
//!
//! So `mcad diff-kernels` can report which operations already work ("coverage"); at bba8d87:
//! primitives, rigid transforms, `extrude` (also with twist and scale), `revolve`, `loft`,
//! `sweep` and `sweep_path` of polygons, circles, closed interpolating splines and sketch
//! profiles (lines, arcs and B-splines), `fillet`, `chamfer`, `fillet_variable` (one radius law
//! for all edges), `make_polyhedron` (planar faces through solidcore's STEP builder), `classify`,
//! `faces`, `edges`, `brep`, `bounds`, `mass_props`, `is_solid`; booleans
//! mostly (some results lose faces); curved solids have volumes closer to
//! the exact value than SMLib's. Profiles and sweeps follow the SMLib facade
//! (`smlib/facade/sc_facade.cpp`, `sc_sketch.cpp`) step by step: the same curves, sections,
//! frames and refusals, so faces and edges come out as SMLib's. Queries work on the
//! shape's own body ([`ShapeData`]); before an operation changes a body it is copied with
//! `SmApiBrepCopy` ([`Inner::copy`]), never with `Brep::clone`, whose geometry has no owner.
//! Face tags (provenance) are solidcore attributes as in the facade (`TAG_ATTR`): set on every
//! face of a new body, carried by solidcore through copies, transforms and Booleans (a face
//! left without one gets the tag of the operand face it lies on, as `resolve_tags`). `edges`
//! reports the tags of an edge's faces and marks as `smooth` what SMLib cannot blend. Fillets
//! and chamfers follow the facade's `do_fillet_like` (`SmApiFilletEdges`, equal-distance chamfer
//! with bevelled corners, radius laws as B-splines at the Greville stations, the same checks of
//! the result, self-overlap as `invalid_faces`, tags resolved as `resolve_tags`); the facade's
//! retry chain by chain after a failed fillet is not ported. `brep` maps the B-rep to the
//! neutral one as the facade's `sc_brep` (`brep.rs`).
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
    /// The shape this one is a similarity transform of, and the matrix: its mass properties
    /// follow from the source's, if those are known by then (no second integration).
    #[cfg(feature = "solidcore")]
    derived: std::cell::RefCell<Option<(std::rc::Weak<ShapeData>, DMat4)>>,
    pub tag: Tag,
}

impl ShapeData {
    #[cfg(feature = "solidcore")]
    fn new(brep: solidcore::Brep, tag: Tag) -> Shape {
        Rc::new(ShapeData {
            brep: std::cell::RefCell::new(brep),
            mass: std::cell::Cell::new(None),
            derived: std::cell::RefCell::new(None),
            tag,
        })
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
mod brep;
#[cfg(feature = "solidcore")]
mod polyhedron;
#[cfg(feature = "solidcore")]
pub use brep::BrepStats;

#[cfg(feature = "solidcore")]
mod imp {
    use super::*;
    use solidcore::blend::fillet_executive::api::fillet_edges;
    use solidcore::blend::fillet_executive::session::FilletSession;
    use solidcore::curve::curve::Curve;
    use solidcore::error::EntityRef;
    use solidcore::geom::a_object::BooleanOperation;
    use solidcore::geom::attribute::{Attribute, Behavior};
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
    use solidcore::topology::brep::p5::{StitchAndOrientOptions, stitch_and_orient};
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

    /// Attribute id of a face's tag (provenance), the SMLib facade's `SC_TAG_ATTR_ID`: a long
    /// attribute with copy behaviour, so solidcore copies it with the B-rep, keeps it on a face
    /// that moves into a Boolean's result and gives it to both pieces of a split face (as
    /// SMLib's default attribute handlers).
    const TAG_ATTR: u64 = solidcore::geom::attribute::ids::USER_1 + 77;

    pub(crate) fn face_tag(b: &solidcore::Brep, f: solidcore::FaceId) -> Option<Tag> {
        let v = b.find_attribute(EntityRef::Face(f), TAG_ATTR)?.long_value()?;
        Tag::try_from(v).ok()
    }

    fn set_face_tag(b: &mut solidcore::Brep, f: solidcore::FaceId, tag: Tag) -> KResult<()> {
        let e = EntityRef::Face(f);
        let old: Vec<_> =
            b.entity_attributes(e).into_iter().filter(|(_, a)| a.attribute_id() == TAG_ATTR).map(|(h, _)| h).collect();
        for h in old {
            b.remove_attribute(e, h).map_err(|e| err("face tag", e))?;
        }
        b.add_attribute(e, Attribute::long(TAG_ATTR, i64::from(tag), Behavior::Copy))
            .map_err(|e| err("face tag", e))?;
        Ok(())
    }

    /// Every face of `b` gets `tag` (a new body: primitives, extrusions, sweeps, skins).
    fn tag_all(b: &mut solidcore::Brep, tag: Tag) -> KResult<()> {
        for f in b.face_list().to_vec() {
            set_face_tag(b, f, tag)?;
        }
        Ok(())
    }

    /// A new shape whose faces all carry `tag`.
    fn made(mut b: solidcore::Brep, tag: Tag) -> KResult<Shape> {
        tag_all(&mut b, tag)?;
        Ok(ShapeData::new(b, tag))
    }

    /// The diagonal of the bounding box of `b` (1 for an empty body or a failed query).
    fn model_diag(k: &solidcore::Kernel, b: &solidcore::Brep) -> f64 {
        if b.face_list().is_empty() {
            return 1.0;
        }
        match q::brep_bounding_box(k, b, false) {
            Ok(x) => {
                let d = (0..3).map(|i| (x.max_at(i) - x.min_at(i)).powi(2)).sum::<f64>().sqrt();
                if d > 0.0 && d.is_finite() { d } else { 1.0 }
            }
            Err(_) => 1.0,
        }
    }

    /// A tagged face of an input of an operation: input index, face, tag, bounding box.
    type FaceBox = (usize, solidcore::FaceId, Tag, DVec3, DVec3);

    /// Uniform clamped knots of a radius law with `m` control radii: degree min(3, m - 1), as
    /// the facade's `law_knots`.
    fn law_knots(m: u32) -> (usize, Vec<f64>) {
        let p = 3.min(m as usize - 1);
        let inner = m as usize - p - 1;
        let mut k = vec![0.0; p + 1];
        k.extend((1..=inner).map(|i| i as f64 / (inner + 1) as f64));
        k.extend(std::iter::repeat_n(1.0, p + 1));
        (p, k)
    }

    /// Greville abscissae of the law's control radii (the stations), as `law_greville`.
    fn law_greville(m: u32) -> Vec<f64> {
        let (p, k) = law_knots(m);
        (0..m as usize).map(|j| (1..=p).map(|i| k[j + i]).sum::<f64>() / p as f64).collect()
    }

    /// The radius law through `radii` at the stations as the facade's `make_law`: a B-spline
    /// with control points (r_j, t_j, 0), so its x is the radius and its y the parameter.
    fn make_law(radii: &[f64]) -> KResult<solidcore::blend::fillet_standard_solver::p1::FilletLaw> {
        use solidcore::blend::fillet_standard_solver::p1::{FilletLaw, FilletLawKind};
        let m = radii.len() as u32;
        let (p, k) = law_knots(m);
        let g = law_greville(m);
        let c = cad_kernel::brep::BSplineCurve {
            degree: p as u32,
            knots: k,
            points: radii.iter().zip(&g).map(|(&r, &t)| DVec3::new(r, t, 0.0)).collect(),
            weights: None,
        };
        let law_curve = bspline(&c, pt, BSplineCurveForm::Unspecified, "fillet (radius law)")?;
        Ok(FilletLaw::new(FilletLawKind::BSpline { law_curve, extended_law: None }))
    }

    /// A variable-radius fillet of `edges` with the law `laws[i]` along `edges[i]`, as the
    /// facade's `blend_core`: the constant-radius solver where all values of an edge agree.
    /// solidcore's fillet session takes one law for all edges: different laws on several
    /// edges are `Unsupported`.
    fn variable_fillet(
        k: &mut solidcore::Kernel,
        b: &mut solidcore::Brep,
        edges: &[solidcore::EdgeId],
        laws: &[&Vec<f64>],
    ) -> KResult<Result<(), SolidError>> {
        use solidcore::blend::fillet_solver::p1::FilletSurfaceGeneratorType;
        let constant = |rs: &Vec<f64>| {
            let (lo, hi) = rs.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
            rs.len() == 1 || hi - lo <= 1e-12 * hi
        };
        if laws.iter().all(|l| constant(l)) {
            let radii: Vec<f64> = laws.iter().map(|l| l[0]).collect();
            let xs = vec![FilletSurfaceGeneratorType::Circular; edges.len()];
            return Ok(solidcore::blend::fillet_executive::api::fillet_edges_per_edge(
                k, b, edges, &radii, &xs, 1, 1.0,
            ));
        }
        if laws.iter().any(|l| l != &laws[0]) {
            return Err(unsupported(
                "fillet_variable: different radius laws on several edges (solidcore's fillet session \
                 takes one law for all edges)",
            ));
        }
        let law = make_law(laws[0])?;
        let hi = laws[0].iter().copied().fold(f64::MIN, f64::max);
        let r = (|| {
            {
                let mut session = FilletSession::new(k, b)?;
                session.prepare_variable_radius_edges(k, edges, hi, law, 1, 1)?;
                session.do_fillet(k)?;
            }
            stitch_and_orient(k, b, &StitchAndOrientOptions::default())
        })();
        b.refresh_cached_topology();
        Ok(r)
    }

    /// An equal-distance chamfer as the facade's `blend_core`: the constant-radius solver with
    /// a linear cross-section, every corner a bevel, `DoFilleting`, then `StitchAndOrient`.
    fn chamfer_edges(
        k: &mut solidcore::Kernel,
        b: &mut solidcore::Brep,
        edges: &[solidcore::EdgeId],
        d: f64,
    ) -> Result<(), SolidError> {
        let r = (|| {
            {
                let mut session = FilletSession::new(k, b)?;
                session.prepare_constant_radius_edges(k, edges, d, 0, 1, 1.0)?;
                session.create_corners(k, None)?;
                for c in session.fillet_corners()? {
                    session.set_corner_bevel(c, true)?;
                }
                session.do_filleting(k, None)?;
            }
            stitch_and_orient(k, b, &StitchAndOrientOptions::default())
        })();
        b.refresh_cached_topology();
        r
    }

    /// Length of `e` (polyline through 33 points of its curve), as the facade's `edge_length`.
    fn edge_length(k: &solidcore::Kernel, b: &solidcore::Brep, e: Option<solidcore::EdgeId>) -> f64 {
        let Some(ed) = e.and_then(|e| b.edge(e).ok()) else { return 0.0 };
        let Some(c) = ed.curve.as_ref() else { return 0.0 };
        let (t0, t1) = (ed.interval.min(), ed.interval.max());
        let pts: Vec<DVec3> =
            (0..=32).filter_map(|j| c.evaluate_point(k, t0 + (t1 - t0) * f64::from(j) / 32.0).ok().map(dv)).collect();
        pts.windows(2).map(|w| (w[1] - w[0]).length()).sum()
    }

    /// What [`Inner::transform`] follows through the transform: vertices with their points,
    /// edges with three interior curve points and their faces.
    struct Probes {
        vertices: Vec<(solidcore::VertexId, DVec3)>,
        edges: Vec<(solidcore::EdgeId, [DVec3; 3], Vec<solidcore::FaceId>)>,
        /// Per face: the middle point of its first edge and the face's normal there, out of
        /// the material (`None` where it cannot be evaluated).
        faces: Vec<(solidcore::FaceId, DVec3, Option<DVec3>)>,
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

    /// Is the linear part of `f` a similarity (as the facade's `similarity`)?
    fn similar(f: &DMat4) -> bool {
        let c = [f.x_axis.truncate(), f.y_axis.truncate(), f.z_axis.truncate()];
        let l = c.map(|v| v.length());
        l[0] > 0.0
            && l[0].is_finite()
            && (l[0] - l[1]).abs() <= 1e-7 * l[0]
            && (l[0] - l[2]).abs() <= 1e-7 * l[0]
            && [(0, 1), (0, 2), (1, 2)].iter().all(|&(i, j)| c[i].dot(c[j]).abs() <= 1e-9 * l[0] * l[0])
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
                    let form = match (arc, similar(frame)) {
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

    /// The points of a polygon or spline loop as the SMLib facade takes them (`parse_loops`): without
    /// repeated consecutive points and without a last point equal to the first; at least 3.
    fn loop_points(ps: &[DVec2], op: &str) -> KResult<Vec<DVec2>> {
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
            made(r.map_err(|e| err(op, e))?, tag)
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

        /// A solid of planar faces (`polyhedron.rs`), as the facade's `sc_make_polyhedron`: a
        /// closed manifold solid of positive volume, else `Failed`.
        pub fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Shape> {
            const OP: &str = "make_polyhedron";
            let body = crate::polyhedron::body(points, faces)?;
            let built = step_brep::build(&body).map_err(|e| {
                KernelError::failed(format!("solidcore: {OP}: planar faces not stitched into a solid ({e})"))
            })?;
            // made in the builder's own kernel: an independent copy in ours
            let b = self.copy(&built)?;
            let s = made(b, tag)?;
            if !self.is_solid(&s) {
                return Err(KernelError::failed(format!(
                    "solidcore: {OP}: the faces do not form a closed manifold solid"
                )));
            }
            let v = self.mass_props(&s).map_err(|e| check_err(OP, e))?.volume;
            if v.is_nan() || v <= 0.0 {
                return Err(KernelError::failed(format!(
                    "solidcore: {OP}: orientation could not be fixed (volume {v})"
                )));
            }
            Ok(s)
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
            let probes = self.probes(&b)?;
            self.transform_brep(&mut b, &pl, scale)?;
            let t = ShapeData::new(b, s.tag);
            // solidcore @ c9de85d returned success but moved only the vertices (on a
            // `Brep::clone`, whose geometry has no owner, it still does). Hand out only a result
            // whose vertices, curves and surfaces moved with the matrix: the probes first (cheap),
            // where they cannot decide (a closest-point solve fails) volume and centroid.
            if let Err(why) = self.probes_moved(&t, &probes, m) {
                self.transform_check_volume(s, &t, m, &why)?;
            }
            if similar(m) {
                *t.derived.borrow_mut() = Some((Rc::downgrade(s), *m));
            }
            Ok(t)
        }

        /// `SmBrep::Transform` of `b` as the facade's `apply_matrix`: without the tolerance
        /// scaling of `SmApiTransform` (`transform_independent_object` re-derives the model
        /// size from the box of the vertices: a torus or a revolved circle has a single vertex,
        /// so every tolerance became 0, and the context's model size changed too; later
        /// Booleans with the body missed their intersections and returned the operand
        /// unchanged or no manifold solid). A scaled body gets its tolerances from its full
        /// box; the context keeps its model size.
        fn transform_brep(&mut self, b: &mut solidcore::Brep, pl: &Axis2Placement, scale: Option<Vec3>) -> KResult<()> {
            use solidcore::geom::tol;
            use solidcore::topology::{face, vertex};
            solidcore::topology::brep::p5::transform(&mut self.k, b, pl, scale, false)
                .map_err(|e| err("transform", e))?;
            if scale.is_none() {
                return Ok(());
            }
            let size = model_diag(&self.k, b);
            let context = tol::context_model_size_estimate(&self.k, &self.k.context);
            let zone = b.set_this_model_size_estimate(&mut self.k, size);
            tol::set_context_model_size_estimate(&mut self.k.context, context);
            let op = "transform (tolerances)";
            for v in b.vertex_list().to_vec() {
                vertex::set_tolerance(b, v, zone, false, false).map_err(|e| err(op, e))?;
            }
            for e in b.edge_list().to_vec() {
                edge::set_tolerance(b, e, zone, false, false).map_err(|e| err(op, e))?;
            }
            for f in b.face_list().to_vec() {
                face::p3::set_tolerance(b, f, zone, false, false).map_err(|e| err(op, e))?;
            }
            Ok(())
        }

        /// Points to follow through a transform: every vertex, and per edge three interior
        /// points of its curve (at a quarter, half and three quarters of its interval) with
        /// the faces they lie on.
        fn probes(&self, b: &solidcore::Brep) -> KResult<Probes> {
            let op = "transform (check)";
            let mut vertices = Vec::with_capacity(b.vertex_list().len());
            for &v in b.vertex_list() {
                vertices.push((v, dv(q::vertex_get_point(b, v).map_err(|e| err(op, e))?)));
            }
            let mut edges = Vec::with_capacity(b.edge_list().len());
            for &e in b.edge_list() {
                let ed = b.edge(e).map_err(|e| err(op, e))?;
                let Some(curve) = ed.curve.as_ref() else { continue };
                let iv = ed.interval;
                let mut pts = [DVec3::ZERO; 3];
                for (j, p) in pts.iter_mut().enumerate() {
                    let t = iv.min() + (iv.max() - iv.min()) * (j + 1) as f64 / 4.0;
                    *p = dv(curve.evaluate_point(&self.k, t).map_err(|e| err(op, e))?);
                }
                let faces = edge::faces(b, e, None).map_err(|e| err(op, e))?;
                edges.push((e, pts, faces));
            }
            let mut faces: Vec<(solidcore::FaceId, DVec3, Option<DVec3>)> = Vec::new();
            for (_, pts, fs) in &edges {
                for &f in fs {
                    if faces.iter().all(|x| x.0 != f) {
                        faces.push((f, pts[1], crate::brep::outward_normal(&self.k, b, f, pts[1])));
                    }
                }
            }
            Ok(Probes { vertices, edges, faces })
        }

        /// Did `probes` (taken before the transform) move with `m` to `t`? Vertices and curve
        /// points at the same normalised parameter, else the closest point on the curve; per
        /// face the middle point of an edge on its surface, and the face's normal there (out of
        /// the material) as the inverse transpose maps it: geometry that moved but lost its
        /// orientation or its frame fails. `Err` names the first point that did not, or the
        /// solve that failed.
        fn probes_moved(&self, t: &Shape, p: &Probes, m: &DMat4) -> Result<(), String> {
            let b = t.brep.borrow();
            let b = &*b;
            let pts = p.vertices.iter().map(|v| m.transform_point3(v.1));
            let (lo, hi) =
                pts.fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(lo, hi), q| (lo.min(q), hi.max(q)));
            let size = if lo.x <= hi.x { (hi - lo).length() } else { 1.0 };
            // far below any misplaced geometry, above the solvers' tolerances
            let tol = 1e-7 * size.max(1.0);
            for &(v, before) in &p.vertices {
                let want = m.transform_point3(before);
                let got = q::vertex_get_point(b, v).map(dv).map_err(|e| format!("vertex: {e}"))?;
                if (got - want).length() > tol {
                    return Err(format!("vertex at {got:.6?}, expected {want:.6?}"));
                }
            }
            for (e, before, _) in &p.edges {
                let ed = b.edge(*e).map_err(|e| format!("edge: {e}"))?;
                let curve = ed.curve.as_ref().ok_or("edge without curve")?;
                let iv = ed.interval;
                for (j, &p0) in before.iter().enumerate() {
                    let want = m.transform_point3(p0);
                    let t = iv.min() + (iv.max() - iv.min()) * (j + 1) as f64 / 4.0;
                    let got = curve.evaluate_point(&self.k, t).map(dv).map_err(|e| format!("curve: {e}"))?;
                    if (got - want).length() > tol {
                        let d = q::edge_closest_point(&self.k, b, *e, pt(want))
                            .map_err(|e| format!("closest point on an edge: {e}"))?
                            .closest
                            .distance;
                        if d > tol {
                            return Err(format!("edge point {want:.6?} is {d:.2e} off its curve"));
                        }
                    }
                }
            }
            // each face: the point on its surface and its outward normal moved with the matrix
            // (normals with the inverse transpose: also the orientation after a mirror)
            let cof = glam::DMat3::from_mat4(*m).inverse().transpose();
            for &(f, p0, n0) in &p.faces {
                let want = m.transform_point3(p0);
                let srf = solidcore::topology::face::p2::surface(b, f)
                    .map_err(|e| format!("face: {e}"))?
                    .ok_or("face without surface")?;
                let d = q::surface_closest_point(&self.k, srf, pt(want))
                    .map_err(|e| format!("closest point on a surface: {e}"))?
                    .closest
                    .distance;
                if d > tol {
                    return Err(format!("edge point {want:.6?} is {d:.2e} off the surface of its face"));
                }
                let Some(n0) = n0 else { continue };
                let n_want = (cof * n0).normalize();
                let n = crate::brep::outward_normal(&self.k, b, f, want).ok_or("face normal not evaluated")?;
                if n.distance(n_want) > 1e-6 {
                    return Err(format!("face normal {n:.6?} at {want:.6?}, expected {n_want:.6?}"));
                }
            }
            Ok(())
        }

        /// The fallback check of [`Inner::transform`]: volume and centroid of `t` against those of
        /// `s` mapped by `m`, within the accuracy of the volume integration.
        fn transform_check_volume(&mut self, s: &Shape, t: &Shape, m: &DMat4, why: &str) -> KResult<()> {
            let before = self.mass_props(s).map_err(|e| check_err("transform", e))?;
            let after = self.mass_props(t).map_err(|e| match e {
                KernelError::Unsupported(_) => check_err("transform", e),
                e => unsupported(&format!("transform: the result fails the check ({e})")),
            })?;
            let want_c = m.transform_point3(before.centroid);
            let want_v = before.volume * m.determinant().abs();
            // the integration is accurate to about 1e-6 relative (mass_props asks for 1e-6):
            // the centroid within that of the body's size, the volume within that of itself
            let size = want_v.abs().cbrt().max(1e-9);
            let tol_c = 1e-5 * (size + 1e-3 * want_c.length());
            if (after.centroid - want_c).length() > tol_c || (after.volume - want_v).abs() > 1e-5 * want_v.abs() {
                return Err(unsupported(&format!(
                    "transform: SmBrep::Transform (unit topology.brep_p5) did not move the body with the \
                     matrix ({why}; centroid {:.6?}, expected {:.6?})",
                    after.centroid, want_c
                )));
            }
            Ok(())
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
                    if !similar(frame) {
                        return Err(unsupported(&format!(
                            "{op}: circle profile under a non-uniform frame (an ellipse; SMLib does not take it either)"
                        )));
                    }
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
                    let pts: Vec<DVec3> = loop_points(ps, op)?.iter().map(|p| dv(p3(*p))).collect();
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
            if !twist.is_finite() || !scale.is_finite() || scale.min_element() < 0.0 {
                return Err(KernelError::InvalidInput("solidcore: extrude: bad twist/scale".into()));
            }
            if twist.abs() >= 1e-12 || (scale - DVec2::ONE).abs().max_element() >= 1e-12 {
                let s = self.extrude_skin(&p.lp, h, twist, scale, tag)?;
                return self.transform(&s, &p.frame);
            }
            // as the SMLib facade: sweep in the profile plane, then place the solid
            let curves = self.loop_curves(&p.lp, &DMat4::IDENTITY, "extrude")?;
            let refs: Vec<Option<&Curve>> = curves.iter().map(Some).collect();
            let b = sw::create_linear_sweep(&mut self.k, &refs, Vec3::new(0.0, 0.0, h.signum()), h.abs(), true)
                .map_err(|e| err("extrude", e))?;
            let s = self.require_solid(made(b, tag)?, "extrude", "")?;
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
            let hint = if polygon { " (polygon profile: open seams between the side faces?)" } else { "" };
            self.require_solid(made(b, tag)?, OP, hint)
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

        /// The loop as one closed curve (the facade's `loop_curves` with `single`, for lofts
        /// between sections that do not match segment by segment): a polygon is a degree-1
        /// B-spline, a circle a B-spline arc over 360°.
        fn loop_curve_single(&mut self, lp: &Loop, frame: &DMat4, op: &str) -> KResult<Vec<Curve>> {
            match lp {
                Loop::Polygon(ps) => {
                    let pts: Vec<DVec3> =
                        loop_points(ps, op)?.iter().map(|p| frame.transform_point3(p.extend(0.0))).collect();
                    Ok(vec![polyline_curve(&pts, true, op)?])
                }
                Loop::Circle { center, r } => {
                    if !similar(frame) {
                        return Err(unsupported(&format!(
                            "{op}: circle profile under a non-uniform frame (an ellipse; SMLib does not take it either)"
                        )));
                    }
                    let c = ac::create_arc(pt(center.extend(0.0)), *r, 0.0, 360.0).map_err(|e| err(op, e))?;
                    let mut c = Curve::from(c);
                    if let Some((pl, scale)) = Self::placement(frame)? {
                        transform_independent_object(&mut self.k, Some(TransformObject::Curve(&mut c)), &pl, scale)
                            .map_err(|e| err(op, e))?;
                    }
                    Ok(vec![c])
                }
                _ => self.loop_curves(lp, frame, op),
            }
        }

        /// A closed solid of positive volume, as the facade's `require_solid`.
        fn require_solid(&mut self, s: Shape, what: &str, hint: &str) -> KResult<Shape> {
            if self.is_empty(&s) || !self.is_solid(&s) {
                // as the facade's `require_solid`, unless the caller knows better
                let hint = if hint.is_empty() { " (self-intersecting or open profile?)" } else { hint };
                return Err(KernelError::failed(format!("solidcore: {what} did not produce a closed solid{hint}")));
            }
            let v = self.mass_props(&s).map_err(|e| check_err(what, e))?.volume;
            if v.is_nan() || v <= 0.0 {
                return Err(KernelError::failed(format!("solidcore: {what} produced an invalid solid (volume {v})")));
            }
            Ok(s)
        }

        /// Skin through the sections (`SmApiCreateSkinPrimitive`, capped), `degree` in the loft
        /// direction.
        fn skin(&mut self, sections: &[Vec<Curve>], degree: usize, what: &str, tag: Tag) -> KResult<Shape> {
            let counts: Vec<usize> = sections.iter().map(Vec::len).collect();
            let refs: Vec<Option<&Curve>> = sections.iter().flatten().map(Some).collect();
            let b = solidcore::sweep::api_surfaces::create_skin_primitive(&mut self.k, &counts, &refs, degree, true)
                .map_err(|e| err(what, e))?
                .brep;
            self.require_solid(made(b, tag)?, what, "")
        }

        /// `extrude` with twist or scale, as the SMLib facade: a skin through `k + 1` sections
        /// (one per 7.5° of twist, at least 2; bicubic, ruled for a pure scale). Section
        /// `t` is the profile turned by `twist · t` about the origin, then scaled by
        /// `lerp(1, scale, t)` (OpenSCAD's order), at height `h · t`.
        fn extrude_skin(&mut self, lp: &Loop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Shape> {
            const OP: &str = "extrude";
            if scale.min_element() <= 1e-9 {
                return Err(unsupported("extrude: scale 0 (an apex; SMLib does not take it either)"));
            }
            if matches!(lp, Loop::Circle { .. }) && (scale.x - scale.y).abs() > 1e-12 {
                return Err(unsupported(
                    "extrude: non-uniform scale of a circle profile (SMLib does not take it either)",
                ));
            }
            let k = if twist.abs() < 1e-12 { 1 } else { ((twist.abs() / 7.5f64.to_radians()).ceil() as usize).max(2) };
            let mut secs = Vec::with_capacity(k + 1);
            for i in 0..=k {
                let t = i as f64 / k as f64;
                let (sn, c) = (twist * t).sin_cos();
                let sc = DVec2::ONE + (scale - DVec2::ONE) * t;
                let lift = DMat4::from_translation(DVec3::new(0.0, 0.0, h * t));
                let map = |p: &DVec2| DVec2::new((c * p.x - sn * p.y) * sc.x, (sn * p.x + c * p.y) * sc.y);
                secs.push(match lp {
                    // the points move (a spline is interpolated through the moved points)
                    Loop::Polygon(ps) => self.loop_curves(&Loop::Polygon(ps.iter().map(map).collect()), &lift, OP)?,
                    Loop::Spline(ps) => self.loop_curves(&Loop::Spline(ps.iter().map(map).collect()), &lift, OP)?,
                    // the curves move: scale(sx, sy) · rotate(a) (for circles sx = sy)
                    _ => {
                        let m = DMat4::from_cols(
                            DVec3::new(c * sc.x, sn * sc.y, 0.0).extend(0.0),
                            DVec3::new(-sn * sc.x, c * sc.y, 0.0).extend(0.0),
                            DVec3::new(0.0, 0.0, sc.x).extend(0.0),
                            DVec3::new(0.0, 0.0, h * t).extend(1.0),
                        );
                        self.loop_curves(lp, &m, OP)?
                    }
                });
            }
            self.skin(&secs, if k == 1 { 1 } else { 3 }, "extrude (skin)", tag)
        }

        /// `loft`, as the SMLib facade (`sc_loft`): sections are matched segment by segment
        /// when all are polygons with the same number of points or all composite loops with
        /// the same number of segments, otherwise every section is one closed curve; degree
        /// 1 (`ruled`) or min(3, sections - 1).
        pub fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Shape> {
            const OP: &str = "loft";
            if sections.len() < 2 {
                return Err(KernelError::InvalidInput("solidcore: loft needs at least 2 sections".into()));
            }
            let sections = cad_kernel::loft_sections(sections);
            let n_poly = |l: &Loop| match l {
                Loop::Polygon(ps) => loop_points(ps, OP).ok().map(|v| v.len()),
                _ => None,
            };
            let n_comp = |l: &Loop| match l {
                Loop::Composite(v) => Some(v.len()),
                _ => None,
            };
            let all_eq = |f: &dyn Fn(&Loop) -> Option<usize>| {
                let first = f(&sections[0].lp);
                first.is_some() && sections.iter().all(|s| f(&s.lp) == first)
            };
            let segmentwise = all_eq(&n_poly) || all_eq(&n_comp);
            let mut secs = Vec::with_capacity(sections.len());
            for s in &sections {
                secs.push(if segmentwise {
                    self.loop_curves(&s.lp, &s.frame, OP)?
                } else {
                    self.loop_curve_single(&s.lp, &s.frame, OP)?
                });
            }
            let degree = if ruled { 1 } else { (sections.len() - 1).min(3) };
            self.skin(&secs, degree, OP, tag)
        }

        pub fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
            if !angle.is_finite() || angle == 0.0 {
                return Err(KernelError::InvalidInput("solidcore: revolve: angle must be non-zero".into()));
            }
            let deg = angle.to_degrees().clamp(-360.0, 360.0);
            Self::check_revolve_points(p)?;
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
            self.require_solid(made(b, tag)?, "revolve", "")
        }

        /// The facade's check before a revolve (`sc_revolve`): the polygon or spline points, or
        /// the circle (under a similarity frame), stay at x ≥ 0 after the frame, within 1e-9 of
        /// the frame's scale.
        fn check_revolve_points(p: &PlacedLoop) -> KResult<()> {
            let f = &p.frame;
            let scale = (f.x_axis.truncate().length() + f.y_axis.truncate().length()).max(1.0);
            let x = |q: DVec2| f.transform_point3(DVec3::new(q.x, q.y, 0.0)).x;
            let minx = match &p.lp {
                Loop::Circle { center, r } if similar(f) => x(*center) - r * f.x_axis.truncate().length(),
                Loop::Polygon(ps) | Loop::Spline(ps) => ps.iter().map(|&q| x(q)).fold(f64::MAX, f64::min),
                _ => f64::MAX,
            };
            if minx < -1e-9 * scale {
                return Err(KernelError::InvalidInput(
                    "solidcore: revolve: the profile crosses the axis (x < 0 after its frame)".into(),
                ));
            }
            Ok(())
        }

        /// Profile curves of a revolve, as the SMLib facade (`revolve_profile_curves`): a
        /// closed spline cannot pass through two points on the axis without crossing it, so
        /// two spline points that follow each other on the axis are joined by a line along
        /// the axis and the spline runs open between such lines.
        fn revolve_curves(&mut self, p: &PlacedLoop) -> KResult<Vec<Curve>> {
            const OP: &str = "revolve";
            let Loop::Spline(ps) = &p.lp else { return self.loop_curves(&p.lp, &p.frame, OP) };
            let pts: Vec<DVec3> =
                loop_points(ps, OP)?.iter().map(|q| p.frame.transform_point3(q.extend(0.0))).collect();
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
            // solidcore carries the tags (split pieces keep theirs); a face without one gets
            // the tag of the operand face it lies on, else the Boolean's
            self.resolve_tags(&mut acc, shapes, tag, false)?;
            Ok(ShapeData::new(acc, tag))
        }

        /// Tags of the faces of `r`, the result of an operation on `inputs`, as the facade's
        /// `resolve_tags` ("Provenance" in `sc_facade.cpp`): a face with a tag keeps it
        /// (`verify`: only if an interior point lies on an input face with that tag); a face
        /// without one, or whose tag is not confirmed, gets the tag of the input face it lies on
        /// (closest point on the face's surface, within its box), or `op_tag` if it lies on none.
        /// Returns the number of faces that lie on no input face (new faces).
        fn resolve_tags(
            &mut self,
            r: &mut solidcore::Brep,
            inputs: &[Shape],
            op_tag: Tag,
            verify: bool,
        ) -> KResult<usize> {
            let mut tol = 1e-6 * model_diag(&self.k, r);
            for s in inputs {
                tol = tol.max(1e-6 * model_diag(&self.k, &s.brep.borrow()));
            }
            // (input, face, tag, box) of every tagged input face, once needed
            let mut boxes: Option<Vec<FaceBox>> = None;
            let mut fresh = 0;
            for f in r.face_list().to_vec() {
                let has = face_tag(r, f);
                if has.is_some() && !verify {
                    continue;
                }
                let p = match q::face_internal_point(&self.k, r, f) {
                    Ok(p) => dv(p),
                    Err(_) => {
                        if has.is_none() {
                            set_face_tag(r, f, op_tag)?;
                            fresh += 1;
                        }
                        continue;
                    }
                };
                if boxes.is_none() {
                    let mut v = Vec::new();
                    for (i, s) in inputs.iter().enumerate() {
                        let b = s.brep.borrow();
                        for &g in b.face_list() {
                            let Some(t) = face_tag(&b, g) else { continue };
                            let (lo, hi) = match q::face_bounding_box(&self.k, &b, g, false) {
                                Ok(x) => (
                                    DVec3::new(x.min_at(0), x.min_at(1), x.min_at(2)),
                                    DVec3::new(x.max_at(0), x.max_at(1), x.max_at(2)),
                                ),
                                Err(_) => (DVec3::splat(-1e300), DVec3::splat(1e300)),
                            };
                            v.push((i, g, t, lo, hi));
                        }
                    }
                    boxes = Some(v);
                }
                let (mut best, mut best_tag, mut confirmed, mut on_input) = (f64::MAX, op_tag, false, false);
                for &(i, g, t, lo, hi) in boxes.iter().flatten() {
                    if p.cmplt(lo - tol).any() || p.cmpgt(hi + tol).any() {
                        continue;
                    }
                    let b = inputs[i].brep.borrow();
                    let Ok(Some(srf)) = solidcore::topology::face::p2::surface(&b, g) else { continue };
                    let Ok(c) = q::surface_closest_point(&self.k, srf, pt(p)) else { continue };
                    let d = c.closest.distance;
                    if d > tol {
                        continue;
                    }
                    on_input = true;
                    if has == Some(t) {
                        confirmed = true;
                        break;
                    }
                    if d < best {
                        (best, best_tag) = (d, t);
                    }
                }
                if confirmed {
                    continue;
                }
                if !on_input {
                    fresh += 1;
                }
                set_face_tag(r, f, best_tag)?;
            }
            Ok(fresh)
        }

        /// The points of `edge` at the Greville abscissae of a radius law with `m` control
        /// radii, as the facade's `sc_law_stations`.
        pub fn law_stations(&mut self, s: &Shape, edges: &[u32], counts: &[u32]) -> KResult<Vec<Vec<DVec3>>> {
            if edges.len() != counts.len() {
                return Err(KernelError::InvalidInput("solidcore: law_stations: one count per edge".into()));
            }
            let b = s.brep.borrow();
            let all = q::get_edges(&b).map_err(|e| err("law_stations", e))?;
            let mut out = Vec::with_capacity(edges.len());
            for (&id, &m) in edges.iter().zip(counts) {
                let e = *all.get(id as usize).ok_or_else(|| {
                    KernelError::InvalidInput(format!("solidcore: law_stations: edge id {id} out of range"))
                })?;
                if !(2..=256).contains(&m) {
                    return Err(KernelError::InvalidInput("solidcore: a radius law needs 2 to 256 stations".into()));
                }
                let ed = b.edge(e).map_err(|e| err("law_stations", e))?;
                let curve = ed
                    .curve
                    .as_ref()
                    .ok_or_else(|| KernelError::failed("solidcore: law_stations: edge without curve"))?;
                let (t0, t1) = (ed.interval.min(), ed.interval.max());
                let mut pts = Vec::with_capacity(m as usize);
                for g in law_greville(m) {
                    pts.push(dv(curve
                        .evaluate_point(&self.k, t0 + g * (t1 - t0))
                        .map_err(|e| err("law_stations", e))?));
                }
                out.push(pts);
            }
            Ok(out)
        }

        /// Fillet (`chamfer`: equal-distance chamfer, corners bevelled) of the edges `ids` (indices
        /// of [`Inner::edges`]) with size `r`, or with a radius law per edge (`laws`, the values at
        /// the stations of [`Inner::law_stations`]), as the facade's `do_fillet_like`: edges with
        /// nothing to blend are skipped, the result is checked (closed if the input is, a volume
        /// change a blend of that size can make, new faces) and its faces get their tags.
        pub fn fillet_like(
            &mut self,
            s: &Shape,
            ids: &[u32],
            r: f64,
            tag: Tag,
            chamfer: bool,
            laws: Option<&[Vec<f64>]>,
        ) -> KResult<Shape> {
            let what = if chamfer { "chamfer" } else { "fillet" };
            let invalid = |m: String| KernelError::InvalidInput(format!("solidcore: {what}: {m}"));
            let failed = |m: String, edges: Vec<u32>| KernelError::Failed {
                msg: format!("solidcore: {what}: {m}"),
                edges,
                places: Vec::new(),
            };
            let mut r = r;
            if let Some(laws) = laws {
                if laws.len() != ids.len() || laws.iter().any(Vec::is_empty) {
                    return Err(invalid("radii for every edge".into()));
                }
                r = 0.0;
                for (&id, l) in ids.iter().zip(laws) {
                    for &x in l {
                        if x <= 0.0 || !x.is_finite() {
                            return Err(KernelError::InvalidInput(format!(
                                "solidcore: fillet radius must be > 0 (edge {id})"
                            )));
                        }
                        r = r.max(x);
                    }
                }
            }
            if r <= 0.0 || !r.is_finite() {
                return Err(invalid("size must be > 0".into()));
            }
            if ids.is_empty() {
                return Err(invalid("no edges given".into()));
            }
            // the selection on the shape's own body, in the order of `edges`
            let (kept, skipped, n_edges) = {
                let mut b = s.brep.borrow_mut();
                let all = q::get_edges(&b).map_err(|e| err(what, e))?;
                let (mut kept, mut skipped) = (Vec::<u32>::new(), Vec::new());
                for &id in ids {
                    let &e = all
                        .get(id as usize)
                        .ok_or_else(|| invalid(format!("edge id {id} out of range (shape has {} edges)", all.len())))?;
                    if kept.contains(&id) || skipped.contains(&id) {
                        continue;
                    }
                    // nothing to blend at seams and other tangent-continuous or open edges
                    if edge::blend_edgeuse(&self.k, &mut b, e).map_err(|e| err(what, e))?.is_some() {
                        kept.push(id);
                    } else {
                        skipped.push(id);
                    }
                }
                (kept, skipped, all.len())
            };
            if kept.is_empty() {
                return Err(failed("none of the edges can be blended (smooth, seam or open edges)".into(), skipped));
            }
            let mut b = self.copy(&s.brep.borrow())?;
            let all = q::get_edges(&b).map_err(|e| err(what, e))?;
            if all.len() != n_edges {
                return Err(failed("the B-rep copy changed the edge count".into(), kept));
            }
            let sel: Vec<solidcore::EdgeId> = kept.iter().map(|&i| all[i as usize]).collect();
            let run = if chamfer {
                chamfer_edges(&mut self.k, &mut b, &sel, r)
            } else if let Some(laws) = laws {
                let per_edge: Vec<&Vec<f64>> =
                    kept.iter().map(|i| &laws[ids.iter().position(|j| j == i).expect("kept id")]).collect();
                variable_fillet(&mut self.k, &mut b, &sel, &per_edge)?
            } else {
                // circular cross-section, G1 (SetFilletParameters as the facade calls it)
                fillet_edges(&mut self.k, &mut b, &sel, r, 1, 1, 1.0)
            };
            run.map_err(|e| match err(what, e) {
                KernelError::Failed { msg, .. } => KernelError::Failed { msg, edges: kept.clone(), places: Vec::new() },
                e => e,
            })?;
            if b.face_list().is_empty() {
                return Err(failed("removed all faces".into(), kept));
            }
            let t = ShapeData::new(b, tag);
            // only faces near the blended edges can overlap (within 4 r of their boxes): the
            // facade checks every face, which costs two point classifications per face
            let near: Vec<(DVec3, DVec3)> = {
                let b0 = s.brep.borrow();
                let all = q::get_edges(&b0).map_err(|e| err(what, e))?;
                kept.iter()
                    .filter_map(|&i| q::edge_bounding_box(&self.k, &b0, all[i as usize], false).ok())
                    .map(|x| {
                        let (lo, hi) = (
                            DVec3::new(x.min_at(0), x.min_at(1), x.min_at(2)),
                            DVec3::new(x.max_at(0), x.max_at(1), x.max_at(2)),
                        );
                        (lo - DVec3::splat(4.0 * r), hi + DVec3::splat(4.0 * r))
                    })
                    .collect()
            };
            let bad = self.invalid_faces(&t, (near.len() == kept.len()).then_some(&near[..]))?;
            if !bad.is_empty() {
                // the selected edges near the faces that fail, else all (as the facade's
                // `edges_near_faces` with a reach of 3 r)
                let b0 = s.brep.borrow();
                let all = q::get_edges(&b0).map_err(|e| err(what, e))?;
                let near: Vec<u32> = kept
                    .iter()
                    .copied()
                    .filter(|&i| {
                        bad.iter().any(|p| {
                            q::edge_closest_point(&self.k, &b0, all[i as usize], pt(*p))
                                .is_ok_and(|c| c.closest.distance <= 3.0 * r + 1e-6)
                        })
                    })
                    .collect();
                return Err(failed(
                    format!("the result overlaps itself (size {r} too large for the adjacent faces?)"),
                    if near.is_empty() { kept } else { near },
                ));
            }
            if self.is_solid(s) {
                if !self.is_solid(&t) {
                    return Err(failed(
                        "solidcore returned a body that is not closed (faces missing): the blends of \
                         neighbouring edges collide, size too large for the wall between them?"
                            .into(),
                        kept,
                    ));
                }
                // as the facade: at most what a blend of that size can change near the edges
                let v0 = self.mass_props(s).map_err(|e| check_err(what, e))?.volume;
                let v1 = self.mass_props(&t).map_err(|e| check_err(what, e))?.volume;
                let b0 = s.brep.borrow();
                let reach: f64 = kept
                    .iter()
                    .map(|&i| {
                        let len = edge_length(&self.k, &b0, q::get_edges(&b0).ok().map(|a| a[i as usize]));
                        20.0 * r * r * len + 10.0 * r * r * r
                    })
                    .sum();
                if v1 <= 1e-9 * v0.abs() || (v1 - v0).abs() > reach + 2e-2 * v0.abs() {
                    return Err(failed(
                        format!("solidcore returned a wrong body (volume {v1} instead of about {v0})"),
                        kept,
                    ));
                }
            }
            let fresh = {
                let mut b = t.brep.borrow_mut();
                self.resolve_tags(&mut b, std::slice::from_ref(s), tag, true)?
            };
            if fresh == 0 {
                return Err(failed(format!("solidcore returned the shape unchanged (size {r} too large?)"), kept));
            }
            Ok(t)
        }

        /// The sample points of the faces of a closed solid that are not boundaries between
        /// inside and outside (the facade's `invalid_faces`: a point a little along the
        /// outward normal, 1e-4 of the model size, is not outside, or one against it not
        /// inside): the result overlaps itself. Empty for open bodies; faces without a sample
        /// or whose points cannot be classified are skipped. `near`: only faces whose box meets
        /// one of these boxes (all faces if `None`).
        fn invalid_faces(&mut self, t: &Shape, near: Option<&[(DVec3, DVec3)]>) -> KResult<Vec<DVec3>> {
            if !self.is_solid(t) {
                return Ok(Vec::new());
            }
            let samples: Vec<(DVec3, DVec3)> = {
                let mut b = t.brep.borrow_mut();
                let fs = q::get_faces(&b).map_err(|e| err("fillet (check)", e))?;
                let meets = |b: &solidcore::Brep, f| {
                    let Some(near) = near else { return true };
                    let Ok(x) = q::face_bounding_box(&self.k, b, f, false) else { return true };
                    let (lo, hi) = (
                        DVec3::new(x.min_at(0), x.min_at(1), x.min_at(2)),
                        DVec3::new(x.max_at(0), x.max_at(1), x.max_at(2)),
                    );
                    near.iter().any(|(a, z)| lo.cmple(*z).all() && hi.cmpge(*a).all())
                };
                let fs: Vec<_> = fs.into_iter().filter(|&f| meets(&b, f)).collect();
                fs.into_iter().filter_map(|f| crate::brep::face_sample(&self.k, &mut b, f)).collect()
            };
            let eps = 1e-4 * model_diag(&self.k, &t.brep.borrow());
            let mut bad = Vec::new();
            for (p, n) in samples {
                if let Ok(c) = self.classify(t, &[p + n * eps, p - n * eps])
                    && c != [Class::Outside, Class::Inside]
                {
                    bad.push(p);
                }
            }
            Ok(bad)
        }

        /// Sample points: an interior point (`SmApiFaceInternalPoint`) and the vertices of the
        /// boundary loops; only the vertices while the interior point is not ported.
        pub fn faces(&mut self, s: &Shape) -> KResult<Vec<FaceInfo>> {
            let mut b = s.brep.borrow_mut();
            let b = &mut *b;
            // (`SmApiGetFaces` fails on an empty body: the empty result of a Boolean)
            if b.face_list().is_empty() {
                return Ok(Vec::new());
            }
            let fs = q::get_faces(b).map_err(|e| err("faces", e))?;
            let mut out = Vec::with_capacity(fs.len());
            for (i, f) in fs.into_iter().enumerate() {
                let mut points = Vec::new();
                match q::face_internal_point(&self.k, b, f) {
                    Ok(p) => points.push(dv(p)),
                    Err(e) if e.is_unported() => {}
                    // (fails on tiny faces, e.g. what is left of a box face between fillets
                    // of almost half its width): a sample as the facade's `face_sample`
                    Err(e) => match crate::brep::face_sample(&self.k, b, f) {
                        Some((p, _)) => points.push(p),
                        None => return Err(err("faces", e)),
                    },
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
                out.push(FaceInfo { id: i as FaceId, tag: face_tag(b, f).unwrap_or(s.tag), points });
            }
            Ok(out)
        }

        /// Start point, `samples` interior points of the curve, end point; the tags of the faces
        /// (twice the face's for a seam, as the facade's `sc_edges`); `smooth` where solidcore
        /// finds nothing to blend (`SmEdge::GetBlendEdgeuse` is none: seams, poles, edges
        /// between faces tangent within 2°, open edges), as SMLib.
        pub fn edges(&mut self, s: &Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
            let mut b = s.brep.borrow_mut();
            let b = &mut *b;
            if b.face_list().is_empty() {
                return Ok(Vec::new());
            }
            let es = q::get_edges(b).map_err(|e| err("edges", e))?;
            let mut out = Vec::with_capacity(es.len());
            for (i, e) in es.into_iter().enumerate() {
                let smooth =
                    edge::blend_edgeuse(&self.k, b, e).map_err(|e| err("edges (blend edge use)", e))?.is_none();
                let faces = edge::faces(b, e, None).map_err(|e| err("edges", e))?;
                let mut face_tags: Vec<Tag> = faces.iter().map(|&f| face_tag(b, f).unwrap_or(s.tag)).collect();
                if faces.len() == 1 && edge::edgeuse_count(b, e).map_err(|e| err("edges", e))? >= 4 {
                    face_tags.push(face_tags[0]);
                }
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
                out.push(EdgeInfo { id: i as u32, points, face_tags, smooth });
            }
            Ok(out)
        }

        pub fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
            if let Some(m) = s.mass.get() {
                return Ok(m);
            }
            if let Some(m) = self.derived_mass(s)? {
                return Ok(m);
            }
            if self.is_empty(s) {
                return Ok(MassProps { volume: 0.0, area: 0.0, centroid: DVec3::ZERO });
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

        /// The mass properties of `s` from those of the shape it is a similarity transform of
        /// (along a chain of transforms, computed once at its start), while that shape lives.
        fn derived_mass(&mut self, s: &ShapeData) -> KResult<Option<MassProps>> {
            let derived = s.derived.borrow_mut().take();
            let Some((src, f)) = derived else { return Ok(None) };
            let Some(src) = src.upgrade() else { return Ok(None) };
            let m = self.mass_props(&src)?;
            let det = f.determinant().abs();
            let m = MassProps {
                volume: m.volume * det,
                area: m.area * det.powf(2.0 / 3.0),
                centroid: f.transform_point3(m.centroid),
            };
            s.mass.set(Some(m));
            Ok(Some(m))
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

        pub fn brep(&mut self, s: &Shape) -> KResult<(cad_kernel::brep::Brep, crate::BrepStats)> {
            crate::brep::convert(&mut self.k, &mut s.brep.borrow_mut(), s.tag)
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

    /// [`Kernel::brep`] with conversion statistics.
    #[cfg(feature = "solidcore")]
    pub fn brep_with_stats(&mut self, s: &Shape) -> KResult<(cad_kernel::brep::Brep, BrepStats)> {
        self.inner.brep(s)
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

    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_polyhedron", |i| i.make_polyhedron(points, faces, tag))
    }

    fn transform(&mut self, s: &Shape, m: &DMat4) -> KResult<Shape> {
        with_sc!(self, "transform", |i| i.transform(s, m))
    }

    fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
        with_sc!(self, "boolean", |i| i.boolean(op, shapes, tag))
    }

    fn fillet(&mut self, s: &Shape, edges: &[u32], r: f64, tag: Tag) -> KResult<Shape> {
        let _ = (s, edges, r, tag);
        with_sc!(self, "fillet", |i| i.fillet_like(s, edges, r, tag, false, None))
    }

    fn chamfer(&mut self, s: &Shape, edges: &[u32], d: f64, tag: Tag) -> KResult<Shape> {
        let _ = (s, edges, d, tag);
        with_sc!(self, "chamfer", |i| i.fillet_like(s, edges, d, tag, true, None))
    }

    fn law_stations(&mut self, s: &Shape, edges: &[u32], counts: &[u32]) -> KResult<Vec<Vec<DVec3>>> {
        let _ = (s, edges, counts);
        with_sc!(self, "law_stations", |i| i.law_stations(s, edges, counts))
    }

    fn fillet_variable(&mut self, s: &Shape, edges: &[u32], radii: &[Vec<f64>], tag: Tag) -> KResult<Shape> {
        let _ = (s, edges, radii, tag);
        with_sc!(self, "fillet_variable", |i| i.fillet_like(s, edges, 1.0, tag, false, Some(radii)))
    }

    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "extrude", |i| i.extrude(p, h, twist, scale, tag))
    }

    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "revolve", |i| i.revolve(p, angle, tag))
    }

    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "loft", |i| i.loft(sections, ruled, tag))
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

    fn brep(&mut self, s: &Shape) -> KResult<cad_kernel::brep::Brep> {
        let _ = s;
        with_sc!(self, "brep", |i| i.brep(s).map(|(b, _)| b))
    }

    fn export(&mut self, _shapes: &[Shape], _fmt: cad_kernel::ExportFormat) -> KResult<Vec<u8>> {
        Err(unsupported("export (not wired in kernel-solidcore; STEP goes through `brep`)"))
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

    /// The interpolating spline of a closed spline profile: the same as
    /// `cad_kernel::curve2::interpolate` through the points and the first again, within 1e-12
    /// (a one-off comparison with SMLib's `SmApiCreateCurveInterpPoints` through `Kernel::brep`
    /// of kernel-smlib agreed to 3.6e-15; not part of this test).
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

#[cfg(test)]
mod pin {
    use super::{KERNEL_ID, SOLIDCORE_REV};

    /// `scripts/ci/solidcore.sh` rewrites the pin with `sed` in three places: they must agree.
    #[test]
    fn kernel_id_and_cargo_rev_name_the_pin() {
        assert_eq!(SOLIDCORE_REV.len(), 40, "{SOLIDCORE_REV}");
        assert_eq!(KERNEL_ID, format!("solidcore-{}", &SOLIDCORE_REV[..7]));
        let toml = include_str!("../Cargo.toml");
        assert!(toml.contains(&format!("rev = \"{SOLIDCORE_REV}\"")), "Cargo.toml pins another solidcore commit");
    }
}
