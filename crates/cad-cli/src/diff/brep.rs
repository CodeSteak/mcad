//! The neutral B-reps of both kernels (`Kernel::brep`, plan 1.11), compared per node:
//! [`Summary`] counts what one kernel's B-rep holds and checks its conventions
//! ([`Brep::convention_problems`]); [`compare`] matches the B-reps of SMLib and solidcore.
//!
//! * counts: vertices, loops, vertex loops, solids, voids, sheets, faces per surface kind and
//!   edges per curve kind (`kind()` of the neutral types) must agree;
//! * edges match one to one by curve kind and the point at half their length (within 1e-6 of
//!   the model size; the point by arc length, so another parameterisation of the same curve
//!   matches);
//! * faces match one to one by surface kind and the half-length points of their loop edges
//!   (a face with the same tag first); at each such point the face normals (normal of the
//!   surface, flipped where not `same_sense`) must point the same way (orientation; B-spline
//!   surfaces have no normal without (u, v) and are compared by position only), and the tags
//!   must agree.

use std::collections::BTreeMap;

use cad_kernel::brep::{Brep, Loop, Surface};
use glam::DVec3;
use serde::Serialize;

/// Counts and conventions of one kernel's B-rep of a node (all parts together).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub vertices: usize,
    pub loops: usize,
    pub vertex_loops: usize,
    pub solids: usize,
    pub voids: usize,
    pub sheets: usize,
    pub surfaces: BTreeMap<&'static str, usize>,
    pub curves: BTreeMap<&'static str, usize>,
    pub approximated_faces: usize,
    pub approximated_edges: usize,
    /// Conventions not met ([`Brep::convention_problems`]).
    pub problems: Vec<String>,
    /// `Kernel::brep` failed.
    pub error: Option<String>,
    #[serde(skip)]
    pub unsupported: bool,
    #[serde(skip)]
    shape: Shape,
}

/// A face as [`compare`] matches it: surface kind, tag, points with the oriented normal there.
type FaceKey = (&'static str, u32, Vec<(DVec3, Option<DVec3>)>);

/// What [`compare`] matches: per edge (kind, point at half length), per face (kind, tag,
/// points with the oriented normal there).
#[derive(Debug, Clone, Default)]
struct Shape {
    size: f64,
    edges: Vec<(&'static str, DVec3)>,
    faces: Vec<FaceKey>,
}

/// The point at half the length of a polyline.
fn half_length(p: &[DVec3]) -> DVec3 {
    let total: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut s = 0.0;
    for w in p.windows(2) {
        let l = w[0].distance(w[1]);
        if s + l >= 0.5 * total && l > 0.0 {
            return w[0].lerp(w[1], (0.5 * total - s) / l);
        }
        s += l;
    }
    p[0]
}

impl Summary {
    pub fn add(&mut self, b: &Brep) {
        let (lo, hi) = b
            .vertices
            .iter()
            .fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(a, c), p| (a.min(*p), c.max(*p)));
        let size = if b.vertices.is_empty() { 1.0 } else { (hi - lo).length().max(1.0) };
        self.shape.size = self.shape.size.max(size);
        self.vertices += b.vertices.len();
        for body in &b.bodies {
            match body {
                cad_kernel::brep::Body::Solid { voids, .. } => {
                    self.solids += 1;
                    self.voids += voids.len();
                }
                cad_kernel::brep::Body::Sheet { shells } => self.sheets += shells.len(),
            }
        }
        for p in b.convention_problems(1e-6 * size) {
            if self.problems.len() < 8 {
                self.problems.push(p);
            }
        }
        let mid: Vec<DVec3> = (0..b.edges.len())
            .map(|i| half_length(&b.edge_points(cad_kernel::brep::OrientedEdge { edge: i as u32, forward: true }, 64)))
            .collect();
        for (e, m) in b.edges.iter().zip(&mid) {
            *self.curves.entry(e.curve.kind()).or_insert(0) += 1;
            self.approximated_edges += usize::from(e.approximated);
            self.shape.edges.push((e.curve.kind(), *m));
        }
        for f in b.faces() {
            *self.surfaces.entry(f.surface.kind()).or_insert(0) += 1;
            self.approximated_faces += usize::from(f.approximated);
            let mut pts = Vec::new();
            for l in &f.loops {
                self.loops += 1;
                match l {
                    Loop::Vertex(_) => self.vertex_loops += 1,
                    Loop::Edges(es) => {
                        for oe in es {
                            let p = mid[oe.edge as usize];
                            let n = match f.surface {
                                Surface::BSpline(_) => None,
                                _ => f.surface.normal_at(p, None).map(|n| if f.same_sense { n } else { -n }),
                            };
                            pts.push((p, n));
                        }
                    }
                }
            }
            self.shape.faces.push((f.surface.kind(), f.tag, pts));
        }
    }
}

/// Differences between the B-reps of SMLib (`a`) and solidcore (`b`); empty if they agree.
pub fn compare(a: &Summary, b: &Summary) -> Vec<String> {
    let mut notes = Vec::new();
    let count = |what: &str, x: usize, y: usize, notes: &mut Vec<String>| {
        if x != y {
            notes.push(format!("{what} {x} vs {y}"));
        }
    };
    count("vertices", a.vertices, b.vertices, &mut notes);
    count("loops", a.loops, b.loops, &mut notes);
    count("vertex loops", a.vertex_loops, b.vertex_loops, &mut notes);
    count("solids", a.solids, b.solids, &mut notes);
    count("voids", a.voids, b.voids, &mut notes);
    count("sheets", a.sheets, b.sheets, &mut notes);
    if a.surfaces != b.surfaces {
        notes.push(format!("surfaces {:?} vs {:?}", a.surfaces, b.surfaces));
    }
    if a.curves != b.curves {
        notes.push(format!("curves {:?} vs {:?}", a.curves, b.curves));
    }
    if !b.problems.is_empty() {
        notes.push(format!("solidcore: {}", b.problems.join("; ")));
    }
    if !notes.is_empty() {
        return notes;
    }
    let tol = 1e-6 * a.shape.size.max(b.shape.size);
    let mut free = vec![true; b.shape.edges.len()];
    let mut unmatched = 0;
    for (k, p) in &a.shape.edges {
        let hit =
            (0..free.len()).find(|&j| free[j] && b.shape.edges[j].0 == *k && b.shape.edges[j].1.distance(*p) <= tol);
        match hit {
            Some(j) => free[j] = false,
            None => unmatched += 1,
        }
    }
    if unmatched > 0 {
        notes.push(format!("{unmatched} edges at other positions"));
    }
    let mut free = vec![true; b.shape.faces.len()];
    let (mut unmatched, mut flipped, mut retagged) = (0, 0, 0);
    for (k, tag, pts) in &a.shape.faces {
        // the same points (each one once): whether the normals there point the same way
        let fits = |(k2, _, pts2): &FaceKey| -> Option<bool> {
            if k2 != k || pts2.len() != pts.len() {
                return None;
            }
            let mut used = vec![false; pts2.len()];
            let mut same_way = true;
            for (p, n) in pts {
                let j = (0..pts2.len()).find(|&j| !used[j] && pts2[j].0.distance(*p) <= tol)?;
                used[j] = true;
                if let (Some(n), Some(m)) = (n, pts2[j].1) {
                    same_way &= n.dot(m) > 0.0;
                }
            }
            Some(same_way)
        };
        // a face with the same tag first
        let candidates = (0..free.len()).filter(|&j| free[j]);
        let hit = candidates
            .clone()
            .filter(|&j| b.shape.faces[j].1 == *tag)
            .chain(candidates.filter(|&j| b.shape.faces[j].1 != *tag))
            .find_map(|j| fits(&b.shape.faces[j]).map(|w| (j, w)));
        match hit {
            Some((j, w)) => {
                free[j] = false;
                flipped += usize::from(!w);
                retagged += usize::from(b.shape.faces[j].1 != *tag);
            }
            None => unmatched += 1,
        }
    }
    if unmatched > 0 {
        notes.push(format!("{unmatched} faces at other positions"));
    }
    if flipped > 0 {
        notes.push(format!("{flipped} faces oriented the other way"));
    }
    if retagged > 0 {
        notes.push(format!("{retagged} faces with other tags"));
    }
    notes
}
