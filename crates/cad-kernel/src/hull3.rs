//! Convex hull of points in space (`hull()` of planar bodies, centres of equal spheres).
//!
//! Incremental hull: a start tetrahedron of four extreme points, then every point outside the
//! current hull replaces the faces it sees by a fan to their horizon. Points within the
//! tolerance of a face count as inside, so coplanar points do not split faces; the triangles
//! of one plane are merged into one convex polygon at the end.

use glam::{DVec2, DVec3};

/// Faces as convex polygons (indices into `points`, counter-clockwise seen from outside).
#[derive(Debug, Clone, PartialEq)]
pub struct Hull3 {
    pub points: Vec<DVec3>,
    pub faces: Vec<Vec<u32>>,
}

impl Hull3 {
    /// Every edge of the faces once in each direction (a closed polyhedron).
    pub fn is_closed(&self) -> bool {
        closed(&self.faces)
    }

    /// The faces with every polygon that is not planar to within `1e-12` of the size split
    /// into a fan of triangles (a convex polygon: the fan is valid). Points that are coplanar
    /// only within the hull tolerance (rounded mesh vertices) give such polygons; the kernel
    /// needs exactly planar faces.
    pub fn planar_faces(&self) -> Vec<Vec<u32>> {
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for p in &self.points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let tight = 1e-12 * (hi - lo).length().max(lo.abs().max(hi.abs()).max_element()).max(1.0);
        let mut out = Vec::with_capacity(self.faces.len());
        for f in &self.faces {
            let q: Vec<DVec3> = f.iter().map(|&i| self.points[i as usize]).collect();
            let mut n = DVec3::ZERO;
            for k in 1..q.len().saturating_sub(1) {
                n += (q[k] - q[0]).cross(q[k + 1] - q[0]);
            }
            let n = n.normalize_or_zero();
            if f.len() <= 3 || q.iter().all(|p| n.dot(*p - q[0]).abs() <= tight) {
                out.push(f.clone());
            } else {
                out.extend((1..f.len() - 1).map(|k| vec![f[0], f[k], f[k + 1]]));
            }
        }
        out
    }
}

/// What the points span when they have no volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flat {
    Point,
    Line,
    Plane,
}

#[derive(Clone, Copy)]
struct Tri {
    v: [usize; 3],
    n: DVec3,
    d: f64,
}

fn tri(p: &[DVec3], a: usize, b: usize, c: usize) -> Tri {
    let n = (p[b] - p[a]).cross(p[c] - p[a]).normalize_or_zero();
    Tri { v: [a, b, c], n, d: n.dot(p[a]) }
}

/// Hull of `pts`, or what they span if it has no volume. `tol`: distance below which a point
/// counts as on a plane, line or point.
pub fn hull3(pts: &[DVec3], tol: f64) -> Result<Hull3, Flat> {
    let p: Vec<DVec3> = pts.to_vec();
    if p.is_empty() {
        return Err(Flat::Point);
    }
    // start tetrahedron
    let i0 = (0..p.len()).min_by(|&a, &b| p[a].x.total_cmp(&p[b].x)).unwrap_or(0);
    let i1 = (0..p.len()).max_by(|&a, &b| p[a].distance(p[i0]).total_cmp(&p[b].distance(p[i0]))).unwrap_or(0);
    if p[i1].distance(p[i0]) <= tol {
        return Err(Flat::Point);
    }
    let dir = (p[i1] - p[i0]).normalize();
    let line_dist = |q: DVec3| {
        let v = q - p[i0];
        (v - dir * v.dot(dir)).length()
    };
    let i2 = (0..p.len()).max_by(|&a, &b| line_dist(p[a]).total_cmp(&line_dist(p[b]))).unwrap_or(0);
    if line_dist(p[i2]) <= tol {
        return Err(Flat::Line);
    }
    let n = (p[i1] - p[i0]).cross(p[i2] - p[i0]).normalize();
    let i3 = (0..p.len()).max_by(|&a, &b| n.dot(p[a] - p[i0]).abs().total_cmp(&n.dot(p[b] - p[i0]).abs())).unwrap_or(0);
    if n.dot(p[i3] - p[i0]).abs() <= tol {
        return Err(Flat::Plane);
    }
    let mut faces: Vec<Tri> = Vec::new();
    let base = [i0, i1, i2, i3];
    let centre = (p[i0] + p[i1] + p[i2] + p[i3]) / 4.0;
    for (a, b, c) in [(0, 1, 2), (0, 1, 3), (0, 2, 3), (1, 2, 3)] {
        let mut t = tri(&p, base[a], base[b], base[c]);
        if t.n.dot(centre) - t.d > 0.0 {
            t = tri(&p, base[a], base[c], base[b]);
        }
        faces.push(t);
    }
    for (i, q) in p.iter().enumerate() {
        if base.contains(&i) {
            continue;
        }
        let dist: Vec<f64> = faces.iter().map(|f| f.n.dot(*q) - f.d).collect();
        let Some(top) = (0..faces.len()).max_by(|&a, &b| dist[a].total_cmp(&dist[b])) else { continue };
        if dist[top] <= tol {
            continue;
        }
        // visible faces: connected through shared edges to the face `q` lies farthest above (with
        // points coplanar within the tolerance the plain sign test can give a visible set that
        // is not one patch, and the hull would not close)
        let mut by_edge: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..3 {
                by_edge.insert((f.v[k], f.v[(k + 1) % 3]), fi);
            }
        }
        let mut visible = vec![false; faces.len()];
        visible[top] = true;
        let mut stack = vec![top];
        while let Some(fi) = stack.pop() {
            let f = faces[fi];
            for k in 0..3 {
                if let Some(&g) = by_edge.get(&(f.v[(k + 1) % 3], f.v[k]))
                    && !visible[g]
                    && dist[g] > tol
                {
                    visible[g] = true;
                    stack.push(g);
                }
            }
        }
        // horizon: directed edges of visible faces whose reverse is not on a visible face
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (f, _) in faces.iter().zip(&visible).filter(|(_, v)| **v) {
            for k in 0..3 {
                edges.push((f.v[k], f.v[(k + 1) % 3]));
            }
        }
        let horizon: Vec<(usize, usize)> = edges.iter().copied().filter(|&(a, b)| !edges.contains(&(b, a))).collect();
        // the patch must be a disk: every horizon vertex starts exactly one horizon edge
        let mut starts: Vec<usize> = horizon.iter().map(|e| e.0).collect();
        starts.sort_unstable();
        if starts.windows(2).any(|w| w[0] == w[1]) {
            continue;
        }
        let mut kept: Vec<Tri> = faces.iter().zip(&visible).filter(|(_, v)| !**v).map(|(f, _)| *f).collect();
        for (a, b) in horizon {
            kept.push(tri(&p, a, b, i));
        }
        faces = kept;
    }
    let merged = merge(&p, &faces, tol);
    if closed(&merged.faces) {
        return Ok(merged);
    }
    // Points coplanar only within the tolerance (rounded mesh vertices) can make the merged
    // polygons disagree on their shared boundary points: keep the triangles then.
    let tris: Vec<Tri> = faces.iter().filter(|t| t.n != DVec3::ZERO).copied().collect();
    let mut points: Vec<DVec3> = Vec::new();
    let mut index: Vec<Option<u32>> = vec![None; p.len()];
    let faces = tris
        .iter()
        .map(|t| {
            t.v.iter()
                .map(|&i| {
                    *index[i].get_or_insert_with(|| {
                        points.push(p[i]);
                        (points.len() - 1) as u32
                    })
                })
                .collect()
        })
        .collect();
    Ok(Hull3 { points, faces })
}

/// Every directed edge of the polygons once, and its reverse once.
fn closed(faces: &[Vec<u32>]) -> bool {
    let mut e = std::collections::HashSet::new();
    for f in faces {
        for k in 0..f.len() {
            if !e.insert((f[k], f[(k + 1) % f.len()])) {
                return false;
            }
        }
    }
    e.iter().all(|&(a, b)| e.contains(&(b, a)))
}

/// Merges the triangles of one plane into convex polygons and keeps only used points.
fn merge(p: &[DVec3], tris: &[Tri], tol: f64) -> Hull3 {
    let mut groups: Vec<(DVec3, f64, Vec<usize>)> = Vec::new();
    for t in tris {
        if t.n == DVec3::ZERO {
            continue;
        }
        match groups.iter_mut().find(|g| g.0.dot(t.n) > 1.0 - 1e-9 && (g.1 - t.d).abs() <= tol) {
            Some(g) => g.2.extend_from_slice(&t.v),
            None => groups.push((t.n, t.d, t.v.to_vec())),
        }
    }
    let mut points: Vec<DVec3> = Vec::new();
    let mut index: Vec<Option<u32>> = vec![None; p.len()];
    let mut faces = Vec::new();
    for (n, _, mut vs) in groups {
        vs.sort_unstable();
        vs.dedup();
        // polygon order: 2D hull in the plane (collinear points dropped)
        let e1 = n.any_orthonormal_vector();
        let e2 = n.cross(e1);
        let q: Vec<(DVec2, usize)> = vs.iter().map(|&i| (DVec2::new(p[i].dot(e1), p[i].dot(e2)), i)).collect();
        let ring = ring2(&q, tol);
        if ring.len() < 3 {
            continue;
        }
        let face: Vec<u32> = ring
            .iter()
            .map(|&i| {
                *index[i].get_or_insert_with(|| {
                    points.push(p[i]);
                    (points.len() - 1) as u32
                })
            })
            .collect();
        faces.push(face);
    }
    Hull3 { points, faces }
}

/// Counter-clockwise convex hull of 2D points (indices), collinear points dropped.
fn ring2(q: &[(DVec2, usize)], tol: f64) -> Vec<usize> {
    let mut s: Vec<(DVec2, usize)> = q.to_vec();
    s.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    if s.len() < 3 {
        return s.iter().map(|x| x.1).collect();
    }
    // a point counts as collinear if it lies within tol of the line through its neighbours
    let left = |o: DVec2, a: DVec2, b: DVec2| {
        let ob = b - o;
        let l = ob.length().max(1e-300);
        ob.perp_dot(a - o) / l
    };
    let mut h: Vec<(DVec2, usize)> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &(DVec2, usize)>> =
            if pass == 0 { Box::new(s.iter()) } else { Box::new(s.iter().rev()) };
        for &x in it {
            while h.len() >= start + 2 && left(h[h.len() - 2].0, h[h.len() - 1].0, x.0) >= -tol {
                h.pop();
            }
            h.push(x);
        }
        h.pop();
    }
    h.iter().map(|x| x.1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(h: &Hull3) -> f64 {
        let mut v = 0.0;
        for f in &h.faces {
            let a = h.points[f[0] as usize];
            for k in 1..f.len() - 1 {
                let (b, c) = (h.points[f[k] as usize], h.points[f[k + 1] as usize]);
                v += a.dot(b.cross(c)) / 6.0;
            }
        }
        v
    }

    #[test]
    fn cube_corners_and_inner_points() {
        let mut pts = Vec::new();
        for i in 0..8 {
            pts.push(DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * 10.0);
        }
        // points inside, on faces and on edges
        pts.push(DVec3::splat(5.0));
        pts.push(DVec3::new(5.0, 5.0, 0.0));
        pts.push(DVec3::new(5.0, 0.0, 0.0));
        let h = hull3(&pts, 1e-9).unwrap();
        assert_eq!(h.points.len(), 8);
        assert_eq!(h.faces.len(), 6);
        assert!(h.faces.iter().all(|f| f.len() == 4));
        assert!((volume(&h) - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn two_boxes_and_flat_sets() {
        let mut pts = Vec::new();
        for i in 0..8 {
            let c = DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
            pts.push(c * 2.0);
            pts.push(c * 2.0 + DVec3::new(10.0, 0.0, 0.0));
        }
        let h = hull3(&pts, 1e-9).unwrap();
        assert_eq!(h.faces.len(), 6);
        assert!((volume(&h) - 12.0 * 4.0).abs() < 1e-9);
        // skew: a tetrahedron and its volume
        let t = [DVec3::ZERO, DVec3::X * 3.0, DVec3::Y * 4.0, DVec3::Z * 5.0];
        let h = hull3(&t, 1e-9).unwrap();
        assert_eq!(h.faces.len(), 4);
        assert!((volume(&h) - 10.0).abs() < 1e-9);
        assert_eq!(hull3(&[DVec3::ONE; 3], 1e-9), Err(Flat::Point));
        assert_eq!(hull3(&[DVec3::ZERO, DVec3::X, DVec3::X * 2.0], 1e-9), Err(Flat::Line));
        assert_eq!(hull3(&[DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::ONE.with_z(0.0)], 1e-9), Err(Flat::Plane));
    }

    #[test]
    fn sphere_points() {
        // points on a sphere: every point is a vertex, triangles stay triangles
        let mut pts = Vec::new();
        let n = 200;
        for i in 0..n {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - z * z).sqrt();
            let a = i as f64 * 2.399963229728653;
            pts.push(DVec3::new(r * a.cos(), r * a.sin(), z) * 10.0);
        }
        let h = hull3(&pts, 1e-9).unwrap();
        assert_eq!(h.points.len(), n);
        // Euler: V - E + F = 2 with triangles: F = 2V - 4
        assert_eq!(h.faces.len(), 2 * n - 4);
        let v = volume(&h);
        assert!(v > 0.95 * 4.0 / 3.0 * std::f64::consts::PI * 1000.0 && v < 4.0 / 3.0 * std::f64::consts::PI * 1000.0);
    }

    #[test]
    fn rounded_mesh_vertices_give_a_closed_hull() {
        // UV sphere in f32 (an STL): its quads are planar only up to rounding, the poles are
        // rings of points within 1e-6 of each other
        let (segs, rings) = (22usize, 23usize);
        let mut pts = Vec::new();
        for i in 0..=rings {
            for j in 0..segs {
                let th = std::f32::consts::PI * i as f32 / rings as f32;
                let ph = 2.0 * std::f32::consts::PI * j as f32 / segs as f32;
                let r = 20f32;
                let p = [r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()];
                pts.push(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            }
        }
        let h = hull3(&pts, 1e-9 * 40.0).unwrap();
        assert!(closed(&h.faces), "every edge once in each direction");
        let v = volume(&h);
        assert!(v > 0.95 * 4.0 / 3.0 * std::f64::consts::PI * 8000.0 && v < 4.0 / 3.0 * std::f64::consts::PI * 8000.0);
    }
}
