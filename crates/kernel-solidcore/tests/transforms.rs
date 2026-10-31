//! Transforms of curved solids followed by a Boolean (tester finding on task C): every primitive
//! under rotations, mirrors, uniform scales and their products, cut by a half-space. The two
//! halves add up to the exact volume of the transformed solid (a Boolean that misses the
//! intersection returns the whole body or none).

#![cfg(feature = "solidcore")]

use cad_kernel::{BoolOp, Kernel, KernelError, Loop, PlacedLoop};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::{Shape, SolidcoreKernel};
use std::f64::consts::{PI, TAU};

fn mirror(n: DVec3) -> DMat4 {
    let n = n.normalize();
    DMat4::from_mat3(glam::DMat3::IDENTITY - 2.0 * glam::DMat3::from_cols(n * n.x, n * n.y, n * n.z))
}

/// The primitives of the tester's matrix with their exact volumes.
fn primitives(k: &mut SolidcoreKernel) -> Vec<(&'static str, Shape, f64)> {
    let xz = DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2);
    let circle = PlacedLoop { lp: Loop::Circle { center: DVec2::new(6.0, 0.0), r: 2.0 }, frame: xz };
    let poly = vec![DVec2::new(2.0, -4.0), DVec2::new(6.0, -3.0), DVec2::new(5.0, 4.0), DVec2::new(2.0, 3.0)];
    // Pappus: 2π · area · centroid x (shoelace)
    let (mut a, mut cx) = (0.0, 0.0);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = p.x * q.y - q.x * p.y;
        a += c / 2.0;
        cx += (p.x + q.x) * c / 6.0;
    }
    let down = DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0));
    let mut out = vec![
        ("torus", k.make_torus(6.0, 2.0, 1).unwrap(), 2.0 * PI * PI * 6.0 * 4.0),
        ("revolved circle", k.revolve(&circle, TAU, 1).unwrap(), 2.0 * PI * PI * 6.0 * 4.0),
        ("sphere", k.make_sphere(5.0, 1).unwrap(), 4.0 / 3.0 * PI * 125.0),
        ("revolved polygon", k.revolve(&PlacedLoop { lp: Loop::Polygon(poly), frame: xz }, TAU, 1).unwrap(), TAU * cx),
    ];
    let c = k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap();
    out.push(("cylinder", k.transform(&c, &down).unwrap(), PI * 90.0));
    let c = k.make_cylinder(10.0, 4.0, 1.0, 1).unwrap();
    out.push(("cone", k.transform(&c, &down).unwrap(), PI * 10.0 / 3.0 * (16.0 + 4.0 + 1.0)));
    out.push(("box", k.make_box(DVec3::new(-3.0, -4.0, -5.0), DVec3::new(3.0, 4.0, 5.0), 1).unwrap(), 480.0));
    let _ = a;
    out
}

#[test]
fn transformed_solids_cut_by_a_half_space() {
    let mut k = SolidcoreKernel::new();
    let rot = DMat4::from_euler(glam::EulerRot::ZYX, 50f64.to_radians(), 40f64.to_radians(), 30f64.to_radians());
    let sc = DMat4::from_scale(DVec3::splat(1.7));
    let transforms = [
        ("rot", rot),
        ("mirx", mirror(DVec3::X)),
        ("mirz", mirror(DVec3::Z)),
        ("mirrot", mirror(DVec3::new(1.0, 1.0, 0.0)) * rot),
        ("scrot", sc * rot),
        ("rotsc", rot * sc),
        ("neg", DMat4::from_scale(DVec3::splat(-1.0)) * rot),
        ("sc", DMat4::from_scale(DVec3::splat(0.3))),
    ];
    let mut failures = Vec::new();
    for (pname, p, exact) in primitives(&mut k) {
        for (tname, m) in &transforms {
            // as the language does: one transform node after the other
            let t = k.transform(&p, m).unwrap();
            let h = k.make_box(DVec3::new(-50.0, -50.0, 0.37), DVec3::new(50.0, 50.0, 100.0), 2).unwrap();
            let want = exact * m.determinant().abs();
            let mut vol = |op| -> Result<f64, KernelError> {
                let r = k.boolean(op, &[t.clone(), h.clone()], 3)?;
                Ok(if k.is_empty(&r) { 0.0 } else { k.mass_props(&r)?.volume })
            };
            match (vol(BoolOp::Intersection), vol(BoolOp::Difference)) {
                (Ok(a), Ok(b)) => {
                    if a <= 0.0 || b <= 0.0 || (a + b - want).abs() > 1e-5 * want {
                        failures.push(format!("{pname} {tname}: {a} + {b} != {want}"));
                    }
                }
                // a loud refusal is no wrong result
                (Err(KernelError::Unsupported(e)), _) | (_, Err(KernelError::Unsupported(e))) => {
                    println!("{pname} {tname}: unsupported: {e}")
                }
                (a, b) => failures.push(format!("{pname} {tname}: {a:?} {b:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
