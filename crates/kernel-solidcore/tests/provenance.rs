//! Face tags (provenance) and edge information over solidcore (plan 1.9, 1.10): tags as the
//! SMLib facade carries them (`smlib/facade/sc_facade.cpp`, "Provenance"), `smooth` edges as
//! SMLib's `GetBlendEdgeuse`.

#![cfg(feature = "solidcore")]

use cad_kernel::{BoolOp, Kernel, Loop, PlacedLoop, Segment, Tag};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::{Shape, SolidcoreKernel};
use std::f64::consts::{FRAC_PI_2, PI};

/// The face tags of `s`, sorted.
fn tags(k: &mut SolidcoreKernel, s: &Shape) -> Vec<Tag> {
    let mut t: Vec<Tag> = k.faces(s, 0).unwrap().iter().map(|f| f.tag).collect();
    t.sort_unstable();
    t
}

#[test]
fn tags_survive_transform_and_booleans() {
    let mut k = SolidcoreKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let b = k.make_box(DVec3::splat(5.0), DVec3::splat(15.0), 2).unwrap();
    assert_eq!(tags(&mut k, &a), vec![1; 6]);
    let moved = k.transform(&b, &DMat4::from_translation(DVec3::new(0.5, 0.0, 0.0))).unwrap();
    assert_eq!(tags(&mut k, &moved), vec![2; 6]);
    let u = k.boolean(BoolOp::Union, &[a.clone(), b.clone()], 3).unwrap();
    assert_eq!(tags(&mut k, &u), [vec![1; 6], vec![2; 6]].concat());
    let d = k.boolean(BoolOp::Difference, &[a.clone(), b.clone()], 3).unwrap();
    assert_eq!(tags(&mut k, &d), [vec![1; 6], vec![2; 3]].concat());
    let i = k.boolean(BoolOp::Intersection, &[a.clone(), b.clone()], 3).unwrap();
    assert_eq!(tags(&mut k, &i), [vec![1; 3], vec![2; 3]].concat());
    // the edges where the boxes cross separate a face of each
    let new: Vec<_> =
        k.edges(&u, 0).unwrap().into_iter().filter(|e| e.face_tags == [1, 2] || e.face_tags == [2, 1]).collect();
    assert_eq!(new.len(), 6, "{new:?}");
}

#[test]
fn smooth_edges_are_seams_poles_and_tangent_edges() {
    let mut k = SolidcoreKernel::new();
    let c = k.make_cylinder(10.0, 5.0, 5.0, 1).unwrap();
    let e = k.edges(&c, 1).unwrap();
    // two rims to blend, the seam not
    let sharp = e.iter().filter(|e| !e.smooth).count();
    assert_eq!(sharp, 2, "{e:?}");
    assert!(e.iter().all(|e| e.face_tags.len() == 2), "{e:?}");
    let s = k.make_sphere(5.0, 1).unwrap();
    let e = k.edges(&s, 1).unwrap();
    assert!(e.iter().all(|e| e.smooth), "{e:?}");
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    assert!(k.edges(&b, 0).unwrap().iter().all(|e| !e.smooth));
}

#[test]
fn tangent_edges_of_a_rounded_profile_are_smooth() {
    let mut k = SolidcoreKernel::new();
    let line = |a: [f64; 2], b: [f64; 2]| Segment::Line { a: DVec2::from(a), b: DVec2::from(b) };
    let arc = |c: [f64; 2], a0: f64, a1: f64| Segment::Arc { center: DVec2::from(c), r: 2.0, a0, a1 };
    let segs = vec![
        line([2.0, 0.0], [18.0, 0.0]),
        arc([18.0, 2.0], -FRAC_PI_2, 0.0),
        line([20.0, 2.0], [20.0, 8.0]),
        arc([18.0, 8.0], 0.0, FRAC_PI_2),
        line([18.0, 10.0], [2.0, 10.0]),
        arc([2.0, 8.0], FRAC_PI_2, PI),
        line([0.0, 8.0], [0.0, 2.0]),
        arc([2.0, 2.0], PI, 1.5 * PI),
    ];
    let p = PlacedLoop { lp: Loop::Composite(segs), frame: DMat4::IDENTITY };
    let s = k.extrude(&p, 5.0, 0.0, DVec2::ONE, 7).unwrap();
    let e = k.edges(&s, 1).unwrap();
    assert_eq!(e.len(), 24);
    // the 8 vertical joints of lines and arcs are tangent; the 16 rims are sharp
    let vertical = |e: &cad_kernel::EdgeInfo| (e.points[0].z - e.points.last().unwrap().z).abs() > 1.0;
    assert!(e.iter().all(|e| e.smooth == vertical(e)), "{e:?}");
    assert!(e.iter().all(|e| e.face_tags == [7, 7]), "{e:?}");
    assert_eq!(tags(&mut k, &s), vec![7; 10]);
}

#[test]
fn tags_of_new_bodies_and_split_faces() {
    let mut k = SolidcoreKernel::new();
    let sq = PlacedLoop {
        lp: Loop::Polygon(vec![DVec2::ZERO, DVec2::new(4.0, 0.0), DVec2::new(4.0, 4.0), DVec2::new(0.0, 4.0)]),
        frame: DMat4::IDENTITY,
    };
    let ring = PlacedLoop {
        lp: Loop::Circle { center: DVec2::new(5.0, 0.0), r: 1.0 },
        frame: DMat4::from_rotation_x(FRAC_PI_2),
    };
    let e = k.extrude(&sq, 3.0, 0.0, DVec2::ONE, 4).unwrap();
    assert_eq!(tags(&mut k, &e), vec![4; 6]);
    let t = k.extrude(&sq, 3.0, 0.5, DVec2::ONE, 5).unwrap();
    assert!(tags(&mut k, &t).iter().all(|&x| x == 5));
    let r = k.revolve(&ring, PI, 6).unwrap();
    assert!(tags(&mut k, &r).iter().all(|&x| x == 6));
    let lifted = PlacedLoop { frame: DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0)), ..sq.clone() };
    let l = k.loft(&[sq.clone(), lifted], false, 7).unwrap();
    assert!(tags(&mut k, &l).iter().all(|&x| x == 7));
    let sw = k.sweep(&sq.lp, &[DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0)], cad_kernel::PathInterp::Linear, 8).unwrap();
    assert!(tags(&mut k, &sw).iter().all(|&x| x == 8));
    // a bar through a plate: the plate's top, bottom and the two sides it crosses are split in
    // two pieces each, all keep tag 1 (10 faces); the bar's 6 faces keep tag 2 (two of them
    // with a hole now)
    let plate = k.make_box(DVec3::ZERO, DVec3::new(20.0, 20.0, 2.0), 1).unwrap();
    let bar = k.make_box(DVec3::new(-5.0, 8.0, -3.0), DVec3::new(25.0, 12.0, 5.0), 2).unwrap();
    let u = k.boolean(BoolOp::Union, &[plate.clone(), bar], 3).unwrap();
    assert_eq!(tags(&mut k, &u), [vec![1; 10], vec![2; 6]].concat());
    // a fillet on the new edges: the fillet faces are new (tag 9), the rest keep theirs
    let es = k.edges(&u, 0).unwrap();
    let new: Vec<u32> = es
        .iter()
        .filter(|e| !e.smooth && e.face_tags.len() == 2 && e.face_tags[0] != e.face_tags[1])
        .map(|e| e.id)
        .collect();
    // the 8 concave edges where the plate meets the bar: 4 along it, 4 at the plate's sides
    assert_eq!(new.len(), 8, "{es:?}");
    let f = k.fillet(&u, &new, 0.5, 9).unwrap();
    let tf = tags(&mut k, &f);
    assert_eq!(tf.iter().filter(|&&x| x == 1).count(), 10, "{tf:?}");
    assert_eq!(tf.iter().filter(|&&x| x == 2).count(), 6, "{tf:?}");
    assert!(tf.iter().filter(|&&x| x == 9).count() >= 8, "{tf:?}");
    // 4 fillets along the bar of length 20, 4 vertical ones of length 2 (concave: material
    // added), corners aside
    let v0 = k.mass_props(&u).unwrap().volume;
    let v1 = k.mass_props(&f).unwrap().volume;
    let add = (1.0 - PI / 4.0) * 0.25 * (4.0 * 20.0 + 4.0 * 2.0);
    assert!((v1 - v0 - add).abs() < 0.05 * add, "{v0} {v1} {add}");
}
