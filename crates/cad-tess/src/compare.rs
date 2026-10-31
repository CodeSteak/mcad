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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SurfaceDistance {
    pub distance: Distance,
    /// The chord error: what the triangles add to the distance of their corners. This is the
    /// tessellation's error; the corners' distances are the B-rep's (an edge that lies off its
    /// face's surface, e.g. at the fitted end of a sweep).
    pub chord: Distance,
    /// Largest distance of a mesh vertex from the surface of its face.
    pub vertex_gap: f64,
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
    let mut out = SurfaceDistance::default();
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
        for c in &cs {
            out.vertex_gap = out.vertex_gap.max(c.1);
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
            let gap = w[0] * cs[0].1 + w[1] * cs[1].1 + w[2] * cs[2].1;
            out.chord.add((best - gap).max(0.0), p);
        }
    }
    out.distance = out.distance.finish();
    out.chord = out.chord.finish();
    out
}

/// Uniform grid of the triangles of a mesh (nearest-triangle queries).
struct Grid<'a> {
    mesh: &'a Mesh,
    lo: DVec3,
    cell: f64,
    n: [i64; 3],
    cells: HashMap<[i64; 3], Vec<u32>>,
}

impl<'a> Grid<'a> {
    fn new(mesh: &'a Mesh) -> Option<Grid<'a>> {
        let (lo, hi) = mesh.bounds()?;
        let (lo, hi) = (lo - DVec3::splat(1e-9), hi + DVec3::splat(1e-9));
        let ext = hi - lo;
        let tris = (mesh.indices.len() / 3).max(1) as f64;
        // about two triangles per cell over the surface: cells of the size of the mean triangle
        let area: f64 = (0..mesh.indices.len() / 3)
            .map(|t| {
                let [a, b, c] = triangle(mesh, t);
                0.5 * (b - a).cross(c - a).length()
            })
            .sum();
        let cell = (2.0 * area / tris).sqrt().max(ext.max_element() / 512.0).max(1e-6);
        let n = [0, 1, 2].map(|i| ((ext[i] / cell).ceil() as i64).max(1));
        let mut cells: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
        for t in 0..mesh.indices.len() / 3 {
            let [a, b, c] = triangle(mesh, t);
            let (tl, th) = (a.min(b).min(c), a.max(b).max(c));
            let (il, ih) = (Self::index(lo, cell, n, tl), Self::index(lo, cell, n, th));
            for x in il[0]..=ih[0] {
                for y in il[1]..=ih[1] {
                    for z in il[2]..=ih[2] {
                        cells.entry([x, y, z]).or_default().push(t as u32);
                    }
                }
            }
        }
        Some(Grid { mesh, lo, cell, n, cells })
    }

    fn index(lo: DVec3, cell: f64, n: [i64; 3], p: DVec3) -> [i64; 3] {
        let q = (p - lo) / cell;
        [0, 1, 2].map(|i| (q[i].floor() as i64).clamp(0, n[i] - 1))
    }

    /// Distance from `p` to the nearest triangle: shells of cells around `p` until no closer
    /// triangle can lie farther out.
    fn distance(&self, p: DVec3) -> f64 {
        let c = Self::index(self.lo, self.cell, self.n, p);
        // distance of p outside the grid box
        let hi = self.lo + DVec3::new(self.n[0] as f64, self.n[1] as f64, self.n[2] as f64) * self.cell;
        let out = (self.lo - p).max(p - hi).max(DVec3::ZERO).length();
        let mut best = f64::INFINITY;
        let rmax = self.n.iter().copied().max().unwrap_or(1);
        for r in 0..=rmax {
            // cells r rings out are at least (r − 1) cells away (and `out` for a point outside)
            if best <= out.max((r as f64 - 1.0) * self.cell) {
                break;
            }
            for x in (c[0] - r).max(0)..=(c[0] + r).min(self.n[0] - 1) {
                for y in (c[1] - r).max(0)..=(c[1] + r).min(self.n[1] - 1) {
                    for z in (c[2] - r).max(0)..=(c[2] + r).min(self.n[2] - 1) {
                        // the shell only
                        if (x - c[0]).abs() != r && (y - c[1]).abs() != r && (z - c[2]).abs() != r {
                            continue;
                        }
                        for &t in self.cells.get(&[x, y, z]).map_or(&[][..], |v| &v[..]) {
                            best = best.min(point_triangle(p, &triangle(self.mesh, t as usize)));
                        }
                    }
                }
            }
        }
        best
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
    let Some(g) = Grid::new(to) else { return d };
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
    }
}
