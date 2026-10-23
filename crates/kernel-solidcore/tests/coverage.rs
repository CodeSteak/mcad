//! What solidcore can do today: every kernel operation either works or reports
//! `Unsupported("solidcore: …")`; nothing panics.

use cad_kernel::{BoolOp, Kernel, KernelError, Loop, PlacedLoop};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::SolidcoreKernel;

fn show<T>(name: &str, r: &Result<T, KernelError>) {
    match r {
        Ok(_) => println!("{name}: ok"),
        Err(e) => println!("{name}: {e}"),
    }
}

fn unsupported_or_ok<T>(r: &Result<T, KernelError>) -> bool {
    match r {
        Ok(_) => true,
        Err(KernelError::Unsupported(m)) => m.starts_with("solidcore"),
        Err(KernelError::Failed { msg, .. }) | Err(KernelError::InvalidInput(msg)) => msg.starts_with("solidcore"),
    }
}

#[test]
fn every_operation_answers_without_panicking() {
    let mut k = SolidcoreKernel::new();
    let bx = k.make_box(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0), 1);
    show("make_box", &bx);
    assert!(unsupported_or_ok(&bx));
    let sp = k.make_sphere(2.0, 1);
    show("make_sphere", &sp);
    assert!(unsupported_or_ok(&sp));
    for (n, r) in [
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
    ] {
        show(n, &r);
        assert!(unsupported_or_ok(&r));
    }
    let lp = PlacedLoop { lp: Loop::Circle { center: DVec2::ZERO, r: 1.0 }, frame: DMat4::IDENTITY };
    let r = k.extrude(&lp, 2.0, 0.0, DVec2::ONE, 1);
    assert!(matches!(r, Err(KernelError::Unsupported(_))));
    if let Ok(b) = &bx {
        for (n, ok) in [
            ("transform", unsupported_or_ok(&k.transform(b, &DMat4::IDENTITY))),
            ("boolean", unsupported_or_ok(&k.boolean(BoolOp::Union, &[b.clone(), b.clone()], 2))),
            ("faces", unsupported_or_ok(&k.faces(b, 1))),
            ("edges", unsupported_or_ok(&k.edges(b, 1))),
            ("mass_props", unsupported_or_ok(&k.mass_props(b))),
            ("bounds", unsupported_or_ok(&k.bounds(b))),
            ("tessellate", unsupported_or_ok(&k.tessellate(b, 0.1))),
        ] {
            println!("{n}: {ok}");
            assert!(ok, "{n}");
        }
    }
}
