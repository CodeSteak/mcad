//! What solidcore can do today: every kernel operation either works or reports
//! `Unsupported("solidcore: …")` naming what blocks it; nothing panics. With `--nocapture`
//! the test prints the answer of every operation (`cargo test -p kernel-solidcore --features
//! solidcore -- --nocapture`).

use cad_kernel::{BoolOp, Kernel, KernelError, Loop, PlacedLoop};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::SolidcoreKernel;

fn show<T>(name: &str, r: &Result<T, KernelError>) {
    match r {
        Ok(_) => println!("{name}: ok"),
        Err(e) => println!("{name}: {e}"),
    }
}

/// Works, or fails with a message of this crate (`solidcore: <op>…`).
fn answers<T>(name: &str, r: &Result<T, KernelError>) -> bool {
    show(name, r);
    match r {
        Ok(_) => true,
        Err(KernelError::Unsupported(m)) => m.starts_with("solidcore"),
        Err(KernelError::Failed { msg, .. }) | Err(KernelError::InvalidInput(msg)) => msg.starts_with("solidcore"),
    }
}

#[cfg(feature = "solidcore")]
fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).abs().max_element() < 1e-6
}

#[test]
fn every_operation_answers_without_panicking() {
    let mut k = SolidcoreKernel::new();
    let bx = k.make_box(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0), 1);
    assert!(answers("make_box", &bx));
    let square = PlacedLoop {
        lp: Loop::Polygon(vec![DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0), DVec2::new(0.0, 1.0)]),
        frame: DMat4::IDENTITY,
    };
    let circle = PlacedLoop { lp: Loop::Circle { center: DVec2::new(3.0, 0.0), r: 1.0 }, frame: DMat4::IDENTITY };
    let spline = PlacedLoop {
        lp: Loop::Spline(vec![DVec2::new(1.0, 0.0), DVec2::new(2.0, 1.0), DVec2::new(1.0, 2.0)]),
        frame: DMat4::IDENTITY,
    };
    let lifted = PlacedLoop { frame: DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0)), ..square.clone() };
    for (n, r) in [
        ("make_sphere", k.make_sphere(2.0, 1)),
        ("make_cylinder", k.make_cylinder(3.0, 1.0, 1.0, 1)),
        ("make_cylinder (cone)", k.make_cylinder(3.0, 1.0, 0.5, 1)),
        ("make_cylinder (tip down)", k.make_cylinder(3.0, 0.0, 0.5, 1)),
        ("make_torus", k.make_torus(3.0, 1.0, 1)),
        (
            "make_polyhedron",
            k.make_polyhedron(
                &[DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::Z],
                &[vec![0, 1, 2], vec![0, 1, 3], vec![1, 2, 3], vec![0, 2, 3]],
                1,
            ),
        ),
        ("extrude (polygon)", k.extrude(&square, 2.0, 0.0, DVec2::ONE, 1)),
        ("extrude (polygon, placed)", k.extrude(&lifted, 2.0, 0.0, DVec2::ONE, 1)),
        ("extrude (circle)", k.extrude(&circle, 2.0, 0.0, DVec2::ONE, 1)),
        ("extrude (spline)", k.extrude(&spline, 2.0, 0.0, DVec2::ONE, 1)),
        ("extrude (twist)", k.extrude(&square, 2.0, 1.0, DVec2::ONE, 1)),
        ("revolve (circle)", k.revolve(&circle, std::f64::consts::TAU, 1)),
        ("loft", k.loft(&[square.clone(), lifted.clone()], false, 1)),
    ] {
        assert!(answers(n, &r), "{n}");
        if let Ok(s) = &r {
            println!("  mass_props: {:?}", k.mass_props(s));
        }
    }
    let shear =
        DMat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    // without the feature even the box is unsupported
    let Ok(b) = bx else { return };
    for (n, ok) in [
        ("transform (identity)", answers("transform (identity)", &k.transform(&b, &DMat4::IDENTITY))),
        ("transform (shear)", answers("transform (shear)", &k.transform(&b, &shear))),
        ("boolean", answers("boolean", &k.boolean(BoolOp::Union, &[b.clone(), b.clone()], 2))),
        ("faces", answers("faces", &k.faces(&b, 1))),
        ("edges", answers("edges", &k.edges(&b, 1))),
        ("mass_props", answers("mass_props", &k.mass_props(&b))),
        ("bounds", answers("bounds", &k.bounds(&b))),
        ("classify", answers("classify", &k.classify(&b, &[DVec3::new(0.5, 1.0, 1.5)]))),
        ("tessellate", answers("tessellate", &k.tessellate(&b, 0.1))),
    ] {
        assert!(ok, "{n}");
    }
    // shear has no counterpart in solidcore
    assert!(matches!(k.transform(&b, &shear), Err(KernelError::Unsupported(_))));
}

/// What already works agrees with the SMLib results for the same input.
#[cfg(feature = "solidcore")]
#[test]
fn box_queries_and_transform() {
    use cad_kernel::Class;
    let mut k = SolidcoreKernel::new();
    let b = k.make_box(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0), 1).unwrap();
    let m = k.mass_props(&b).unwrap();
    assert!((m.volume - 6.0).abs() < 1e-9, "{m:?}");
    assert!((m.area - 22.0).abs() < 1e-9, "{m:?}");
    assert!(close(m.centroid, DVec3::new(0.5, 1.0, 1.5)), "{m:?}");
    let (lo, hi) = k.bounds(&b).unwrap().unwrap();
    assert!(close(lo, DVec3::ZERO) && close(hi, DVec3::new(1.0, 2.0, 3.0)), "{lo} {hi}");
    assert!(k.is_solid(&b));
    assert!(!k.is_empty(&b));

    // translate, rotate 90° about z, mirror in x, scale
    let cases = [
        (DMat4::from_translation(DVec3::new(10.0, 0.0, -1.0)), DVec3::new(10.0, 0.0, -1.0), DVec3::new(11.0, 2.0, 2.0)),
        (DMat4::from_rotation_z(std::f64::consts::FRAC_PI_2), DVec3::new(-2.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 3.0)),
        (DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)), DVec3::new(-1.0, 0.0, 0.0), DVec3::new(0.0, 2.0, 3.0)),
        (DMat4::from_scale(DVec3::new(2.0, 1.0, 0.5)), DVec3::ZERO, DVec3::new(2.0, 2.0, 1.5)),
    ];
    for (mat, lo_want, hi_want) in cases {
        match k.transform(&b, &mat) {
            Ok(t) => {
                let (lo, hi) = k.bounds(&t).unwrap().unwrap();
                assert!(close(lo, lo_want) && close(hi, hi_want), "{mat}: {lo} {hi}");
                let v = k.mass_props(&t).unwrap().volume;
                assert!((v - 6.0 * mat.determinant().abs()).abs() < 1e-6, "{mat}: volume {v}");
            }
            Err(e) => {
                println!("transform {mat}: {e}");
                assert!(matches!(e, KernelError::Unsupported(_)), "{e}");
            }
        }
    }

    match k.classify(&b, &[DVec3::new(0.5, 1.0, 1.5), DVec3::new(5.0, 1.0, 1.5), DVec3::new(1.0, 1.0, 1.5)]) {
        Ok(c) => assert_eq!(c, vec![Class::Inside, Class::Outside, Class::On]),
        Err(e) => {
            println!("classify: {e}");
            assert!(matches!(e, KernelError::Unsupported(_)), "{e}");
        }
    }
}

/// Queries work on the shape's own body (no copy per call): faces and edges keep their ids and
/// order across other queries, the shape answers after it was an operand, and the volume is
/// cached.
#[cfg(feature = "solidcore")]
#[test]
fn queries_in_place_are_stable() {
    let mut k = SolidcoreKernel::new();
    let c = k.make_cylinder(10.0, 5.0, 5.0, 1).unwrap();
    let s = k.transform(&c, &DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))).unwrap();
    let f0 = k.faces(&s, 0).unwrap();
    let e0 = k.edges(&s, 2).unwrap();
    let m0 = k.mass_props(&s).unwrap();
    k.classify(&s, &[DVec3::new(1.0, 2.0, 8.0)]).unwrap();
    let b = k.make_box(DVec3::ZERO, DVec3::splat(4.0), 2).unwrap();
    k.boolean(BoolOp::Difference, &[s.clone(), b], 3).unwrap();
    let f1 = k.faces(&s, 0).unwrap();
    let e1 = k.edges(&s, 2).unwrap();
    assert_eq!(f0.len(), f1.len());
    for (a, b) in f0.iter().zip(&f1) {
        assert_eq!(a.id, b.id);
        assert!(a.points.iter().zip(&b.points).all(|(p, q)| close(*p, *q)), "{a:?} {b:?}");
    }
    assert_eq!(e0.len(), e1.len());
    for (a, b) in e0.iter().zip(&e1) {
        assert_eq!(a.id, b.id);
        assert!(a.points.iter().zip(&b.points).all(|(p, q)| close(*p, *q)), "{a:?} {b:?}");
    }
    assert_eq!(k.mass_props(&s).unwrap(), m0);
    let exact = std::f64::consts::PI * 25.0 * 10.0;
    assert!((m0.volume - exact).abs() < 1e-5 * exact, "{}", m0.volume);
    let (copies, _) = k.copy_stats();
    // one for the transform, two for the boolean operands; none for the queries
    assert_eq!(copies, 3, "{copies}");
}
