//! Native integration tests of kernel-smlib (`cargo test -p kernel-smlib`).

mod common;

use cad_kernel::*;
use common::*;
use glam::{DMat4, DVec2, DVec3};
use std::collections::BTreeSet;

const T_BOX: Tag = 1;
const T_CYL: Tag = 2;
const T_OP: Tag = 50;
const T_FIL: Tag = 77;

fn cube(k: &mut kernel_smlib::SmlibKernel, a: f64, tag: Tag) -> kernel_smlib::Shape {
    k.make_box(DVec3::ZERO, DVec3::splat(a), tag).unwrap()
}

/// Edge points of the shape that lie on a circle of radius r around the vertical axis (cx, cy).
fn on_cylinder(p: DVec3, cx: f64, cy: f64, r: f64) -> bool {
    ((p.x - cx).hypot(p.y - cy) - r).abs() < 1e-6
}

// ---------------------------------------------------------------- primitives

#[test]
fn box_volume_area_bounds() {
    let mut k = k();
    let b = k.make_box(DVec3::new(1.0, 2.0, 3.0), DVec3::new(11.0, 22.0, 33.0), T_BOX).unwrap();
    let mp = k.mass_props(&b).unwrap();
    assert_approx("volume", mp.volume, 10.0 * 20.0 * 30.0, 1e-9);
    assert_approx("area", mp.area, 2.0 * (200.0 + 600.0 + 300.0), 1e-6);
    assert!((mp.centroid - DVec3::new(6.0, 12.0, 18.0)).length() < 1e-6);
    let (lo, hi) = k.bounds(&b).unwrap().unwrap();
    assert!((lo - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-6, "{lo}");
    assert!((hi - DVec3::new(11.0, 22.0, 33.0)).length() < 1e-6, "{hi}");
    assert!(k.is_solid(&b));
    assert!(!k.is_empty(&b));
    assert_eq!(k.faces(&b, 2).unwrap().len(), 6);
    assert_eq!(k.edges(&b, 2).unwrap().len(), 12);
    assert!(k.faces(&b, 2).unwrap().iter().all(|f| f.tag == T_BOX));
}

#[test]
fn sphere_volume() {
    let mut k = k();
    let s = k.make_sphere(3.0, 5).unwrap();
    assert_approx("sphere volume", volume(&mut k, &s), 4.0 / 3.0 * PI_ * 27.0, 1e-4);
    assert!(k.is_solid(&s));
    let (lo, hi) = k.bounds(&s).unwrap().unwrap();
    assert!((lo + DVec3::splat(3.0)).length() < 1e-3 && (hi - DVec3::splat(3.0)).length() < 1e-3);
}

#[test]
fn cylinder_and_cones() {
    let mut k = k();
    let c = k.make_cylinder(10.0, 2.0, 2.0, 1).unwrap();
    assert_approx("cylinder", volume(&mut k, &c), PI_ * 4.0 * 10.0, 1e-5);
    let frustum = k.make_cylinder(10.0, 3.0, 1.0, 1).unwrap();
    assert_approx("frustum", volume(&mut k, &frustum), PI_ * 10.0 / 3.0 * (9.0 + 3.0 + 1.0), 1e-5);
    // cone tip at the top
    let cone = k.make_cylinder(6.0, 2.0, 0.0, 1).unwrap();
    assert_approx("cone", volume(&mut k, &cone), PI_ * 4.0 * 6.0 / 3.0, 1e-5);
    let (lo, hi) = k.bounds(&cone).unwrap().unwrap();
    assert!(lo.z.abs() < 1e-6 && (hi.z - 6.0).abs() < 1e-6);
    // cone tip at the bottom: r1 = 0
    let inv = k.make_cylinder(6.0, 0.0, 2.0, 1).unwrap();
    assert_approx("inverted cone", volume(&mut k, &inv), PI_ * 4.0 * 6.0 / 3.0, 1e-5);
    let cls = k
        .classify(&inv, &[DVec3::new(0.0, 0.0, 5.5), DVec3::new(1.9, 0.0, 0.5), DVec3::new(0.0, 0.0, -0.1)])
        .unwrap();
    assert_eq!(cls, vec![Class::Inside, Class::Outside, Class::Outside]);
    assert!(matches!(k.make_cylinder(5.0, 0.0, 0.0, 1), Err(KernelError::InvalidInput(_))));
}

#[test]
fn torus_volume() {
    let mut k = k();
    let t = k.make_torus(5.0, 1.0, 3).unwrap();
    assert_approx("torus", volume(&mut k, &t), 2.0 * PI_ * PI_ * 5.0 * 1.0, 1e-4);
    assert!(k.is_solid(&t));
    let cls = k.classify(&t, &[DVec3::new(5.0, 0.0, 0.0), DVec3::ZERO]).unwrap();
    assert_eq!(cls, vec![Class::Inside, Class::Outside]);
}

#[test]
fn polyhedron_tetrahedron() {
    let mut k = k();
    let pts = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(6.0, 0.0, 0.0),
        DVec3::new(0.0, 6.0, 0.0),
        DVec3::new(0.0, 0.0, 6.0),
    ];
    // mixed orientation on purpose
    let faces = vec![vec![0, 1, 2], vec![0, 1, 3], vec![1, 2, 3], vec![0, 3, 2]];
    match k.make_polyhedron(&pts, &faces, 4) {
        Ok(p) => {
            assert_approx("tetra volume", volume(&mut k, &p), 36.0, 1e-6);
            assert_eq!(k.faces(&p, 1).unwrap().len(), 4);
        }
        Err(e) => panic!("polyhedron failed: {e}"),
    }
}

// ---------------------------------------------------------------- booleans and provenance

fn box_minus_cylinder(k: &mut kernel_smlib::SmlibKernel) -> kernel_smlib::Shape {
    let b = cube(k, 10.0, T_BOX);
    let c = k.make_cylinder(12.0, 2.0, 2.0, T_CYL).unwrap();
    let c = k.transform(&c, &translate(5.0, 5.0, -1.0)).unwrap();
    k.boolean(BoolOp::Difference, &[b, c], T_OP).unwrap()
}

#[test]
fn box_minus_cylinder_volume_and_provenance() {
    let mut k = k();
    let d = box_minus_cylinder(&mut k);
    assert_approx("volume", volume(&mut k, &d), 1000.0 - PI_ * 4.0 * 10.0, 1e-5);
    assert!(k.is_solid(&d));
    let faces = k.faces(&d, 3).unwrap();
    assert_eq!(faces.len(), 7);
    for f in &faces {
        let bore = f.points.iter().all(|p| on_cylinder(*p, 5.0, 5.0, 2.0));
        if bore {
            assert_eq!(f.tag, T_CYL, "bore face must carry the cylinder's tag");
        } else {
            assert_eq!(f.tag, T_BOX, "box faces keep the box tag: {:?}", f.points[0]);
        }
    }
    assert_eq!(faces.iter().filter(|f| f.tag == T_CYL).count(), 1);
    // edges between bore and box faces are "new" edges of the boolean (different child tags)
    let edges = k.edges(&d, 4).unwrap();
    let new_edges: Vec<_> = edges
        .iter()
        .filter(|e| e.face_tags.contains(&T_BOX) && e.face_tags.contains(&T_CYL))
        .collect();
    assert_eq!(new_edges.len(), 2, "two circular rim edges");
    for e in new_edges {
        assert!(e.points.iter().all(|p| on_cylinder(*p, 5.0, 5.0, 2.0)));
    }
}

#[test]
fn union_and_intersection() {
    let mut k = k();
    let a = cube(&mut k, 10.0, 1);
    let b = k.make_box(DVec3::new(5.0, 0.0, 0.0), DVec3::new(15.0, 10.0, 10.0), 2).unwrap();
    let u = k.boolean(BoolOp::Union, &[a.clone(), b.clone()], 3).unwrap();
    assert_approx("union", volume(&mut k, &u), 1500.0, 1e-6);
    let i = k.boolean(BoolOp::Intersection, &[a.clone(), b.clone()], 3).unwrap();
    assert_approx("intersection", volume(&mut k, &i), 500.0, 1e-6);
    let (lo, hi) = k.bounds(&i).unwrap().unwrap();
    assert!((lo - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-5 && (hi - DVec3::splat(10.0)).length() < 1e-5);
    // n-ary union
    let c = k.make_sphere(2.0, 4).unwrap();
    let c = k.transform(&c, &translate(20.0, 5.0, 5.0)).unwrap();
    let u3 = k.boolean(BoolOp::Union, &[a.clone(), b.clone(), c], 3).unwrap();
    assert_approx("union of three (disjoint sphere)", volume(&mut k, &u3), 1500.0 + 4.0 / 3.0 * PI_ * 8.0, 1e-4);
    // tags: union faces come from a or b only
    let tags: BTreeSet<Tag> = k.faces(&u, 1).unwrap().iter().map(|f| f.tag).collect();
    assert!(tags.is_subset(&BTreeSet::from([1, 2])), "{tags:?}");
    // sphere sticking out of a box: intersection
    let s = k.make_sphere(6.0, 9).unwrap();
    let s = k.transform(&s, &translate(5.0, 5.0, 5.0)).unwrap();
    let bs = k.boolean(BoolOp::Intersection, &[a.clone(), s], 3).unwrap();
    let v = volume(&mut k, &bs);
    assert!(v < 1000.0 && v > 4.0 / 3.0 * PI_ * 125.0, "box ∩ sphere volume {v}");
    let tags: BTreeSet<Tag> = k.faces(&bs, 1).unwrap().iter().map(|f| f.tag).collect();
    assert_eq!(tags, BTreeSet::from([1, 9]));
    // disjoint intersection → empty
    let far = k.make_box(DVec3::splat(100.0), DVec3::splat(101.0), 2).unwrap();
    let e = k.boolean(BoolOp::Intersection, &[a.clone(), far], 3).unwrap();
    assert!(k.is_empty(&e));
    assert_eq!(k.bounds(&e).unwrap(), None);
}

#[test]
fn difference_of_many() {
    let mut k = k();
    let b = cube(&mut k, 10.0, 1);
    let mut shapes = vec![b];
    for (i, x) in [2.5, 7.5].iter().enumerate() {
        let c = k.make_cylinder(12.0, 1.0, 1.0, 10 + i as u32).unwrap();
        shapes.push(k.transform(&c, &translate(*x, 5.0, -1.0)).unwrap());
    }
    let d = k.boolean(BoolOp::Difference, &shapes, 3).unwrap();
    assert_approx("two holes", volume(&mut k, &d), 1000.0 - 2.0 * PI_ * 10.0, 1e-5);
    let tags: BTreeSet<Tag> = k.faces(&d, 1).unwrap().iter().map(|f| f.tag).collect();
    assert_eq!(tags, BTreeSet::from([1, 10, 11]));
}

// ---------------------------------------------------------------- fillets and chamfers

#[test]
fn fillet_all_edges_of_box() {
    let mut k = k();
    let b = cube(&mut k, 10.0, T_BOX);
    let all: Vec<u32> = (0..12).collect();
    let f = k.fillet(&b, &all, 1.0, T_FIL).unwrap();
    // rounded box: L^3 + 6 L^2 r + 3 pi L r^2 + 4/3 pi r^3 with L = a - 2r
    let l = 8.0;
    let want = l * l * l + 6.0 * l * l + 3.0 * PI_ * l + 4.0 / 3.0 * PI_;
    assert_approx("rounded box", volume(&mut k, &f), want, 1e-4);
    let faces = k.faces(&f, 2).unwrap();
    assert_eq!(faces.iter().filter(|f| f.tag == T_BOX).count(), 6);
    assert_eq!(faces.iter().filter(|f| f.tag == T_FIL).count(), faces.len() - 6);
    assert!(k.is_solid(&f));
}

#[test]
fn fillet_single_edge_and_provenance() {
    let mut k = k();
    let b = cube(&mut k, 10.0, T_BOX);
    // edge along y at x = 10, z = 10
    let e = k
        .edges_where(&b, |e| e.points.iter().all(|p| (p.x - 10.0).abs() < 1e-9 && (p.z - 10.0).abs() < 1e-9))
        .unwrap();
    assert_eq!(e.len(), 1);
    let f = k.fillet(&b, &e, 2.0, T_FIL).unwrap();
    assert_approx("single fillet", volume(&mut k, &f), 1000.0 - (1.0 - PI_ / 4.0) * 4.0 * 10.0, 1e-5);
    let faces = k.faces(&f, 3).unwrap();
    let fil: Vec<_> = faces.iter().filter(|f| f.tag == T_FIL).collect();
    assert_eq!(fil.len(), 1, "exactly one fillet face");
    for p in &fil[0].points {
        let r = (p.x - 8.0).hypot(p.z - 8.0);
        assert!((r - 2.0).abs() < 1e-6, "fillet face point {p} not on the r=2 arc");
    }
    assert_eq!(faces.iter().filter(|f| f.tag == T_BOX).count(), 6);
}

#[test]
fn fillet_after_boolean_keeps_tags() {
    let mut k = k();
    let d = box_minus_cylinder(&mut k);
    // fillet the top rim of the bore
    let rim = k
        .edges_where(&d, |e| e.points.iter().all(|p| on_cylinder(*p, 5.0, 5.0, 2.0) && (p.z - 10.0).abs() < 1e-9))
        .unwrap();
    assert_eq!(rim.len(), 1);
    let f = k.fillet(&d, &rim, 0.5, T_FIL).unwrap();
    let v = volume(&mut k, &f);
    assert!(v < 1000.0 - PI_ * 40.0 && v > 1000.0 - PI_ * 40.0 - 5.0, "volume {v}");
    let tags: BTreeSet<Tag> = k.faces(&f, 1).unwrap().iter().map(|f| f.tag).collect();
    assert_eq!(tags, BTreeSet::from([T_BOX, T_CYL, T_FIL]));
}

#[test]
fn chamfer_edges() {
    let mut k = k();
    let b = cube(&mut k, 10.0, T_BOX);
    let c1 = k.chamfer(&b, &[0], 1.0, T_FIL).unwrap();
    // setback 1 on both faces of a 90° edge: removes a 1x1/2 prism of length 10
    assert_approx("single chamfer", volume(&mut k, &c1), 1000.0 - 0.5 * 10.0, 1e-5);
    let faces = k.faces(&c1, 2).unwrap();
    assert_eq!(faces.iter().filter(|f| f.tag == T_FIL).count(), 1);
    let all: Vec<u32> = (0..12).collect();
    let call = k.chamfer(&b, &all, 1.0, T_FIL).unwrap();
    let v = volume(&mut k, &call);
    assert!(v < 1000.0 - 12.0 * 0.5 * 8.0 + 1e-6 && v > 900.0, "all-edge chamfer volume {v}");
    assert!(k.is_solid(&call));
}

#[test]
fn fillet_too_large_fails_cleanly() {
    let mut k = k();
    let b = cube(&mut k, 10.0, T_BOX);
    let all: Vec<u32> = (0..12).collect();
    match k.fillet(&b, &all, 6.0, T_FIL) {
        Err(KernelError::Failed { msg, edges }) => {
            assert!(!msg.is_empty());
            assert!(!edges.is_empty(), "failing edges reported");
            assert!(edges.iter().all(|e| *e < 12));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    // single edge with a radius larger than the faces
    match k.fillet(&b, &[0], 11.0, T_FIL) {
        Err(KernelError::Failed { msg, .. }) => assert!(!msg.is_empty()),
        other => panic!("expected Failed, got {other:?}"),
    }
    // the kernel stays usable
    let f = k.fillet(&b, &[0], 1.0, T_FIL).unwrap();
    assert!(volume(&mut k, &f) < 1000.0);
    // bad edge ids
    assert!(matches!(k.fillet(&b, &[99], 1.0, 1), Err(KernelError::InvalidInput(_))));
    // radius exactly half the box: SMLib reports the failing edge
    match k.fillet(&b, &all, 5.0, T_FIL) {
        Err(KernelError::Failed { edges, .. }) => assert!(!edges.is_empty()),
        other => panic!("expected Failed, got {other:?}"),
    }
    // SMLib silently returns the unchanged box for a too large chamfer: detected
    match k.chamfer(&b, &all, 6.0, T_FIL) {
        Err(KernelError::Failed { msg, .. }) => assert!(msg.contains("unchanged"), "{msg}"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn fillet_cylinder_rims_skips_seam() {
    let mut k = k();
    let c = k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap();
    // all edges, including the seam of the lateral face (which cannot be blended)
    let all: Vec<u32> = (0..k.edge_count(&c) as u32).collect();
    let f = k.fillet(&c, &all, 1.0, T_FIL).unwrap();
    // Pappus: each rim removes the region r^2 (1 - pi/4) at centroid radius R - r (10 - 3 pi) / (12 - 3 pi)
    let r: f64 = 1.0;
    let a = r * r * (1.0 - PI_ / 4.0);
    let rho = 3.0 - r * (10.0 - 3.0 * PI_) / (12.0 - 3.0 * PI_);
    let want = PI_ * 9.0 * 10.0 - 2.0 * 2.0 * PI_ * rho * a;
    assert_approx("rounded cylinder", volume(&mut k, &f), want, 1e-4);
    assert_eq!(k.faces(&f, 1).unwrap().iter().filter(|f| f.tag == T_FIL).count(), 2);
}

#[test]
fn invalid_profiles_fail_cleanly() {
    let mut k = k();
    // bow-tie polygon
    let bow = vec![DVec2::new(0.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(2.0, 0.0), DVec2::new(0.0, 2.0)];
    let r = k.extrude(&placed(Loop::Polygon(bow), DMat4::IDENTITY), 1.0, 0.0, DVec2::ONE, 3);
    assert!(matches!(r, Err(KernelError::Failed { .. })), "{r:?}");
    // profile crossing the revolve axis
    let rect = vec![DVec2::new(-1.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0), DVec2::new(-1.0, 1.0)];
    let r = k.revolve(&placed(Loop::Polygon(rect), DMat4::from_rotation_x(PI_ / 2.0)), PI_, 3);
    assert!(matches!(r, Err(KernelError::InvalidInput(_))), "{r:?}");
    // degenerate input
    assert!(matches!(k.make_box(DVec3::ZERO, DVec3::new(1.0, 0.0, 1.0), 1), Err(KernelError::InvalidInput(_))));
    assert!(matches!(k.make_sphere(-1.0, 1), Err(KernelError::InvalidInput(_))));
    // too thick shell
    let b = cube(&mut k, 10.0, T_BOX);
    assert!(matches!(k.shell(&b, 6.0, &[0], 3), Err(KernelError::Failed { .. })));
}

// ---------------------------------------------------------------- queries

#[test]
fn classify_points() {
    let mut k = k();
    let d = box_minus_cylinder(&mut k);
    let pts = [
        DVec3::new(1.0, 1.0, 1.0),  // inside material
        DVec3::new(5.0, 5.0, 5.0),  // inside the hole
        DVec3::new(20.0, 0.0, 0.0), // outside
        DVec3::new(10.0, 5.0, 5.0), // on a face
        DVec3::new(7.0, 5.0, 5.0),  // on the bore
        DVec3::new(0.0, 0.0, 0.0),  // on a vertex
    ];
    let c = k.classify(&d, &pts).unwrap();
    assert_eq!(c, vec![Class::Inside, Class::Outside, Class::Outside, Class::On, Class::On, Class::On]);
}

#[test]
fn tessellation_closed_with_tags_and_ids() {
    let mut k = k();
    let d = box_minus_cylinder(&mut k);
    let all: Vec<u32> = k
        .edges_where(&d, |e| e.points.iter().all(|p| p.z > 9.999) && !e.points.iter().all(|p| on_cylinder(*p, 5.0, 5.0, 2.0)))
        .unwrap();
    assert_eq!(all.len(), 4);
    let f = k.fillet(&d, &all, 1.0, T_FIL).unwrap();
    for s in [&d, &f] {
        let m = k.tessellate(s, 0.01).unwrap();
        assert!(m.triangle_count() > 20);
        assert_eq!(m.tri_tags.len(), m.triangle_count());
        assert_eq!(m.tri_faces.len(), m.triangle_count());
        assert_eq!(m.normals.len(), m.positions.len());
        assert!(mesh_closed(&m), "mesh must be closed and consistently oriented");
        let vol = volume(&mut k, s);
        assert_approx("mesh volume", mesh_volume(&m), vol, 2e-3);
        // tri tags agree with faces()
        let faces = k.faces(s, 1).unwrap();
        for (t, fi) in m.tri_tags.iter().zip(&m.tri_faces) {
            assert_eq!(faces[*fi as usize].tag, *t);
        }
        let tags: BTreeSet<Tag> = m.tri_tags.iter().copied().collect();
        let face_tags: BTreeSet<Tag> = faces.iter().map(|f| f.tag).collect();
        assert_eq!(tags, face_tags, "every face appears in the mesh");
        // edge segments lie on the edge with the same id (ids stable between edges/tessellate)
        let edges = k.edges(s, 32).unwrap();
        assert_eq!(m.edge_ids.len() * 6, m.edge_segments.len());
        let seen: BTreeSet<u32> = m.edge_ids.iter().copied().collect();
        assert_eq!(seen.len(), edges.len(), "every edge has segments");
        for (i, id) in m.edge_ids.iter().enumerate() {
            let e = &edges[*id as usize];
            for j in 0..2 {
                let o = i * 6 + j * 3;
                let p = DVec3::new(m.edge_segments[o] as f64, m.edge_segments[o + 1] as f64, m.edge_segments[o + 2] as f64);
                let d = e
                    .points
                    .windows(2)
                    .map(|w| dist_point_segment(p, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(d < 0.05, "segment point {p} is {d} away from edge {id}");
            }
        }
        // vertex normals agree with the triangle winding (which is outward: positive volume)
        let pos = |i: u32| {
            let i = i as usize * 3;
            DVec3::new(m.positions[i] as f64, m.positions[i + 1] as f64, m.positions[i + 2] as f64)
        };
        let nor = |i: u32| {
            let i = i as usize * 3;
            DVec3::new(m.normals[i] as f64, m.normals[i + 1] as f64, m.normals[i + 2] as f64)
        };
        let mut bad = 0;
        for t in m.indices.chunks_exact(3) {
            let tn = (pos(t[1]) - pos(t[0])).cross(pos(t[2]) - pos(t[0]));
            if tn.length() < 1e-9 {
                continue;
            }
            if t.iter().any(|&v| nor(v).dot(tn) < 0.0) {
                bad += 1;
            }
        }
        assert_eq!(bad, 0, "triangles whose vertex normals disagree with the winding");
    }
}

fn dist_point_segment(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).length()
}

#[test]
fn edges_and_faces_stable() {
    let mut k = k();
    let d = box_minus_cylinder(&mut k);
    let e1 = k.edges(&d, 3).unwrap();
    let f1 = k.faces(&d, 3).unwrap();
    let _ = k.tessellate(&d, 0.05).unwrap();
    let _ = k.classify(&d, &[DVec3::ONE]).unwrap();
    let e2 = k.edges(&d, 3).unwrap();
    let f2 = k.faces(&d, 3).unwrap();
    assert_eq!(e1, e2);
    assert_eq!(f1, f2);
    // a filleted copy does not change the original
    let _ = k.fillet(&d, &[0], 0.5, 9).unwrap();
    assert_eq!(k.edges(&d, 3).unwrap(), e1);
}

// ---------------------------------------------------------------- transforms

#[test]
fn transform_with_mirror() {
    let mut k = k();
    let d = box_minus_cylinder(&mut k);
    let v0 = volume(&mut k, &d);
    let mirror = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
    let m = k.transform(&d, &mirror).unwrap();
    assert_approx("mirrored volume", volume(&mut k, &m), v0, 1e-6);
    assert!(volume(&mut k, &m) > 0.0);
    let (lo, hi) = k.bounds(&m).unwrap().unwrap();
    assert!((lo - DVec3::new(-10.0, 0.0, 0.0)).length() < 1e-5, "{lo}");
    assert!((hi - DVec3::new(0.0, 10.0, 10.0)).length() < 1e-5, "{hi}");
    // orientation: classification and outward mesh normals still correct
    let c = k.classify(&m, &[DVec3::new(-1.0, 1.0, 1.0), DVec3::new(-5.0, 5.0, 5.0)]).unwrap();
    assert_eq!(c, vec![Class::Inside, Class::Outside]);
    let mesh = k.tessellate(&m, 0.01).unwrap();
    assert!(mesh_closed(&mesh));
    assert_approx("mirrored mesh volume", mesh_volume(&mesh), v0, 2e-3);
    let tags: BTreeSet<Tag> = k.faces(&m, 1).unwrap().iter().map(|f| f.tag).collect();
    assert_eq!(tags, BTreeSet::from([T_BOX, T_CYL]));
    // rotation + non-uniform scale + translation
    let t = DMat4::from_translation(DVec3::new(3.0, 0.0, 0.0))
        * DMat4::from_rotation_z(0.3)
        * DMat4::from_scale(DVec3::new(2.0, 1.0, 0.5));
    let s = k.transform(&d, &t).unwrap();
    assert_approx("scaled volume", volume(&mut k, &s), v0 * 1.0, 1e-4);
    // a mirrored and filleted shape
    let mf = k.fillet(&m, &[0], 0.5, 9).unwrap();
    assert!(volume(&mut k, &mf) < v0);
}

// ---------------------------------------------------------------- profiles

fn placed(lp: Loop, frame: DMat4) -> PlacedLoop {
    PlacedLoop { lp, frame }
}

#[test]
fn extrude_polygon_and_circle() {
    let mut k = k();
    let sq = placed(Loop::Polygon(square(4.0)), DMat4::IDENTITY);
    let e = k.extrude(&sq, 5.0, 0.0, DVec2::ONE, 7).unwrap();
    assert_approx("extruded square", volume(&mut k, &e), 80.0, 1e-6);
    assert_eq!(k.faces(&e, 1).unwrap().len(), 6);
    assert!(k.faces(&e, 1).unwrap().iter().all(|f| f.tag == 7));
    let (lo, hi) = k.bounds(&e).unwrap().unwrap();
    assert!(lo.length() < 1e-6 && (hi - DVec3::new(4.0, 4.0, 5.0)).length() < 1e-6);
    // clockwise polygon, placed with a frame
    let cw = vec![DVec2::new(0.0, 0.0), DVec2::new(0.0, 2.0), DVec2::new(3.0, 2.0), DVec2::new(3.0, 0.0)];
    let fr = DMat4::from_translation(DVec3::new(0.0, 0.0, 10.0)) * DMat4::from_rotation_x(PI_ / 2.0);
    let e2 = k.extrude(&placed(Loop::Polygon(cw), fr), 4.0, 0.0, DVec2::ONE, 7).unwrap();
    assert_approx("framed extrude", volume(&mut k, &e2), 24.0, 1e-6);
    let (lo, hi) = k.bounds(&e2).unwrap().unwrap();
    assert!((lo - DVec3::new(0.0, -4.0, 10.0)).length() < 1e-6, "{lo}");
    assert!((hi - DVec3::new(3.0, 0.0, 12.0)).length() < 1e-6, "{hi}");
    // circle
    let c = placed(Loop::Circle { center: DVec2::new(1.0, 1.0), r: 2.0 }, DMat4::IDENTITY);
    let ec = k.extrude(&c, 3.0, 0.0, DVec2::ONE, 7).unwrap();
    assert_approx("extruded circle", volume(&mut k, &ec), PI_ * 4.0 * 3.0, 1e-5);
    // negative height
    let en = k.extrude(&sq, -2.0, 0.0, DVec2::ONE, 7).unwrap();
    assert_approx("negative extrude", volume(&mut k, &en), 32.0, 1e-6);
    let (lo, _) = k.bounds(&en).unwrap().unwrap();
    assert!((lo.z + 2.0).abs() < 1e-6);
    // spline profile
    let sp = Loop::Spline(vec![DVec2::new(2.0, 0.0), DVec2::new(0.0, 2.0), DVec2::new(-2.0, 0.0), DVec2::new(0.0, -2.0)]);
    let es = k.extrude(&placed(sp, DMat4::IDENTITY), 1.0, 0.0, DVec2::ONE, 7).unwrap();
    let v = volume(&mut k, &es);
    assert!(v > 8.0 && v < 16.0, "spline extrude volume {v}");
}

#[test]
fn extrude_with_twist_and_scale() {
    let mut k = k();
    let sq = vec![DVec2::new(-1.0, -1.0), DVec2::new(1.0, -1.0), DVec2::new(1.0, 1.0), DVec2::new(-1.0, 1.0)];
    let p = placed(Loop::Polygon(sq), DMat4::IDENTITY);
    // pure scale: frustum of a pyramid, V = h/3 (A1 + A2 + sqrt(A1 A2))
    match k.extrude(&p, 3.0, 0.0, DVec2::splat(0.5), 7) {
        Ok(s) => assert_approx("tapered extrude", volume(&mut k, &s), 1.0 * (4.0 + 1.0 + 2.0), 1e-4),
        Err(KernelError::Unsupported(_)) => {}
        Err(e) => panic!("{e}"),
    }
    // twist: volume of a twisted square prism equals the untwisted one
    match k.extrude(&p, 4.0, PI_ / 2.0, DVec2::ONE, 7) {
        Ok(s) => assert_approx("twisted extrude", volume(&mut k, &s), 16.0, 2e-2),
        Err(KernelError::Unsupported(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn revolve_polygon_and_circle() {
    let mut k = k();
    // rectangle x in [2, 3], z in [0, 1] (profile xy-plane mapped to xz)
    let rect = vec![DVec2::new(2.0, 0.0), DVec2::new(3.0, 0.0), DVec2::new(3.0, 1.0), DVec2::new(2.0, 1.0)];
    let to_xz = DMat4::from_rotation_x(PI_ / 2.0);
    let r = k.revolve(&placed(Loop::Polygon(rect.clone()), to_xz), 2.0 * PI_, 8).unwrap();
    assert_approx("revolved ring", volume(&mut k, &r), PI_ * (9.0 - 4.0) * 1.0, 1e-5);
    assert!(k.is_solid(&r));
    let half = k.revolve(&placed(Loop::Polygon(rect), to_xz), PI_, 8).unwrap();
    assert_approx("half ring", volume(&mut k, &half), PI_ * 5.0 / 2.0, 1e-5);
    // circle at x = 4, r = 1 → torus
    let c = placed(Loop::Circle { center: DVec2::new(4.0, 0.0), r: 1.0 }, to_xz);
    let t = k.revolve(&c, 2.0 * PI_, 8).unwrap();
    assert_approx("revolved circle", volume(&mut k, &t), 2.0 * PI_ * PI_ * 4.0, 1e-4);
}

#[test]
fn loft_two_profiles() {
    let mut k = k();
    let a = placed(Loop::Polygon(square(2.0)), DMat4::IDENTITY);
    let b = placed(Loop::Polygon(square(2.0)), translate(0.0, 0.0, 3.0));
    let l = k.loft(&[a.clone(), b], true, 6).unwrap();
    assert_approx("ruled loft prism", volume(&mut k, &l), 12.0, 1e-5);
    // square to smaller square: frustum
    let small = vec![DVec2::new(0.5, 0.5), DVec2::new(1.5, 0.5), DVec2::new(1.5, 1.5), DVec2::new(0.5, 1.5)];
    let c = placed(Loop::Polygon(small), translate(0.0, 0.0, 3.0));
    let f = k.loft(&[a, c], true, 6).unwrap();
    assert_approx("ruled loft frustum", volume(&mut k, &f), 1.0 * (4.0 + 1.0 + 2.0), 1e-4);
    // circle to circle (smooth, 3 sections)
    let c1 = placed(Loop::Circle { center: DVec2::ZERO, r: 2.0 }, DMat4::IDENTITY);
    let c2 = placed(Loop::Circle { center: DVec2::ZERO, r: 1.0 }, translate(0.0, 0.0, 2.0));
    let c3 = placed(Loop::Circle { center: DVec2::ZERO, r: 2.0 }, translate(0.0, 0.0, 4.0));
    let s = k.loft(&[c1.clone(), c2.clone(), c3], false, 6).unwrap();
    let v = volume(&mut k, &s);
    assert!(v > PI_ * 4.0 && v < PI_ * 16.0, "smooth loft volume {v}");
    assert!(k.is_solid(&s));
    // ruled circle → circle = cone frustum
    let fr = k.loft(&[c1, c2], true, 6).unwrap();
    assert_approx("circle frustum", volume(&mut k, &fr), PI_ * 2.0 / 3.0 * (4.0 + 2.0 + 1.0), 1e-4);
}

#[test]
fn sweep_along_path() {
    let mut k = k();
    let circle = Loop::Circle { center: DVec2::ZERO, r: 0.5 };
    // straight path: cylinder
    let s = k
        .sweep(&circle, &[DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0)], PathInterp::Linear, 3)
        .unwrap();
    assert_approx("straight sweep", volume(&mut k, &s), PI_ * 0.25 * 10.0, 1e-4);
    // curved path through 5 points (spline)
    let path: Vec<DVec3> = (0..5)
        .map(|i| {
            let a = i as f64 * PI_ / 8.0;
            DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 0.0)
        })
        .collect();
    let c = k.sweep(&circle, &path, PathInterp::Spline, 3).unwrap();
    let (lo, hi) = k.bounds(&c).unwrap().unwrap();
    assert!((hi.x - 10.5).abs() < 1e-2 && (lo.z + 0.5).abs() < 1e-6, "{lo} {hi}");
    // pipe of a quarter circle (radius 10) ≈ pi r^2 * length
    let want = PI_ * 0.25 * (10.0 * PI_ / 2.0);
    assert_approx("curved sweep", volume(&mut k, &c), want, 2e-2);
    assert!(k.faces(&c, 1).unwrap().iter().all(|f| f.tag == 3));
    // square profile, linear path
    let sq = Loop::Polygon(vec![DVec2::new(-0.5, -0.5), DVec2::new(0.5, -0.5), DVec2::new(0.5, 0.5), DVec2::new(-0.5, 0.5)]);
    let s2 = k.sweep(&sq, &[DVec3::ZERO, DVec3::new(5.0, 0.0, 0.0)], PathInterp::Linear, 3).unwrap();
    assert_approx("square sweep", volume(&mut k, &s2), 5.0, 1e-4);
    // profile offset from its origin on a straight path: plain translation
    let off = Loop::Polygon(square(1.0));
    let s3 = k.sweep(&off, &[DVec3::ZERO, DVec3::new(0.0, 0.0, 5.0)], PathInterp::Linear, 3).unwrap();
    let (lo, hi) = k.bounds(&s3).unwrap().unwrap();
    assert!(lo.length() < 1e-6 && (hi - DVec3::new(1.0, 1.0, 5.0)).length() < 1e-6, "{lo} {hi}");
    // planar polyline path (mitered L): volume between inner and outer length
    let l_path = [DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 10.0, 0.0)];
    let s4 = k.sweep(&sq, &l_path, PathInterp::Linear, 3).unwrap();
    assert!(k.is_solid(&s4));
    assert_approx("mitered L sweep", volume(&mut k, &s4), 20.0, 1e-3);
    // non-planar polyline: unsupported, spline works
    let np = [DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 10.0, 0.0), DVec3::new(10.0, 10.0, 10.0)];
    assert!(matches!(k.sweep(&sq, &np, PathInterp::Linear, 3), Err(KernelError::Unsupported(_))));
    assert!(k.sweep(&circle, &np, PathInterp::Spline, 3).is_ok());
}

#[test]
fn shell_box() {
    let mut k = k();
    let b = cube(&mut k, 10.0, T_BOX);
    let faces = k.faces(&b, 1).unwrap();
    let top = faces.iter().find(|f| f.points.iter().all(|p| (p.z - 10.0).abs() < 1e-9)).unwrap().id;
    let s = k.shell(&b, 1.0, &[top], T_OP).unwrap();
    // open box: outer 10x10x10 minus inner 8x8x9
    assert_approx("open shell", volume(&mut k, &s), 1000.0 - 8.0 * 8.0 * 9.0, 1e-4);
    let fs = k.faces(&s, 1).unwrap();
    assert_eq!(fs.iter().filter(|f| f.tag == T_BOX).count(), 5, "outer faces keep their tag");
    assert!(fs.iter().any(|f| f.tag == T_OP), "inner walls get the shell tag");
    // closed shell (hollow)
    let h = k.shell(&b, 1.0, &[], T_OP).unwrap();
    assert_approx("hollow box", volume(&mut k, &h), 1000.0 - 512.0, 1e-4);
    let c = k.classify(&h, &[DVec3::splat(5.0), DVec3::splat(0.5)]).unwrap();
    assert_eq!(c, vec![Class::Outside, Class::Inside]);
}

#[test]
fn nurbs_surface_and_thicken() {
    let mut k = k();
    let rows: Vec<Vec<DVec3>> = (0..4)
        .map(|i| (0..4).map(|j| DVec3::new(i as f64 * 2.0, j as f64 * 2.0, ((i + j) as f64 * 0.7).sin() * 0.5)).collect())
        .collect();
    let s = k.nurbs_surface(&rows, 4).unwrap();
    assert!(!k.is_solid(&s));
    assert!(!k.is_empty(&s));
    assert_eq!(k.faces(&s, 1).unwrap().len(), 1);
    let mp = k.mass_props(&s).unwrap();
    assert!(mp.area > 36.0 && mp.area < 40.0, "surface area {}", mp.area);
    let m = k.tessellate(&s, 0.01).unwrap();
    assert!(m.triangle_count() > 0);
    match k.thicken(&s, 0.5, 9) {
        Ok(t) => {
            assert!(k.is_solid(&t));
            assert_approx("thickened volume", volume(&mut k, &t), mp.area * 0.5, 0.05);
            let tags: BTreeSet<Tag> = k.faces(&t, 1).unwrap().iter().map(|f| f.tag).collect();
            assert!(tags.contains(&4) && tags.contains(&9), "{tags:?}");
        }
        Err(e) => panic!("thicken failed: {e}"),
    }
}

#[test]
fn handles_are_released() {
    let mut k = k();
    // other tests run in parallel: measure only our own allocations with a margin
    let before = k.live_handles();
    {
        let d = box_minus_cylinder(&mut k);
        let _f = k.fillet(&d, &[0], 0.5, 9).unwrap();
        let _c = d.clone();
    }
    let after = k.live_handles();
    assert!(after <= before + 8, "handles leak: {before} -> {after}");
}
