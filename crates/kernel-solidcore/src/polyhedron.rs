//! `make_polyhedron` (plan 1.7): a solid of planar polygonal faces, one face per polygon as the
//! SMLib facade's `sc_make_polyhedron` (`SmPolyBrep::CreatePoly` per face, then
//! `SmBrep::CreateFromPolyBrep`; no coplanar faces merged). solidcore has no poly B-reps
//! (`SmApiPoly*` is outside its scope), so the faces go in as a B-rep record
//! (`solidcore_record::brep`: one planar face per polygon with line edges, the shared edges
//! already joined) built by `solidcore_convert::brep::to_kernel_body` with the healer on. As SMLib's poly B-rep, points closer than its tolerance
//! (`max(1e-9, 1e-9 · diagonal)`) are one vertex; the faces are oriented consistently and
//! outward first (SMLib's `OrientTrimmedSurfaces`).

use std::collections::{BTreeMap, HashMap};

use cad_kernel::{KResult, KernelError};
use glam::DVec3;
use solidcore_record::brep as r;

fn invalid(m: impl std::fmt::Display) -> KernelError {
    KernelError::InvalidInput(format!("solidcore: make_polyhedron: {m}"))
}

/// The B-rep record of the polyhedron (faces oriented outward).
pub(crate) fn record(points: &[DVec3], faces: &[Vec<u32>]) -> KResult<r::Brep> {
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

    // the record: one region pair around one closed shell, every face forward (its plane's
    // natural normal is the outward normal), edges and vertices in the record's canonical order
    // (first appearance walking faces → loop → edges, then edges → start, end)
    let a3 = |p: DVec3| [p.x, p.y, p.z];
    let mut edge_ix: HashMap<(u32, u32), u32> = HashMap::new();
    let mut edge_ends: Vec<(u32, u32)> = Vec::new();
    let mut edge_uses: Vec<Vec<r::EdgeUseRef>> = Vec::new();
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
        let y = n.cross(x);
        let uv = |v: u32| {
            let d = verts[v as usize] - p[0];
            [d.dot(x), d.dot(y)]
        };
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        let mut oriented = Vec::with_capacity(q.len());
        for k in 0..q.len() {
            let (a, b) = (q[k], q[(k + 1) % q.len()]);
            let next = edge_ends.len() as u32;
            let e = *edge_ix.entry((a.min(b), a.max(b))).or_insert_with(|| {
                // the edge runs as its first use walks it
                edge_ends.push((a, b));
                edge_uses.push(Vec::new());
                next
            });
            let (s, t) = edge_ends[e as usize];
            let forward = s == a;
            edge_uses[e as usize].push(r::EdgeUseRef {
                face: fi as u32,
                r#loop: 0,
                position: k as u32,
                upper: !forward,
            });
            let (us, ut) = (uv(s), uv(t));
            for c in [us, ut] {
                lo = [lo[0].min(c[0]), lo[1].min(c[1])];
                hi = [hi[0].max(c[0]), hi[1].max(c[1])];
            }
            oriented.push(r::OrientedEdge {
                edge: e,
                forward,
                pcurve: r::Curve2::Spline { knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![us, ut] },
            });
        }
        out_faces.push(r::Face {
            surface: r::Surface::Plane { origin: a3(p[0]), u_axis: a3(x), v_axis: a3(y) },
            nurbs: None,
            domain: [[lo[0], hi[0]], [lo[1], hi[1]]],
            loops: vec![r::Loop::Edges { edges: oriented }],
        });
    }
    let mut vert_ix: HashMap<u32, u32> = HashMap::new();
    let mut vertices = Vec::new();
    let mut vix = |v: u32| -> u32 {
        *vert_ix.entry(v).or_insert_with(|| {
            vertices.push(r::Vertex { point: a3(verts[v as usize]), tolerance: tol });
            vertices.len() as u32 - 1
        })
    };
    let edges: Vec<r::Edge> = edge_ends
        .iter()
        .zip(edge_uses)
        .map(|(&(s, t), uses)| r::Edge {
            curve: r::Curve::Line { start: a3(verts[s as usize]), end: a3(verts[t as usize]) },
            nurbs: None,
            interval: [0.0, 1.0],
            vertices: [vix(s), vix(t)],
            tolerance: tol,
            uses,
        })
        .collect();
    let faces: Vec<r::OrientedFace> =
        (0..out_faces.len()).map(|f| r::OrientedFace { face: f as u32, forward: true }).collect();
    Ok(r::Brep {
        version: r::VERSION,
        tolerance: tol,
        regions: vec![r::Region { shells: vec![0], void: false }, r::Region { shells: vec![0], void: false }],
        shells: vec![r::Shell::Faces { faces, wires: Vec::new() }],
        faces: out_faces,
        edges,
        vertices,
    })
}
