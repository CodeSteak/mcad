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
//! [`check`] measures a mesh (watertightness, orientation, degenerate triangles, volume),
//! [`surface_distance`] and [`mesh_distance`] its distance to the exact surfaces and to another
//! mesh (plan 2.7).
//!
//! ```
//! # use cad_kernel::brep::Brep;
//! # fn kernel_brep() -> Brep { Brep::default() }
//! let brep: Brep = kernel_brep();
//! let mesh = cad_tess::tessellate(&brep, 0.05).unwrap();
//! let c = cad_tess::check(&mesh);
//! assert!(c.watertight() || mesh.is_empty()); // an empty B-rep gives an empty mesh
//! ```

use std::collections::HashMap;

use cad_kernel::brep::Brep;
use cad_kernel::{FaceId, KernelError, Mesh};
use glam::DVec3;

mod bspline;
mod check;
mod compare;
mod edges;
mod face;
mod surface;
mod uv;

pub use check::{MeshCheck, check};
pub use compare::{Distance, SurfaceDistance, local_allowance, mesh_distance, mesh_excess, surface_distance};

/// Settings of a tessellation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Largest distance between the mesh and the surface (chord height), mm.
    pub tol: f64,
    /// Largest turn of an edge's tangent between two of its points and largest angle step on
    /// curved surfaces, radians (as SMLib's 20°).
    pub angle: f64,
    /// Refinement passes per face (each measures only what the last one changed).
    pub max_passes: usize,
    /// Most triangles of one face and of the whole mesh: a face that would need more (from
    /// its grid, or during the refinement) fails with "tolerance too fine" instead of running
    /// out of time and memory. A guard, not a quality limit: the default of [`MAX_TRIANGLES`]
    /// for both is a binary STL of 100 MB, far beyond what a realistic part needs at 0.01 mm
    /// (a sphere of radius 300: 300 000).
    pub max_face_triangles: usize,
    pub max_triangles: usize,
}

/// Default of [`Options::max_triangles`] and [`Options::max_face_triangles`].
pub const MAX_TRIANGLES: usize = 2_000_000;

/// The tolerance is at least this fraction of the model size: finer is below the resolution of
/// the mesh's `f32` positions (about 6e-8 of a coordinate) and would only cost time.
pub const MIN_REL_TOL: f64 = 1e-6;

/// A face whose deviation after the refinement is above this many times the tolerance makes
/// [`tessellate`] fail; up to it the mesh is kept and the face listed in [`Stats::unconverged`].
pub const MAX_MISS: f64 = 2.0;

impl Options {
    pub fn new(tol: f64) -> Options {
        Options {
            tol,
            angle: 20f64.to_radians(),
            max_passes: 40,
            max_face_triangles: MAX_TRIANGLES,
            max_triangles: MAX_TRIANGLES,
        }
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
    /// Faces without triangles and why. (A tolerance that needs more triangles than
    /// [`Options::max_face_triangles`] or [`Options::max_triangles`] allow is an error.)
    pub failed_faces: Vec<(FaceId, String)>,
    /// Largest deviation from the surface measured on the final mesh of a curved face (planar
    /// faces: 0): at each triangle's centroid, at three inner points of a triangle whose centroid
    /// is off by more than a third of the tolerance, and at each inner edge's midpoint
    /// (boundary edges are the edges' polylines, within the tolerance of their curves).
    pub max_deviation: f64,
    /// Faces whose refinement ended above the tolerance (passes used up), with their
    /// deviation; [`tessellate`] fails if one is above [`MAX_MISS`] times the tolerance.
    pub unconverged: Vec<(FaceId, f64)>,
    /// The tolerance used: the requested one, at least [`MIN_REL_TOL`] of the model size.
    pub tol: f64,
    /// Faces with triangles that face against their surface (folds) after the repair, and how
    /// many; [`tessellate`] fails on any.
    pub folded: Vec<(FaceId, usize)>,
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
    t.strict()?;
    Ok(t.mesh)
}

impl Tessellation {
    /// An error for the first failed face, for a face whose deviation stayed above
    /// [`MAX_MISS`] times the tolerance, or for a face with folded triangles.
    pub fn strict(&self) -> Result<(), TessError> {
        if let Some((id, why)) = self.stats.failed_faces.first() {
            let n = self.stats.failed_faces.len();
            let more = if n > 1 { format!(" ({} more faces)", n - 1) } else { String::new() };
            return Err(TessError(format!("face {id}: {why}{more}")));
        }
        let t = self.stats.tol;
        if let Some((id, d)) = self.stats.unconverged.iter().find(|(_, d)| *d > MAX_MISS * t) {
            return Err(TessError(format!(
                "face {id}: tolerance {} mm not reached ({d:.3e} mm after the refinement)",
                mm(t)
            )));
        }
        if let Some((id, n)) = self.stats.folded.first() {
            return Err(TessError(format!(
                "face {id}: {n} triangles facing against the surface at tolerance {} mm",
                mm(t)
            )));
        }
        Ok(())
    }
}

/// The mesh of `brep` with `opts`; faces that fail are left out and listed in the stats.
pub fn tessellate_with(brep: &Brep, opts: &Options) -> Result<Tessellation, TessError> {
    if !(opts.tol > 0.0 && opts.tol.is_finite()) {
        return Err(TessError("tolerance must be > 0".into()));
    }
    brep.validate().map_err(TessError)?;
    // degrees the evaluation handles (the kernels write 7 at most)
    let too_high = |d: u32| d as usize >= bspline::MAXP;
    let high = format!("degree above {}", bspline::MAXP - 1);
    for (i, e) in brep.edges.iter().enumerate() {
        if let cad_kernel::brep::Curve::BSpline(c) = &e.curve
            && too_high(c.degree)
        {
            return Err(TessError(format!("edge {i}: {high}")));
        }
    }
    for f in brep.faces() {
        use cad_kernel::brep::Surface as S;
        let d = match &f.surface {
            S::BSpline(s) => s.degree_u.max(s.degree_v),
            S::Revolution { curve, .. } | S::Extrusion { curve, .. } => curve.degree,
            _ => 0,
        };
        if too_high(d) {
            return Err(TessError(format!("face {}: {high}", f.id)));
        }
    }
    let size = model_size(brep);
    let tol = opts.tol.max(MIN_REL_TOL * size);
    let angle = opts.angle.clamp(1f64.to_radians(), 60f64.to_radians());
    let ep = edges::discretise(brep, tol, angle);
    let eps = (1e-7 * size).max(1e-9);
    let mut params = face::Params { tol, angle, max_passes: opts.max_passes, max_triangles: opts.max_face_triangles };
    let mut mesh = Mesh::default();
    let mut stats = Stats { tol, ..Default::default() };
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
        // what is left of the whole mesh's budget
        let left = opts.max_triangles.saturating_sub(mesh.indices.len() / 3);
        params.max_triangles = opts.max_face_triangles.min(left);
        let fm = face::mesh_face(&g, loops, &ep.points, &params, orient, &mut next_id);
        splits.extend_from_slice(&fm.splits);
        stats.refined += fm.refined;
        stats.lost_constraints += fm.lost_constraints;
        stats.max_deviation = stats.max_deviation.max(fm.deviation);
        if let Some(n) = fm.too_fine {
            // no use meshing the other faces
            let limit = if left < opts.max_face_triangles {
                format!("at most {left} left of {} in all", opts.max_triangles)
            } else {
                format!("at most {} per face", opts.max_face_triangles)
            };
            return Err(TessError(format!(
                "face {}: tolerance {} mm too fine: more than {n} triangles ({limit})",
                f.id,
                mm(tol)
            )));
        }
        if fm.deviation > tol {
            stats.unconverged.push((f.id, fm.deviation));
        }
        if fm.folds > 0 {
            stats.folded.push((f.id, fm.folds));
        }
        if fm.tris.is_empty() {
            stats.failed_faces.push((f.id, "no triangles".into()));
            continue;
        }
        // vertices: a boundary point once per face and normal (seam copies meet, unless the
        // surface has a crease there), the others as they are
        let mut by_id: HashMap<u32, Vec<(u32, DVec3)>> = HashMap::new();
        let mut index = Vec::with_capacity(fm.verts.len());
        for v in &fm.verts {
            let shared = v.id.filter(|_| !v.pole);
            let n = g.normal(v.uv, fm.bounds).map_or(DVec3::ZERO, |n| n * orient);
            let same = shared.and_then(|id| by_id.get(&id)).and_then(|c| c.iter().find(|(_, m)| m.dot(n) > 0.9));
            if let Some((i, _)) = same {
                index.push(*i);
                continue;
            }
            let i = (mesh.positions.len() / 3) as u32;
            if let Some(id) = shared {
                by_id.entry(id).or_default().push((i, n));
            }
            index.push(i);
            vid.push(shared);
            mesh.positions.extend_from_slice(&[v.p.x as f32, v.p.y as f32, v.p.z as f32]);
            mesh.normals.extend_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
        }
        for t in &fm.tris {
            let [a, b, c] = t.map(|k| fm.verts[k as usize]);
            // collapsed at a pole, two copies of one boundary point, or two points that are
            // one at the `f32` resolution of the output (a crossing of loops split right at a
            // boundary point): welded by position, such a triangle would use its other edge
            // twice (a non-manifold edge), and its neighbours close the surface without it
            let [pa, pb, pc] = [a.p, b.p, c.p].map(|p| p.as_vec3());
            if pa == pb || pb == pc || pc == pa {
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
    drop_fins(&mut mesh);
    fill_missing_normals(&mut mesh);
    turn_normals_against(&mut mesh);
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

/// Removes triangles that coincide with another one at the `f32` positions of the output: a
/// pair facing each other is a fin (two faces each meshing the sliver between loops that cross
/// at a cusp, where both faces end in one line: a crescent whose arcs touch), and taking both
/// away leaves every edge with as many uses in each direction as before; a copy facing the
/// same way goes alone.
fn drop_fins(m: &mut Mesh) {
    let n = m.indices.len() / 3;
    let key = |m: &Mesh, i: u32| {
        let k = 3 * i as usize;
        [m.positions[k].to_bits(), m.positions[k + 1].to_bits(), m.positions[k + 2].to_bits()]
    };
    // corners by position, rotated to start at the smallest: equal for the same orientation
    let mut by: HashMap<[[u32; 3]; 3], Vec<usize>> = HashMap::new();
    for t in 0..n {
        let c = [0, 1, 2].map(|k| key(m, m.indices[3 * t + k]));
        let r = (0..3).min_by_key(|k| c[*k]).unwrap_or(0);
        by.entry([c[r], c[(r + 1) % 3], c[(r + 2) % 3]]).or_default().push(t);
    }
    let mut drop = vec![false; n];
    let empty = Vec::new();
    for (k, ts) in &by {
        let rev = [k[0], k[2], k[1]];
        let os = by.get(&rev).unwrap_or(&empty);
        if os.len() > ts.len() || (os.len() == ts.len() && rev < *k) {
            continue; // the pair is handled from the other side
        }
        // pairs facing each other, then the copies facing the same way beyond the first
        for t in os.iter().chain(&ts[..os.len()]).chain(ts.iter().skip(os.len() + 1)) {
            drop[*t] = true;
        }
    }
    if !drop.iter().any(|d| *d) {
        return;
    }
    let keep: Vec<usize> = (0..n).filter(|t| !drop[*t]).collect();
    m.indices = keep.iter().flat_map(|t| m.indices[3 * t..3 * t + 3].to_vec()).collect();
    m.tri_tags = keep.iter().map(|t| m.tri_tags[*t]).collect();
    m.tri_faces = keep.iter().map(|t| m.tri_faces[*t]).collect();
}

/// Diagonal of the box of the vertices and of a few points per edge (1 for an empty B-rep).
fn model_size(brep: &Brep) -> f64 {
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    let mut add = |p: DVec3| {
        if p.is_finite() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    };
    brep.vertices.iter().for_each(|p| add(*p));
    for e in &brep.edges {
        let (a, b) = (brep.vertices[e.start as usize], brep.vertices[e.end as usize]);
        cad_kernel::brep::sample_edge(&e.curve, a, b, 9).into_iter().for_each(&mut add);
    }
    let d = if lo.x.is_finite() { (hi - lo).length() } else { 0.0 };
    if d > 0.0 { d } else { 1.0 }
}

/// A length in mm for messages: four significant digits (the tolerance raised to
/// [`MIN_REL_TOL`] of the model size has many).
fn mm(x: f64) -> String {
    if !(x.is_finite() && x != 0.0) {
        return format!("{x}");
    }
    let e = x.abs().log10().floor() as i32;
    let s = 10f64.powi(3 - e);
    format!("{}", (x * s).round() / s)
}

/// `a > b`; `false` if either is NaN (spelled out where a negation keeps NaN out).
pub(crate) fn gt(a: f64, b: f64) -> bool {
    a > b
}

/// A vertex normal that points against the mean of its triangles (area-weighted) becomes that
/// mean: at a collapsed corner of a B-spline (a variable fillet's end) the surface's limit
/// normal depends on the direction of approach and can face away from the triangles there.
fn turn_normals_against(m: &mut Mesh) {
    let pos = |m: &Mesh, i: u32| {
        let k = 3 * i as usize;
        DVec3::new(m.positions[k] as f64, m.positions[k + 1] as f64, m.positions[k + 2] as f64)
    };
    let mut sum = vec![DVec3::ZERO; m.normals.len() / 3];
    for t in m.indices.chunks_exact(3) {
        let n = (pos(m, t[1]) - pos(m, t[0])).cross(pos(m, t[2]) - pos(m, t[0]));
        for &v in t {
            sum[v as usize] += n;
        }
    }
    for (v, s) in sum.iter().enumerate() {
        let k = 3 * v;
        let n = DVec3::new(m.normals[k] as f64, m.normals[k + 1] as f64, m.normals[k + 2] as f64);
        if let Some(s) = s.try_normalize()
            && n.dot(s) < 0.0
        {
            m.normals[k..k + 3].copy_from_slice(&[s.x as f32, s.y as f32, s.z as f32]);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use cad_kernel::brep::*;

    /// A half disc bounded by a line and a B-spline edge of the given degree.
    fn half_disc(deg: u32) -> Brep {
        let (a, b) = (DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0));
        let points: Vec<DVec3> = (0..=deg)
            .map(|i| {
                let t = i as f64 / deg as f64;
                DVec3::new(10.0 - 10.0 * t, 5.0 * (std::f64::consts::PI * t).sin(), 0.0)
            })
            .collect();
        let mut knots = vec![0.0; deg as usize + 1];
        knots.extend(vec![1.0; deg as usize + 1]);
        let c = BSplineCurve { degree: deg, knots, points, weights: None };
        let frame = Frame::new(DVec3::ZERO, DVec3::Z, DVec3::X).unwrap();
        let oe = |edge| OrientedEdge { edge, forward: true };
        let face = Face {
            id: 0,
            tag: 1,
            surface: Surface::Plane { frame },
            same_sense: true,
            loops: vec![Loop::Edges(vec![oe(0), oe(1)])],
            approximated: false,
        };
        Brep {
            vertices: vec![a, b],
            edges: vec![
                Edge { start: 0, end: 1, curve: Curve::Line { origin: a, dir: DVec3::X }, approximated: false },
                Edge { start: 1, end: 0, curve: Curve::BSpline(c), approximated: false },
            ],
            bodies: vec![Body::Sheet { shells: vec![Shell { faces: vec![face] }] }],
        }
    }

    /// A curve degree beyond the evaluation is an error, not a panic; tolerances below the
    /// model's resolution are raised to it; one that needs too many triangles is an error.
    #[test]
    fn limits() {
        assert!(tessellate(&half_disc(5), 0.05).is_ok_and(|m| m.triangle_count() > 0));
        let e = tessellate(&half_disc(10), 0.05).unwrap_err();
        assert!(e.0.contains("degree above"), "{e}");
        let t = tessellate_with(&half_disc(5), &Options::new(1e-300)).unwrap();
        assert!(t.stats.tol >= MIN_REL_TOL * 10.0, "{:?}", t.stats);
        let few = Options { max_face_triangles: 50, ..Options::new(1e-3) };
        let e = tessellate_with(&half_disc(5), &few).unwrap_err();
        assert!(e.0.contains("too fine") && e.0.contains("at most 50 per face"), "{e}");
        let few = Options { max_triangles: 40, ..Options::new(1e-3) };
        let e = tessellate_with(&half_disc(5), &few).unwrap_err();
        assert!(e.0.contains("at most 40 left of 40 in all"), "{e}");
    }
}
