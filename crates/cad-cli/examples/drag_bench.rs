//! Native measurement of "drag → coarse scene" (Testbericht 3, B14): the module `hole_plate`
//! of `tests/models/spec/hole_plate.mcad` is evaluated in its module context, then its plate
//! thickness `t` is changed in steps while a drag is active (coarse tessellation), each change
//! as a new source text that is evaluated to idle (kernel job + coarse root mesh), as the UI
//! does it. Prints median/p95 per change and the kernel time per method.
//!
//!   cargo run --release -p cad-cli --example drag_bench [-- FILE MODULE PARAM FROM TO STEPS MODE]
//!
//! MODE: `draft` (default: drag active, slow fillets left out), `nodraft` (drag active, coarse
//! meshes, fillets always computed), `fine` (no drag: full evaluation, fine meshes).

use std::collections::BTreeMap;

use cad_engine::CountingKernel;
use cad_session::protocol::{FromEngine, ToEngine};
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let file = a.first().cloned().unwrap_or_else(|| "tests/models/spec/hole_plate.mcad".into());
    let module = a.get(1).cloned().unwrap_or_else(|| "hole_plate".into());
    let param = a.get(2).cloned().unwrap_or_else(|| "t".into());
    let from: f64 = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(4.0);
    let to: f64 = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(10.0);
    let steps: usize = a.get(5).map(|s| s.parse().unwrap()).unwrap_or(30);
    let mode = a.get(6).cloned().unwrap_or_else(|| "draft".into());
    let text = std::fs::read_to_string(&file).expect("read file");

    let clock = std_clock();
    let c2 = clock.clone();
    let kernel = CountingKernel::with_clock(SmlibKernel::new(), Box::new(move || c2()));
    let mut opts = SessionOptions::default();
    if mode == "nodraft" {
        opts.engine.drag_blend_budget_ms = None;
    }
    let mut s = Session::new(kernel, opts, clock.clone());
    let mut version = 1;
    s.post(ToEngine::SetSource { text: text.clone(), version });
    s.post(ToEngine::SetContext { module: Some(module.clone()), overrides: BTreeMap::new() });
    let t0 = clock();
    s.run_to_idle();
    println!("initial evaluation + fine mesh: {:.1} ms", clock() - t0);

    // drag: coarse tessellation (and draft evaluations) from now on
    if mode != "fine" {
        s.post(ToEngine::DragStart { handle: 0 });
    }
    s.engine_mut().kernel_mut().reset();
    let mut lat = Vec::new();
    let mut warnings = 0;
    let pat = format!("{param} = {}", fmt(from));
    assert!(text.contains(&pat), "`{pat}` not in the source");
    let mut prev = String::new();
    for i in 1..=steps {
        let v = from + (to - from) * i as f64 / steps as f64;
        let src = text.replacen(&pat, &format!("{param} = {}", fmt(v)), 1);
        if src == prev {
            continue;
        }
        prev = src.clone();
        version += 1;
        let t = clock();
        let mut msgs = s.post(ToEngine::SetSource { text: src, version });
        msgs.extend(s.run_to_idle());
        lat.push(clock() - t);
        for m in &msgs {
            if let FromEngine::Diagnostics { items, .. } = m {
                warnings = items.len();
            }
        }
    }
    let k = s.engine_mut().kernel_mut();
    let times = k.times.clone();
    let counts = k.counts.clone();
    let n = lat.len();
    let total: f64 = lat.iter().sum();
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "drag → coarse scene: n {n}, median {:.1} ms, p95 {:.1} ms, max {:.1} ms, mean {:.1} ms; diagnostics at the end: {warnings}",
        lat[n / 2],
        lat[((n as f64 * 0.95) as usize).min(n - 1)],
        lat[n - 1],
        total / n as f64
    );
    let mut tv: Vec<_> = times.iter().collect();
    tv.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    let ksum: f64 = times.values().sum();
    println!("kernel time per change {:.1} ms (rest: language evaluation, scene, engine)", ksum / n as f64);
    for (name, ms) in tv {
        println!(
            "  {name:14} {:8.1} ms per change  {:5} calls per change",
            ms / n as f64,
            counts[name] as f64 / n as f64
        );
    }
    // release: fine again
    let t = clock();
    s.post(ToEngine::DragEnd { handle: 0, target: None });
    s.run_to_idle();
    println!("drag end → fine mesh: {:.1} ms", clock() - t);
}

fn fmt(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
}
