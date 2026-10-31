//! `sc_brep` stream (see `smlib/facade/sc_facade.h`) → neutral [`cad_kernel::brep::Brep`].
//!
//! The facade writes every surface and curve as exact NURBS, analytic surfaces additionally with
//! their canonical parameters. Here
//! * analytic parameters are accepted only if the NURBS lies on the analytic surface (9 x 9
//!   samples within `1e-6` of the model size), otherwise the face stays NURBS;
//! * edge curves are trimmed to the edge's interval, then recognised as lines and circles where
//!   they are exactly that (`1e-9` of the model size);
//! * each face's `same_sense` compares the STEP normal of its surface with the normal SMLib
//!   reports for the written side (out of the material) at an interior sample point.

use cad_kernel::brep::{
    BSplineCurve, BSplineSurface, Body, Brep, Edge, Face, Frame, Loop, OrientedEdge, Shell, Surface, edge_curve,
};
use cad_kernel::{KResult, KernelError};
use glam::DVec3;

/// Statistics of one conversion (diagnostics, tests, report).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrepStats {
    /// Faces whose analytic parameters did not match their NURBS (kept as NURBS).
    pub analytic_rejected: usize,
    /// Faces whose orientation came from the fallback (no usable interior sample).
    pub orientation_fallback: usize,
    /// Edges whose curve ran from the end vertex to the start vertex (reversed).
    pub edges_reversed: usize,
    /// Largest distance between an edge curve's ends and its vertices.
    pub max_vertex_gap: f64,
}

struct Rd<'a> {
    d: &'a [f64],
    i: usize,
}

fn bad(msg: &str) -> KernelError {
    KernelError::failed(format!("sc_brep: {msg}"))
}

impl Rd<'_> {
    fn f(&mut self) -> KResult<f64> {
        let v = *self.d.get(self.i).ok_or_else(|| bad("stream too short"))?;
        self.i += 1;
        Ok(v)
    }
    fn n(&mut self) -> KResult<usize> {
        let v = self.f()?;
        if !(v >= 0.0 && v.fract() == 0.0 && v < 1e9) {
            return Err(bad("bad count/index"));
        }
        Ok(v as usize)
    }
    fn b(&mut self) -> KResult<bool> {
        Ok(self.f()? != 0.0)
    }
    fn v3(&mut self) -> KResult<DVec3> {
        Ok(DVec3::new(self.f()?, self.f()?, self.f()?))
    }
    fn vec(&mut self, n: usize) -> KResult<Vec<f64>> {
        if self.i + n > self.d.len() {
            return Err(bad("stream too short"));
        }
        let v = self.d[self.i..self.i + n].to_vec();
        self.i += n;
        Ok(v)
    }
    fn ctrl(&mut self, n: usize) -> KResult<(Vec<DVec3>, Option<Vec<f64>>)> {
        let mut pts = Vec::with_capacity(n);
        let mut w = Vec::with_capacity(n);
        for _ in 0..n {
            pts.push(self.v3()?);
            w.push(self.f()?);
        }
        let rational = w.iter().any(|x| (x - w[0]).abs() > 1e-14 * w[0].abs()) || w.first().is_some_and(|x| *x != 1.0);
        Ok((pts, rational.then_some(w)))
    }
    fn curve(&mut self) -> KResult<BSplineCurve> {
        let degree = self.n()? as u32;
        let nc = self.n()?;
        let nk = self.n()?;
        let knots = self.vec(nk)?;
        let (points, weights) = self.ctrl(nc)?;
        let c = BSplineCurve { degree, knots, points, weights };
        c.check().map_err(|m| bad(&m))?;
        Ok(c)
    }
    fn surface(&mut self) -> KResult<BSplineSurface> {
        let degree_u = self.n()? as u32;
        let degree_v = self.n()? as u32;
        let nu = self.n()?;
        let nv = self.n()?;
        let nku = self.n()?;
        let knots_u = self.vec(nku)?;
        let nkv = self.n()?;
        let knots_v = self.vec(nkv)?;
        let (points, weights) = self.ctrl(nu * nv)?;
        let s = BSplineSurface { degree_u, degree_v, knots_u, knots_v, nu, nv, points, weights };
        s.check().map_err(|m| bad(&m))?;
        Ok(s)
    }
}

/// Analytic candidate from the facade's class and parameters.
fn analytic(cls: u32, prm: &[f64], generator: Option<BSplineCurve>) -> Option<Surface> {
    let v = |i: usize| DVec3::new(prm[i], prm[i + 1], prm[i + 2]);
    let frame = || Frame::new(v(0), v(3), v(6));
    let pos = |x: f64| x.is_finite() && x > 0.0;
    match cls {
        1 => Some(Surface::Plane { frame: frame()? }),
        2 if pos(prm[9]) => Some(Surface::Cylinder { frame: frame()?, r: prm[9] }),
        3 => {
            let (r, a) = (prm[9], prm[10]);
            let mut f = frame()?;
            if !(r >= 0.0 && a.is_finite()) || a.abs() >= std::f64::consts::FRAC_PI_2 {
                return None;
            }
            if a.abs() < 1e-12 {
                return pos(r).then_some(Surface::Cylinder { frame: f, r });
            }
            if a < 0.0 {
                // same cone with the axis reversed: r + h tan(a) = r + (-h) tan(-a)
                f.z = -f.z;
            }
            Some(Surface::Cone { frame: f, r, semi_angle: a.abs() })
        }
        4 if pos(prm[9]) => Some(Surface::Sphere { frame: frame()?, r: prm[9] }),
        5 if pos(prm[9]) && pos(prm[10]) => Some(Surface::Torus { frame: frame()?, major: prm[9], minor: prm[10] }),
        6 => {
            let f = frame()?;
            Some(Surface::Revolution { curve: generator?, axis_origin: f.origin, axis_dir: f.z })
        }
        7 => {
            let d = v(3);
            (d.length() > 0.0 && d.is_finite()).then_some(Surface::Extrusion { curve: generator?, dir: d })
        }
        _ => None,
    }
}

fn on_surface(s: &Surface, nurbs: &BSplineSurface, tol: f64) -> bool {
    nurbs.samples(9).iter().all(|(_, _, p)| s.distance(*p).is_some_and(|d| d <= tol))
}

/// Converts the facade stream of one shape.
pub fn parse(d: &[f64]) -> KResult<(Brep, BrepStats)> {
    let mut r = Rd { d, i: 0 };
    let mut st = BrepStats::default();
    if r.f()? != 1.0 {
        return Err(bad("unknown format"));
    }
    let _approx_tol = r.f()?;
    let nv = r.n()?;
    let mut vertices = Vec::with_capacity(nv);
    for _ in 0..nv {
        vertices.push(r.v3()?);
    }
    let (lo, hi) =
        vertices.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let size = if nv > 0 { (hi - lo).length().max(1.0) } else { 1.0 };
    let tol_rec = 1e-9 * size;
    let tol_srf = 1e-6 * size;

    let ne = r.n()?;
    let mut edges = Vec::with_capacity(ne);
    for _ in 0..ne {
        let start = r.n()?;
        let end = r.n()?;
        let _cls = r.n()?;
        let (t0, t1) = (r.f()?, r.f()?);
        let approximated = r.b()?;
        let c = r.curve()?;
        if start >= nv || end >= nv {
            return Err(bad("vertex index out of range"));
        }
        let (curve, reversed, gap) = edge_curve(&c, t0, t1, vertices[start], vertices[end], start == end, tol_rec);
        st.edges_reversed += usize::from(reversed);
        st.max_vertex_gap = st.max_vertex_gap.max(gap);
        edges.push(Edge { start: start as u32, end: end as u32, curve, approximated });
    }

    let nb = r.n()?;
    let mut bodies = Vec::new();
    for _ in 0..nb {
        let kind = r.n()?;
        let ns = r.n()?;
        let mut outers = Vec::new();
        let mut voids = Vec::new();
        for _ in 0..ns {
            let inner = r.b()?;
            let nf = r.n()?;
            let mut faces = Vec::with_capacity(nf);
            for _ in 0..nf {
                faces.push(face(&mut r, &mut st, ne, nv, tol_srf)?);
            }
            if inner { voids.push(Shell { faces }) } else { outers.push(Shell { faces }) }
        }
        if kind == 1 {
            outers.extend(voids);
            bodies.push(Body::Sheet { shells: outers });
            continue;
        }
        if outers.is_empty() {
            return Err(bad("solid without outer shell"));
        }
        // several outer shells (bodies that only touch): one solid each; voids go to the first
        let mut it = outers.into_iter();
        bodies.push(Body::Solid { outer: it.next().unwrap(), voids });
        for outer in it {
            bodies.push(Body::Solid { outer, voids: Vec::new() });
        }
    }
    if r.i != d.len() {
        return Err(bad("trailing data"));
    }
    let b = Brep { vertices, edges, bodies };
    b.validate().map_err(|m| bad(&m))?;
    Ok((b, st))
}

fn face(r: &mut Rd, st: &mut BrepStats, ne: usize, nv: usize, tol_srf: f64) -> KResult<Face> {
    let id = r.n()? as u32;
    let tag = r.n()? as u32;
    let cls = r.n()? as u32;
    let prm = r.vec(13)?;
    let approximated = r.b()?;
    let nurbs = r.surface()?;
    let generator = if cls == 6 || cls == 7 { Some(r.curve()?) } else { None };
    let ok = r.b()?;
    let p = r.v3()?;
    let n = r.v3()?;
    let (u, v) = (r.f()?, r.f()?);
    let same = r.b()?;
    let nl = r.n()?;
    let mut loops = Vec::with_capacity(nl);
    for _ in 0..nl {
        let k = r.n()?;
        if k == 0 {
            let vtx = r.n()?;
            if vtx >= nv {
                return Err(bad("vertex index out of range"));
            }
            loops.push(Loop::Vertex(vtx as u32));
            continue;
        }
        let mut es = Vec::with_capacity(k);
        for _ in 0..k {
            let e = r.n()?;
            if e >= ne {
                return Err(bad("edge index out of range"));
            }
            es.push(OrientedEdge { edge: e as u32, forward: r.b()? });
        }
        loops.push(Loop::Edges(es));
    }

    let surface = match analytic(cls, &prm, generator) {
        Some(s) if on_surface(&s, &nurbs, tol_srf) => s,
        Some(_) => {
            st.analytic_rejected += 1;
            Surface::BSpline(nurbs.clone())
        }
        None => Surface::BSpline(nurbs.clone()),
    };
    // orientation: STEP normal of `surface` against the written side's normal
    let uv_ok = ok && (nurbs.eval(u, v) - p).length() <= 1e-6 * (1.0 + p.length());
    let sense = if ok {
        let uv = uv_ok.then_some((u, v));
        surface.normal_at(p, uv).map(|ns| ns.dot(n) > 0.0)
    } else {
        None
    };
    let same_sense = match sense {
        Some(s) => s,
        None => {
            // SMLib's normal is ∂u × ∂v of its NURBS: compare at the domain centre
            st.orientation_fallback += 1;
            let ((u0, u1), (v0, v1)) = nurbs.domain();
            let (uc, vc) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
            let pc = nurbs.eval(uc, vc);
            let nw = nurbs.normal(uc, vc) * if same { 1.0 } else { -1.0 };
            surface.normal_at(pc, Some((uc, vc))).map(|ns| ns.dot(nw) > 0.0).unwrap_or(same)
        }
    };
    Ok(Face { id, tag, surface, same_sense, loops, approximated })
}
