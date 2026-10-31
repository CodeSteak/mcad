//! `make_polyhedron` (plan 1.7): a solid of planar polygonal faces, one face per polygon as the
//! SMLib facade's `sc_make_polyhedron` (`SmPolyBrep::CreatePoly` per face, then
//! `SmBrep::CreateFromPolyBrep`; no coplanar faces merged). solidcore has no poly B-reps
//! (`SmApiPoly*` is outside its scope), so the faces go through its STEP builder
//! (`step_brep::build`: one planar face per polygon with line edges, then
//! `SmApiStitchIntoSolid`). As SMLib's poly B-rep, points closer than its tolerance
//! (`max(1e-9, 1e-9 · diagonal)`) are one vertex; the faces are oriented consistently and
//! outward first (SMLib's `OrientTrimmedSurfaces`).

use std::collections::{BTreeMap, HashMap};

use cad_kernel::{KResult, KernelError};
use glam::DVec3;
use step_import::model as sm;

fn invalid(m: impl std::fmt::Display) -> KernelError {
    KernelError::InvalidInput(format!("solidcore: make_polyhedron: {m}"))
}

/// The neutral STEP body of the polyhedron (faces oriented outward).
pub(crate) fn body(points: &[DVec3], faces: &[Vec<u32>]) -> KResult<sm::Body> {
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

    let a3 = |p: DVec3| [p.x, p.y, p.z];
    let mut edges: Vec<sm::Edge> = Vec::new();
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
        let frame = sm::Frame { origin: a3(p[0]), x: a3(x), y: a3(n.cross(x)), z: a3(n) };
        let mut oriented = Vec::with_capacity(q.len());
        for k in 0..q.len() {
            let (a, b) = (q[k], q[(k + 1) % q.len()]);
            let key = (a.min(b), a.max(b));
            let e = *eix.entry(key).or_insert_with(|| {
                let (s, t) = (verts[key.0 as usize], verts[key.1 as usize]);
                edges.push(sm::Edge {
                    step_id: edges.len() as u64 + 1,
                    start: key.0 as usize,
                    end: key.1 as usize,
                    curve: sm::Curve::Line { origin: a3(s), dir: a3((t - s).normalize()), magnitude: 1.0 },
                    same_sense: true,
                    t_start: 0.0,
                    t_end: s.distance(t),
                });
                edges.len() - 1
            });
            oriented.push(sm::OrientedEdge { edge: e, forward: a == key.0 });
        }
        out_faces.push(sm::Face {
            step_id: fi as u64 + 1,
            surface: sm::Surface::Plane { frame },
            same_sense: true,
            loops: vec![sm::Loop { outer: true, orientation: true, bound: sm::Bound::Edges(oriented) }],
        });
    }
    Ok(sm::Body {
        step_id: 1,
        name: None,
        kind: sm::BodyKind::ManifoldSolid,
        unit_mm: 1.0,
        uncertainty_mm: None,
        shells: vec![sm::Shell { step_id: 1, closed: true, orientation: true, faces: (0..out_faces.len()).collect() }],
        faces: out_faces,
        edges,
        vertices: verts.iter().enumerate().map(|(i, p)| sm::Vertex { step_id: i as u64 + 1, point: a3(*p) }).collect(),
    })
}
