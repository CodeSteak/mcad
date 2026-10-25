//! Triangle meshes as bodies without a kernel B-rep (`import("x.stl")` as a mesh, Plan 21
//! phase B).
//!
//! A [`TriMesh`] is welded and consistently oriented (outward for closed meshes, see
//! `cad-stl`). Coplanar neighbouring triangles form planar faces: every triangle belongs to
//! one face ([`TriMesh::tri_face`]), every face has one boundary loop ([`TriMesh::loops`],
//! counter-clockwise about its outward normal; vertices on a straight run between two faces
//! are left out). Regions that would need holes stay single triangles. The same loops give the
//! faces of `polyhedron` (B-rep mode), the display mesh and a facetted [`Brep`] (STEP export,
//! measuring).
//!
//! All queries take the placement `m` (mesh coordinates → target system, any affine map).

use std::collections::HashMap;

use glam::{DMat4, DVec3};

use crate::brep::{self, Brep, Curve, Frame, OrientedEdge, Shell, Surface};
use crate::{MassProps, Mesh, Tag};

/// Edges between faces whose normals differ by more than this angle are drawn (degrees).
pub const FEATURE_ANGLE: f64 = 20.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriMesh {
    pub points: Vec<DVec3>,
    /// Indices into `points`, counter-clockwise seen from outside.
    pub tris: Vec<[u32; 3]>,
    /// Planar face of every triangle (index into `loops`).
    pub tri_face: Vec<u32>,
    /// Boundary loop of every face.
    pub loops: Vec<Vec<u32>>,
    /// Every edge has exactly two triangles (a closed, manifold surface: a solid).
    pub closed: bool,
}

/// An edge between faces (or on the open boundary): loop segment `a → b` of face `faces[0]`
/// (and `b → a` of `faces[1]`, if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceEdge {
    pub a: u32,
    pub b: u32,
    pub faces: [u32; 2],
    /// Only one face (open boundary).
    pub open: bool,
}

impl TriMesh {
    pub fn face_count(&self) -> usize {
        self.loops.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Estimated memory in bytes.
    pub fn bytes(&self) -> usize {
        24 * self.points.len() + 16 * self.tris.len() + self.loops.iter().map(|l| 24 + 4 * l.len()).sum::<usize>() + 64
    }

    /// Axis-aligned bounds of the placed vertices (`None` without triangles).
    pub fn bounds(&self, m: &DMat4) -> Option<(DVec3, DVec3)> {
        if self.tris.is_empty() {
            return None;
        }
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for p in &self.points {
            let q = m.transform_point3(*p);
            lo = lo.min(q);
            hi = hi.max(q);
        }
        Some((lo, hi))
    }

    /// Volume (closed meshes; 0 for open ones), area and centroid of the placed mesh.
    pub fn mass_props(&self, m: &DMat4) -> MassProps {
        let sign = if m.determinant() < 0.0 { -1.0 } else { 1.0 };
        let (mut vol, mut area, mut c) = (0.0, 0.0, DVec3::ZERO);
        let mut ac = DVec3::ZERO;
        for t in &self.tris {
            let [a, b, d] = t.map(|i| m.transform_point3(self.points[i as usize]));
            let v = sign * a.dot(b.cross(d)) / 6.0;
            vol += v;
            c += v * (a + b + d) / 4.0;
            let ar = 0.5 * (b - a).cross(d - a).length();
            area += ar;
            ac += ar * (a + b + d) / 3.0;
        }
        if !self.closed {
            let centroid = if area > 0.0 { ac / area } else { DVec3::ZERO };
            return MassProps { volume: 0.0, area, centroid };
        }
        let centroid = if vol.abs() > 0.0 { c / vol } else { DVec3::ZERO };
        MassProps { volume: vol, area, centroid }
    }

    /// Edges between faces from the loops (each loop segment once).
    pub fn face_edges(&self) -> Vec<FaceEdge> {
        let mut out: Vec<FaceEdge> = Vec::new();
        let mut by_key: HashMap<(u32, u32), usize> = HashMap::new();
        for (f, l) in self.loops.iter().enumerate() {
            for i in 0..l.len() {
                let (a, b) = (l[i], l[(i + 1) % l.len()]);
                match by_key.get(&(b, a)) {
                    Some(&e) if out[e].open => {
                        out[e].faces[1] = f as u32;
                        out[e].open = false;
                    }
                    _ => {
                        by_key.insert((a, b), out.len());
                        out.push(FaceEdge { a, b, faces: [f as u32, f as u32], open: true });
                    }
                }
            }
        }
        out
    }

    /// Unit normals of the placed faces (area weighted over their triangles; a mirror keeps
    /// them outward).
    fn face_normals(&self, q: &[DVec3], flip: bool) -> Vec<DVec3> {
        let mut n = vec![DVec3::ZERO; self.loops.len()];
        for (t, &f) in self.tris.iter().zip(&self.tri_face) {
            let [a, b, c] = t.map(|i| q[i as usize]);
            n[f as usize] += (b - a).cross(c - a);
        }
        n.iter().map(|v| if flip { -*v } else { *v }.normalize_or_zero()).collect()
    }

    /// Display mesh of the placed mesh: flat shading per face, all triangles with `tag`, face
    /// ids = face indices; edges (ids as in [`TriMesh::face_edges`]) where the faces meet at
    /// more than [`FEATURE_ANGLE`] and along the open boundary.
    pub fn display(&self, m: &DMat4, tag: Tag) -> Mesh {
        let q: Vec<DVec3> = self.points.iter().map(|p| m.transform_point3(*p)).collect();
        let flip = m.determinant() < 0.0;
        let normals = self.face_normals(&q, flip);
        let mut out = Mesh::default();
        let mut index: HashMap<(u32, u32), u32> = HashMap::new();
        let mut vert = |out: &mut Mesh, f: u32, v: u32| -> u32 {
            *index.entry((f, v)).or_insert_with(|| {
                let p = q[v as usize];
                let n = normals[f as usize];
                out.positions.extend_from_slice(&[p.x as f32, p.y as f32, p.z as f32]);
                out.normals.extend_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
                (out.positions.len() / 3 - 1) as u32
            })
        };
        for (t, &f) in self.tris.iter().zip(&self.tri_face) {
            let i = [vert(&mut out, f, t[0]), vert(&mut out, f, t[1]), vert(&mut out, f, t[2])];
            if flip {
                out.indices.extend_from_slice(&[i[0], i[2], i[1]]);
            } else {
                out.indices.extend_from_slice(&i);
            }
            out.tri_tags.push(tag);
            out.tri_faces.push(f);
        }
        let cos = FEATURE_ANGLE.to_radians().cos();
        for (id, e) in self.face_edges().iter().enumerate() {
            if !e.open && normals[e.faces[0] as usize].dot(normals[e.faces[1] as usize]) >= cos {
                continue;
            }
            for p in [q[e.a as usize], q[e.b as usize]] {
                out.edge_segments.extend_from_slice(&[p.x as f32, p.y as f32, p.z as f32]);
            }
            out.edge_ids.push(id as u32);
        }
        out
    }

    /// Facetted B-rep of the placed mesh: one plane per face, one line per [`FaceEdge`] (same
    /// ids), all faces with `tag`. Closed meshes: one solid per connected part (inward shells
    /// as voids of the solid around them); open meshes: sheets.
    pub fn brep(&self, m: &DMat4, tag: Tag) -> Brep {
        let q: Vec<DVec3> = self.points.iter().map(|p| m.transform_point3(*p)).collect();
        let flip = m.determinant() < 0.0;
        let normals = self.face_normals(&q, flip);
        let edges = self.face_edges();
        // vertices used by loops, compacted
        let mut vmap: HashMap<u32, u32> = HashMap::new();
        let mut vertices = Vec::new();
        let mut vid = |v: u32| -> u32 {
            *vmap.entry(v).or_insert_with(|| {
                vertices.push(q[v as usize]);
                (vertices.len() - 1) as u32
            })
        };
        let mut bedges = Vec::with_capacity(edges.len());
        let mut use_of: HashMap<(u32, u32), (u32, bool)> = HashMap::new();
        for (i, e) in edges.iter().enumerate() {
            let (a, b) = (vid(e.a), vid(e.b));
            let (pa, pb) = (q[e.a as usize], q[e.b as usize]);
            // a mirror reverses the loops: the edge then runs b → a
            let (s, t) = if flip { (b, a) } else { (a, b) };
            let (ps, pt) = if flip { (pb, pa) } else { (pa, pb) };
            bedges.push(brep::Edge {
                start: s,
                end: t,
                curve: Curve::Line { origin: ps, dir: (pt - ps).normalize_or_zero() },
                approximated: false,
            });
            use_of.insert((e.a, e.b), (i as u32, true));
            if !e.open {
                use_of.insert((e.b, e.a), (i as u32, false));
            }
        }
        let mut faces = Vec::with_capacity(self.loops.len());
        for (f, l) in self.loops.iter().enumerate() {
            let mut oes: Vec<OrientedEdge> = (0..l.len())
                .filter_map(|i| use_of.get(&(l[i], l[(i + 1) % l.len()])))
                .map(|&(edge, forward)| OrientedEdge { edge, forward })
                .collect();
            if flip {
                oes.reverse();
            }
            let o = q[l[0] as usize];
            let x = q[l[1 % l.len()] as usize] - o;
            let frame = Frame::new(o, normals[f], x).unwrap_or(Frame { origin: o, z: DVec3::Z, x: DVec3::X });
            faces.push(brep::Face {
                id: f as u32,
                tag,
                surface: Surface::Plane { frame },
                same_sense: true,
                loops: vec![brep::Loop::Edges(oes)],
                approximated: false,
            });
        }
        let bodies = self.body_groups(&faces, &q, flip);
        let mut slots: Vec<Option<brep::Face>> = faces.into_iter().map(Some).collect();
        let mut take = |ids: &[u32]| Shell { faces: ids.iter().filter_map(|&i| slots[i as usize].take()).collect() };
        let bodies = bodies
            .into_iter()
            .map(|(outer, voids)| {
                if self.closed {
                    brep::Body::Solid { outer: take(&outer), voids: voids.iter().map(|v| take(v)).collect() }
                } else {
                    brep::Body::Sheet { shells: vec![take(&outer)] }
                }
            })
            .collect();
        Brep { vertices, edges: bedges, bodies }
    }

    /// Faces grouped into bodies: connected parts (through shared loop edges); closed meshes:
    /// parts with negative volume become voids of the first positive part whose bounds contain
    /// them.
    #[allow(clippy::type_complexity)]
    fn body_groups(&self, faces: &[brep::Face], q: &[DVec3], flip: bool) -> Vec<(Vec<u32>, Vec<Vec<u32>>)> {
        let n = faces.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(p: &mut [u32], mut i: u32) -> u32 {
            while p[i as usize] != i {
                p[i as usize] = p[p[i as usize] as usize];
                i = p[i as usize];
            }
            i
        }
        for e in self.face_edges() {
            if !e.open {
                let (a, b) = (find(&mut parent, e.faces[0]), find(&mut parent, e.faces[1]));
                if a != b {
                    parent[a as usize] = b;
                }
            }
        }
        let mut groups: Vec<Vec<u32>> = Vec::new();
        let mut gid: HashMap<u32, usize> = HashMap::new();
        for f in 0..n as u32 {
            let r = find(&mut parent, f);
            let g = *gid.entry(r).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[g].push(f);
        }
        if !self.closed || groups.len() == 1 {
            return groups.into_iter().map(|g| (g, Vec::new())).collect();
        }
        // volume and bounds per group (from the triangles)
        let mut vol = vec![0.0; groups.len()];
        let mut lo = vec![DVec3::splat(f64::INFINITY); groups.len()];
        let mut hi = vec![DVec3::splat(f64::NEG_INFINITY); groups.len()];
        let face_group: HashMap<u32, usize> =
            groups.iter().enumerate().flat_map(|(g, fs)| fs.iter().map(move |&f| (f, g))).collect();
        for (t, f) in self.tris.iter().zip(&self.tri_face) {
            let g = face_group[f];
            let [a, b, c] = t.map(|i| q[i as usize]);
            vol[g] += a.dot(b.cross(c));
            for p in [a, b, c] {
                lo[g] = lo[g].min(p);
                hi[g] = hi[g].max(p);
            }
        }
        if flip {
            vol.iter_mut().for_each(|v| *v = -*v);
        }
        let mut out: Vec<(Vec<u32>, Vec<Vec<u32>>)> = Vec::new();
        let mut outer_of: Vec<Option<usize>> = vec![None; groups.len()];
        for g in 0..groups.len() {
            if vol[g] >= 0.0 {
                outer_of[g] = Some(out.len());
                out.push((groups[g].clone(), Vec::new()));
            }
        }
        for g in 0..groups.len() {
            if vol[g] >= 0.0 {
                continue;
            }
            let host = (0..groups.len()).find(|&h| {
                vol[h] >= 0.0 && lo[h].cmple(lo[g]).all() && hi[g].cmple(hi[h]).all() && outer_of[h].is_some()
            });
            match host.and_then(|h| outer_of[h]) {
                Some(o) => out[o].1.push(groups[g].clone()),
                None => out.push((groups[g].clone(), Vec::new())),
            }
        }
        out
    }
}

impl TriMesh {
    /// Section of the placed closed mesh by the plane through `point` with unit `normal`: the
    /// closed polylines of plane ∩ mesh, oriented about `normal` with the material on the left
    /// (outlines counter-clockwise, holes clockwise). A vertex on the plane counts as above it
    /// (consistent for both triangles of an edge).
    pub fn section_loops(&self, m: &DMat4, point: DVec3, normal: DVec3) -> Vec<Vec<DVec3>> {
        if !self.closed {
            return Vec::new();
        }
        let q: Vec<DVec3> = self.points.iter().map(|p| m.transform_point3(*p)).collect();
        let flip = m.determinant() < 0.0;
        let d: Vec<f64> = q.iter().map(|p| (*p - point).dot(normal)).collect();
        let above = |v: u32| d[v as usize] >= 0.0;
        let cut = |a: u32, b: u32| {
            let (da, db) = (d[a as usize], d[b as usize]);
            let t = da / (da - db);
            q[a as usize] + (q[b as usize] - q[a as usize]) * t
        };
        let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
        // segment from edge key to edge key, with its points
        let mut next: HashMap<(u32, u32), ((u32, u32), DVec3)> = HashMap::new();
        for t in &self.tris {
            let t = if flip { [t[0], t[2], t[1]] } else { *t };
            let crossing: Vec<(u32, u32)> =
                (0..3).map(|k| (t[k], t[(k + 1) % 3])).filter(|&(a, b)| above(a) != above(b)).collect();
            if crossing.len() != 2 {
                continue;
            }
            let [a, b, c] = t.map(|i| q[i as usize]);
            let tn = (b - a).cross(c - a);
            let (e0, e1) = (crossing[0], crossing[1]);
            let (p0, p1) = (cut(e0.0, e0.1), cut(e1.0, e1.1));
            // material to the left about the normal: direction along normal × outward normal
            if (p1 - p0).dot(normal.cross(tn)) >= 0.0 {
                next.insert(key(e0.0, e0.1), (key(e1.0, e1.1), p0));
            } else {
                next.insert(key(e1.0, e1.1), (key(e0.0, e0.1), p1));
            }
        }
        let mut loops = Vec::new();
        while let Some(&start) = next.keys().next() {
            let mut l = Vec::new();
            let mut k = start;
            while let Some((n, p)) = next.remove(&k) {
                l.push(p);
                k = n;
            }
            if l.len() >= 3 {
                loops.push(l);
            }
        }
        loops
    }
}

/// Planar faces from closed loops in one plane (as [`TriMesh::section_loops`] gives them):
/// every counter-clockwise loop (about `normal`) is an outline, every clockwise loop a hole of
/// the smallest outline around it. A B-rep of these cap faces only (lines, all faces with
/// `tag`), the form a kernel's section cap has (`cad_measure::section::cut_contours`).
pub fn cap_brep(loops: &[Vec<DVec3>], point: DVec3, normal: DVec3, tag: Tag) -> Brep {
    let x = normal.any_orthonormal_vector();
    let y = normal.cross(x);
    let uv = |p: DVec3| ((p - point).dot(x), (p - point).dot(y));
    let area = |l: &[DVec3]| -> f64 {
        (0..l.len())
            .map(|i| {
                let (a, b) = (uv(l[i]), uv(l[(i + 1) % l.len()]));
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<f64>()
            / 2.0
    };
    let inside = |p: DVec3, l: &[DVec3]| {
        let (px, py) = uv(p);
        let mut c = false;
        for i in 0..l.len() {
            let (a, b) = (uv(l[i]), uv(l[(i + 1) % l.len()]));
            if (a.1 > py) != (b.1 > py) && px < a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                c = !c;
            }
        }
        c
    };
    let areas: Vec<f64> = loops.iter().map(|l| area(l)).collect();
    let outers: Vec<usize> = (0..loops.len()).filter(|&i| areas[i] > 0.0).collect();
    let mut holes_of: Vec<Vec<usize>> = vec![Vec::new(); loops.len()];
    for h in (0..loops.len()).filter(|&i| areas[i] <= 0.0) {
        let host = outers
            .iter()
            .copied()
            .filter(|&o| inside(loops[h][0], &loops[o]))
            .min_by(|&a, &b| areas[a].total_cmp(&areas[b]));
        if let Some(o) = host {
            holes_of[o].push(h);
        }
    }
    let mut b = Brep::default();
    let edge_loop = |b: &mut Brep, l: &[DVec3]| -> brep::Loop {
        let base = b.vertices.len() as u32;
        b.vertices.extend_from_slice(l);
        let n = l.len() as u32;
        let mut oes = Vec::with_capacity(l.len());
        for i in 0..n {
            let (s, t) = (base + i, base + (i + 1) % n);
            let (ps, pt) = (b.vertices[s as usize], b.vertices[t as usize]);
            b.edges.push(brep::Edge {
                start: s,
                end: t,
                curve: Curve::Line { origin: ps, dir: (pt - ps).normalize_or_zero() },
                approximated: false,
            });
            oes.push(OrientedEdge { edge: (b.edges.len() - 1) as u32, forward: true });
        }
        brep::Loop::Edges(oes)
    };
    let mut faces = Vec::new();
    for (k, &o) in outers.iter().enumerate() {
        let mut ls = vec![edge_loop(&mut b, &loops[o])];
        for &h in &holes_of[o] {
            ls.push(edge_loop(&mut b, &loops[h]));
        }
        faces.push(brep::Face {
            id: k as u32,
            tag,
            surface: Surface::Plane { frame: Frame { origin: point, z: normal, x } },
            same_sense: true,
            loops: ls,
            approximated: false,
        });
    }
    if !faces.is_empty() {
        b.bodies.push(brep::Body::Sheet { shells: vec![Shell { faces }] });
    }
    b
}

/// Is `p` inside the closed surface made of `tris`? Parity of the crossings of a ray from `p`
/// in a fixed skew direction (Möller–Trumbore; a ray through an edge or corner is unlikely).
pub fn ray_parity(p: DVec3, tris: impl IntoIterator<Item = [DVec3; 3]>) -> bool {
    let d = DVec3::new(0.5181, 0.6127, 0.5967).normalize();
    let mut inside = false;
    for [a, b, c] in tris {
        let (e1, e2) = (b - a, c - a);
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-300 {
            continue;
        }
        let f = 1.0 / det;
        let s = p - a;
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let qv = s.cross(e1);
        let v = f * d.dot(qv);
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        if f * e2.dot(qv) > 0.0 {
            inside = !inside;
        }
    }
    inside
}

impl TriMesh {
    /// Is the point (target system) inside the placed closed mesh? `false` for open meshes.
    pub fn contains(&self, m: &DMat4, p: DVec3) -> bool {
        self.closed && ray_parity(p, self.tris.iter().map(|t| t.map(|i| m.transform_point3(self.points[i as usize]))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit cube from 12 triangles, merged to 6 quads.
    fn cube() -> TriMesh {
        let points: Vec<DVec3> =
            (0..8).map(|i| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64)).collect();
        let quads: [[u32; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut tris = Vec::new();
        let mut tri_face = Vec::new();
        for (f, q) in quads.iter().enumerate() {
            tris.push([q[0], q[1], q[2]]);
            tris.push([q[0], q[2], q[3]]);
            tri_face.extend([f as u32, f as u32]);
        }
        TriMesh { points, tris, tri_face, loops: quads.iter().map(|q| q.to_vec()).collect(), closed: true }
    }

    #[test]
    fn cube_mass_display_brep() {
        let c = cube();
        let m = DMat4::from_scale(DVec3::new(2.0, 3.0, 4.0));
        let mp = c.mass_props(&m);
        assert!((mp.volume - 24.0).abs() < 1e-9, "{mp:?}");
        assert!((mp.area - 52.0).abs() < 1e-9);
        assert!((mp.centroid - DVec3::new(1.0, 1.5, 2.0)).length() < 1e-9);
        let d = c.display(&m, 7);
        assert_eq!(d.triangle_count(), 12);
        assert_eq!(d.positions.len() / 3, 24);
        assert_eq!(d.edge_ids.len(), 12);
        let b = c.brep(&m, 7);
        b.validate().unwrap();
        assert_eq!(b.faces().count(), 6);
        assert_eq!(b.edges.len(), 12);
        assert_eq!(b.vertices.len(), 8);
        // mirrored: still outward, still valid
        let mm = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        assert!((c.mass_props(&mm).volume - 1.0).abs() < 1e-12);
        c.brep(&mm, 1).validate().unwrap();
        // section: one square loop (counter-clockwise about +z), one cap face
        let loops = c.section_loops(&m, DVec3::new(0.0, 0.0, 1.0), DVec3::Z);
        assert_eq!(loops.len(), 1);
        let cap = cap_brep(&loops, DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 3);
        cap.validate().unwrap();
        assert_eq!(cap.faces().count(), 1);
        let l = &loops[0];
        let a: f64 = (0..l.len()).map(|i| l[i].x * l[(i + 1) % l.len()].y - l[(i + 1) % l.len()].x * l[i].y).sum();
        assert!((a / 2.0 - 6.0).abs() < 1e-9, "{a}");
        let dm = c.display(&mm, 1);
        let p = |i: u32| {
            let k = 3 * i as usize;
            DVec3::new(dm.positions[k] as f64, dm.positions[k + 1] as f64, dm.positions[k + 2] as f64)
        };
        let mut vol = 0.0;
        for t in dm.indices.chunks_exact(3) {
            vol += p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0;
        }
        assert!((vol - 1.0).abs() < 1e-6, "{vol}");
    }
}
