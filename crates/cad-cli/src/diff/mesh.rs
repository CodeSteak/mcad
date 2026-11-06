//! `diff-reference --mesh` (plan 2.7): the mesh of each file's result (root node, every B-rep
//! part) from solidcore's `tessellate` (`cad-tess` on its B-rep), checked with tolerance `tol`:
//! triangles, watertight (`cad_tess::MeshCheck::watertight`: every edge used twice in opposite
//! directions, no duplicate triangles, facing outwards, finite positions), volume against the
//! exact one (`mass_props`) as `|V − exact| / (tol · area)` (≤ 1 passes), distance to the exact
//! surfaces of the B-rep (sampled per triangle, see `cad_tess::surface_distance`), time of
//! `tessellate`.
//!
//! Verdict `ok` if the mesh is watertight, its volume within `tol · area`, its chord error (the
//! distance to the surface beyond that of the triangles' corners) within [`SURFACE_SLACK`] ·
//! `tol`, and its vertices where the B-rep puts them, within `tol`: inside a face on the face's
//! surface, on a face's boundary on the B-rep's edges of its face (which may lie off the face;
//! loops crossing at a cusp are split on the chords between edge points).

use std::time::Instant;

use cad_kernel::brep::{Body, Brep};
use cad_kernel::{Kernel, Mesh};
use cad_tess::Distance;
use serde::Serialize;

/// The meshes of the result parts.
pub struct Parts {
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
pub fn mesh_parts<K: Kernel>(k: &mut K, shapes: &[(K::Shape, bool)], tol: f64) -> Parts {
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
    Parts { parts }
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

/// The meshes of a file's result.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub parts: usize,
    pub triangles: usize,
    /// Hash of the meshes' positions and indices (hex): the same meshes, bit for bit, have the
    /// same hash (e.g. with any `--jobs`).
    pub hash: String,
    pub watertight: bool,
    pub boundary_edges: usize,
    /// Edges used by more than two triangles (a fin).
    pub nonmanifold_edges: usize,
    pub misoriented_edges: usize,
    /// Triangles with the same corners as another one.
    pub duplicate_triangles: usize,
    /// Closed solid parts whose mesh faces inwards (negative volume).
    pub inside_out: usize,
    /// Positions with a NaN or infinite coordinate.
    pub non_finite: usize,
    pub volume: f64,
    pub exact_volume: Option<f64>,
    /// `|volume − exact| / (tol · area)`, solid parts.
    pub volume_ratio: Option<f64>,
    /// Distance to the exact surfaces of the B-rep.
    pub surface: Option<Dist>,
    /// The part of `surface` the triangles add to their corners' distance (the tessellation's
    /// chord error; the corners' own distance is the B-rep's, see `vertex_gap`).
    pub chord: Option<Dist>,
    /// Largest distance of a mesh vertex from the surface of its face (an edge of the B-rep
    /// off its face, e.g. at the fitted end of a sweep).
    pub vertex_gap: f64,
    /// The same for the vertices inside their face (surface points: about 0).
    pub inner_vertex_gap: f64,
    /// Largest distance of a vertex on a face's boundary from the B-rep's edges (edge points:
    /// about 0, however far the edge lies off the face).
    pub edge_vertex_gap: f64,
    /// `tessellate`, ms (with `brep`).
    pub ms: f64,
    /// `brep` alone, ms.
    pub brep_ms: f64,
    pub error: Option<String>,
}

/// The check of a file's meshes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Check {
    pub tol: f64,
    /// `ok`, `differ` (see the module docs), `failed` (a mesh failed) or `skipped` (no B-rep
    /// parts).
    pub verdict: &'static str,
    #[serde(flatten)]
    pub metrics: Metrics,
    pub notes: Vec<String>,
}

/// The distance to the exact surface may exceed `tol` by this factor (positions are `f32`; the
/// refinement measures at sample points, this check at others).
pub const SURFACE_SLACK: f64 = 1.1;

/// The metrics of the parts.
fn metrics(p: &Parts, tol: f64) -> Metrics {
    let mut m = Metrics { parts: p.parts.len(), watertight: true, ..Default::default() };
    let (mut exact, mut solid_vol, mut solid_area) = (0.0, 0.0, 0.0);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
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
        {
            use std::hash::Hasher;
            for x in &mesh.positions {
                hasher.write_u32(x.to_bits());
            }
            for i in &mesh.indices {
                hasher.write_u32(*i);
            }
        }
        m.non_finite += c.non_finite;
        m.watertight &= c.non_finite == 0;
        m.volume += c.volume;
        let closed = part.brep.as_ref().is_none_or(|b| b.bodies.iter().all(|b| matches!(b, Body::Solid { .. })));
        if part.solid && closed {
            m.boundary_edges += c.boundary_edges;
            m.nonmanifold_edges += c.nonmanifold_edges;
            m.misoriented_edges += c.misoriented_edges;
            m.duplicate_triangles += c.duplicate_triangles;
            m.inside_out += usize::from(c.inside_out);
            m.watertight &= c.watertight();
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
            m.inner_vertex_gap = m.inner_vertex_gap.max(d.inner_vertex_gap);
            m.edge_vertex_gap = m.edge_vertex_gap.max(d.edge_vertex_gap);
            surface = Some(match surface {
                Some((s, c)) => (Distance::max_of(s, d.distance), Distance::max_of(c, d.chord)),
                None => (d.distance, d.chord),
            });
        }
    }
    if have_exact && solid_area > 0.0 {
        m.exact_volume = Some(exact);
        m.volume_ratio = Some((solid_vol - exact).abs() / (tol * solid_area));
    }
    m.hash = format!("{:016x}", std::hash::Hasher::finish(&hasher));
    m.surface = surface.map(|s| s.0.into());
    m.chord = surface.map(|s| s.1.into());
    m
}

/// Checks the meshes (see the module docs).
pub fn check(b: &Parts, tol: f64) -> Check {
    let mb = metrics(b, tol);
    let mut c = Check { tol, verdict: "ok", ..Default::default() };
    if b.parts.is_empty() {
        // no B-rep parts (2D result, a mesh)
        c.verdict = "skipped";
        return c;
    }
    if let Some(e) = &mb.error {
        c.verdict = "failed";
        c.notes.push(e.clone());
    } else {
        if !mb.watertight {
            c.notes.push(format!(
                "not watertight ({} open, {} non-manifold, {} misoriented edges, {} duplicate triangles, {} parts inside out, {} non-finite positions)",
                mb.boundary_edges,
                mb.nonmanifold_edges,
                mb.misoriented_edges,
                mb.duplicate_triangles,
                mb.inside_out,
                mb.non_finite
            ));
        }
        if let Some(r) = mb.volume_ratio.filter(|r| *r > 1.0) {
            c.notes.push(format!("volume off by {:.3} · tol · area", r));
        }
        if let Some(s) = mb.chord.filter(|s| s.max > SURFACE_SLACK * tol) {
            c.notes.push(format!("chord {:.4} mm off the surface at {:?}", s.max, s.at));
        }
        // the vertices against the B-rep: inside a face on its surface, on a face's boundary on
        // its edges (which may lie off the face)
        if mb.inner_vertex_gap > tol {
            c.notes.push(format!("a vertex inside a face {:.4} mm off its surface", mb.inner_vertex_gap));
        }
        if mb.edge_vertex_gap > tol {
            c.notes.push(format!("a vertex on a face's boundary {:.4} mm off the B-rep's edges", mb.edge_vertex_gap));
        }
        if !c.notes.is_empty() {
            c.verdict = "differ";
        }
    }
    c.metrics = mb;
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel_solidcore::SolidcoreKernel;

    /// The meshes of a cylinder and a sphere.
    fn parts(tol: f64) -> Parts {
        let mut k = SolidcoreKernel::new();
        let shapes = vec![(k.make_cylinder(10.0, 5.0, 5.0, 1).unwrap(), true), (k.make_sphere(4.0, 2).unwrap(), true)];
        let shapes: Vec<_> = shapes
            .into_iter()
            .enumerate()
            .map(|(i, (s, solid))| {
                let m = glam::DMat4::from_translation(glam::DVec3::new(20.0 * i as f64, 0.0, 0.0));
                (k.transform(&s, &m).unwrap(), solid)
            })
            .collect();
        mesh_parts(&mut k, &shapes, tol)
    }

    fn moved(mut p: Parts, f: impl Fn(usize, &mut [f32])) -> Parts {
        for part in &mut p.parts {
            if let Ok(m) = &mut part.mesh {
                for (i, v) in m.positions.chunks_exact_mut(3).enumerate() {
                    f(i, v);
                }
            }
        }
        p
    }

    /// A displaced mesh is no longer `ok`: all vertices shifted, shrunk, one vertex moved off
    /// its face or edge, a box corner moved along one of its edges.
    #[test]
    fn displaced_meshes_differ() {
        let tol = 0.05;
        let c = check(&parts(tol), tol);
        assert_eq!(c.verdict, "ok", "{:?}", c.notes);
        // one vertex of the sphere (in the middle of its vertex list) 0.3 mm outwards
        let mut one = parts(tol);
        if let Ok(m) = &mut one.parts[1].mesh {
            let k = 3 * (m.positions.len() / 6);
            let p = glam::Vec3::from_slice(&m.positions[k..k + 3]);
            let q = p + (p - glam::Vec3::new(20.0, 0.0, 0.0)).normalize() * 0.3;
            m.positions[k..k + 3].copy_from_slice(&q.to_array());
        }
        let c = check(&one, tol);
        assert_eq!(c.verdict, "differ", "{:?}", c.notes);
        assert!(c.metrics.vertex_gap > 0.25, "{}", c.metrics.vertex_gap);
        assert!(c.notes.iter().any(|n| n.contains("vertex")), "{:?}", c.notes);
        for (what, f) in [
            ("shift", Box::new(|_: usize, v: &mut [f32]| v[2] += 0.3) as Box<dyn Fn(usize, &mut [f32])>),
            ("shrink", Box::new(|_: usize, v: &mut [f32]| v.iter_mut().for_each(|x| *x *= 0.99))),
            (
                "first",
                Box::new(|i: usize, v: &mut [f32]| {
                    if i == 0 {
                        v[0] += 0.3
                    }
                }),
            ),
        ] {
            let c = check(&moved(parts(tol), f), tol);
            assert_eq!(c.verdict, "differ", "{what}: {:?}", c.notes);
        }
        // a box corner moved along one of its edges: on the B-rep's edges, but off the edges of
        // the faces whose corner it is
        let boxed = || {
            let mut k = SolidcoreKernel::new();
            let b = k.make_box(glam::DVec3::ZERO, glam::DVec3::new(10.0, 8.0, 6.0), 1).unwrap();
            mesh_parts(&mut k, &[(b, true)], tol)
        };
        assert_eq!(check(&boxed(), tol).verdict, "ok");
        let corner = moved(boxed(), |_, v| {
            if v.iter().all(|x| *x == 0.0) {
                v[2] += 0.3;
            }
        });
        let c = check(&corner, tol);
        assert_eq!(c.verdict, "differ", "{:?}", c.notes);
        assert!(c.metrics.edge_vertex_gap > 0.25, "{}", c.metrics.edge_vertex_gap);
    }
}
