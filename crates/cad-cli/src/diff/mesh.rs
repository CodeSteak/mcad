//! `diff-kernels --mesh` (plan 2.7): the meshes of each file's result (root node, every B-rep
//! part) from both kernels' `tessellate` (SMLib: its own tessellator; solidcore: `cad-tess` on
//! its B-rep), compared with tolerance `tol`:
//!
//! * per kernel: triangles, watertight (every edge used twice in opposite directions),
//!   volume against the exact one (`mass_props`) as `|V − exact| / (tol · area)` (≤ 1 passes),
//!   distance to the exact surfaces of the kernel's own B-rep (sampled per triangle, see
//!   `cad_tess::surface_distance`), time of `tessellate`;
//! * solidcore against SMLib: the sampled distance in both directions (`cad_tess::mesh_distance`)
//!   and its maximum (Hausdorff distance).
//!
//! SMLib holds the tolerance only along triangle edges (twisted free-form faces bulge away
//! from their triangles' centres), so the verdict looks at solidcore's distance to the exact
//! surface, not at the distance to SMLib's mesh: `ok` if solidcore's mesh is watertight, its
//! volume within `tol · area` and its chord error (the distance to the surface beyond that of
//! the triangles' corners, which is the B-rep's) within [`SURFACE_SLACK`] · `tol`;
//! the Hausdorff distance to SMLib is expected within both kernels' surface distances plus
//! [`KERNEL_SLACK`] of the size where the root node is `same` (both B-reps describe the same
//! body; a note otherwise).

use std::time::Instant;

use cad_kernel::brep::{Body, Brep};
use cad_kernel::{Kernel, Mesh};
use cad_tess::Distance;
use serde::Serialize;

/// The meshes of one kernel's result parts.
pub struct Parts {
    /// The root node.
    pub root: Option<u32>,
    pub parts: Vec<Part>,
}

pub struct Part {
    /// For the distance to the exact surfaces.
    pub brep: Option<Brep>,
    pub mesh: Result<Mesh, String>,
    /// `tessellate`, ms.
    pub ms: f64,
    /// `brep` alone (part of solidcore's `tessellate`), ms.
    pub brep_ms: f64,
    /// Exact volume (`mass_props`) of a solid part.
    pub exact: Option<f64>,
    pub solid: bool,
}

/// Meshes every part with `tol`: B-rep, `tessellate` (timed), exact volume.
pub fn mesh_parts<K: Kernel>(k: &mut K, root: Option<u32>, shapes: &[(K::Shape, bool)], tol: f64) -> Parts {
    let mut parts = Vec::new();
    for (s, solid) in shapes {
        if k.is_empty(s) {
            continue;
        }
        let t0 = Instant::now();
        let brep = k.brep(s).ok();
        let brep_ms = t0.elapsed().as_secs_f64() * 1e3;
        let t0 = Instant::now();
        let mesh = k.tessellate(s, tol).map_err(|e| e.to_string());
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let exact = if *solid { k.mass_props(s).ok().map(|m| m.volume) } else { None };
        parts.push(Part { brep, mesh, ms, brep_ms, exact, solid: *solid });
    }
    Parts { root, parts }
}

/// Distance in the JSON output.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Dist {
    pub max: f64,
    pub mean: f64,
    /// Where the largest distance was found.
    pub at: [f64; 3],
}

impl From<Distance> for Dist {
    fn from(d: Distance) -> Dist {
        Dist { max: d.max, mean: d.mean, at: d.at.to_array() }
    }
}

/// One kernel's meshes of a file's result.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub parts: usize,
    pub triangles: usize,
    pub watertight: bool,
    pub boundary_edges: usize,
    pub misoriented_edges: usize,
    pub volume: f64,
    pub exact_volume: Option<f64>,
    /// `|volume − exact| / (tol · area)`, solid parts.
    pub volume_ratio: Option<f64>,
    /// Distance to the exact surfaces of the kernel's B-rep.
    pub surface: Option<Dist>,
    /// The part of `surface` the triangles add to their corners' distance (the tessellation's
    /// chord error; the corners' own distance is the B-rep's, see `vertex_gap`).
    pub chord: Option<Dist>,
    /// Largest distance of a mesh vertex from the surface of its face (an edge of the B-rep
    /// off its face, e.g. at the fitted end of a sweep).
    pub vertex_gap: f64,
    /// `tessellate`, ms (solidcore: with `brep`).
    pub ms: f64,
    /// `brep` alone, ms.
    pub brep_ms: f64,
    pub error: Option<String>,
}

/// solidcore's meshes against SMLib's.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Comparison {
    pub tol: f64,
    /// `ok`, `differ` (see the module docs), `failed` (a mesh failed) or `skipped` (no B-rep
    /// parts).
    pub verdict: &'static str,
    pub smlib: Metrics,
    pub solidcore: Metrics,
    /// From solidcore's mesh to SMLib's, from SMLib's to solidcore's, the larger.
    pub to_smlib: Option<Dist>,
    pub from_smlib: Option<Dist>,
    pub hausdorff: Option<f64>,
    pub notes: Vec<String>,
}

/// solidcore's distance to the exact surface may exceed `tol` by this factor (positions are
/// `f32`; the refinement measures at sample points, this check at others).
pub const SURFACE_SLACK: f64 = 1.1;
/// Hausdorff distance to SMLib beyond both surface distances, relative to the model size: the
/// kernels fit blends and sweeps a little differently.
pub const KERNEL_SLACK: f64 = 1e-4;

fn metrics(p: &Parts, tol: f64) -> (Metrics, Mesh) {
    let mut m = Metrics { parts: p.parts.len(), watertight: true, ..Default::default() };
    let mut all = Mesh::default();
    let (mut exact, mut solid_vol, mut solid_area) = (0.0, 0.0, 0.0);
    let mut have_exact = true;
    let mut surface: Option<(Distance, Distance)> = None;
    for part in &p.parts {
        m.ms += part.ms;
        m.brep_ms += part.brep_ms;
        let mesh = match &part.mesh {
            Ok(x) => x,
            Err(e) => {
                m.error.get_or_insert(e.clone());
                continue;
            }
        };
        let c = cad_tess::check(mesh);
        m.triangles += c.triangles;
        m.volume += c.volume;
        let closed = part.brep.as_ref().is_none_or(|b| b.bodies.iter().all(|b| matches!(b, Body::Solid { .. })));
        if part.solid && closed {
            m.boundary_edges += c.boundary_edges;
            m.misoriented_edges += c.misoriented_edges;
            m.watertight &= c.boundary_edges + c.misoriented_edges == 0;
            match part.exact {
                Some(e) => {
                    exact += e;
                    solid_vol += c.volume;
                    solid_area += c.area;
                }
                None => have_exact = false,
            }
        }
        if let Some(b) = &part.brep {
            let d = cad_tess::surface_distance(b, mesh);
            m.vertex_gap = m.vertex_gap.max(d.vertex_gap);
            surface = Some(match surface {
                Some((s, c)) => (Distance::max_of(s, d.distance), Distance::max_of(c, d.chord)),
                None => (d.distance, d.chord),
            });
        }
        append(&mut all, mesh);
    }
    if have_exact && solid_area > 0.0 {
        m.exact_volume = Some(exact);
        m.volume_ratio = Some((solid_vol - exact).abs() / (tol * solid_area));
    }
    m.surface = surface.map(|s| s.0.into());
    m.chord = surface.map(|s| s.1.into());
    (m, all)
}

fn append(all: &mut Mesh, m: &Mesh) {
    let base = (all.positions.len() / 3) as u32;
    all.positions.extend_from_slice(&m.positions);
    all.indices.extend(m.indices.iter().map(|i| i + base));
}

/// Compares the meshes of both kernels (see the module docs). `same`: the root node's verdict
/// is `same` (else the bodies differ and so may the meshes: no note on the Hausdorff distance).
pub fn compare(a: &Parts, b: &Parts, tol: f64, same: bool) -> Comparison {
    let (ma, mesh_a) = metrics(a, tol);
    let (mb, mesh_b) = metrics(b, tol);
    let mut c = Comparison { tol, verdict: "ok", ..Default::default() };
    if a.parts.is_empty() && b.parts.is_empty() {
        // no B-rep parts (2D result, a mesh)
        c.verdict = "skipped";
        return c;
    }
    if let Some(e) = &mb.error {
        c.verdict = "failed";
        c.notes.push(format!("solidcore: {e}"));
    }
    if ma.error.is_some() {
        c.notes.push(format!("SMLib: {}", ma.error.clone().unwrap_or_default()));
    }
    if c.verdict == "ok" {
        if !mb.watertight {
            c.notes.push(format!(
                "solidcore: not watertight ({} open, {} misoriented edges)",
                mb.boundary_edges, mb.misoriented_edges
            ));
        }
        if let Some(r) = mb.volume_ratio.filter(|r| *r > 1.0) {
            c.notes.push(format!("solidcore: volume off by {:.3} · tol · area", r));
        }
        if let Some(s) = mb.chord.filter(|s| s.max > SURFACE_SLACK * tol) {
            c.notes.push(format!("solidcore: chord {:.4} mm off the surface at {:?}", s.max, s.at));
        }
        if !c.notes.is_empty() {
            c.verdict = "differ";
        }
    }
    if !mesh_a.indices.is_empty() && !mesh_b.indices.is_empty() {
        let to = cad_tess::mesh_distance(&mesh_b, &mesh_a);
        let from = cad_tess::mesh_distance(&mesh_a, &mesh_b);
        let h = to.max.max(from.max);
        let size = mesh_a.bounds().map_or(0.0, |(lo, hi)| (hi - lo).length());
        let lim = ma.surface.map_or(tol, |s| s.max.max(tol)) + mb.surface.map_or(tol, |s| s.max.max(tol));
        if same && h > lim + KERNEL_SLACK * size {
            let w = if to.max >= from.max { to } else { from };
            c.notes.push(format!("{h:.4} mm from SMLib's mesh at {:?} (expected ≤ {:.4})", w.at.to_array(), lim));
        }
        (c.to_smlib, c.from_smlib, c.hausdorff) = (Some(to.into()), Some(from.into()), Some(h));
    }
    c.smlib = ma;
    c.solidcore = mb;
    c
}
