//! Tessellator for the neutral B-rep ([`cad_kernel::brep::Brep`], plan phase 2): a triangle
//! mesh ([`cad_kernel::Mesh`]) within a chordal tolerance, for any kernel that hands over its
//! B-rep (`Kernel::brep`). Pure Rust (Wasm-capable), no kernel code.
//!
//! 1. Every edge is discretised once ([`edges`]): both faces of an edge use the same points, so
//!    the mesh is watertight.
//! 2. Each face is triangulated in its parameter plane: boundary points inverted onto the
//!    surface ([`surface`], [`uv`]), a constrained Delaunay triangulation with grid points,
//!    refined against the surface ([`face`]).
//! 3. Output ([`tessellate`]): positions, normals exactly from the surface (oriented by
//!    `same_sense`), triangles counter-clockwise seen from outside, face tag and id per
//!    triangle, every edge's polyline with its id.
//!
//! [`check`] measures a mesh (watertightness, orientation, degenerate triangles, volume).
//!
//! ```
//! # use cad_kernel::brep::Brep;
//! # fn kernel_brep() -> Brep { Brep::default() }
//! let brep: Brep = kernel_brep();
//! let mesh = cad_tess::tessellate(&brep, 0.05).unwrap();
//! let c = cad_tess::check(&mesh);
//! assert!(c.watertight());
//! ```

use std::collections::HashMap;

use cad_kernel::brep::Brep;
use cad_kernel::{FaceId, KernelError, Mesh};
use glam::DVec3;

mod bspline;
mod check;
mod edges;
mod face;
mod surface;
mod uv;

pub use check::{MeshCheck, check};

/// Settings of a tessellation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Largest distance between the mesh and the surface (chord height), mm.
    pub tol: f64,
    /// Largest turn of an edge's tangent between two of its points and largest angle step on
    /// curved surfaces, radians (as SMLib's 20°).
    pub angle: f64,
    /// Refinement passes per face.
    pub max_passes: usize,
    /// No refinement beyond this many triangles in one face.
    pub max_face_triangles: usize,
}

impl Options {
    pub fn new(tol: f64) -> Options {
        Options { tol, angle: 20f64.to_radians(), max_passes: 12, max_face_triangles: 200_000 }
    }
}

/// Figures of a tessellation (tests, `diff-kernels`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub faces: usize,
    /// Points the refinement added.
    pub refined: usize,
    /// Boundary segments that could not be kept (loops crossing in their parameter plane).
    pub lost_constraints: usize,
    /// Faces without triangles and why.
    pub failed_faces: Vec<(FaceId, String)>,
    /// Largest deviation of a triangle centroid or inner edge midpoint from the surface in the
    /// last refinement pass of a curved face (planar faces: 0).
    pub max_deviation: f64,
}

#[derive(Debug, Clone)]
pub struct Tessellation {
    pub mesh: Mesh,
    pub stats: Stats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TessError(pub String);

impl std::fmt::Display for TessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tessellate: {}", self.0)
    }
}

impl std::error::Error for TessError {}

impl From<TessError> for KernelError {
    fn from(e: TessError) -> KernelError {
        KernelError::failed(e.to_string())
    }
}

/// The mesh of `brep` within `tol` (mm). Fails if a face cannot be triangulated (see
/// [`tessellate_with`] for the mesh of the other faces).
pub fn tessellate(brep: &Brep, tol: f64) -> Result<Mesh, TessError> {
    let t = tessellate_with(brep, &Options::new(tol))?;
    if let Some((id, why)) = t.stats.failed_faces.first() {
        return Err(TessError(format!("face {id}: {why}")));
    }
    Ok(t.mesh)
}

/// The mesh of `brep` with `opts`; faces that fail are left out and listed in the stats.
pub fn tessellate_with(brep: &Brep, opts: &Options) -> Result<Tessellation, TessError> {
    if !(opts.tol > 0.0 && opts.tol.is_finite()) {
        return Err(TessError("tolerance must be > 0".into()));
    }
    brep.validate().map_err(TessError)?;
    for f in brep.faces() {
        if let cad_kernel::brep::Surface::BSpline(s) = &f.surface
            && (s.degree_u.max(s.degree_v) as usize) >= bspline::MAXP
        {
            return Err(TessError(format!("face {}: degree above {}", f.id, bspline::MAXP - 1)));
        }
    }
    let angle = opts.angle.clamp(1f64.to_radians(), 60f64.to_radians());
    let ep = edges::discretise(brep, opts.tol, angle);
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for p in &ep.points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let size = if lo.x.is_finite() { (hi - lo).length() } else { 1.0 };
    let eps = (1e-7 * size).max(1e-9);
    let params =
        face::Params { tol: opts.tol, angle, max_passes: opts.max_passes, max_triangles: opts.max_face_triangles };
    let mut mesh = Mesh::default();
    let mut stats = Stats::default();
    let mut next_id = ep.points.len() as u32;
    let mut splits = Vec::new();
    // boundary point id of each mesh vertex
    let mut vid: Vec<Option<u32>> = Vec::new();
    for f in brep.faces() {
        stats.faces += 1;
        let g = surface::Geom::new(&f.surface, eps);
        let orient = if f.same_sense { 1.0 } else { -1.0 };
        let loops = match uv::face_loops(f, &g, &ep, orient) {
            Ok(l) if l.is_empty() => {
                stats.failed_faces.push((f.id, "no loop with an area".into()));
                continue;
            }
            Ok(l) => l,
            Err(e) => {
                stats.failed_faces.push((f.id, e));
                continue;
            }
        };
        let fm = face::mesh_face(&g, loops, &ep.points, &params, orient, &mut next_id);
        splits.extend_from_slice(&fm.splits);
        stats.refined += fm.refined;
        stats.lost_constraints += fm.lost_constraints;
        stats.max_deviation = stats.max_deviation.max(fm.deviation);
        if fm.tris.is_empty() {
            stats.failed_faces.push((f.id, "no triangles".into()));
            continue;
        }
        // vertices: boundary points once per face (seam copies meet), the rest as they are
        let base = (mesh.positions.len() / 3) as u32;
        let mut by_id: HashMap<u32, u32> = HashMap::new();
        let mut index = Vec::with_capacity(fm.verts.len());
        let mut count = 0u32;
        for v in &fm.verts {
            let shared = v.id.filter(|_| !v.pole);
            if let Some(i) = shared.and_then(|id| by_id.get(&id)) {
                index.push(*i);
                continue;
            }
            let i = base + count;
            count += 1;
            if let Some(id) = shared {
                by_id.insert(id, i);
            }
            index.push(i);
            vid.push(shared);
            let n = g.normal(v.uv, fm.bounds).map_or(DVec3::ZERO, |n| n * orient);
            mesh.positions.extend_from_slice(&[v.p.x as f32, v.p.y as f32, v.p.z as f32]);
            mesh.normals.extend_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
        }
        for t in &fm.tris {
            let [a, b, c] = t.map(|k| fm.verts[k as usize]);
            // collapsed at a pole, or two copies of one boundary point
            if a.p == b.p || b.p == c.p || c.p == a.p {
                continue;
            }
            let [ia, ib, ic] = t.map(|k| index[k as usize]);
            if orient > 0.0 {
                mesh.indices.extend_from_slice(&[ia, ib, ic]);
            } else {
                mesh.indices.extend_from_slice(&[ia, ic, ib]);
            }
            mesh.tri_tags.push(f.tag);
            mesh.tri_faces.push(f.id);
        }
    }
    apply_splits(&mut mesh, &mut vid, &splits);
    fill_missing_normals(&mut mesh);
    for (i, ids) in ep.edges.iter().enumerate() {
        for w in ids.windows(2) {
            for id in w {
                let p = ep.points[*id as usize];
                mesh.edge_segments.extend_from_slice(&[p.x as f32, p.y as f32, p.z as f32]);
            }
            mesh.edge_ids.push(i as u32);
        }
    }
    Ok(Tessellation { mesh, stats })
}

/// Splits the triangles of the other side of each boundary segment a face split where its
/// loops crossed (see `face::Split`), so the mesh stays watertight.
fn apply_splits(m: &mut Mesh, vid: &mut Vec<Option<u32>>, splits: &[face::Split]) {
    if splits.is_empty() {
        return;
    }
    // per segment (lower id, higher id): the points from the lower end
    let mut by_seg: HashMap<(u32, u32), Vec<(f64, face::Split)>> = HashMap::new();
    for s in splits {
        let (key, t) = if s.a < s.b { ((s.a, s.b), s.t) } else { ((s.b, s.a), 1.0 - s.t) };
        let v = by_seg.entry(key).or_default();
        if !v.iter().any(|(_, o)| o.id == s.id) {
            v.push((t, *s));
        }
    }
    for v in by_seg.values_mut() {
        v.sort_by(|x, y| x.0.total_cmp(&y.0));
    }
    let mut t = 0;
    while t < m.indices.len() / 3 {
        let tri = [m.indices[3 * t], m.indices[3 * t + 1], m.indices[3 * t + 2]];
        let hit = (0..3).find_map(|k| {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            let (Some(a), Some(b)) = (vid[i as usize], vid[j as usize]) else { return None };
            by_seg.get(&(a.min(b), a.max(b))).map(|pts| (k, a > b, pts.clone()))
        });
        let Some((k, from_high, mut pts)) = hit else {
            t += 1;
            continue;
        };
        if from_high {
            pts.reverse();
        }
        let (i, j, o) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
        let normal = |m: &Mesh, v: u32| {
            let q = 3 * v as usize;
            DVec3::new(m.normals[q] as f64, m.normals[q + 1] as f64, m.normals[q + 2] as f64)
        };
        let (ni, nj) = (normal(m, i), normal(m, j));
        let mut chain = vec![i];
        for (s, sp) in &pts {
            let n = ni.lerp(nj, if from_high { 1.0 - s } else { *s }).normalize_or_zero();
            let x = (m.positions.len() / 3) as u32;
            m.positions.extend_from_slice(&[sp.p.x as f32, sp.p.y as f32, sp.p.z as f32]);
            m.normals.extend_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
            vid.push(Some(sp.id));
            chain.push(x);
        }
        chain.push(j);
        let (tag, face) = (m.tri_tags[t], m.tri_faces[t]);
        m.indices[3 * t..3 * t + 3].copy_from_slice(&[chain[0], chain[1], o]);
        for w in chain[1..].windows(2) {
            m.indices.extend_from_slice(&[w[0], w[1], o]);
            m.tri_tags.push(tag);
            m.tri_faces.push(face);
        }
        // the triangle may need another split on another side: look at it again
    }
}

/// `a > b`; `false` if either is NaN (spelled out where a negation keeps NaN out).
pub(crate) fn gt(a: f64, b: f64) -> bool {
    a > b
}

/// Vertices whose surface normal was undefined get the normal of a triangle using them.
fn fill_missing_normals(m: &mut Mesh) {
    let pos = |m: &Mesh, i: u32| {
        let k = 3 * i as usize;
        DVec3::new(m.positions[k] as f64, m.positions[k + 1] as f64, m.positions[k + 2] as f64)
    };
    let missing: Vec<bool> = m.normals.chunks_exact(3).map(|n| n == [0.0, 0.0, 0.0]).collect();
    if !missing.iter().any(|x| *x) {
        return;
    }
    for t in m.indices.clone().chunks_exact(3) {
        let n = (pos(m, t[1]) - pos(m, t[0])).cross(pos(m, t[2]) - pos(m, t[0])).normalize_or_zero();
        for &v in t {
            let k = 3 * v as usize;
            if missing[v as usize] && m.normals[k..k + 3] == [0.0, 0.0, 0.0] {
                m.normals[k..k + 3].copy_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
            }
        }
    }
}
