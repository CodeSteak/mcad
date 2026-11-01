//! Checks of a mesh (plan 2.6), usable for any [`Mesh`] (also SMLib's): vertices welded by
//! position, then every edge must be used by exactly two triangles in opposite directions
//! (watertight, consistently oriented), the enclosed volume must be positive (normals
//! outwards), every position finite, no triangle may be degenerate, and the enclosed volume
//! must match the exact one within `tol · area`.

use std::collections::HashMap;

use cad_kernel::Mesh;
use glam::DVec3;

/// Result of [`check`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshCheck {
    pub triangles: usize,
    /// Distinct positions.
    pub vertices: usize,
    /// Edges used by one triangle only (holes; the rim of an open sheet).
    pub boundary_edges: usize,
    /// Edges used by more than two triangles.
    pub nonmanifold_edges: usize,
    /// Edges used twice in the same direction (orientation flips).
    pub misoriented_edges: usize,
    /// Triangles with two equal corners or (almost) no area.
    pub degenerate: usize,
    /// Enclosed volume (divergence theorem; meaningful when watertight), mm³.
    pub volume: f64,
    pub area: f64,
    /// Triangles with an index beyond the vertices (skipped).
    pub bad_indices: usize,
    /// Positions with a NaN or infinite coordinate.
    pub non_finite: usize,
    /// Closed and consistently oriented, but a connected piece encloses a negative volume
    /// without lying inside another piece (a void lies inside its body and faces inwards by
    /// design): its triangles face inwards.
    pub inside_out: bool,
    /// Triangles with the same three corners as another one (either orientation; a fin of
    /// two triangles facing each other closes its own edges).
    pub duplicate_triangles: usize,
}

impl MeshCheck {
    /// Closed, manifold, consistently oriented outwards, without duplicate triangles, with
    /// finite positions and not empty. A crack closed by a zero-area
    /// triangle (a T-junction) leaves no gap and counts as closed; `degenerate` reports such
    /// triangles (also slivers whose height is below the `f32` resolution of the positions, at
    /// tolerances near [`crate::MIN_REL_TOL`]). Not detected: self-intersections.
    pub fn watertight(&self) -> bool {
        self.triangles > 0
            && self.boundary_edges == 0
            && self.nonmanifold_edges == 0
            && self.misoriented_edges == 0
            && self.bad_indices == 0
            && self.non_finite == 0
            && self.duplicate_triangles == 0
            && !self.inside_out
    }

    /// `|volume − exact| ≤ tol · area`: a mesh within `tol` of the surface encloses about
    /// that much more or less.
    pub fn volume_ok(&self, exact: f64, tol: f64) -> bool {
        (self.volume - exact).abs() <= tol * self.area
    }
}

/// Measures `mesh` (see the module docs).
pub fn check(mesh: &Mesh) -> MeshCheck {
    let mut weld: HashMap<[u32; 3], u32> = HashMap::new();
    let ids: Vec<u32> = mesh
        .positions
        .chunks_exact(3)
        .map(|p| {
            let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
            let n = weld.len() as u32;
            *weld.entry(key).or_insert(n)
        })
        .collect();
    let pos = |i: u32| {
        let k = 3 * i as usize;
        DVec3::new(mesh.positions[k] as f64, mesh.positions[k + 1] as f64, mesh.positions[k + 2] as f64)
    };
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for i in 0..(mesh.positions.len() / 3) as u32 {
        lo = lo.min(pos(i));
        hi = hi.max(pos(i));
    }
    let size2 = if lo.x.is_finite() { (hi - lo).length_squared() } else { 0.0 };
    let mut out = MeshCheck { triangles: mesh.indices.len() / 3, vertices: weld.len(), ..Default::default() };
    out.non_finite = mesh.positions.chunks_exact(3).filter(|p| !p.iter().all(|x| x.is_finite())).count();
    // per undirected edge: uses in direction (lo → hi) and (hi → lo)
    let mut uses: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    // welded triangles (sorted corners) and the triangles kept
    let mut tris: HashMap<[u32; 3], u32> = HashMap::new();
    let mut kept: Vec<[u32; 3]> = Vec::new();
    let nv = ids.len();
    for t in mesh.indices.chunks_exact(3) {
        if t.iter().any(|i| *i as usize >= nv) {
            out.bad_indices += 1;
            continue;
        }
        let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
        let cr = (b - a).cross(c - a);
        out.area += 0.5 * cr.length();
        out.volume += a.dot(b.cross(c)) / 6.0;
        let w = [ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]];
        if w[0] == w[1] || w[1] == w[2] || w[2] == w[0] || cr.length() <= 1e-14 * size2 {
            out.degenerate += 1;
        } else {
            let mut key = w;
            key.sort_unstable();
            let n = tris.entry(key).or_default();
            *n += 1;
            if *n > 1 {
                out.duplicate_triangles += 1;
            }
        }
        kept.push([t[0], t[1], t[2]]);
        for k in 0..3 {
            let (x, y) = (w[k], w[(k + 1) % 3]);
            if x == y {
                continue;
            }
            let e = uses.entry((x.min(y), x.max(y))).or_default();
            if x < y { e.0 += 1 } else { e.1 += 1 }
        }
    }
    for (f, r) in uses.values() {
        match f + r {
            1 => out.boundary_edges += 1,
            2 if *f != 1 => out.misoriented_edges += 1,
            2 => {}
            _ => out.nonmanifold_edges += 1,
        }
    }
    let closed = out.triangles > 0 && out.boundary_edges + out.nonmanifold_edges + out.misoriented_edges == 0;
    out.inside_out = closed && (out.volume < 0.0 || inverted_piece(&kept, &ids, &pos));
    out
}

/// A connected piece of a closed mesh that encloses a negative volume and does not lie inside
/// the others (winding number of the other pieces at one of its points below one half): a void
/// lies inside its body, an inverted body does not.
fn inverted_piece(tris: &[[u32; 3]], ids: &[u32], pos: &impl Fn(u32) -> DVec3) -> bool {
    // pieces: union of the welded corners
    let n = ids.iter().map(|i| *i as usize + 1).max().unwrap_or(0);
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for t in tris {
        let w = t.map(|i| ids[i as usize]);
        for k in 1..3 {
            let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[k]));
            if a != b {
                parent[a as usize] = b;
            }
        }
    }
    // per piece its volume, bounding box and triangles
    struct Piece {
        volume: f64,
        lo: DVec3,
        hi: DVec3,
        tris: Vec<usize>,
    }
    let mut pieces: HashMap<u32, Piece> = HashMap::new();
    for (k, t) in tris.iter().enumerate() {
        let id = find(&mut parent, ids[t[0] as usize]);
        let [a, b, c] = t.map(pos);
        let p = pieces.entry(id).or_insert(Piece {
            volume: 0.0,
            lo: DVec3::splat(f64::INFINITY),
            hi: DVec3::splat(f64::NEG_INFINITY),
            tris: Vec::new(),
        });
        p.volume += a.dot(b.cross(c)) / 6.0;
        p.lo = p.lo.min(a).min(b).min(c);
        p.hi = p.hi.max(a).max(b).max(c);
        p.tris.push(k);
    }
    if pieces.len() < 2 {
        return false;
    }
    pieces.iter().filter(|(_, p)| p.volume < 0.0).any(|(id, piece)| {
        let [a, b, c] = tris[piece.tris[0]].map(pos);
        let q = (a + b + c) / 3.0;
        // winding number of the other pieces (solid angles, Van Oosterom & Strackee); a closed
        // piece whose box does not hold `q` adds nothing (a cube minus 125 balls: 3.6 s without
        // this filter)
        let mut w = 0.0;
        let around = pieces.iter().filter(|(o, p)| *o != id && q.cmpge(p.lo).all() && q.cmple(p.hi).all());
        for t in around.flat_map(|(_, p)| p.tris.iter().map(|k| tris[*k])) {
            let [x, y, z] = t.map(|i| pos(i) - q);
            let (lx, ly, lz) = (x.length(), y.length(), z.length());
            let num = x.dot(y.cross(z));
            let den = lx * ly * lz + x.dot(y) * lz + y.dot(z) * lx + z.dot(x) * ly;
            w += 2.0 * num.atan2(den);
        }
        w / (4.0 * std::f64::consts::PI) < 0.5
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra(flip: bool) -> Mesh {
        let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut t = vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];
        if flip {
            t.swap(0, 1);
        }
        Mesh { positions: p.iter().flatten().map(|x: &f64| *x as f32).collect(), indices: t, ..Default::default() }
    }

    #[test]
    fn tetrahedron() {
        let c = check(&tetra(false));
        assert!(c.watertight(), "{c:?}");
        assert!((c.volume - 1.0 / 6.0).abs() < 1e-7);
        assert!(c.volume_ok(1.0 / 6.0, 1e-6));
        let c = check(&tetra(true));
        assert_eq!(c.misoriented_edges, 3);
        // inside out: closed and consistent, but facing inwards
        let mut m = tetra(false);
        m.indices.chunks_exact_mut(3).for_each(|t| t.swap(1, 2));
        let c = check(&m);
        assert!(c.inside_out && c.volume < 0.0 && !c.watertight(), "{c:?}");
        // a NaN position
        let mut m = tetra(false);
        m.positions[4] = f32::NAN;
        let c = check(&m);
        assert_eq!(c.non_finite, 1);
        assert!(!c.watertight(), "{c:?}");
        let mut m = tetra(false);
        m.indices.truncate(9);
        assert_eq!(check(&m).boundary_edges, 3);
        // empty, an index out of range, a crack patched with a zero-area triangle
        assert!(!check(&Mesh::default()).watertight());
        let mut m = tetra(false);
        m.indices.extend_from_slice(&[0, 1, 9]);
        let c = check(&m);
        assert_eq!(c.bad_indices, 1);
        assert!(!c.watertight());
        // edge 1–2 split at its midpoint 4 on one side, closed by the triangle (1, 4, 2)
        let mut m = tetra(false);
        m.positions.extend_from_slice(&[0.5, 0.5, 0.0]);
        let t = &mut m.indices;
        // the bottom triangle (0, 2, 1) becomes (0, 2, 4) and (0, 4, 1)
        t.splice(0..3, [0, 2, 4, 0, 4, 1]);
        t.extend_from_slice(&[1, 4, 2]);
        let c = check(&m);
        assert_eq!((c.boundary_edges, c.misoriented_edges, c.nonmanifold_edges), (0, 0, 0), "{c:?}");
        assert_eq!(c.degenerate, 1);
        assert!(c.watertight());
        // a duplicate triangle, a fin of two triangles facing each other (closed by itself)
        let mut m = tetra(false);
        m.indices.extend_from_slice(&[0, 2, 1]);
        let c = check(&m);
        assert!(c.duplicate_triangles == 1 && !c.watertight(), "{c:?}");
        let mut m = tetra(false);
        m.positions.extend_from_slice(&[5.0, 0.0, 0.0, 6.0, 0.0, 0.0, 5.0, 1.0, 0.0]);
        m.indices.extend_from_slice(&[4, 5, 6, 4, 6, 5]);
        let c = check(&m);
        assert_eq!((c.boundary_edges, c.nonmanifold_edges, c.misoriented_edges), (0, 0, 0), "{c:?}");
        assert!(c.duplicate_triangles == 1 && !c.watertight(), "{c:?}");
    }

    /// Two tetrahedra: one inverted beside the other is inside out though the net volume is
    /// positive; an inverted small one inside a large one is a void.
    #[test]
    fn inside_out_per_piece() {
        let piece = |m: &mut Mesh, s: f32, o: f32, flip: bool| {
            let t = tetra(false);
            let base = (m.positions.len() / 3) as u32;
            m.positions.extend(t.positions.iter().map(|x| x * s + o));
            for tri in t.indices.chunks_exact(3) {
                let tri = if flip { [tri[0], tri[2], tri[1]] } else { [tri[0], tri[1], tri[2]] };
                m.indices.extend(tri.iter().map(|i| i + base));
            }
        };
        let mut m = Mesh::default();
        piece(&mut m, 2.0, 0.0, false);
        piece(&mut m, 1.0, 5.0, true);
        let c = check(&m);
        assert!(c.volume > 0.0 && c.inside_out && !c.watertight(), "{c:?}");
        let mut m = Mesh::default();
        piece(&mut m, 4.0, 0.0, false);
        piece(&mut m, 1.0, 0.3, true);
        let c = check(&m);
        assert!(!c.inside_out && c.watertight(), "{c:?}");
    }
}
