//! `make_polyhedron` (plan 1.7): a solid of planar polygonal faces, one face per polygon as the
//! SMLib facade's `sc_make_polyhedron` (`SmPolyBrep::CreatePoly` per face, then
//! `SmBrep::CreateFromPolyBrep`; no coplanar faces merged). solidcore has no poly B-reps
//! (`SmApiPoly*` is outside its scope), so the faces are built topology first with solidcore's
//! `BrepConstructor` (`SmBrepConstructor`): one planar face per polygon, each line edge and
//! vertex made once and shared by its faces, then every face gets its UV trim curves. These are
//! the calls the STEP builder `step_brep::build` made for such a body up to solidcore `f4f05b1`
//! (upstream deleted `step-brep` and `step-import` in #14038 and #14194; the STEP reader that
//! replaced them, `solidcore-step`, reads files into records and has no builder for this). As
//! SMLib's poly B-rep, points closer than its tolerance (`max(1e-9, 1e-9 · diagonal)`) are one
//! vertex; the faces are oriented consistently and outward first (SMLib's
//! `OrientTrimmedSurfaces`).

use std::collections::{BTreeMap, HashMap};

use cad_kernel::{KResult, KernelError};
use glam::DVec3;
use solidcore::curve::curve::Curve;
use solidcore::curve::line::{BoundedLineOptions, Line};
use solidcore::geom::axis2_placement::Axis2Placement;
use solidcore::geom::extent1d::Extent1d;
use solidcore::geom::extent2d::Extent2d;
use solidcore::nurbs::b_spline_surface_n_lib::p2::OrientType;
use solidcore::surface::plane::Plane;
use solidcore::surface::surface::Surface;
use solidcore::topology::brep_constructor::{BrepConstructor, VertexNode};
use solidcore::topology::trimming_tools;
use solidcore::{Brep, EdgeId, Kernel, Point3, SolidError, Vec3, VertexId};

fn invalid(m: impl std::fmt::Display) -> KernelError {
    KernelError::InvalidInput(format!("solidcore: make_polyhedron: {m}"))
}

/// A planar face: its plane (origin, x and y axis; the normal points out of the material) and
/// its one loop, counter-clockwise about the normal, as edges with the direction the loop runs
/// along each (true: from the edge's start vertex to its end vertex).
pub(crate) struct PlanarFace {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub uses: Vec<(usize, bool)>,
}

/// A closed polyhedral body: welded vertices, line edges (start, end vertex) and planar faces
/// oriented outward.
pub(crate) struct Body {
    pub vertices: Vec<DVec3>,
    pub edges: Vec<(u32, u32)>,
    pub faces: Vec<PlanarFace>,
}

/// The body of the polyhedron (faces oriented outward).
pub(crate) fn body(points: &[DVec3], faces: &[Vec<u32>]) -> KResult<Body> {
    if points.len() < 4 || faces.len() < 4 {
        return Err(invalid("a polyhedron needs at least 4 points and 4 faces"));
    }
    if points.iter().any(|p| !p.is_finite()) {
        return Err(invalid("non-finite point"));
    }
    let (lo, hi) =
        points.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let diag = (hi - lo).length();
    let tol = (1e-9 * diag).max(1e-9);
    // weld points within the tolerance (the poly B-rep takes positions, not indices)
    let mut cell: HashMap<[i64; 3], u32> = HashMap::new();
    let mut verts: Vec<DVec3> = Vec::new();
    let mut weld = |p: DVec3| -> u32 {
        let k = (p / tol).round();
        let key = [k.x as i64, k.y as i64, k.z as i64];
        *cell.entry(key).or_insert_with(|| {
            verts.push(p);
            verts.len() as u32 - 1
        })
    };
    let mut polys: Vec<Vec<u32>> = Vec::with_capacity(faces.len());
    for f in faces {
        let mut q: Vec<u32> = Vec::with_capacity(f.len());
        for &i in f {
            let p = *points.get(i as usize).ok_or_else(|| invalid("point index out of range"))?;
            let v = weld(p);
            if q.last() != Some(&v) {
                q.push(v);
            }
        }
        while q.len() > 1 && q[0] == q[q.len() - 1] {
            q.pop();
        }
        if q.len() < 3 {
            return Err(invalid("a face needs at least 3 distinct points"));
        }
        polys.push(q);
    }
    // undirected edge → the faces using it and the direction (true: from the lower index)
    let mut uses: BTreeMap<(u32, u32), Vec<(usize, bool)>> = BTreeMap::new();
    for (fi, q) in polys.iter().enumerate() {
        for k in 0..q.len() {
            let (a, b) = (q[k], q[(k + 1) % q.len()]);
            uses.entry((a.min(b), a.max(b))).or_default().push((fi, a < b));
        }
    }
    if uses.values().any(|u| u.len() != 2) {
        return Err(KernelError::failed(
            "solidcore: make_polyhedron: the faces do not form a closed manifold solid (every edge must belong to \
             exactly two faces)",
        ));
    }
    // consistent orientation: neighbours run their shared edge in opposite directions
    let mut flip: Vec<Option<bool>> = vec![None; polys.len()];
    let mut adj: Vec<Vec<(usize, bool)>> = vec![Vec::new(); polys.len()];
    for u in uses.values() {
        let ((f, d), (g, e)) = (u[0], u[1]);
        // same direction: one of the two must be flipped
        adj[f].push((g, d == e));
        adj[g].push((f, d == e));
    }
    for start in 0..polys.len() {
        if flip[start].is_some() {
            continue;
        }
        flip[start] = Some(false);
        let mut stack = vec![start];
        while let Some(f) = stack.pop() {
            let ff = flip[f].expect("visited");
            for &(g, differ) in &adj[f] {
                let want = ff ^ differ;
                match flip[g] {
                    None => {
                        flip[g] = Some(want);
                        stack.push(g);
                    }
                    Some(x) if x != want => {
                        return Err(KernelError::failed(
                            "solidcore: make_polyhedron: the faces cannot be oriented consistently (not orientable)",
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
    }
    for (q, f) in polys.iter_mut().zip(&flip) {
        if *f == Some(true) {
            q.reverse();
        }
    }
    // outward: positive signed volume (divergence theorem over the polygon fans)
    let vol: f64 = polys
        .iter()
        .map(|q| {
            (1..q.len() - 1)
                .map(|k| verts[q[0] as usize].dot(verts[q[k] as usize].cross(verts[q[k + 1] as usize])))
                .sum::<f64>()
        })
        .sum::<f64>()
        / 6.0;
    if vol.abs() <= 1e-12 * diag.powi(3) {
        return Err(invalid("the polyhedron has no volume"));
    }
    if vol < 0.0 {
        polys.iter_mut().for_each(|q| q.reverse());
    }

    let mut edges: Vec<(u32, u32)> = Vec::new();
    let mut eix: HashMap<(u32, u32), usize> = HashMap::new();
    let mut out_faces = Vec::with_capacity(polys.len());
    for (fi, q) in polys.iter().enumerate() {
        let p: Vec<DVec3> = q.iter().map(|&i| verts[i as usize]).collect();
        let mut n = DVec3::ZERO;
        for k in 0..p.len() {
            n += p[k].cross(p[(k + 1) % p.len()]);
        }
        let n = n.try_normalize().ok_or_else(|| invalid(format!("face {fi} is degenerate")))?;
        let off = p.iter().map(|x| (*x - p[0]).dot(n).abs()).fold(0.0, f64::max);
        if off > 1e-6 * diag.max(1.0) {
            return Err(KernelError::Unsupported(format!(
                "solidcore: make_polyhedron: face {fi} is not planar ({off:.2e} off its plane)"
            )));
        }
        let x = (p[1] - p[0]).reject_from(n).try_normalize().unwrap_or_else(|| n.any_orthonormal_vector());
        let mut oriented = Vec::with_capacity(q.len());
        for k in 0..q.len() {
            let (a, b) = (q[k], q[(k + 1) % q.len()]);
            let key = (a.min(b), a.max(b));
            let e = *eix.entry(key).or_insert_with(|| {
                edges.push(key);
                edges.len() - 1
            });
            oriented.push((e, a == key.0));
        }
        out_faces.push(PlanarFace { origin: p[0], x, y: n.cross(x), uses: oriented });
    }
    Ok(Body { vertices: verts, edges, faces: out_faces })
}

fn point(p: DVec3) -> Point3 {
    Point3::new(p.x, p.y, p.z)
}

fn vec3(p: DVec3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

fn orient(same: bool) -> OrientType {
    if same { OrientType::Same } else { OrientType::Opposite }
}

/// `what` failed with `e`.
fn call(what: &'static str) -> impl Fn(SolidError) -> String {
    move |e| format!("{what}: {e}")
}

/// Builds `body` in `k` as one closed shell around one solid region, its faces with their UV
/// trim curves (the tessellator and the mass integration read them).
///
/// # Errors
///
/// The failing constructor call and its error.
pub(crate) fn build(k: &mut Kernel, body: &Body) -> Result<Brep, String> {
    let mut c = BrepConstructor::new();
    c.start_brep(k, solidcore::geom::tol::USE_DEFAULT).map_err(call("StartBrep"))?;
    c.start_region(true, None).map_err(call("StartRegion"))?;
    c.start_shell().map_err(call("StartShell"))?;
    let mut edges: Vec<Option<EdgeId>> = vec![None; body.edges.len()];
    let mut vertices: Vec<Option<VertexId>> = vec![None; body.vertices.len()];
    for (fi, face) in body.faces.iter().enumerate() {
        face_calls(k, &mut c, body, face, &mut edges, &mut vertices).map_err(|e| format!("face {fi}: {e}"))?;
    }
    c.end_shell(k, false).map_err(call("EndShell"))?;
    c.end_region().map_err(call("EndRegion"))?;
    let mut brep = c.end_brep().map_err(call("EndBrep"))?;
    for f in brep.face_list().to_vec() {
        let present = trimming_tools::face_uv_curves_present(&brep, f).map_err(call("UV trims"))?;
        if !present {
            trimming_tools::create_uv_trim_curves(k, &mut brep, f).map_err(call("UV trims"))?;
        }
    }
    Ok(brep)
}

/// The calls of one face: its plane over the box of its vertices, its loop, each edge made at
/// its first use (a line between its vertices) and joined at its second.
fn face_calls(
    k: &mut Kernel,
    c: &mut BrepConstructor,
    body: &Body,
    face: &PlanarFace,
    edges: &mut [Option<EdgeId>],
    vertices: &mut [Option<VertexId>],
) -> Result<(), String> {
    let at = |v: u32| body.vertices.get(v as usize).copied().unwrap_or(DVec3::ZERO);
    let mut surface = Surface::from(
        Plane::create_canonical(&Axis2Placement::new(point(face.origin), vec3(face.x), vec3(face.y)))
            .map_err(call("face surface"))?,
    );
    // the face's parameter box: its vertices in the plane's (u, v)
    let (mut u0, mut v0, mut u1, mut v1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(e, _) in &face.uses {
        let (a, b) = body.edges.get(e).copied().unwrap_or_default();
        for p in [at(a), at(b)] {
            let (u, v) = ((p - face.origin).dot(face.x), (p - face.origin).dot(face.y));
            (u0, v0, u1, v1) = (u0.min(u), v0.min(v), u1.max(u), v1.max(v));
        }
    }
    surface.adjust_step_uv_domain(k, &Extent2d::from_bounds(u0, v0, u1, v1)).map_err(call("AdjustSTEPUVDomain"))?;
    // the face normal is the plane's: it points out of the material
    c.start_face(OrientType::Same).map_err(call("StartFace"))?;
    let domain = surface.natural_uv_domain_with_nurb(k).map_err(call("GetNaturalUVDomain"))?;
    c.set_face_surface(surface, &domain).map_err(call("SetFaceSurface"))?;
    // the loop runs counter-clockwise about the face's normal
    c.start_loop(OrientType::Same).map_err(call("StartLoop"))?;
    for &(ei, forward) in &face.uses {
        let Some(&(a, b)) = body.edges.get(ei) else { continue };
        if let Some(existing) = edges.get(ei).copied().flatten() {
            c.start_edge(k, orient(forward), Some(existing), None, None).map_err(call("StartEdge"))?;
            c.end_edge().map_err(call("EndEdge"))?;
            continue;
        }
        let mut ends = [None, None];
        for (slot, v) in ends.iter_mut().zip([a, b]) {
            if let Some(id) = vertices.get_mut(v as usize) {
                if id.is_none() {
                    *id = Some(c.start_vertex_of_loop(point(at(v))).map_err(call("StartVertexOfLoop"))?);
                }
                *slot = *id;
            }
        }
        let [start, end] = ends;
        let id = c.start_edge(k, orient(forward), None, start, end).map_err(call("StartEdge"))?;
        if let Some(slot) = edges.get_mut(ei) {
            *slot = Some(id);
        }
        let (s, t) = (at(a), at(b));
        let curve = Curve::from(Line::bounded(
            point(s),
            vec3((t - s).normalize()),
            &Extent1d::from_min_max(0.0, s.distance(t)),
            &BoundedLineOptions { scale: 1.0, dim: 3, ..BoundedLineOptions::default() },
        ));
        c.set_edge_curve(k, Some(curve), false).map_err(call("SetEdgeCurve"))?;
        c.set_limits(k, &VertexNode::from_point(point(s)), &VertexNode::from_point(point(t)))
            .map_err(call("SetLimits"))?;
        c.end_edge().map_err(call("EndEdge"))?;
    }
    c.end_loop(k).map_err(call("EndLoop"))?;
    c.end_face().map_err(call("EndFace"))?;
    Ok(())
}
