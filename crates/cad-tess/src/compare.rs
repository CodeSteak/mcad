//! Distances of a mesh (plan 2.7): to the exact surfaces of its B-rep (each triangle sampled and
//! its sample points projected onto the surface of its face) and to another mesh (sample points
//! to the nearest triangle, one direction; the Hausdorff distance is the larger of both).
//! Usable for any mesh whose `tri_faces` are the B-rep's face ids (also SMLib's own).

use std::collections::HashMap;

use cad_kernel::Mesh;
use cad_kernel::brep::Brep;
use glam::{DVec2, DVec3};

use crate::surface::Geom;

/// Sampled distance, mm.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Distance {
    pub max: f64,
    pub mean: f64,
    /// Where the largest distance was found.
    pub at: DVec3,
    pub samples: usize,
}

impl Distance {
    fn add(&mut self, d: f64, p: DVec3) {
        if d > self.max || self.samples == 0 {
            self.max = self.max.max(d);
            self.at = p;
        }
        // running sum; divided in `finish`
        self.mean += d;
        self.samples += 1;
    }

    fn finish(mut self) -> Distance {
        if self.samples > 0 {
            self.mean /= self.samples as f64;
        }
        self
    }

    /// The larger of two directions (Hausdorff distance).
    pub fn max_of(a: Distance, b: Distance) -> Distance {
        let n = a.samples + b.samples;
        let mean = if n > 0 { (a.mean * a.samples as f64 + b.mean * b.samples as f64) / n as f64 } else { 0.0 };
        let w = if b.max > a.max { b } else { a };
        Distance { max: w.max, mean, at: w.at, samples: n }
    }
}

fn pos(m: &Mesh, i: u32) -> DVec3 {
    let k = 3 * i as usize;
    DVec3::new(m.positions[k] as f64, m.positions[k + 1] as f64, m.positions[k + 2] as f64)
}

fn triangle(m: &Mesh, t: usize) -> [DVec3; 3] {
    [pos(m, m.indices[3 * t]), pos(m, m.indices[3 * t + 1]), pos(m, m.indices[3 * t + 2])]
}

/// Barycentric sample points of a triangle: centroid, edge midpoints, inner points towards the
/// corners (where the chord error of a triangle is largest).
const SAMPLES: [[f64; 3]; 7] = [
    [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
    [0.5, 0.5, 0.0],
    [0.0, 0.5, 0.5],
    [0.5, 0.0, 0.5],
    [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0],
    [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0],
    [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0],
];

fn at(t: &[DVec3; 3], w: &[f64; 3]) -> DVec3 {
    t[0] * w[0] + t[1] * w[1] + t[2] * w[2]
}

/// [`surface_distance`]: the sampled distance to the surfaces, and the part of it that is the
/// chord's (the distance minus the corners' distances interpolated at the sample point).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SurfaceDistance {
    pub distance: Distance,
    /// The chord error: what the triangles add to the distance of their corners. This is the
    /// tessellation's error; the corners' distances are the B-rep's (an edge that lies off its
    /// face's surface, e.g. at the fitted end of a sweep).
    pub chord: Distance,
    /// Largest distance of a mesh vertex from the surface of its face.
    pub vertex_gap: f64,
    /// The same for the vertices inside their face (neither on an edge of its triangles that no
    /// other triangle of the face shares nor a point of an edge polyline, `edge_segments`: a
    /// seam), or their distance from the B-rep's edges if smaller (a point where loops that
    /// cross at a cusp were split, on the chord between edge points): they are surface points,
    /// while a boundary vertex is a point of the B-rep's edge, which may lie off the face (see
    /// `chord`).
    pub inner_vertex_gap: f64,
    /// Largest distance of a boundary vertex (on an edge of its face's triangles that no other
    /// triangle of the face shares, or a point of an edge polyline) from the nearest edge of the
    /// B-rep: such a vertex is a point of an edge (or of a chord between two, where loops cross
    /// at a cusp), however far that edge lies off the face's surface (`vertex_gap`).
    pub edge_vertex_gap: f64,
    /// The vertices inside their face that `inner_vertex_gap` finds off the surface (more than
    /// 1e-6 of the size), with that distance: another mesh's local gap may excuse them
    /// ([`local_allowance`]).
    pub inner_off: Vec<(DVec3, f64)>,
    /// Per triangle (in the mesh's order) the largest sampled distance to the surface (0 for
    /// triangles of a face missing in the B-rep): the local allowance of another mesh's
    /// distance to this one ([`mesh_excess`]).
    pub per_triangle: Vec<f64>,
}

/// Segments per free-form edge for [`SurfaceDistance::edge_vertex_gap`].
const EDGE_SAMPLES: usize = 512;

/// Segments of a full turn of radius `r` with a chord sag of 1e-4 mm.
fn arc_samples(r: f64) -> usize {
    let step = 2.0 * (1.0 - 1e-4 / r.max(1e-4)).clamp(-1.0, 1.0).acos();
    ((std::f64::consts::TAU / step.max(1e-6)).ceil() as usize).clamp(16, 16384)
}

/// Distance of `mesh` to the surfaces of `brep`: per triangle the sample points projected onto
/// the surface of its face (`tri_faces`), seeded from the parameters of its corners. Triangles
/// of a face missing in `brep` are skipped.
pub fn surface_distance(brep: &Brep, mesh: &Mesh) -> SurfaceDistance {
    let size = mesh.bounds().map_or(1.0, |(lo, hi)| (hi - lo).length());
    let eps = (1e-7 * size).max(1e-9);
    let mut geoms: HashMap<u32, Geom> = HashMap::new();
    for f in brep.faces() {
        geoms.entry(f.id).or_insert_with(|| Geom::new(&f.surface, eps));
    }
    // parameters of each corner on the surface of the face (a vertex may be shared by faces)
    // and its distance from it
    let mut corner: HashMap<(u32, u32), (DVec2, f64)> = HashMap::new();
    // vertices welded by position (the copies of a pole or seam point are one)
    let mut weld: HashMap<[u32; 3], u32> = HashMap::new();
    let welded: Vec<u32> = mesh
        .positions
        .chunks_exact(3)
        .enumerate()
        .map(|(i, p)| *weld.entry([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).or_insert(i as u32))
        .collect();
    // triangle edges per face (lower vertex, higher vertex) and how often they are used
    let mut uses: HashMap<(u32, u32, u32), u32> = HashMap::new();
    for t in 0..mesh.indices.len() / 3 {
        let Some(&face) = mesh.tri_faces.get(t) else { continue };
        for k in 0..3 {
            let [a, b] = [mesh.indices[3 * t + k], mesh.indices[3 * t + (k + 1) % 3]].map(|i| welded[i as usize]);
            *uses.entry((face, a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let boundary: std::collections::HashSet<(u32, u32)> =
        uses.iter().filter(|(_, n)| **n == 1).flat_map(|((f, a, b), _)| [(*f, *a), (*f, *b)]).collect();
    // points of the B-rep's edges (also a seam inside a face's triangles)
    let edge_points: std::collections::HashSet<[u32; 3]> =
        mesh.edge_segments.chunks_exact(3).map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).collect();
    let on_edge = |v: u32| {
        let k = 3 * v as usize;
        let p = &mesh.positions[k..k + 3];
        edge_points.contains(&[p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
    };
    // the B-rep's edges, densely sampled, as degenerate triangles (segments) for the search
    let mut segs = Mesh::default();
    for e in &brep.edges {
        let (a, b) = (brep.vertices[e.start as usize], brep.vertices[e.end as usize]);
        use cad_kernel::brep::Curve;
        // circles and ellipses: a chord sag of 1e-4 mm (a full circle of radius 300: 3900)
        let n = match &e.curve {
            Curve::Line { .. } => 1,
            Curve::Circle { r, .. } => arc_samples(*r),
            Curve::Ellipse { r1, r2, .. } => arc_samples(r1.max(*r2)),
            _ => EDGE_SAMPLES,
        };
        let pts = cad_kernel::brep::sample_edge(&e.curve, a, b, n);
        for w in pts.windows(2) {
            let k = (segs.positions.len() / 3) as u32;
            for p in [w[0], w[1]] {
                segs.positions.extend_from_slice(&[p.x as f32, p.y as f32, p.z as f32]);
            }
            segs.indices.extend_from_slice(&[k, k + 1, k + 1]);
        }
    }
    let edges = Bvh::new(&segs);
    let mut on_edges: HashMap<u32, f64> = HashMap::new();
    let mut inner_seen: std::collections::HashSet<u32> = Default::default();
    let mut out = SurfaceDistance { per_triangle: vec![0.0; mesh.indices.len() / 3], ..Default::default() };
    for t in 0..mesh.indices.len() / 3 {
        let Some(g) = mesh.tri_faces.get(t).and_then(|f| geoms.get(f)) else { continue };
        let face = mesh.tri_faces[t];
        let tri = triangle(mesh, t);
        let cs: Vec<(DVec2, f64)> = (0..3)
            .map(|k| {
                let v = mesh.indices[3 * t + k];
                *corner.entry((face, v)).or_insert_with(|| {
                    let uv = g.invert(tri[k], None);
                    (uv, (g.eval(uv) - tri[k]).length())
                })
            })
            .collect();
        for (k, c) in cs.iter().enumerate() {
            out.vertex_gap = out.vertex_gap.max(c.1);
            let v = mesh.indices[3 * t + k];
            let mut to_edges =
                || edges.as_ref().map_or(f64::INFINITY, |e| *on_edges.entry(v).or_insert_with(|| e.distance(tri[k])));
            if !boundary.contains(&(face, welded[v as usize])) && !on_edge(v) {
                // off the surface, but on a chord between edge points (loops that cross at a
                // cusp are split there, inside the face's triangles): the edge's gap
                let d = if c.1 > eps { c.1.min(to_edges()) } else { c.1 };
                out.inner_vertex_gap = out.inner_vertex_gap.max(d);
                if d > 10.0 * eps && inner_seen.insert(v) {
                    out.inner_off.push((tri[k], d));
                }
            } else {
                let d = to_edges();
                if d.is_finite() {
                    out.edge_vertex_gap = out.edge_vertex_gap.max(d);
                }
            }
        }
        for w in &SAMPLES {
            let p = at(&tri, w);
            // the nearest corner's parameters as the seed; the best of the corners otherwise
            let k = (0..3).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
            let mut best = (g.eval(g.project(p, cs[k].0)) - p).length();
            for (j, c) in cs.iter().enumerate() {
                if j != k && best > 0.0 {
                    best = best.min((g.eval(g.project(p, c.0)) - p).length());
                }
            }
            out.distance.add(best, p);
            out.per_triangle[t] = out.per_triangle[t].max(best);
            let gap = w[0] * cs[0].1 + w[1] * cs[1].1 + w[2] * cs[2].1;
            out.chord.add((best - gap).max(0.0), p);
        }
    }
    out.distance = out.distance.finish();
    out.chord = out.chord.finish();
    out
}

/// Bounding-volume hierarchy of the triangles of a mesh (nearest-triangle queries): boxes split
/// at the median of their longest axis down to a few triangles. A query visits the nearer child
/// first and skips every box farther than the best triangle so far, so a point far from the
/// mesh costs about as much as one on it (a uniform grid searched in growing shells of cells
/// took a minute for a mesh shifted by 50 mm).
struct Bvh<'a> {
    mesh: &'a Mesh,
    nodes: Vec<Node>,
    /// Triangle indices; a leaf owns a range of them.
    tris: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    lo: DVec3,
    hi: DVec3,
    /// Leaf: `tris[first..first + count]`; inner node: children `first` and `first + 1`.
    first: u32,
    count: u32,
}

/// Most triangles of a leaf.
const LEAF: usize = 4;

impl<'a> Bvh<'a> {
    fn new(mesh: &'a Mesh) -> Option<Bvh<'a>> {
        let n = mesh.indices.len() / 3;
        if n == 0 {
            return None;
        }
        let boxes: Vec<(DVec3, DVec3)> = (0..n)
            .map(|t| {
                let [a, b, c] = triangle(mesh, t);
                (a.min(b).min(c), a.max(b).max(c))
            })
            .collect();
        let mut bvh = Bvh { mesh, nodes: Vec::with_capacity(2 * n / LEAF + 1), tris: (0..n as u32).collect() };
        bvh.nodes.push(Node { lo: DVec3::ZERO, hi: DVec3::ZERO, first: 0, count: n as u32 });
        // nodes to split: (node, start, end)
        let mut todo = vec![(0usize, 0usize, n)];
        while let Some((i, s, e)) = todo.pop() {
            let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            for &t in &bvh.tris[s..e] {
                lo = lo.min(boxes[t as usize].0);
                hi = hi.max(boxes[t as usize].1);
            }
            bvh.nodes[i].lo = lo;
            bvh.nodes[i].hi = hi;
            if e - s <= LEAF {
                bvh.nodes[i].first = s as u32;
                bvh.nodes[i].count = (e - s) as u32;
                continue;
            }
            let ext = hi - lo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            let mid = (s + e) / 2;
            let key = |t: &u32| {
                let (a, b) = boxes[*t as usize];
                a[axis] + b[axis]
            };
            bvh.tris[s..e].select_nth_unstable_by(mid - s, |x, y| key(x).total_cmp(&key(y)));
            let c = bvh.nodes.len();
            bvh.nodes[i].first = c as u32;
            bvh.nodes[i].count = 0;
            bvh.nodes.push(Node { lo, hi, first: 0, count: 0 });
            bvh.nodes.push(Node { lo, hi, first: 0, count: 0 });
            todo.push((c, s, mid));
            todo.push((c + 1, mid, e));
        }
        Some(bvh)
    }

    /// Distance from `p` to the box of node `i` (0 inside).
    fn box_distance(&self, i: usize, p: DVec3) -> f64 {
        let n = &self.nodes[i];
        (n.lo - p).max(p - n.hi).max(DVec3::ZERO).length()
    }

    /// Distance from `p` to the nearest triangle.
    fn distance(&self, p: DVec3) -> f64 {
        self.nearest(p).0
    }

    /// Distance from `p` to the nearest triangle and its index.
    fn nearest(&self, p: DVec3) -> (f64, usize) {
        let mut best = f64::INFINITY;
        let mut which = 0;
        let mut stack = vec![(0usize, self.box_distance(0, p))];
        while let Some((i, d)) = stack.pop() {
            if d >= best {
                continue;
            }
            let n = self.nodes[i];
            if n.count > 0 {
                for &t in &self.tris[n.first as usize..(n.first + n.count) as usize] {
                    let d = point_triangle(p, &triangle(self.mesh, t as usize));
                    if d < best {
                        (best, which) = (d, t as usize);
                    }
                }
                continue;
            }
            let (a, b) = (n.first as usize, n.first as usize + 1);
            let (da, db) = (self.box_distance(a, p), self.box_distance(b, p));
            // the nearer child on top of the stack
            if da <= db {
                stack.push((b, db));
                stack.push((a, da));
            } else {
                stack.push((a, da));
                stack.push((b, db));
            }
        }
        (best, which)
    }
}

/// Distance from `p` to the triangle `t` (Ericson, "Real-Time Collision Detection", 5.1.5).
fn point_triangle(p: DVec3, t: &[DVec3; 3]) -> f64 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.length();
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return bp.length();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (p - (a + ab * v)).length();
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return cp.length();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (p - (a + ac * w)).length();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).length();
    }
    let den = va + vb + vc;
    if den.abs() <= f64::MIN_POSITIVE {
        // degenerate: the nearest of its edges
        return [(a, b), (b, c), (c, a)]
            .iter()
            .map(|(x, y)| {
                let e = *y - *x;
                let s =
                    if e.length_squared() > 0.0 { ((p - *x).dot(e) / e.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                (p - (*x + e * s)).length()
            })
            .fold(f64::INFINITY, f64::min);
    }
    let (v, w) = (vb / den, vc / den);
    (p - (a + ab * v + ac * w)).length()
}

/// Distance from the sample points of `from` (corners and [`SAMPLES`] of every triangle) to
/// the nearest triangle of `to`.
pub fn mesh_distance(from: &Mesh, to: &Mesh) -> Distance {
    let mut d = Distance::default();
    let Some(g) = Bvh::new(to) else { return d };
    for i in 0..(from.positions.len() / 3) as u32 {
        let p = pos(from, i);
        d.add(g.distance(p), p);
    }
    for t in 0..from.indices.len() / 3 {
        let tri = triangle(from, t);
        for w in &SAMPLES {
            let p = at(&tri, w);
            d.add(g.distance(p), p);
        }
    }
    d.finish()
}

/// Per point the allowance (one per triangle of `to`, in mesh order; missing ones count 0) of
/// the nearest triangle of `to`, and the distance to it.
pub fn local_allowance(points: &[DVec3], to: &Mesh, to_allow: &[f64]) -> Vec<(f64, f64)> {
    let Some(g) = Bvh::new(to) else { return vec![(0.0, f64::INFINITY); points.len()] };
    points
        .iter()
        .map(|p| {
            let (d, t) = g.nearest(*p);
            (to_allow.get(t).copied().unwrap_or(0.0), d)
        })
        .collect()
}

/// Like [`mesh_distance`], less what the meshes may lie apart where they are: at each sample
/// point of `from` the distance to `to` minus the larger of the allowances (one per triangle,
/// in mesh order; missing ones count 0) of its own triangle and of the nearest triangle of
/// `to`. With each mesh's [`SurfaceDistance::per_triangle`] as its allowance, a mesh that is
/// off its own B-rep somewhere (a fitted sweep end) is excused there only, not everywhere.
pub fn mesh_excess(from: &Mesh, to: &Mesh, from_allow: &[f64], to_allow: &[f64]) -> Distance {
    let mut d = Distance::default();
    let Some(g) = Bvh::new(to) else { return d };
    let allow = |v: &[f64], t: usize| v.get(t).copied().unwrap_or(0.0);
    for t in 0..from.indices.len() / 3 {
        let tri = triangle(from, t);
        for (k, w) in SAMPLES.iter().enumerate() {
            let p = at(&tri, w);
            let (dist, near) = g.nearest(p);
            d.add((dist - allow(from_allow, t).max(allow(to_allow, near))).max(0.0), p);
            // the corners once per triangle
            if k == 0 {
                for c in tri {
                    let (dist, near) = g.nearest(c);
                    d.add((dist - allow(from_allow, t).max(allow(to_allow, near))).max(0.0), c);
                }
            }
        }
    }
    d.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32) -> Mesh {
        Mesh {
            positions: vec![0.0, 0.0, z, 1.0, 0.0, z, 1.0, 1.0, z, 0.0, 1.0, z],
            indices: vec![0, 1, 2, 0, 2, 3],
            tri_faces: vec![0, 0],
            ..Default::default()
        }
    }

    #[test]
    fn meshes_apart() {
        let d = mesh_distance(&quad(0.0), &quad(0.25));
        assert!((d.max - 0.25).abs() < 1e-6 && (d.mean - 0.25).abs() < 1e-6, "{d:?}");
        assert!(mesh_distance(&quad(0.0), &quad(0.0)).max < 1e-12);
        let t = [DVec3::ZERO, DVec3::X, DVec3::Y];
        assert!((point_triangle(DVec3::new(2.0, 0.0, 0.0), &t) - 1.0).abs() < 1e-12);
        assert!((point_triangle(DVec3::new(0.2, 0.2, 3.0), &t) - 3.0).abs() < 1e-12);
        assert!((point_triangle(DVec3::new(1.0, 1.0, 0.0), &t) - 0.5f64.sqrt()).abs() < 1e-12);
        // far from the mesh: the same distance, found quickly
        let d = mesh_distance(&quad(50.0), &quad(0.0));
        assert!((d.max - 50.0).abs() < 1e-6 && (d.mean - 50.0).abs() < 1e-6, "{d:?}");
        // excused where the target's triangle may be off (first triangle only), not elsewhere
        let d = mesh_excess(&quad(0.0), &quad(0.25), &[], &[0.3, 0.0]);
        assert!((d.max - 0.25).abs() < 1e-6, "{d:?}");
        let d = mesh_excess(&quad(0.0), &quad(0.25), &[], &[0.3, 0.3]);
        assert!(d.max < 1e-12, "{d:?}");
    }

    /// The hierarchy finds the nearest triangle of a larger mesh (against a brute-force search).
    #[test]
    fn nearest_triangle() {
        let mut m = Mesh::default();
        let n = 30;
        for i in 0..=n {
            for j in 0..=n {
                let (x, y) = (i as f32 / n as f32 * 10.0, j as f32 / n as f32 * 10.0);
                m.positions.extend_from_slice(&[x, y, (x * 0.7).sin() * (y * 0.4).cos()]);
            }
        }
        for i in 0..n {
            for j in 0..n {
                let a = (i * (n + 1) + j) as u32;
                let (b, c, d) = (a + 1, a + n as u32 + 1, a + n as u32 + 2);
                m.indices.extend_from_slice(&[a, c, d, a, d, b]);
            }
        }
        let bvh = Bvh::new(&m).unwrap();
        for k in 0..200 {
            let f = k as f64;
            let p = DVec3::new((f * 0.37).sin() * 14.0 + 5.0, (f * 0.91).cos() * 14.0 + 5.0, (f * 0.13).sin() * 6.0);
            let brute =
                (0..m.indices.len() / 3).map(|t| point_triangle(p, &triangle(&m, t))).fold(f64::INFINITY, f64::min);
            assert!((bvh.distance(p) - brute).abs() < 1e-12, "{p}");
        }
    }
}
