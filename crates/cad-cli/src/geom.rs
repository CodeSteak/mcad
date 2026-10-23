//! Geometry helpers of `cad measure`: exact minimum distance between two triangle meshes
//! (triangle–triangle distance with a bounding volume hierarchy).
//!
//! The meshes are tessellations of the exact B-reps; each deviates from its surface by at
//! most the chordal tolerance, so the distance of the meshes differs from the distance of
//! the solids by at most the sum of both tolerances.

use cad_kernel::Mesh;
use glam::{DMat4, DVec3};

pub(crate) type Tri = [DVec3; 3];

/// Triangles of `mesh` placed with `m`.
pub(crate) fn triangles(mesh: &Mesh, m: &DMat4) -> Vec<Tri> {
    let v = |i: u32| {
        let k = i as usize * 3;
        m.transform_point3(DVec3::new(
            mesh.positions[k] as f64,
            mesh.positions[k + 1] as f64,
            mesh.positions[k + 2] as f64,
        ))
    };
    mesh.indices.chunks_exact(3).map(|t| [v(t[0]), v(t[1]), v(t[2])]).collect()
}

#[derive(Clone, Copy)]
struct Aabb {
    lo: DVec3,
    hi: DVec3,
}

impl Aabb {
    fn of(t: &Tri) -> Aabb {
        Aabb { lo: t[0].min(t[1]).min(t[2]), hi: t[0].max(t[1]).max(t[2]) }
    }

    fn join(self, o: Aabb) -> Aabb {
        Aabb { lo: self.lo.min(o.lo), hi: self.hi.max(o.hi) }
    }

    fn dist(&self, o: &Aabb) -> f64 {
        let d = (o.lo - self.hi).max(self.lo - o.hi).max(DVec3::ZERO);
        d.length()
    }
}

enum Node {
    Leaf { bb: Aabb, tris: Vec<usize> },
    Inner { bb: Aabb, a: Box<Node>, b: Box<Node> },
}

impl Node {
    fn bb(&self) -> &Aabb {
        match self {
            Node::Leaf { bb, .. } | Node::Inner { bb, .. } => bb,
        }
    }
}

fn build(tris: &[Tri], boxes: &[Aabb], mut idx: Vec<usize>) -> Node {
    let bb = idx.iter().map(|&i| boxes[i]).reduce(Aabb::join).unwrap_or(Aabb { lo: DVec3::ZERO, hi: DVec3::ZERO });
    if idx.len() <= 4 {
        return Node::Leaf { bb, tris: idx };
    }
    let ext = bb.hi - bb.lo;
    let axis = if ext.x >= ext.y && ext.x >= ext.z {
        0
    } else if ext.y >= ext.z {
        1
    } else {
        2
    };
    let c = |i: usize| (tris[i][0][axis] + tris[i][1][axis] + tris[i][2][axis]) / 3.0;
    idx.sort_by(|&a, &b| c(a).partial_cmp(&c(b)).unwrap_or(std::cmp::Ordering::Equal));
    let right = idx.split_off(idx.len() / 2);
    Node::Inner { bb, a: Box::new(build(tris, boxes, idx)), b: Box::new(build(tris, boxes, right)) }
}

/// Closest point on triangle `t` to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_tri(p: DVec3, t: &Tri) -> DVec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = va + vb + vc;
    if denom.abs() < 1e-300 {
        return a;
    }
    a + ab * (vb / denom) + ac * (vc / denom)
}

/// Closest points of segments p1q1 and p2q2 (Ericson 5.1.9).
fn closest_seg_seg(p1: DVec3, q1: DVec3, p2: DVec3, q2: DVec3) -> (DVec3, DVec3) {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let eps = 1e-18;
    let (s, t);
    if a <= eps && e <= eps {
        return (p1, p2);
    }
    if a <= eps {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= eps {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > eps { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

/// Segment pq crosses triangle t.
fn seg_hits_tri(p: DVec3, q: DVec3, t: &Tri) -> Option<DVec3> {
    let d = q - p;
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let h = d.cross(e2);
    let a = e1.dot(h);
    if a.abs() < 1e-14 {
        return None;
    }
    let f = 1.0 / a;
    let s = p - t[0];
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qv = s.cross(e1);
    let v = f * d.dot(qv);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let w = f * e2.dot(qv);
    (0.0..=1.0).contains(&w).then(|| p + d * w)
}

/// Distance of two triangles and the closest points.
fn tri_tri(a: &Tri, b: &Tri) -> (f64, DVec3, DVec3) {
    for (x, y) in [(a, b), (b, a)] {
        for k in 0..3 {
            if let Some(h) = seg_hits_tri(x[k], x[(k + 1) % 3], y) {
                return (0.0, h, h);
            }
        }
    }
    let mut best = (f64::INFINITY, DVec3::ZERO, DVec3::ZERO);
    let mut take = |p: DVec3, q: DVec3| {
        let d = p.distance(q);
        if d < best.0 {
            best = (d, p, q);
        }
    };
    for k in 0..3 {
        take(a[k], closest_on_tri(a[k], b));
        take(closest_on_tri(b[k], a), b[k]);
        for l in 0..3 {
            let (p, q) = closest_seg_seg(a[k], a[(k + 1) % 3], b[l], b[(l + 1) % 3]);
            take(p, q);
        }
    }
    best
}

/// Result of [`min_distance`].
pub(crate) struct Distance {
    pub dist: f64,
    pub a: DVec3,
    pub b: DVec3,
}

/// Minimum distance between two triangle sets (0 if they touch or intersect).
pub(crate) fn min_distance(a: &[Tri], b: &[Tri]) -> Option<Distance> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let boxes: Vec<Aabb> = b.iter().map(Aabb::of).collect();
    let tree = build(b, &boxes, (0..b.len()).collect());
    let mut best = Distance { dist: f64::INFINITY, a: DVec3::ZERO, b: DVec3::ZERO };
    // visit triangles of `a` nearest to `b`'s box first: tightens the bound early
    let tb = *tree.bb();
    let mut order: Vec<(f64, usize)> = a.iter().enumerate().map(|(i, t)| (Aabb::of(t).dist(&tb), i)).collect();
    order.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, i) in order {
        let ta = &a[i];
        let ba = Aabb::of(ta);
        let mut stack = vec![&tree];
        while let Some(n) = stack.pop() {
            if n.bb().dist(&ba) >= best.dist {
                continue;
            }
            match n {
                Node::Leaf { tris, .. } => {
                    for &j in tris {
                        let (d, p, q) = tri_tri(ta, &b[j]);
                        if d < best.dist {
                            best = Distance { dist: d, a: p, b: q };
                        }
                    }
                }
                Node::Inner { a: l, b: r, .. } => {
                    let (dl, dr) = (l.bb().dist(&ba), r.bb().dist(&ba));
                    if dl < dr {
                        stack.push(r);
                        stack.push(l);
                    } else {
                        stack.push(l);
                        stack.push(r);
                    }
                }
            }
        }
        if best.dist == 0.0 {
            break;
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(o: DVec3, s: f64) -> Vec<Tri> {
        let c = |i: u32| o + DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * s;
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        faces.iter().flat_map(|f| [[c(f[0]), c(f[1]), c(f[2])], [c(f[0]), c(f[2]), c(f[3])]]).collect()
    }

    #[test]
    fn distance_of_cubes() {
        let a = cube(DVec3::ZERO, 10.0);
        let b = cube(DVec3::new(13.0, 0.0, 0.0), 10.0);
        let d = min_distance(&a, &b).unwrap();
        assert!((d.dist - 3.0).abs() < 1e-12, "{}", d.dist);
        // diagonal offset: edge to edge
        let c = cube(DVec3::new(13.0, 14.0, 0.0), 10.0);
        let d = min_distance(&a, &c).unwrap();
        assert!((d.dist - 5.0).abs() < 1e-12, "{}", d.dist);
        // corner to corner
        let e = cube(DVec3::new(11.0, 12.0, 12.0), 10.0);
        assert!((min_distance(&a, &e).unwrap().dist - 3.0).abs() < 1e-12);
        // overlap
        let f = cube(DVec3::new(5.0, 5.0, 5.0), 10.0);
        assert_eq!(min_distance(&a, &f).unwrap().dist, 0.0);
    }
}
