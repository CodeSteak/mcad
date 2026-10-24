//! A measurable body: the neutral B-rep of one shape in context coordinates, its exact edge
//! curves, and the triangles of each face (tessellation, used to decide whether a point lies
//! on a trimmed face and for coarse searches).

use std::collections::HashMap;
use std::rc::Rc;

use cad_kernel::Mesh;
use cad_kernel::brep::{Brep, Loop, Surface};
use glam::{DMat4, DVec3};

use crate::curve::EdgeCurve;
use crate::dist::{Tri, closest_on_tri};
use crate::surface;

/// One face of a [`Body`].
#[derive(Debug, Clone)]
pub struct FaceGeom {
    /// Face index of the kernel (`Mesh::tri_faces`, `brep::Face::id`).
    pub id: u32,
    /// Provenance tag (engine tag).
    pub tag: u32,
    pub surface: Surface,
    pub same_sense: bool,
    pub tris: Vec<Tri>,
    /// Loops as (edge index, forward).
    pub loops: Vec<Vec<(u32, bool)>>,
    /// Boundary edges (distinct).
    pub edges: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Body {
    pub brep: Rc<Brep>,
    pub edges: Vec<EdgeCurve>,
    /// Polyline per edge (display, coarse searches).
    pub polys: Vec<Vec<DVec3>>,
    pub faces: Vec<FaceGeom>,
    /// Faces (indices into `faces`) using each edge.
    pub edge_faces: Vec<Vec<usize>>,
    /// Chordal tolerance of the triangles in mm.
    pub tol: f64,
    /// Closed solid (else an open surface).
    pub solid: bool,
    /// Diagonal of the bounds (relative tolerances).
    pub size: f64,
}

impl Body {
    /// `brep` in context coordinates; `mesh` (tessellation of the same shape with tolerance
    /// `tol`, face ids as in the B-rep) in the shape's own system, placed with `m`.
    pub fn new(brep: Rc<Brep>, mesh: &Mesh, m: &DMat4, tol: f64, solid: bool) -> Body {
        let edges: Vec<EdgeCurve> = brep
            .edges
            .iter()
            .map(|e| EdgeCurve::from_edge(&e.curve, brep.vertices[e.start as usize], brep.vertices[e.end as usize]))
            .collect();
        let polys = edges.iter().map(|c| c.polyline(48)).collect();
        let mut by_id: HashMap<u32, Vec<Tri>> = HashMap::new();
        let v = |i: u32| {
            let k = i as usize * 3;
            m.transform_point3(DVec3::new(
                mesh.positions[k] as f64,
                mesh.positions[k + 1] as f64,
                mesh.positions[k + 2] as f64,
            ))
        };
        for (t, f) in mesh.indices.chunks_exact(3).zip(mesh.tri_faces.iter()) {
            by_id.entry(*f).or_default().push([v(t[0]), v(t[1]), v(t[2])]);
        }
        let mut faces = Vec::new();
        for f in brep.faces() {
            let mut loops = Vec::new();
            let mut fe: Vec<u32> = Vec::new();
            for l in &f.loops {
                if let Loop::Edges(es) = l {
                    loops.push(es.iter().map(|oe| (oe.edge, oe.forward)).collect::<Vec<_>>());
                    for oe in es {
                        if !fe.contains(&oe.edge) {
                            fe.push(oe.edge);
                        }
                    }
                }
            }
            faces.push(FaceGeom {
                id: f.id,
                tag: f.tag,
                surface: f.surface.clone(),
                same_sense: f.same_sense,
                tris: by_id.remove(&f.id).unwrap_or_default(),
                loops,
                edges: fe,
            });
        }
        let mut edge_faces = vec![Vec::new(); edges.len()];
        for (i, f) in faces.iter().enumerate() {
            for &e in &f.edges {
                if let Some(v) = edge_faces.get_mut(e as usize) {
                    v.push(i);
                }
            }
        }
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for p in brep.vertices.iter().chain(faces.iter().flat_map(|f| f.tris.iter().flatten())) {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let size = if lo.x.is_finite() { (hi - lo).length().max(1e-9) } else { 1.0 };
        Body { brep, edges, polys, faces, edge_faces, tol: tol.max(1e-6), solid, size }
    }

    /// Index into `faces` of the kernel face id.
    pub fn face_by_id(&self, id: u32) -> Option<usize> {
        self.faces.iter().position(|f| f.id == id)
    }

    /// Closest point of the face's triangles to `p`: (distance, point).
    pub fn mesh_closest(&self, f: usize, p: DVec3) -> (f64, DVec3) {
        let mut best = (f64::INFINITY, p);
        for t in &self.faces[f].tris {
            let q = closest_on_tri(p, t);
            let d = q.distance(p);
            if d < best.0 {
                best = (d, q);
            }
        }
        best
    }

    /// `p` (on the face's surface) lies on the trimmed face: within `k` chordal tolerances
    /// of its triangles.
    pub fn on_face(&self, f: usize, p: DVec3, k: f64) -> bool {
        self.mesh_closest(f, p).0 <= k * self.tol + 1e-9 * self.size
    }

    /// Closest point of face `f` to `p` and whether it was found in closed form (analytic
    /// surface or boundary curve).
    pub fn face_closest(&self, f: usize, p: DVec3) -> (DVec3, bool) {
        let fg = &self.faces[f];
        let analytic = surface::is_analytic(&fg.surface);
        if let Some(q) = surface::project(&fg.surface, p)
            && self.on_face(f, q, 2.0)
        {
            return (q, analytic);
        }
        // outside the trimmed face: closest point on its boundary
        let mut best: Option<(f64, DVec3, bool)> = None;
        for &e in &fg.edges {
            let c = &self.edges[e as usize];
            let (_, q) = c.closest(p);
            let d = q.distance(p);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, q, c.is_analytic()));
            }
        }
        match best {
            Some((_, q, a)) => (q, a && analytic),
            None => (self.mesh_closest(f, p).1, false),
        }
    }

    /// Outward unit normal of face `f` at `p`.
    pub fn face_normal(&self, f: usize, p: DVec3) -> Option<DVec3> {
        let fg = &self.faces[f];
        surface::normal(&fg.surface, fg.same_sense, p)
    }

    /// Faces adjacent to an edge.
    pub fn faces_of_edge(&self, e: u32) -> &[usize] {
        self.edge_faces.get(e as usize).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Tags of the faces at an edge.
    pub fn edge_tags(&self, e: u32) -> Vec<u32> {
        let mut t: Vec<u32> = self.faces_of_edge(e).iter().map(|&f| self.faces[f].tag).collect();
        t.dedup();
        t
    }

    /// Edges ending at vertex index `v`.
    pub fn vertex_edges(&self, v: u32) -> Vec<u32> {
        self.brep.edges.iter().enumerate().filter(|(_, e)| e.start == v || e.end == v).map(|(i, _)| i as u32).collect()
    }

    /// Axis-aligned bounds: vertices, exact edge extremes, triangle vertices (on the
    /// surfaces), and interior extremes of spheres and tori where they lie on the face.
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        let mut add = |p: DVec3| {
            lo = lo.min(p);
            hi = hi.max(p);
        };
        for p in &self.brep.vertices {
            add(*p);
        }
        for c in &self.edges {
            let (a, b) = c.bounds();
            add(a);
            add(b);
        }
        for (i, f) in self.faces.iter().enumerate() {
            for t in &f.tris {
                for p in t {
                    add(*p);
                }
            }
            let cands: Vec<DVec3> = match &f.surface {
                Surface::Sphere { frame, r } => [DVec3::X, DVec3::Y, DVec3::Z]
                    .iter()
                    .flat_map(|a| [frame.origin + *a * *r, frame.origin - *a * *r])
                    .collect(),
                Surface::Torus { frame, major, minor } => {
                    let mut v = Vec::new();
                    for a in [DVec3::X, DVec3::Y, DVec3::Z] {
                        // extreme along a: on the tube circle in the direction of a's radial part
                        let ra = a - frame.z * a.dot(frame.z);
                        if let Some(e) = ra.try_normalize() {
                            for s in [1.0, -1.0] {
                                let c = frame.origin + e * (s * major);
                                v.push(c + a * *minor);
                                v.push(c - a * *minor);
                            }
                        } else {
                            v.push(frame.origin + frame.z * *minor);
                            v.push(frame.origin - frame.z * *minor);
                        }
                    }
                    v
                }
                _ => Vec::new(),
            };
            for q in cands {
                if self.on_face(i, q, 1.5) {
                    add(q);
                }
            }
        }
        lo.x.is_finite().then_some((lo, hi))
    }
}
