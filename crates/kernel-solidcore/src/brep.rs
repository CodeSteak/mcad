//! solidcore B-rep → neutral [`cad_kernel::brep::Brep`] (plan 1.11), with the conventions of
//! kernel-smlib (`crates/kernel-smlib/src/brep.rs`, the SMLib facade's `sc_brep` stream), so both
//! kernels hand the same B-rep to STEP export, measuring and the tessellator:
//!
//! * vertices in the order of the B-rep's vertex list, edges and faces in the order of `edges`
//!   and `faces` ([`cad_kernel::brep::Face::id`] is the index of `faces`), face tags as `faces`;
//! * every edge curve is taken in its exact B-spline form (lines, circles and ellipses are
//!   B-spline curves in SMLib and solidcore), trimmed to the edge's interval, reversed if it runs
//!   from the end vertex to the start vertex, then recognised as a line or circle
//!   ([`edge_curve`]); other curves (intersection curves, curves on surfaces) are approximated
//!   to the B-rep's tolerance and flagged, as the facade does;
//! * surfaces: planes, cylinders, cones, spheres, tori, surfaces of revolution and extrusion
//!   with their canonical parameters, accepted only if the face's surface lies on them (9 × 9
//!   samples of its UV domain within `1e-6` of the model size), else their B-spline form;
//!   B-spline surfaces as they are; other surfaces (offsets, …) approximated and flagged;
//! * bodies: one per solid region (outer shell and voids), the face uses outside the material
//!   (the mates of the region's face uses), their loops counter-clockwise about the face normal
//!   (solidcore's edge-use order, as SMLib's); no solid region: one sheet of every face's upward
//!   face use;
//! * `same_sense` compares the STEP normal of the face's surface with solidcore's normal of the
//!   written face use at an interior point (as the facade's sample point).

use cad_kernel::brep::{
    BSplineCurve, BSplineSurface, Body, Brep, Edge, Face, Frame, Loop, OrientedEdge, Shell, Surface, edge_curve,
};
use cad_kernel::{KResult, KernelError, Tag};
use glam::DVec3;
use solidcore::curve::curve::Curve as ScCurve;
use solidcore::curve::curve::p2::CurveKind;
use solidcore::intersect::point_class::PointClassType;
use solidcore::nurbs::b_spline_surface_n_lib::p2::OrientType;
use solidcore::query::api_queries as q;
use solidcore::surface::surface::Surface as ScSurface;
use solidcore::surface::surface::p2::SurfaceKind;
use solidcore::topology::edgeuse::p1 as eu;
use solidcore::topology::{FaceuseId, LoopuseStart};
use solidcore::{Point2, SolidError};

use crate::imp::{dv, err, face_tag};

/// Statistics of one conversion (diagnostics, tests), as kernel-smlib's `BrepStats`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrepStats {
    /// Faces whose canonical parameters did not match their surface (kept as B-spline).
    pub analytic_rejected: usize,
    /// Faces whose orientation came from the fallback (no usable interior point).
    pub orientation_fallback: usize,
    /// Edges whose curve ran from the end vertex to the start vertex (reversed).
    pub edges_reversed: usize,
    /// Largest distance between an edge curve's ends and its vertices.
    pub max_vertex_gap: f64,
    /// Faces and edges whose geometry was approximated.
    pub approximated_faces: usize,
    pub approximated_edges: usize,
}

fn bad(msg: impl std::fmt::Display) -> KernelError {
    KernelError::failed(format!("solidcore: brep: {msg}"))
}

fn sc(e: SolidError) -> KernelError {
    err("brep", e)
}

/// Full knot vector from distinct knots and their multiplicities.
fn full_knots(knots: &[f64], mults: &[usize]) -> Vec<f64> {
    knots.iter().zip(mults).flat_map(|(k, m)| std::iter::repeat_n(*k, *m)).collect()
}

/// Weights as kernel-smlib takes them: `None` unless some weight differs from 1 (or another).
fn rational(w: Vec<f64>) -> Option<Vec<f64>> {
    let r = w.first().is_some_and(|w0| *w0 != 1.0 || w.iter().any(|x| (x - w0).abs() > 1e-14 * w0.abs()));
    r.then_some(w)
}

/// The neutral form of a solidcore B-spline curve.
fn nurbs_curve(c: &solidcore::nurbs::b_spline_curve::BSplineCurve) -> KResult<BSplineCurve> {
    let (points, weights) = c.control_polygon().map_err(sc)?;
    let (knots, mults) = c.knots(None).map_err(sc)?;
    let c = BSplineCurve {
        degree: c.degree().map_err(sc)? as u32,
        knots: full_knots(&knots, &mults),
        points: points.into_iter().map(dv).collect(),
        weights: rational(weights),
    };
    c.check().map_err(bad)?;
    Ok(c)
}

/// The exact B-spline form of `c` (its NURB: B-spline curves, lines, ellipses, parabolas,
/// hyperbolas), else an approximation over `[t0, t1]` within `atol` (`true`: approximated), as
/// the facade's `put_edge`.
fn curve_nurbs(k: &solidcore::Kernel, c: &ScCurve, t0: f64, t1: f64, atol: f64) -> KResult<(BSplineCurve, bool)> {
    let exact = match c.kind() {
        CurveKind::BSpline(b) => Some(nurbs_curve(b)?),
        CurveKind::Line(l) => Some(nurbs_curve(&*l.base_with_nurb().map_err(sc)?)?),
        CurveKind::Ellipse(e) => Some(nurbs_curve(e.base())?),
        CurveKind::Parabola(p) => Some(nurbs_curve(p.base())?),
        CurveKind::Hyperbola(h) => Some(nurbs_curve(h.base())?),
        _ => None,
    };
    if let Some(c) = exact {
        return Ok((c, false));
    }
    let opts = solidcore::curve::curve::p1::ApproximateCurveOptions { create_analytics: false, ..Default::default() };
    let a = c.approximate_curve(k, &[t0, t1], atol, &opts).map_err(sc)?;
    match a.curve.kind() {
        CurveKind::BSpline(b) => Ok((nurbs_curve(b)?, true)),
        other => Err(bad(format!("approximation of a curve is no B-spline ({})", kind_name(other)))),
    }
}

fn kind_name(c: &CurveKind) -> &'static str {
    match c {
        CurveKind::BSpline(_) => "B-spline",
        CurveKind::Hermite(_) => "Hermite",
        CurveKind::Hyperbola(_) => "hyperbola",
        CurveKind::Projected(_) => "projected",
        CurveKind::Iso(_) => "iso",
        CurveKind::Offset(_) => "offset",
        CurveKind::Conic(_) => "conic",
        CurveKind::CrvInVolume(_) => "curve in volume",
        CurveKind::CrvOnSurf(_) => "curve on surface",
        CurveKind::TangentField(_) => "tangent field",
        CurveKind::Line(_) => "line",
        CurveKind::Ellipse(_) => "ellipse",
        CurveKind::Parabola(_) => "parabola",
        CurveKind::Composite(_) => "composite",
    }
}

/// The neutral form of a solidcore B-spline surface.
fn nurbs_surface(s: &solidcore::nurbs::b_spline_surface::BSplineSurface) -> KResult<BSplineSurface> {
    let net = s.control_point_net().map_err(sc)?;
    let c = s.canonical().map_err(sc)?;
    let s = BSplineSurface {
        degree_u: c.u_degree as u32,
        degree_v: c.v_degree as u32,
        knots_u: full_knots(&c.u_knots, &c.u_multiplicities),
        knots_v: full_knots(&c.v_knots, &c.v_multiplicities),
        nu: net.u_count,
        nv: net.v_count,
        points: net.points.into_iter().map(dv).collect(),
        weights: rational(net.weights),
    };
    s.check().map_err(bad)?;
    Ok(s)
}

/// The B-spline form of `s`: the NURB of a B-spline surface or an analytic one (planes, cones,
/// spheres, tori, swept surfaces are B-spline surfaces in SMLib and solidcore), else an
/// approximation within `atol` (`true`: approximated), as the facade's `put_surface`.
fn surface_nurbs(k: &solidcore::Kernel, s: &ScSurface, atol: f64) -> KResult<(BSplineSurface, bool)> {
    let exact = match s.kind_with_nurb(k).map_err(sc)? {
        SurfaceKind::BSpline(b) => Some(nurbs_surface(b)?),
        SurfaceKind::Plane(p) => Some(nurbs_surface(&*p.base_with_nurb().map_err(sc)?)?),
        SurfaceKind::Cone(c) => Some(nurbs_surface(c.rev().base())?),
        SurfaceKind::Sphere(x) => Some(nurbs_surface(x.base().base())?),
        SurfaceKind::Torus(t) => Some(nurbs_surface(t.base().base())?),
        SurfaceKind::SurfOfRevolution(r) => Some(nurbs_surface(r.base())?),
        SurfaceKind::SurfOfExtrusion(e) => Some(nurbs_surface(e.base())?),
        _ => None,
    };
    if let Some(n) = exact {
        return Ok((n, false));
    }
    let a = s.approximate_surface(k, atol, 5, false).map_err(sc)?;
    match a.surface.as_ref().map(|s| s.kind()) {
        Some(SurfaceKind::BSpline(b)) => Ok((nurbs_surface(b)?, true)),
        _ => Err(bad("approximation of a surface is no B-spline")),
    }
}

fn frame(p: &solidcore::geom::axis2_placement::Axis2Placement) -> Option<Frame> {
    Frame::new(dv(p.origin()), DVec3::new(p.z_axis().x, p.z_axis().y, p.z_axis().z), {
        let x = p.x_axis();
        DVec3::new(x.x, x.y, x.z)
    })
}

/// The canonical form of an analytic surface (`None`: not analytic, or degenerate
/// parameters), as kernel-smlib's `analytic`.
fn analytic(k: &solidcore::Kernel, s: &ScSurface, atol: f64) -> KResult<Option<Surface>> {
    let pos = |x: f64| x.is_finite() && x > 0.0;
    let gen_curve = |c: Option<&ScCurve>| -> KResult<Option<BSplineCurve>> {
        let Some(c) = c else { return Ok(None) };
        let iv = c.natural_interval();
        Ok(Some(curve_nurbs(k, c, iv.min(), iv.max(), atol)?.0))
    };
    Ok(match s.kind() {
        SurfaceKind::Plane(p) => frame(p.position()).map(|frame| Surface::Plane { frame }),
        SurfaceKind::Cone(c) => {
            let Some(mut f) = frame(c.rev().position()) else { return Ok(None) };
            let (r, a) = (c.base_radius(), c.semi_angle_deg().to_radians());
            if c.is_cylinder() {
                return Ok(pos(r).then_some(Surface::Cylinder { frame: f, r }));
            }
            if !(r >= 0.0 && a.is_finite()) || a.abs() >= std::f64::consts::FRAC_PI_2 {
                return Ok(None);
            }
            if a < 0.0 {
                // the same cone with the axis reversed: r + h tan(a) = r + (-h) tan(-a)
                f.z = -f.z;
            }
            Some(Surface::Cone { frame: f, r, semi_angle: a.abs() })
        }
        SurfaceKind::Sphere(x) => {
            frame(x.base().position()).filter(|_| pos(x.radius())).map(|f| Surface::Sphere { frame: f, r: x.radius() })
        }
        SurfaceKind::Torus(t) => frame(t.base().position())
            .filter(|_| pos(t.major_radius()) && pos(t.minor_radius()))
            .map(|f| Surface::Torus { frame: f, major: t.major_radius(), minor: t.minor_radius() }),
        SurfaceKind::SurfOfRevolution(r) => {
            let (Some(f), Some(curve)) = (frame(r.position()), gen_curve(r.gen_curve())?) else { return Ok(None) };
            Some(Surface::Revolution { curve, axis_origin: f.origin, axis_dir: f.z })
        }
        SurfaceKind::SurfOfExtrusion(e) => {
            let v = e.extrusion_vector();
            let dir = DVec3::new(v.x, v.y, v.z);
            let Some(curve) = gen_curve(e.gen_curve())? else { return Ok(None) };
            (dir.length() > 0.0 && dir.is_finite()).then_some(Surface::Extrusion { curve, dir })
        }
        _ => None,
    })
}

/// Does `s` contain the surface of a face (9 × 9 samples of its UV domain within `tol`)?
fn on_surface(
    k: &solidcore::Kernel,
    s: &Surface,
    srf: &ScSurface,
    d: &solidcore::geom::extent2d::Extent2d,
    tol: f64,
) -> bool {
    (0..9).all(|i| {
        (0..9).all(|j| {
            let u = d.u_min() + (d.u_max() - d.u_min()) * f64::from(i) / 8.0;
            let v = d.v_min() + (d.v_max() - d.v_min()) * f64::from(j) / 8.0;
            srf.evaluate_point(k, Point2::new(u, v)).is_ok_and(|p| s.distance(dv(p)).is_some_and(|d| d <= tol))
        })
    })
}

/// The face's UV domain, else the surface's natural one.
fn uv_domain(
    k: &solidcore::Kernel,
    f: &solidcore::topology::Face,
    srf: &ScSurface,
) -> KResult<solidcore::geom::extent2d::Extent2d> {
    let d = f.uv_domain;
    if d.u_min() <= d.u_max() && d.v_min() <= d.v_max() && d.u_min().is_finite() && d.v_max().is_finite() {
        return Ok(d);
    }
    srf.natural_uv_domain_with_nurb(k).map_err(sc)
}

/// A point of face `f` and its (u, v), as the facade's `face_sample`: the grid point (7 × 7, else
/// 23 × 23 cells of the UV domain `fd`) nearest the domain's centre that lies in the face, else
/// the interior point (a pole or a seam, where the STEP normal is not defined, is no cell centre).
fn sample_point(
    k: &solidcore::Kernel,
    b: &mut solidcore::Brep,
    f: solidcore::FaceId,
    fd: &solidcore::geom::extent2d::Extent2d,
) -> Option<(DVec3, Point2)> {
    let mut uv = None;
    for m in [7u32, 23] {
        let mut cells: Vec<(f64, f64)> = (0..m * m)
            .map(|c| ((f64::from(c / m) + 0.5) / f64::from(m), (f64::from(c % m) + 0.5) / f64::from(m)))
            .collect();
        cells.sort_by(|x, y| ((x.0 - 0.5).hypot(x.1 - 0.5)).total_cmp(&(y.0 - 0.5).hypot(y.1 - 0.5)));
        uv = cells
            .into_iter()
            .map(|(s, t)| {
                Point2::new(fd.u_min() + s * (fd.u_max() - fd.u_min()), fd.v_min() + t * (fd.v_max() - fd.v_min()))
            })
            .find(|&uv| q::face_classify_uv(k, b, f, uv).is_ok_and(|c| c == PointClassType::Face));
        if uv.is_some() {
            break;
        }
    }
    match uv {
        Some(uv) => b
            .face(f)
            .ok()
            .and_then(|x| x.surface.as_ref())
            .and_then(|srf| Some((dv(srf.evaluate_point(k, uv).ok()?), uv))),
        None => q::face_internal_point(k, b, f).ok().and_then(|p| {
            let srf = b.face(f).ok()?.surface.as_ref()?;
            Some((dv(p), *q::surface_closest_point(k, srf, p).ok()?.uvs.first()?))
        }),
    }
}

/// Whether the upward face use of `f` lies in a solid region (then the surface normal points
/// into the material).
fn upward_in_material(b: &solidcore::Brep, f: solidcore::FaceId) -> Option<bool> {
    let fu = b.face(f).ok()?.faceuse?;
    let region = b.faceuse(fu).ok()?.shell.and_then(|s| b.shell(s).ok()?.region);
    Some(region.is_some_and(|r| b.region(r).is_ok_and(|x| !x.is_infinite_region(b, r) && x.is_void() == 0)))
}

/// The unit normal of the surface of `f` at `uv`, out of the material.
fn outward(k: &solidcore::Kernel, b: &solidcore::Brep, f: solidcore::FaceId, uv: Point2) -> Option<DVec3> {
    let srf = b.face(f).ok()?.surface.as_ref()?;
    let n = solidcore::query::brep::evaluate_surface_normal(k, Some(srf), uv).ok()?;
    let n = DVec3::new(n.x, n.y, n.z).try_normalize()?;
    Some(if upward_in_material(b, f)? { -n } else { n })
}

/// The normal of face `f` out of the material at the point of its surface closest to `p`.
pub(crate) fn outward_normal(
    k: &solidcore::Kernel,
    b: &solidcore::Brep,
    f: solidcore::FaceId,
    p: DVec3,
) -> Option<DVec3> {
    let srf = b.face(f).ok()?.surface.as_ref()?;
    let c = q::surface_closest_point(k, srf, solidcore::Point3::new(p.x, p.y, p.z)).ok()?;
    outward(k, b, f, *c.uvs.first()?)
}

/// A point of face `f` and the face's unit normal there, out of the material (the facade's
/// `face_sample`: the surface normal, flipped where the upward face use lies in a solid region).
pub(crate) fn face_sample(
    k: &solidcore::Kernel,
    b: &mut solidcore::Brep,
    f: solidcore::FaceId,
) -> Option<(DVec3, DVec3)> {
    let fd = {
        let face = b.face(f).ok()?;
        uv_domain(k, face, face.surface.as_ref()?).ok()?
    };
    let (p, uv) = sample_point(k, b, f, &fd)?;
    Some((p, outward(k, b, f, uv)?))
}

struct Ctx {
    vix: std::collections::HashMap<solidcore::VertexId, u32>,
    eix: std::collections::HashMap<solidcore::EdgeId, u32>,
    fix: std::collections::HashMap<solidcore::FaceId, u32>,
    atol: f64,
    tol_srf: f64,
    default_tag: Tag,
}

/// The face of face use `fu` (its loops, its side), as the facade's `put_face`.
fn face(k: &solidcore::Kernel, b: &mut solidcore::Brep, cx: &Ctx, st: &mut BrepStats, fu: FaceuseId) -> KResult<Face> {
    let fuse = b.faceuse(fu).map_err(sc)?;
    let (f, fu_same) = (fuse.face.ok_or_else(|| bad("face use without face"))?, fuse.orientation == OrientType::Same);
    let id = *cx.fix.get(&f).ok_or_else(|| bad("face use of an unknown face"))?;
    let tag = face_tag(b, f).unwrap_or(cx.default_tag);
    let mut loops = Vec::new();
    for lu in eu::faceuse_loopuses(b, fu, false, None).map_err(sc)? {
        if let LoopuseStart::Vertexuse(vu) = b.loopuse(lu).map_err(sc)?.start {
            let vu = vu.ok_or_else(|| bad("vertex loop without vertex use"))?;
            let v = b.vertexuse(vu).map_err(sc)?.vertex.ok_or_else(|| bad("vertex loop without vertex"))?;
            loops.push(Loop::Vertex(*cx.vix.get(&v).ok_or_else(|| bad("vertex loop of an unknown vertex"))?));
            continue;
        }
        let eus = eu::loopuse_edgeuses(b, lu).map_err(sc)?;
        if eus.is_empty() {
            return Err(bad("empty loop"));
        }
        let mut es = Vec::with_capacity(eus.len());
        for e in eus {
            let edge = eu::edge(b, e).map_err(sc)?.ok_or_else(|| bad("edge use without edge"))?;
            let edge = *cx.eix.get(&edge).ok_or_else(|| bad("edge use of an unknown edge"))?;
            es.push(OrientedEdge { edge, forward: eu::orientation(b, e).map_err(sc)? == OrientType::Same });
        }
        loops.push(Loop::Edges(es));
    }

    let (fd, srf) = {
        let face = b.face(f).map_err(sc)?;
        let srf = face.surface.as_ref().ok_or_else(|| bad("face without surface"))?;
        (uv_domain(k, face, srf)?, srf)
    };
    let (surface, approximated) = match analytic(k, srf, cx.atol)? {
        Some(s) if on_surface(k, &s, srf, &fd, cx.tol_srf) => (s, false),
        a => {
            st.analytic_rejected += usize::from(a.is_some());
            let (n, approx) = surface_nurbs(k, srf, cx.atol)?;
            (Surface::BSpline(n), approx)
        }
    };
    st.approximated_faces += usize::from(approximated);
    // the normal of the written face use at a point of the face, against the STEP normal
    let srf_normal = |srf: &ScSurface, uv: Point2| -> Option<DVec3> {
        let n = solidcore::query::brep::evaluate_surface_normal(k, Some(srf), uv).ok()?;
        let n = DVec3::new(n.x, n.y, n.z).try_normalize()?;
        Some(if fu_same { n } else { -n })
    };
    let bspline_uv = |p: DVec3, uv: Point2| match &surface {
        Surface::BSpline(n) if (n.eval(uv.x, uv.y) - p).length() <= 1e-6 * (1.0 + p.length()) => Some((uv.x, uv.y)),
        _ => None,
    };
    let sample = sample_point(k, b, f, &fd);
    let sense = sample.and_then(|(p, uv)| {
        let n = srf_normal(b.face(f).ok()?.surface.as_ref()?, uv)?;
        surface.normal_at(p, bspline_uv(p, uv)).map(|ns| ns.dot(n) > 0.0)
    });
    let same_sense = match sense {
        Some(s) => s,
        None => {
            // at the centre of the face's UV domain
            st.orientation_fallback += 1;
            let srf = b.face(f).map_err(sc)?.surface.as_ref().ok_or_else(|| bad("face without surface"))?;
            let uv = fd.mid();
            let c = srf.evaluate_point(k, uv).ok().map(dv);
            match (c, srf_normal(srf, uv)) {
                (Some(p), Some(n)) => surface.normal_at(p, bspline_uv(p, uv)).map_or(fu_same, |ns| ns.dot(n) > 0.0),
                _ => fu_same,
            }
        }
    };
    Ok(Face { id, tag, surface, same_sense, loops, approximated })
}

/// The neutral B-rep of `b` (`default_tag` for faces without a tag).
pub fn convert(k: &mut solidcore::Kernel, b: &mut solidcore::Brep, default_tag: Tag) -> KResult<(Brep, BrepStats)> {
    use solidcore::topology::edge;
    let mut st = BrepStats::default();
    if b.face_list().is_empty() {
        return Ok((Brep::default(), st));
    }
    let vs = b.vertex_list().to_vec();
    let mut vertices = Vec::with_capacity(vs.len());
    for &v in &vs {
        vertices.push(dv(q::vertex_get_point(b, v).map_err(sc)?));
    }
    let es = q::get_edges(b).map_err(sc)?;
    let fs = q::get_faces(b).map_err(sc)?;
    let (lo, hi) =
        vertices.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(a, c), p| (a.min(*p), c.max(*p)));
    let size = if vertices.is_empty() { 1.0 } else { (hi - lo).length().max(1.0) };
    // the facade's `approx_tol`: half the largest zone tolerance of the B-rep, its faces, edges
    let mut zone = solidcore::geom::tol::brep_zone_tol_3d(k, b, solidcore::geom::tol::SmallTopoType::NormalSize);
    for &f in &fs {
        zone = zone.max(b.face(f).map_err(sc)?.tolerance);
    }
    for &e in &es {
        zone = zone.max(b.edge(e).map_err(sc)?.tolerance);
    }
    let zone = if zone > 0.0 && zone.is_finite() { zone } else { 1e-6 };
    let cx = Ctx {
        vix: vs.iter().enumerate().map(|(i, v)| (*v, i as u32)).collect(),
        eix: es.iter().enumerate().map(|(i, e)| (*e, i as u32)).collect(),
        fix: fs.iter().enumerate().map(|(i, f)| (*f, i as u32)).collect(),
        atol: zone / 2.0,
        tol_srf: 1e-6 * size,
        default_tag,
    };
    let tol_rec = 1e-9 * size;

    let mut edges = Vec::with_capacity(es.len());
    for &e in &es {
        let (a, z) = edge::start_end_vertices(b, e).map_err(sc)?;
        let (Some(a), Some(z)) = (a, z) else { return Err(bad("edge without vertices")) };
        let (start, end) = (cx.vix[&a], cx.vix[&z]);
        let ed = b.edge(e).map_err(sc)?;
        let c = ed.curve.as_ref().ok_or_else(|| bad("edge without curve"))?;
        let (t0, t1) = (ed.interval.min(), ed.interval.max());
        let (nurbs, approximated) = curve_nurbs(k, c, t0, t1, cx.atol)?;
        // an approximation runs over [t0, t1] already
        let (t0, t1) = if approximated { nurbs.domain() } else { (t0, t1) };
        let (curve, reversed, gap) =
            edge_curve(&nurbs, t0, t1, vertices[start as usize], vertices[end as usize], start == end, tol_rec);
        st.edges_reversed += usize::from(reversed);
        st.max_vertex_gap = st.max_vertex_gap.max(gap);
        st.approximated_edges += usize::from(approximated);
        edges.push(Edge { start, end, curve, approximated });
    }

    // bodies: one per solid region (outer shell(s) and voids), else one sheet body
    let mut solids: Vec<Vec<(solidcore::ShellId, bool)>> = Vec::new();
    for r in solidcore::topology::brep::p1::regions(b, None).map_err(sc)? {
        let reg = b.region(r).map_err(sc)?;
        if reg.is_infinite_region(b, r) || reg.is_void() != 0 {
            continue;
        }
        let shells: Vec<solidcore::ShellId> = reg
            .shells
            .iter()
            .copied()
            .filter(|&s| b.shell(s).is_ok_and(solidcore::topology::Shell::is_faceuse_shell))
            .collect();
        let mut body = Vec::with_capacity(shells.len());
        for &s in &shells {
            let inner = shells.len() > 1
                && solidcore::topology::Shell::is_inner_shell(k, b, s).map_err(|e| err("brep (inner shell)", e))?;
            body.push((s, inner));
        }
        if !body.is_empty() {
            solids.push(body);
        }
    }
    let mut bodies = Vec::new();
    if solids.is_empty() {
        let mut faces = Vec::with_capacity(fs.len());
        for &f in &fs {
            let fu = b.face(f).map_err(sc)?.faceuse.ok_or_else(|| bad("face without face use"))?;
            faces.push(face(k, b, &cx, &mut st, fu)?);
        }
        bodies.push(Body::Sheet { shells: vec![Shell { faces }] });
    }
    for body in solids {
        let (mut outers, mut voids) = (Vec::new(), Vec::new());
        for (s, inner) in body {
            let mut faces = Vec::new();
            for fu in b.shell(s).map_err(sc)?.get_faceuses().map_err(sc)? {
                // the mate face use lies outside the material
                let mate = b.faceuse(fu).map_err(sc)?.mate.ok_or_else(|| bad("face use without mate"))?;
                faces.push(face(k, b, &cx, &mut st, mate)?);
            }
            if inner { voids.push(Shell { faces }) } else { outers.push(Shell { faces }) }
        }
        if outers.is_empty() {
            return Err(bad("solid without outer shell"));
        }
        // several outer shells (bodies that only touch): one solid each; voids go to the first
        let mut it = outers.into_iter();
        bodies.push(Body::Solid { outer: it.next().expect("outer shell"), voids });
        bodies.extend(it.map(|outer| Body::Solid { outer, voids: Vec::new() }));
    }
    let out = Brep { vertices, edges, bodies };
    out.validate().map_err(bad)?;
    Ok((out, st))
}
