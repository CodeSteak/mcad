//! Edge cases, one per process: `cargo run --example stress -- <case>`.
use cad_kernel::*;
use glam::{DMat4, DVec2, DVec3};
use kernel_smlib::SmlibKernel;
use std::f64::consts::PI;

fn report<T>(k: &mut SmlibKernel, name: &str, r: KResult<T>, f: impl FnOnce(&mut SmlibKernel, T) -> String) {
    match r {
        Ok(v) => {
            let s = f(k, v);
            println!("{name}: ok {s}");
        }
        Err(e) => println!("{name}: err {e:?}"),
    }
}

fn vol(k: &mut SmlibKernel, s: &kernel_smlib::Shape) -> String {
    format!("vol {:?} solid {} faces {}", k.mass_props(s).map(|m| m.volume), k.is_solid(s), k.face_count(s))
}

fn main() {
    let case = std::env::args().nth(1).unwrap_or_default();
    let mut k = SmlibKernel::new();
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let all: Vec<u32> = (0..12).collect();
    match case.as_str() {
        "fillet_half" => {
            let r = k.fillet(&b, &all, 5.0, 2);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "fillet_4_9" => {
            let r = k.fillet(&b, &all, 4.9, 2);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "chamfer_big" => {
            let r = k.chamfer(&b, &all, 6.0, 2);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "union_touching" => {
            let c = k.make_box(DVec3::new(10.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 10.0), 2).unwrap();
            let r = k.boolean(BoolOp::Union, &[b.clone(), c], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "union_coplanar" => {
            let c = k.make_box(DVec3::new(5.0, 5.0, 0.0), DVec3::new(15.0, 15.0, 10.0), 2).unwrap();
            let r = k.boolean(BoolOp::Union, &[b.clone(), c], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "diff_coplanar_top" => {
            let c = k.make_box(DVec3::new(2.0, 2.0, 5.0), DVec3::new(8.0, 8.0, 10.0), 2).unwrap();
            let r = k.boolean(BoolOp::Difference, &[b.clone(), c], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "diff_all" => {
            let c = k.make_box(DVec3::splat(-1.0), DVec3::splat(11.0), 2).unwrap();
            let r = k.boolean(BoolOp::Difference, &[b.clone(), c], 3);
            report(&mut k, &case, r, |k, s| format!("empty {}", k.is_empty(&s)));
        }
        "spheres_tangent" => {
            let s1 = k.make_sphere(1.0, 1).unwrap();
            let s2 = k.make_sphere(1.0, 2).unwrap();
            let s2 = k.transform(&s2, &DMat4::from_translation(DVec3::new(2.0, 0.0, 0.0))).unwrap();
            let r = k.boolean(BoolOp::Union, &[s1, s2], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "cyl_coaxial_same_r" => {
            let c1 = k.make_cylinder(10.0, 2.0, 2.0, 1).unwrap();
            let c2 = k.make_cylinder(10.0, 2.0, 2.0, 2).unwrap();
            let c2 = k.transform(&c2, &DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0))).unwrap();
            let r = k.boolean(BoolOp::Union, &[c1, c2], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "sphere_minus_box_corner" => {
            let s = k.make_sphere(5.0, 1).unwrap();
            let r = k.boolean(BoolOp::Difference, &[s, b.clone()], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "shell_too_thick" => {
            let r = k.shell(&b, 6.0, &[0], 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "fillet_cyl_rim" => {
            let c = k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap();
            let es: Vec<u32> = (0..k.edge_count(&c) as u32).collect();
            let r = k.fillet(&c, &es, 1.0, 2);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "fillet_sphere_box" => {
            let s = k.make_sphere(6.0, 2).unwrap();
            let s = k.transform(&s, &DMat4::from_translation(DVec3::splat(5.0))).unwrap();
            let i = k.boolean(BoolOp::Intersection, &[b.clone(), s], 3).unwrap();
            let es: Vec<u32> = (0..k.edge_count(&i) as u32).collect();
            let r = k.fillet(&i, &es, 0.5, 4);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "revolve_cross_axis" => {
            let rect = vec![DVec2::new(-1.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0), DVec2::new(-1.0, 1.0)];
            let p = PlacedLoop { lp: Loop::Polygon(rect), frame: DMat4::from_rotation_x(PI / 2.0) };
            let r = k.revolve(&p, 2.0 * PI, 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "revolve_touch_axis" => {
            let rect = vec![DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0), DVec2::new(0.0, 1.0)];
            let p = PlacedLoop { lp: Loop::Polygon(rect), frame: DMat4::from_rotation_x(PI / 2.0) };
            let r = k.revolve(&p, 2.0 * PI, 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "self_intersecting_polygon" => {
            let bow = vec![DVec2::new(0.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(2.0, 0.0), DVec2::new(0.0, 2.0)];
            let p = PlacedLoop { lp: Loop::Polygon(bow), frame: DMat4::IDENTITY };
            let r = k.extrude(&p, 1.0, 0.0, DVec2::ONE, 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
        }
        "twist" => {
            let sq = vec![DVec2::new(-1.0, -1.0), DVec2::new(1.0, -1.0), DVec2::new(1.0, 1.0), DVec2::new(-1.0, 1.0)];
            let p = PlacedLoop { lp: Loop::Polygon(sq), frame: DMat4::IDENTITY };
            let r = k.extrude(&p, 4.0, PI / 2.0, DVec2::ONE, 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
            let r = k.extrude(&p, 4.0, PI, DVec2::splat(0.5), 3);
            report(&mut k, "twist+scale", r, |k, s| vol(k, &s));
            let r = k.extrude(&p, 3.0, 0.0, DVec2::splat(0.5), 3);
            report(&mut k, "scale", r, |k, s| vol(k, &s));
            let c = PlacedLoop { lp: Loop::Circle { center: DVec2::ZERO, r: 1.0 }, frame: DMat4::IDENTITY };
            let r = k.extrude(&c, 3.0, 0.0, DVec2::splat(0.5), 3);
            report(&mut k, "scale circle", r, |k, s| vol(k, &s));
        }
        "loft_square_circle" => {
            let a = PlacedLoop {
                lp: Loop::Polygon(vec![
                    DVec2::new(-1.0, -1.0),
                    DVec2::new(1.0, -1.0),
                    DVec2::new(1.0, 1.0),
                    DVec2::new(-1.0, 1.0),
                ]),
                frame: DMat4::IDENTITY,
            };
            let c = PlacedLoop {
                lp: Loop::Circle { center: DVec2::ZERO, r: 1.0 },
                frame: DMat4::from_translation(DVec3::new(0.0, 0.0, 3.0)),
            };
            let r = k.loft(&[a.clone(), c.clone()], true, 3);
            report(&mut k, &case, r, |k, s| vol(k, &s));
            let r = k.loft(&[a, c], false, 3);
            report(&mut k, "loft_square_circle smooth", r, |k, s| vol(k, &s));
        }
        "shell_revolved" => {
            let prof = vec![DVec2::new(2.0, 0.0), DVec2::new(4.0, 0.0), DVec2::new(3.0, 3.0), DVec2::new(2.0, 3.0)];
            let p = PlacedLoop { lp: Loop::Spline(prof), frame: DMat4::from_rotation_x(PI / 2.0) };
            let r = k.revolve(&p, 2.0 * PI, 3).unwrap();
            let t0 = std::time::Instant::now();
            let s = k.shell(&r, 0.2, &[], 4);
            report(&mut k, &case, s, |k, s| vol(k, &s));
            println!("  took {:?}", t0.elapsed());
        }
        "shell_sphere_cyl" => {
            let s = k.make_sphere(5.0, 1).unwrap();
            let r = k.shell(&s, 0.5, &[], 4);
            report(&mut k, "shell sphere", r, |k, s| vol(k, &s));
            let c = k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap();
            let r = k.shell(&c, 0.5, &[1], 4);
            report(&mut k, "shell cylinder open", r, |k, s| vol(k, &s));
        }
        "boolean_many" => {
            let t0 = std::time::Instant::now();
            let mut acc = k.make_box(DVec3::ZERO, DVec3::new(100.0, 100.0, 5.0), 1).unwrap();
            for i in 0..10 {
                for j in 0..10 {
                    let c = k.make_cylinder(7.0, 2.0, 2.0, 2).unwrap();
                    let c = k
                        .transform(
                            &c,
                            &DMat4::from_translation(DVec3::new(5.0 + 10.0 * i as f64, 5.0 + 10.0 * j as f64, -1.0)),
                        )
                        .unwrap();
                    acc = k.boolean(BoolOp::Difference, &[acc, c], 3).unwrap();
                }
            }
            println!("{case}: 100 holes in {:?}; {}", t0.elapsed(), vol(&mut k, &acc));
            let t1 = std::time::Instant::now();
            let m = k.tessellate(&acc, 0.05).unwrap();
            println!("  tessellate {} tris in {:?}", m.triangle_count(), t1.elapsed());
        }
        "tessellate_torus_fine" => {
            let t = k.make_torus(10.0, 1.0, 1).unwrap();
            let t0 = std::time::Instant::now();
            let m = k.tessellate(&t, 0.001).unwrap();
            println!("{case}: {} tris in {:?}", m.triangle_count(), t0.elapsed());
        }
        _ => println!("unknown case {case}"),
    }
}
