//! `make_polyhedron` over solidcore (plan 1.7): one planar face per polygon as SMLib's
//! `sc_make_polyhedron`, oriented outward whatever the input orientation.

#![cfg(feature = "solidcore")]

use cad_kernel::{Kernel, KernelError};
use glam::DVec3;
use kernel_solidcore::SolidcoreKernel;

fn cube_points() -> Vec<DVec3> {
    (0..8).map(|i| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * 2.0).collect()
}

#[test]
fn tetrahedron_and_cube() {
    let mut k = SolidcoreKernel::new();
    let p = vec![DVec3::ZERO, DVec3::X * 3.0, DVec3::Y * 3.0, DVec3::Z * 3.0];
    // inward and outward faces mixed
    let f = vec![vec![0, 1, 2], vec![0, 1, 3], vec![1, 2, 3], vec![0, 3, 2]];
    let t = k.make_polyhedron(&p, &f, 4).unwrap();
    let m = k.mass_props(&t).unwrap();
    assert!((m.volume - 4.5).abs() < 1e-9, "{}", m.volume);
    assert_eq!((k.faces(&t, 0).unwrap().len(), k.edges(&t, 0).unwrap().len()), (4, 6));
    assert!(k.faces(&t, 0).unwrap().iter().all(|f| f.tag == 4));
    let b = k.brep(&t).unwrap();
    assert!(b.convention_problems(1e-9).is_empty(), "{:?}", b.convention_problems(1e-9));
    assert!(b.faces().all(|f| f.surface.kind() == "plane"));

    // a cube of quads, one face given twice as two triangles (no faces merged), all clockwise
    let c = cube_points();
    let f = vec![
        vec![0, 2, 3, 1],
        vec![4, 5, 7, 6],
        vec![0, 1, 5, 4],
        vec![2, 6, 7],
        vec![2, 7, 3],
        vec![0, 4, 6, 2],
        vec![1, 3, 7, 5],
    ];
    let s = k.make_polyhedron(&c, &f, 1).unwrap();
    assert!((k.mass_props(&s).unwrap().volume - 8.0).abs() < 1e-9);
    assert_eq!(k.faces(&s, 0).unwrap().len(), 7);
    // reversed input: the same solid
    let r: Vec<Vec<u32>> = f.iter().map(|q| q.iter().rev().copied().collect()).collect();
    let s = k.make_polyhedron(&c, &r, 1).unwrap();
    assert!((k.mass_props(&s).unwrap().volume - 8.0).abs() < 1e-9);
    // a Boolean with it works
    let bx = k.make_box(DVec3::splat(1.0), DVec3::splat(3.0), 2).unwrap();
    let u = k.boolean(cad_kernel::BoolOp::Union, &[s, bx], 3).unwrap();
    assert!((k.mass_props(&u).unwrap().volume - 15.0).abs() < 1e-9);
}

#[test]
fn bad_polyhedra() {
    let mut k = SolidcoreKernel::new();
    let c = cube_points();
    // a face missing: open
    let open = vec![vec![0, 2, 3, 1], vec![4, 5, 7, 6], vec![0, 1, 5, 4], vec![2, 6, 7, 3], vec![0, 4, 6, 2]];
    match k.make_polyhedron(&c, &open, 1) {
        Err(KernelError::Failed { msg, .. }) => assert!(msg.contains("closed manifold"), "{msg}"),
        other => panic!("{other:?}"),
    }
    // a twisted (non-planar) quad
    let mut p = c.clone();
    p[7].z += 0.5;
    let f = vec![
        vec![0, 2, 3, 1],
        vec![4, 5, 7, 6],
        vec![0, 1, 5, 4],
        vec![2, 6, 7, 3],
        vec![0, 4, 6, 2],
        vec![1, 3, 7, 5],
    ];
    assert!(matches!(k.make_polyhedron(&p, &f, 1), Err(KernelError::Unsupported(_))));
    // index out of range, too few faces
    assert!(matches!(
        k.make_polyhedron(&c, &[vec![0, 1, 9], vec![0, 1, 2], vec![1, 2, 3], vec![0, 2, 3]], 1),
        Err(KernelError::InvalidInput(_))
    ));
    assert!(matches!(k.make_polyhedron(&c, &f[..3], 1), Err(KernelError::InvalidInput(_))));
}
