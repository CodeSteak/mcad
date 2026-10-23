//! Shell of simple bodies at given wall thicknesses (for valgrind checks of SMLib's shell:
//! `valgrind target/release/examples/shell_fit box 4.99 5`).
//!
//!   cargo run --release -p kernel-smlib --example shell_fit -- [box|open|sphere|cyl] t...
//!
//! box: 10 mm cube; open: the cube with face 0 open; sphere: r = 5; cyl: r = 5, h = 20.
use cad_kernel::*;
use glam::DVec3;
use kernel_smlib::SmlibKernel;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let shape = if args.first().is_some_and(|a| a.parse::<f64>().is_err()) { args.remove(0) } else { "box".into() };
    let ts: Vec<f64> = args.iter().map(|a| a.parse().unwrap()).collect();
    let ts = if ts.is_empty() { vec![4.99] } else { ts };
    let mut k = SmlibKernel::new();
    for t in ts {
        let b = match shape.as_str() {
            "sphere" => k.make_sphere(5.0, 1).unwrap(),
            "cyl" => k.make_cylinder(20.0, 5.0, 5.0, 1).unwrap(),
            _ => k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap(),
        };
        let open: &[u32] = if shape == "open" { &[0] } else { &[] };
        match k.shell(&b, t, open, 2) {
            Ok(s) => {
                let mp = k.mass_props(&s).unwrap();
                println!("{shape} t={t}: ok solid={} vol={} faces={}", k.is_solid(&s), mp.volume, k.face_count(&s));
            }
            Err(e) => println!("{shape} t={t}: err {e}"),
        }
    }
}
