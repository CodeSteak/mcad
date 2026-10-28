//! Ray tracing for the studio look: a bounding volume hierarchy over the opaque triangles
//! (binned surface area heuristic, single precision relative to the scene centre).
//! Reflection rays of metals and glossy surfaces and the mirror rays of the ground find the
//! model with it.

use glam::{DVec3, Vec3};

/// An opaque triangle of the scene (world space) with its vertex normals and its item.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RtTri {
    pub p: [DVec3; 3],
    pub n: [DVec3; 3],
    pub item: u32,
}

#[derive(Clone, Copy)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first triangle; inner node: left child (the right one follows the left subtree).
    first: u32,
    /// Leaf: number of triangles (> 0); inner node: 0.
    count: u32,
    /// Inner node: right child.
    right: u32,
}

#[derive(Clone, Copy)]
struct TriF {
    v0: Vec3,
    e1: Vec3,
    e2: Vec3,
}

/// The hierarchy; triangles are referenced by their index in the input slice.
pub(crate) struct Bvh {
    origin: DVec3,
    nodes: Vec<Node>,
    tris: Vec<TriF>,
    /// Input index of the triangles in hierarchy order.
    index: Vec<u32>,
}

/// The nearest hit of a ray.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hit {
    pub t: f64,
    /// Index of the triangle in the input slice.
    pub tri: u32,
    /// Barycentric coordinates of vertices 1 and 2.
    pub u: f64,
    pub v: f64,
}

const LEAF: usize = 4;
const BINS: usize = 16;

fn area(lo: Vec3, hi: Vec3) -> f32 {
    let d = (hi - lo).max(Vec3::ZERO);
    d.x * d.y + d.y * d.z + d.z * d.x
}

impl Bvh {
    pub fn build(input: &[RtTri]) -> Bvh {
        let origin = if input.is_empty() {
            DVec3::ZERO
        } else {
            let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            for t in input {
                for p in t.p {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            (lo + hi) / 2.0
        };
        let tf: Vec<TriF> = input
            .iter()
            .map(|t| {
                let v: [Vec3; 3] = std::array::from_fn(|k| (t.p[k] - origin).as_vec3());
                TriF { v0: v[0], e1: v[1] - v[0], e2: v[2] - v[0] }
            })
            .collect();
        let bounds: Vec<(Vec3, Vec3)> = tf
            .iter()
            .map(|t| {
                let (a, b, c) = (t.v0, t.v0 + t.e1, t.v0 + t.e2);
                (a.min(b).min(c), a.max(b).max(c))
            })
            .collect();
        let cent: Vec<Vec3> = bounds.iter().map(|(l, h)| (*l + *h) * 0.5).collect();
        let mut index: Vec<u32> = (0..input.len() as u32).collect();
        let mut nodes: Vec<Node> = Vec::with_capacity(input.len() / 2 + 1);
        // (node, first, count)
        let mut stack: Vec<(usize, usize, usize)> = Vec::new();
        nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, first: 0, count: 0, right: 0 });
        stack.push((0, 0, index.len()));
        while let Some((ni, first, count)) = stack.pop() {
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut clo, mut chi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for &k in &index[first..first + count] {
                let (l, h) = bounds[k as usize];
                lo = lo.min(l);
                hi = hi.max(h);
                clo = clo.min(cent[k as usize]);
                chi = chi.max(cent[k as usize]);
            }
            nodes[ni].lo = lo;
            nodes[ni].hi = hi;
            let leaf = |nodes: &mut Vec<Node>| {
                nodes[ni].first = first as u32;
                nodes[ni].count = count as u32;
            };
            if count <= LEAF {
                leaf(&mut nodes);
                continue;
            }
            // binned SAH over the longest centroid axis
            let ext = chi - clo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            if ext[axis] <= 1e-9 {
                leaf(&mut nodes);
                continue;
            }
            let scale = BINS as f32 / ext[axis];
            let bin_of = |k: u32| (((cent[k as usize][axis] - clo[axis]) * scale) as usize).min(BINS - 1);
            let mut bc = [0usize; BINS];
            let mut bb = [(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)); BINS];
            for &k in &index[first..first + count] {
                let b = bin_of(k);
                bc[b] += 1;
                let (l, h) = bounds[k as usize];
                bb[b].0 = bb[b].0.min(l);
                bb[b].1 = bb[b].1.max(h);
            }
            let mut best = (f32::INFINITY, 0usize);
            // cost of splitting after bin s
            let mut left_area = [0.0f32; BINS];
            let mut left_count = [0usize; BINS];
            let (mut l, mut h, mut c) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY), 0);
            for s in 0..BINS {
                l = l.min(bb[s].0);
                h = h.max(bb[s].1);
                c += bc[s];
                left_area[s] = area(l, h);
                left_count[s] = c;
            }
            let (mut l, mut h, mut c) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY), 0);
            for s in (1..BINS).rev() {
                l = l.min(bb[s].0);
                h = h.max(bb[s].1);
                c += bc[s];
                let cost = left_area[s - 1] * left_count[s - 1] as f32 + area(l, h) * c as f32;
                if left_count[s - 1] > 0 && c > 0 && cost < best.0 {
                    best = (cost, s);
                }
            }
            let leaf_cost = area(lo, hi) * count as f32;
            if !best.0.is_finite() || (best.0 >= leaf_cost && count <= 16) {
                leaf(&mut nodes);
                continue;
            }
            // partition: bins < split go left
            let split = best.1;
            let part = &mut index[first..first + count];
            let mut i = 0;
            let mut j = part.len();
            while i < j {
                if bin_of(part[i]) < split {
                    i += 1;
                } else {
                    j -= 1;
                    part.swap(i, j);
                }
            }
            let nl = i;
            let li = nodes.len();
            nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, first: 0, count: 0, right: 0 });
            let ri = nodes.len();
            nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, first: 0, count: 0, right: 0 });
            nodes[ni].first = li as u32;
            nodes[ni].right = ri as u32;
            nodes[ni].count = 0;
            stack.push((ri, first + nl, count - nl));
            stack.push((li, first, nl));
        }
        let tris = index.iter().map(|&k| tf[k as usize]).collect();
        Bvh { origin, nodes, tris, index }
    }

    /// Nearest hit of the ray `o + t d` (unit `d`) with `tmin < t < tmax`.
    pub fn intersect(&self, o: DVec3, d: DVec3, tmin: f64, tmax: f64) -> Option<Hit> {
        if self.tris.is_empty() {
            return None;
        }
        let of = (o - self.origin).as_vec3();
        let df = d.as_vec3();
        let inv = Vec3::new(1.0 / df.x, 1.0 / df.y, 1.0 / df.z);
        let (tmin, mut tbest) = (tmin as f32, tmax as f32);
        let mut best: Option<(u32, f32, f32)> = None;
        let slab = |n: &Node, tbest: f32| -> Option<f32> {
            let t0 = (n.lo - of) * inv;
            let t1 = (n.hi - of) * inv;
            let near = t0.min(t1).max_element().max(tmin);
            let far = t0.max(t1).min_element().min(tbest);
            (near <= far).then_some(near)
        };
        let mut stack = [0u32; 64];
        let mut sp = 0usize;
        slab(&self.nodes[0], tbest)?;
        let mut cur = 0u32;
        loop {
            let n = &self.nodes[cur as usize];
            if n.count > 0 {
                for k in n.first..n.first + n.count {
                    let t = &self.tris[k as usize];
                    // Möller–Trumbore
                    let p = df.cross(t.e2);
                    let det = t.e1.dot(p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let id = 1.0 / det;
                    let s = of - t.v0;
                    let u = s.dot(p) * id;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = s.cross(t.e1);
                    let v = df.dot(q) * id;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let tt = t.e2.dot(q) * id;
                    if tt > tmin && tt < tbest {
                        tbest = tt;
                        best = Some((k, u, v));
                    }
                }
            } else {
                let (a, b) = (n.first, n.right);
                let ha = slab(&self.nodes[a as usize], tbest);
                let hb = slab(&self.nodes[b as usize], tbest);
                match (ha, hb) {
                    (Some(x), Some(y)) => {
                        let (near, far) = if x <= y { (a, b) } else { (b, a) };
                        if sp < stack.len() {
                            stack[sp] = far;
                            sp += 1;
                        }
                        cur = near;
                        continue;
                    }
                    (Some(_), None) => {
                        cur = a;
                        continue;
                    }
                    (None, Some(_)) => {
                        cur = b;
                        continue;
                    }
                    (None, None) => {}
                }
            }
            // next node from the stack (skipping boxes beyond the best hit)
            loop {
                if sp == 0 {
                    return best.map(|(k, u, v)| Hit {
                        t: tbest as f64,
                        tri: self.index[k as usize],
                        u: u as f64,
                        v: v as f64,
                    });
                }
                sp -= 1;
                let c = stack[sp];
                if slab(&self.nodes[c as usize], tbest).is_some() {
                    cur = c;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f64, item: u32) -> [RtTri; 2] {
        let p = |x: f64, y: f64| DVec3::new(x, y, z);
        let n = [DVec3::Z; 3];
        [
            RtTri { p: [p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)], n, item },
            RtTri { p: [p(0.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)], n, item },
        ]
    }

    #[test]
    fn nearest_hit_through_many_layers() {
        let mut tris = Vec::new();
        for k in 0..200 {
            tris.extend(quad(k as f64 * 0.5, k));
        }
        let bvh = Bvh::build(&tris);
        let h = bvh.intersect(DVec3::new(0.3, 0.6, 200.0), DVec3::NEG_Z, 1e-6, 1e9).expect("hit");
        assert_eq!(tris[h.tri as usize].item, 199);
        assert!((h.t - (200.0 - 99.5)).abs() < 1e-3, "{}", h.t);
        let h = bvh.intersect(DVec3::new(0.3, 0.6, 10.2), DVec3::Z, 1e-6, 1e9).expect("hit");
        assert_eq!(tris[h.tri as usize].item, 21);
        assert!(bvh.intersect(DVec3::new(2.0, 0.6, 10.2), DVec3::Z, 1e-6, 1e9).is_none());
        assert!(bvh.intersect(DVec3::new(0.3, 0.6, 10.2), DVec3::Z, 1e-6, 0.2).is_none());
    }
}
