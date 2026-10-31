//! Checks of a mesh (plan 2.6), usable for any [`Mesh`] (also SMLib's): vertices welded by
//! position, then every edge must be used by exactly two triangles in opposite directions
//! (watertight, consistently oriented), no triangle may be degenerate, and the enclosed volume
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
}

impl MeshCheck {
    /// Closed, manifold, consistently oriented and not empty. A crack closed by a zero-area
    /// triangle (a T-junction) leaves no gap and counts as closed; `degenerate` reports such
    /// triangles (also slivers whose height is below the `f32` resolution of the positions, at
    /// tolerances near [`crate::MIN_REL_TOL`]). Not detected: self-intersections and fins of two
    /// coinciding triangles facing each other.
    pub fn watertight(&self) -> bool {
        self.triangles > 0
            && self.boundary_edges == 0
            && self.nonmanifold_edges == 0
            && self.misoriented_edges == 0
            && self.bad_indices == 0
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
    // per undirected edge: uses in direction (lo → hi) and (hi → lo)
    let mut uses: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
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
        }
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
    out
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
    }
}
