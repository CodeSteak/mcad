//! `hull()` and `minkowski()` of 3D children: the exact special cases (Plan 21, phase A).
//!
//! The children are described as bodies in the operation's system ([`bodies`]): planar bodies
//! by their vertices (boxes, polyhedra, extrusions of polygons), spheres, cylinders and cones
//! (frustums along an axis) and prisms (extrusions of profiles with arcs). Spheres, frustums
//! and prisms need a similarity (rotation, mirror, uniform scale, translation) on the way from
//! the primitive to the operation; planar bodies take any affine map. Then one exact
//! construction is chosen ([`hull_plan`], [`minkowski_plan`]); anything else is reported with
//! its reason and the engine shows the children unchanged.
//!
//! | `hull` of | construction |
//! |---|---|
//! | planar bodies only | convex hull of the vertices → polyhedron |
//! | spheres, cylinders, cones on one axis | 2D hull of the half sections (ρ, t) → revolve |
//! | prisms along one direction over the same range (cylinders, boxes, extrusions) | 2D hull of the sections → extrude |
//! | equal spheres, centres not in a plane | hull of the centres → offset by r |
//!
//! | `minkowski` of | construction |
//! |---|---|
//! | a body + spheres | offset by the sum of the radii, moved by the sum of the centres |
//! | spheres only | a sphere |
//! | two parallel prisms (e.g. `linear_extrude` + `cylinder`) | 2D Minkowski sum of the sections (circle: rounded offset; two convex, one a polygon: hull), extruded over the summed range |
//! | two convex planar bodies | hull of the pairwise vertex sums → polyhedron |

use cad_ir::{Dim, NodeIdx, Op, Prim, Tree};
use cad_kernel::hull3::{Flat, Hull3, hull3};
use cad_kernel::region2::{self, Fill, Loop2, Operand};
use cad_kernel::{BoolOp, Segment, hull2};
use glam::{DAffine2, DMat4, DVec2, DVec3, DVec4};

use crate::op2d;
use crate::profile::{self, Expr2};

/// `hull()` of an imported mesh takes at most this many vertices (the hull of n points on a
/// sphere has about 2n faces; 2000 points: ~1.5 s in the kernel).
pub(crate) const MAX_MESH_HULL_POINTS: usize = 2000;

/// A right prism: `section` in the xy-plane of the rigid `frame`, from `z0` to `z0 + h` along
/// the frame's z-axis.
#[derive(Debug, Clone)]
pub(crate) struct Prism {
    pub frame: DMat4,
    pub z0: f64,
    pub h: f64,
    pub section: Vec<Loop2>,
}

#[derive(Debug, Clone)]
pub(crate) enum Body {
    /// Planar faces only: the vertices; as prisms along these axes (boxes, extrusions under a
    /// similarity); `convex` if known to be convex.
    Planar {
        pts: Vec<DVec3>,
        prisms: Vec<Prism>,
        convex: bool,
    },
    Sphere {
        c: DVec3,
        r: f64,
    },
    /// Cylinder or cone from `a` along the unit vector `d` (radius `r1` at `a`, `r2` at the
    /// other end).
    Frustum {
        a: DVec3,
        d: DVec3,
        h: f64,
        r1: f64,
        r2: f64,
    },
    /// Extrusion of a profile with arcs (no twist, no scale).
    Prism(Prism),
}

/// Uniform scale factor of the linear part of `m` if it is a similarity.
fn similarity(m: &DMat4) -> Option<f64> {
    let (x, y, z) = (m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate());
    let s = x.length();
    let ok = s > 0.0
        && (y.length() - s).abs() <= 1e-9 * s
        && (z.length() - s).abs() <= 1e-9 * s
        && x.dot(y).abs() <= 1e-9 * s * s
        && x.dot(z).abs() <= 1e-9 * s * s
        && y.dot(z).abs() <= 1e-9 * s * s;
    ok.then_some(s)
}

fn rigid(m: &DMat4, s: f64) -> DMat4 {
    *m * DMat4::from_scale(DVec3::splat(1.0 / s))
}

/// Rigid frame with its z-axis along `d` at `o`.
fn axis_frame(o: DVec3, d: DVec3) -> DMat4 {
    let x = d.any_orthonormal_vector();
    let y = d.cross(x);
    DMat4::from_cols(x.extend(0.0), y.extend(0.0), d.extend(0.0), o.extend(1.0))
}

fn scaled(loops: &[Loop2], s: f64) -> Vec<Loop2> {
    let m = DAffine2::from_scale(DVec2::splat(s));
    loops.iter().map(|lp| lp.iter().map(|g| g.map(&m)).collect()).collect()
}

fn circle_loop(c: DVec2, r: f64) -> Loop2 {
    use std::f64::consts::{PI, TAU};
    vec![Segment::Arc { center: c, r, a0: 0.0, a1: PI }, Segment::Arc { center: c, r, a0: PI, a1: TAU }]
}

fn poly_loop(p: &[DVec2]) -> Loop2 {
    (0..p.len()).map(|i| Segment::Line { a: p[i], b: p[(i + 1) % p.len()] }).collect()
}

impl Body {
    /// The body as prisms (empty if it is none).
    fn prisms(&self, tol: f64) -> Vec<Prism> {
        match self {
            Body::Planar { prisms, .. } => prisms.clone(),
            Body::Prism(p) => vec![p.clone()],
            Body::Frustum { a, d, h, r1, r2 } if (r1 - r2).abs() <= tol => {
                vec![Prism { frame: axis_frame(*a, *d), z0: 0.0, h: *h, section: vec![circle_loop(DVec2::ZERO, *r1)] }]
            }
            _ => Vec::new(),
        }
    }

    fn extent(&self, lo: &mut DVec3, hi: &mut DVec3) {
        let mut add = |p: DVec3, r: f64| {
            *lo = lo.min(p - DVec3::splat(r));
            *hi = hi.max(p + DVec3::splat(r));
        };
        match self {
            Body::Planar { pts, .. } => pts.iter().for_each(|p| add(*p, 0.0)),
            Body::Sphere { c, r } => add(*c, *r),
            Body::Frustum { a, d, h, r1, r2 } => {
                add(*a, *r1);
                add(*a + *d * *h, *r2);
            }
            Body::Prism(p) => {
                for s in p.section.iter().flatten() {
                    for z in [p.z0, p.z0 + p.h] {
                        add(p.frame.transform_point3(s.start().extend(z)), 0.0);
                    }
                    if let Segment::Arc { center, r, .. } = s {
                        add(p.frame.transform_point3(center.extend(p.z0)), *r);
                        add(p.frame.transform_point3(center.extend(p.z0 + p.h)), *r);
                    }
                }
            }
        }
    }
}

/// Size (bounding box diagonal) of the bodies and the tolerance of comparisons.
fn size_tol(bodies: &[Body]) -> (f64, f64) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for b in bodies {
        b.extent(&mut lo, &mut hi);
    }
    let size = if lo.x.is_finite() { (hi - lo).length().max(1e-6) } else { 1.0 };
    (size, 1e-9 * size.max(1.0))
}

/// The bodies of the subtree `n` whose system maps into the operation's by `m` (the bodies of
/// unions, groups, transforms and nested hulls are collected). `Err`: why the subtree is no
/// special case.
pub(crate) fn bodies(tree: &Tree, n: NodeIdx, m: DMat4, out: &mut Vec<Body>) -> Result<(), String> {
    let node = tree.node(n);
    let kids = |out: &mut Vec<Body>| -> Result<(), String> {
        for c in tree.solid_children(n) {
            if tree.dim(c) == Some(Dim::Two) {
                continue;
            }
            bodies(tree, c, m * tree.node(c).local, out)?;
        }
        Ok(())
    };
    match &node.op {
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty | Op::Hull => kids(out),
        Op::Boolean { op: BoolOp::Union, fillet, chamfer, fillet_law }
            if !fillet.is_some_and(|r| r > 0.0) && !chamfer.is_some_and(|c| c > 0.0) && fillet_law.is_none() =>
        {
            kids(out)
        }
        Op::Prim { prim, place } => {
            if node.blend.is_some_and(|(_, r)| r > 0.0) {
                return Err(format!("{} mit $fillet/$chamfer", node.op.label()));
            }
            let m2 = m * *place;
            match prim {
                Prim::Box { min, max } => {
                    let pts = (0..8)
                        .map(|i| {
                            let c = DVec3::new(
                                if i & 1 == 0 { min.x } else { max.x },
                                if i & 2 == 0 { min.y } else { max.y },
                                if i & 4 == 0 { min.z } else { max.z },
                            );
                            m2.transform_point3(c)
                        })
                        .collect();
                    let prisms = match similarity(&m2) {
                        Some(s) => box_prisms(&rigid(&m2, s), *min * s, *max * s),
                        None => Vec::new(),
                    };
                    out.push(Body::Planar { pts, prisms, convex: true });
                }
                Prim::Polyhedron { points, .. } => {
                    let pts = points.iter().map(|p| m2.transform_point3(*p)).collect();
                    out.push(Body::Planar { pts, prisms: Vec::new(), convex: false });
                }
                // an imported mesh (B-rep or not): its vertices
                Prim::Mesh { mesh, .. } if mesh.mesh.points.len() > MAX_MESH_HULL_POINTS => {
                    return Err(format!(
                        "Netz mit {} Ecken (höchstens {MAX_MESH_HULL_POINTS}: die Hülle würde zu viele Flächen haben)",
                        mesh.mesh.points.len()
                    ));
                }
                Prim::Mesh { mesh, .. } => {
                    let pts = mesh.mesh.points.iter().map(|p| m2.transform_point3(*p)).collect();
                    out.push(Body::Planar { pts, prisms: Vec::new(), convex: false });
                }
                Prim::Sphere { r } => {
                    let s = similarity(&m2).ok_or("verzerrte Kugel (ungleichmäßige Skalierung)")?;
                    out.push(Body::Sphere { c: m2.transform_point3(DVec3::ZERO), r: r * s });
                }
                Prim::Cylinder { h, r1, r2 } => {
                    let s = similarity(&m2).ok_or("verzerrter Zylinder (ungleichmäßige Skalierung)")?;
                    out.push(Body::Frustum {
                        a: m2.transform_point3(DVec3::ZERO),
                        d: m2.transform_vector3(DVec3::Z).normalize(),
                        h: h * s,
                        r1: r1 * s,
                        r2: r2 * s,
                    });
                }
                Prim::Torus { .. } => return Err("Torus".into()),
            }
            Ok(())
        }
        Op::LinearExtrude { h, twist, scale, center } => extrusion(tree, n, m, *h, *twist, *scale, *center, out),
        other => Err(format!("`{}` als Kind", other.label())),
    }
}

/// A box `[min, max]` (already scaled) in the rigid frame `r` as prisms along its three axes.
fn box_prisms(r: &DMat4, min: DVec3, max: DVec3) -> Vec<Prism> {
    let rect = |a: DVec2, b: DVec2| vec![poly_loop(&[a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)])];
    // frames whose z-axis is the box's x, y, z: (u, v, w) → point
    let px = DMat4::from_cols(DVec4::Y, DVec4::Z, DVec4::X, DVec4::W);
    let py = DMat4::from_cols(DVec4::Z, DVec4::X, DVec4::Y, DVec4::W);
    vec![
        Prism { frame: *r, z0: min.z, h: max.z - min.z, section: rect(min.truncate(), max.truncate()) },
        Prism {
            frame: *r * px,
            z0: min.x,
            h: max.x - min.x,
            section: rect(DVec2::new(min.y, min.z), DVec2::new(max.y, max.z)),
        },
        Prism {
            frame: *r * py,
            z0: min.y,
            h: max.y - min.y,
            section: rect(DVec2::new(min.z, min.x), DVec2::new(max.z, max.x)),
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn extrusion(
    tree: &Tree,
    n: NodeIdx,
    m: DMat4,
    h: f64,
    twist: f64,
    scale: DVec2,
    center: bool,
    out: &mut Vec<Body>,
) -> Result<(), String> {
    let mut warn = Vec::new();
    let mut args = Vec::new();
    for c in tree.solid_children(n) {
        if profile::open_sketch_in(tree, c) {
            return Err("linear_extrude einer offenen Skizze".into());
        }
        args.push(profile::lift(tree, c, tree.node(c).local, &mut warn)?);
    }
    let e = Expr2::Op(BoolOp::Union, args);
    if e.leaves().iter().any(|l| !profile::is_planar(&l.m)) {
        return Err("Profil nicht in der xy-Ebene".into());
    }
    let loops = op2d::expr_loops(&e)?;
    if loops.is_empty() || h <= 0.0 {
        return Ok(());
    }
    let z0 = if center { -h / 2.0 } else { 0.0 };
    let lines_only = loops.iter().flatten().all(|s| matches!(s, Segment::Line { .. }));
    let sim = similarity(&m);
    if lines_only {
        if twist != 0.0 {
            return Err("linear_extrude mit twist".into());
        }
        let mut pts = Vec::new();
        for s in loops.iter().flatten() {
            let v = s.start();
            pts.push(m.transform_point3(v.extend(z0)));
            pts.push(m.transform_point3((v * scale).extend(z0 + h)));
        }
        let prisms = match sim {
            Some(s) if scale == DVec2::ONE => {
                vec![Prism { frame: rigid(&m, s), z0: z0 * s, h: h * s, section: scaled(&loops, s) }]
            }
            _ => Vec::new(),
        };
        out.push(Body::Planar { pts, prisms, convex: hull2::is_convex(&loops) });
        return Ok(());
    }
    let Some(s) = sim else { return Err("linear_extrude mit Bögen unter ungleichmäßiger Skalierung".into()) };
    // a single circle: a cylinder or cone
    if let [lp] = loops.as_slice()
        && let Some((c, r)) = single_circle(lp)
        && scale.x == scale.y
        && (twist == 0.0 || c.length() <= 1e-12 * r)
    {
        out.push(Body::Frustum {
            a: m.transform_point3(c.extend(z0)),
            d: m.transform_vector3(DVec3::Z).normalize(),
            h: h * s,
            r1: r * s,
            r2: r * scale.x * s,
        });
        return Ok(());
    }
    if twist != 0.0 || scale != DVec2::ONE {
        return Err("linear_extrude eines Profils mit Bögen und twist/scale".into());
    }
    out.push(Body::Prism(Prism { frame: rigid(&m, s), z0: z0 * s, h: h * s, section: scaled(&loops, s) }));
    Ok(())
}

/// Centre and radius if the loop consists of arcs of one circle.
fn single_circle(lp: &Loop2) -> Option<(DVec2, f64)> {
    let mut it = lp.iter();
    let Some(Segment::Arc { center, r, .. }) = it.next() else { return None };
    let tol = 1e-9 * r.max(1.0);
    it.all(
        |s| matches!(s, Segment::Arc { center: c, r: rr, .. } if c.distance(*center) <= tol && (rr - r).abs() <= tol),
    )
    .then_some((*center, *r))
}

// ---------------------------------------------------------------- plans

/// What the engine builds.
#[derive(Debug, Clone)]
pub(crate) enum Plan {
    Polyhedron(Hull3),
    /// Revolve `section` (ρ, t; ρ ≥ 0) once around the z-axis of `frame`.
    Revolve {
        frame: DMat4,
        section: Vec<Loop2>,
    },
    /// Extrude `section` along the z-axis of `frame` from `z0` by `h`.
    Extrude {
        frame: DMat4,
        z0: f64,
        h: f64,
        section: Vec<Loop2>,
    },
    /// Offset the polyhedron by `r` (equal spheres).
    Spheres {
        hull: Hull3,
        r: f64,
    },
    /// `minkowski`: offset child `child` (index among the solid 3D children) by `r`, move it.
    Offset {
        child: usize,
        r: f64,
        shift: DVec3,
    },
    Sphere {
        c: DVec3,
        r: f64,
    },
}

impl Plan {
    /// Hint for `mcad inspect` (which special case was taken).
    pub fn hint(&self) -> String {
        match self {
            Plan::Polyhedron(h) => format!("exakt: Polyeder ({} Flächen)", h.faces.len()),
            Plan::Revolve { .. } => "exakt: Rotationskörper".into(),
            Plan::Extrude { .. } => "exakt: Prisma".into(),
            Plan::Spheres { r, .. } => format!("exakt: Hülle der Mittelpunkte + Offset {}", fmt(*r)),
            Plan::Offset { r, .. } => format!("exakt: Offset {}", fmt(*r)),
            Plan::Sphere { .. } => "exakt: Kugel".into(),
        }
    }
}

fn fmt(x: f64) -> String {
    let s = format!("{x:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The exact construction of `hull()` of these bodies, or why there is none.
pub(crate) fn hull_plan(bodies: &[Body]) -> Result<Plan, String> {
    if bodies.is_empty() {
        return Err("keine Körper".into());
    }
    let (size, tol) = size_tol(bodies);
    if bodies.iter().all(|b| matches!(b, Body::Planar { .. })) {
        let pts: Vec<DVec3> = bodies
            .iter()
            .flat_map(|b| if let Body::Planar { pts, .. } = b { pts.clone() } else { Vec::new() })
            .collect();
        return match hull3(&pts, tol) {
            Ok(h) if h.is_closed() => Ok(Plan::Polyhedron(h)),
            Ok(_) => Err("Hülle numerisch nicht geschlossen (fast koplanare Punkte)".into()),
            Err(_) => Err("die Kinder haben kein Volumen".into()),
        };
    }
    if let Some(p) = axis_plan(bodies, size, tol) {
        return Ok(p);
    }
    if let Some(p) = prism_plan(bodies, tol) {
        return Ok(p);
    }
    let spheres: Vec<(DVec3, f64)> =
        bodies.iter().filter_map(|b| if let Body::Sphere { c, r } = b { Some((*c, *r)) } else { None }).collect();
    if spheres.len() == bodies.len() {
        let r = spheres[0].1;
        if spheres.iter().any(|s| (s.1 - r).abs() > tol) {
            return Err("Kugeln mit verschiedenen Radien nicht auf einer Achse".into());
        }
        let centres: Vec<DVec3> = spheres.iter().map(|s| s.0).collect();
        return match hull3(&centres, tol) {
            Ok(hull) => Ok(Plan::Spheres { hull, r }),
            Err(Flat::Plane) => Err("gleich große Kugeln mit Mittelpunkten in einer Ebene".into()),
            Err(_) => Err("Kugeln nicht auf einer Achse".into()),
        };
    }
    let planar = bodies.iter().any(|b| matches!(b, Body::Planar { .. }));
    Err(if planar {
        "ebene und runde Körper gemischt (exakt nur als parallele Prismen gleicher Höhe, z. B. Quader und Zylinder)"
            .into()
    } else {
        "Kugeln, Zylinder und Kegel weder auf einer Achse noch als parallele Zylinder gleicher Höhe".into()
    })
}

/// Spheres, cylinders and cones on one axis: the 2D hull of their half sections, revolved.
fn axis_plan(bodies: &[Body], size: f64, tol: f64) -> Option<Plan> {
    if !bodies.iter().all(|b| matches!(b, Body::Sphere { .. } | Body::Frustum { .. })) {
        return None;
    }
    let (o, d) = match bodies
        .iter()
        .find_map(|b| if let Body::Frustum { a, d, .. } = b { Some((*a, *d)) } else { None })
    {
        Some(x) => x,
        None => {
            let cs: Vec<DVec3> =
                bodies.iter().filter_map(|b| if let Body::Sphere { c, .. } = b { Some(*c) } else { None }).collect();
            let far = cs.iter().copied().max_by(|a, b| a.distance(cs[0]).total_cmp(&b.distance(cs[0])))?;
            if far.distance(cs[0]) <= tol { (cs[0], DVec3::Z) } else { (cs[0], (far - cs[0]).normalize()) }
        }
    };
    let off_axis = |p: DVec3| {
        let v = p - o;
        (v - d * v.dot(d)).length()
    };
    let t = |p: DVec3| (p - o).dot(d);
    let mut loops = Vec::new();
    for b in bodies {
        match b {
            Body::Sphere { c, r } => {
                if off_axis(*c) > tol {
                    return None;
                }
                loops.push(circle_loop(DVec2::new(0.0, t(*c)), *r));
            }
            Body::Frustum { a, d: df, h, r1, r2 } => {
                if df.cross(d).length() > 1e-9 || off_axis(*a) > tol {
                    return None;
                }
                let t0 = t(*a);
                let t1 = t0 + h * df.dot(d).signum();
                let mut p = vec![DVec2::new(-r1, t0), DVec2::new(*r1, t0), DVec2::new(*r2, t1), DVec2::new(-r2, t1)];
                p.dedup_by(|a, b| a.distance(*b) <= tol);
                if p.len() >= 3 {
                    loops.push(poly_loop(&p));
                }
            }
            _ => return None,
        }
    }
    let h = hull2::hull(&loops);
    if h.is_empty() {
        return None;
    }
    // the half ρ ≥ 0
    let b = 4.0 * size + 1.0;
    let half = poly_loop(&[DVec2::new(0.0, -b), DVec2::new(b, -b), DVec2::new(b, b), DVec2::new(0.0, b)]);
    let section =
        region2::combine(&[Operand::new(vec![h], Fill::NonZero), Operand::new(vec![half], Fill::NonZero)], &|x| {
            x[0] && x[1]
        });
    (!section.is_empty()).then(|| Plan::Revolve { frame: axis_frame(o, d), section })
}

/// Range along `d` (a unit vector parallel to the prism's axis) of a prism.
fn prism_range(p: &Prism, d: DVec3) -> (f64, f64) {
    let base = p.frame.w_axis.truncate().dot(d);
    if p.frame.z_axis.truncate().dot(d) > 0.0 {
        (base + p.z0, base + p.z0 + p.h)
    } else {
        (base - p.z0 - p.h, base - p.z0)
    }
}

/// Section of prism `q` in the xy-plane of `frame` (whose z-axis is parallel to q's).
fn section_in(frame: &DMat4, q: &Prism) -> Vec<Loop2> {
    let m = frame.inverse() * q.frame;
    let a = profile::affine2(&m);
    q.section.iter().map(|lp| lp.iter().map(|s| s.map(&a)).collect()).collect()
}

/// For every body a prism along the direction of one of the first body's prisms, over the same
/// range: the first prism and the matching prism of each body.
fn parallel_prisms(bodies: &[Body], tol: f64) -> Option<(Prism, Vec<Prism>)> {
    let first = bodies.first()?.prisms(tol);
    'cand: for c in &first {
        let d = c.frame.z_axis.truncate();
        let range = prism_range(c, d);
        let mut chosen = Vec::new();
        for b in bodies {
            let found = b.prisms(tol).into_iter().find(|q| {
                let qd = q.frame.z_axis.truncate();
                let r = prism_range(q, d);
                qd.cross(d).length() <= 1e-9 && (r.0 - range.0).abs() <= tol && (r.1 - range.1).abs() <= tol
            });
            match found {
                Some(q) => chosen.push(q),
                None => continue 'cand,
            }
        }
        return Some((c.clone(), chosen));
    }
    None
}

/// Prisms along one direction over the same range: the 2D hull of their sections, extruded.
fn prism_plan(bodies: &[Body], tol: f64) -> Option<Plan> {
    let (c, prisms) = parallel_prisms(bodies, tol)?;
    let loops: Vec<Loop2> = prisms.iter().flat_map(|q| section_in(&c.frame, q)).collect();
    let h = hull2::hull(&loops);
    (!h.is_empty()).then(|| Plan::Extrude { frame: c.frame, z0: c.z0, h: c.h, section: vec![h] })
}

/// The exact construction of `minkowski()` of the children (`kids[i]`: the bodies of solid
/// child i, or why it has none), or why there is none.
pub(crate) fn minkowski_plan(kids: &[Result<Vec<Body>, String>]) -> Result<Plan, String> {
    let mut shift = DVec3::ZERO;
    let mut r = 0.0;
    let mut rest = Vec::new();
    for (i, k) in kids.iter().enumerate() {
        match k {
            Ok(b) if matches!(b.as_slice(), [Body::Sphere { .. }]) => {
                if let [Body::Sphere { c, r: rr }] = b.as_slice() {
                    shift += *c;
                    r += rr;
                }
            }
            _ => rest.push(i),
        }
    }
    match rest.as_slice() {
        [] if r > 0.0 => return Ok(Plan::Sphere { c: shift, r }),
        [i] if r > 0.0 => return Ok(Plan::Offset { child: *i, r, shift }),
        [i, j] if r == 0.0 => {
            if let (Ok(a), Ok(b)) = (&kids[*i], &kids[*j])
                && let (Some(p), Some(q)) = (single(a), single(b))
            {
                if let Some(plan) = prism_sum(p, q)? {
                    return Ok(plan);
                }
                if let (Body::Planar { pts: pa, convex: true, .. }, Body::Planar { pts: pb, convex: true, .. }) = (p, q)
                {
                    let sums: Vec<DVec3> = pa.iter().flat_map(|x| pb.iter().map(move |y| *x + *y)).collect();
                    let (_, tol) = size_tol(&[p.clone(), q.clone()]);
                    let tol = tol * 2.0;
                    return hull3(&sums, tol).map(Plan::Polyhedron).map_err(|_| "die Summe hat kein Volumen".into());
                }
            }
        }
        _ => {}
    }
    Err("exakt nur Körper + Kugel(n), zwei parallele Prismen (z. B. linear_extrude und cylinder) oder zwei konvexe \
         ebene Körper"
        .into())
}

fn single(b: &[Body]) -> Option<&Body> {
    if let [x] = b { Some(x) } else { None }
}

/// Minkowski sum of two parallel prisms: the 2D sum of the sections over the summed range.
/// `Ok(None)`: not two parallel prisms; `Err`: parallel, but the 2D sum is no exact case.
fn prism_sum(p: &Body, q: &Body) -> Result<Option<Plan>, String> {
    let (_, tol) = size_tol(&[p.clone(), q.clone()]);
    for a in p.prisms(tol) {
        let d = a.frame.z_axis.truncate();
        for b in q.prisms(tol) {
            if b.frame.z_axis.truncate().cross(d).length() > 1e-9 {
                continue;
            }
            let (ra, rb) = (prism_range(&a, d), prism_range(&b, d));
            // sections in a's frame; the sum of points x + y projects to u(x) + u(y) + u(origin)
            let sa = a.section.clone();
            let sb = section_in(&a.frame, &b);
            let o = a.frame.w_axis.truncate();
            let inv = a.frame.inverse();
            let shift = inv.transform_vector3(o).truncate();
            let sum = section_sum(&sa, &sb).ok_or_else(|| {
                "parallele Prismen, aber die Querschnitte sind weder Profil + Kreis noch zwei konvexe (eines ein Polygon)"
                    .to_string()
            })?;
            let m = DAffine2::from_translation(shift);
            let section: Vec<Loop2> = sum.iter().map(|lp| lp.iter().map(|s| s.map(&m)).collect()).collect();
            let base = o.dot(d);
            let z0 = ra.0 + rb.0 - base;
            let h = (ra.1 - ra.0) + (rb.1 - rb.0);
            return Ok(Some(Plan::Extrude { frame: a.frame, z0, h, section }));
        }
    }
    Ok(None)
}

/// Exact 2D Minkowski sum: a region and a circle (rounded offset, moved), or two convex
/// regions of which one is a polygon.
fn section_sum(a: &[Loop2], b: &[Loop2]) -> Option<Vec<Loop2>> {
    let circle = |l: &[Loop2]| if let [lp] = l { single_circle(lp) } else { None };
    let moved = |l: Vec<Loop2>, c: DVec2| -> Vec<Loop2> {
        let m = DAffine2::from_translation(c);
        l.into_iter().map(|lp| lp.iter().map(|s| s.map(&m)).collect()).collect()
    };
    if let Some((c, r)) = circle(b) {
        return Some(moved(region2::offset_round(a, r), c));
    }
    if let Some((c, r)) = circle(a) {
        return Some(moved(region2::offset_round(b, r), c));
    }
    let poly = |l: &[Loop2]| -> Option<Vec<DVec2>> {
        (l.len() == 1 && l[0].iter().all(|s| matches!(s, Segment::Line { .. })))
            .then(|| l[0].iter().map(|s| s.start()).collect())
    };
    if !hull2::is_convex(a) || !hull2::is_convex(b) {
        return None;
    }
    match (poly(a), poly(b)) {
        (Some(v), _) => Some(vec![hull2::minkowski_convex(&v, b)]),
        (_, Some(v)) => Some(vec![hull2::minkowski_convex(&v, a)]),
        _ => None,
    }
}
