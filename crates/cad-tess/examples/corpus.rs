//! Acceptance run of the tessellator over model files (plan 2.6): every root part of each file
//! is meshed from the kernel's neutral B-rep and checked (watertight, orientation, degenerate
//! triangles, volume against the kernel's exact volume within `tol · area`), next to the
//! kernel's own tessellation where it has one (SMLib).
//!
//! ```text
//! cargo run --release -p cad-tess --example corpus [--features solidcore] -- \
//!     [--kernel smlib|solidcore|both] [--tol 0.05] [--json out.json] [--dump] [files…]
//! ```
//! Without files: the corpus of `diff-kernels` (`scripts/ci/solidcore.sh`). `--dump` prints the
//! neutral B-reps as JSON instead.

use std::path::{Path, PathBuf};
use std::time::Instant;

use cad_kernel::Kernel;
use cad_session::protocol::ToEngine;
use cad_session::{Session, SessionOptions, std_clock};
use serde_json::{Value, json};

fn corpus(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for d in ["tests/models/openscad-compat", "tests/models/spec", "tests/models/everyday", "tests/everyday/bugs"] {
        let mut v: Vec<PathBuf> = Vec::new();
        for p in std::fs::read_dir(root.join(d)).unwrap().map(|e| e.unwrap().path()) {
            if p.is_dir() {
                let main = p.join(p.file_name().unwrap()).with_extension("mcad");
                if main.is_file() {
                    v.push(main);
                }
            } else if cad_eval_like(&p) {
                v.push(p);
            }
        }
        v.sort();
        files.extend(v);
    }
    files.push(root.join("tests/models/project-housing/housing.mcad"));
    files
}

fn cad_eval_like(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("mcad" | "scad"))
}

/// Evaluates `file` and runs `f` on the kernel with each root part (and whether it is solid).
fn with_parts<K: Kernel + 'static, R>(k: K, file: &Path, mut f: impl FnMut(&mut K, &K::Shape, bool) -> R) -> Vec<R>
where
    K::Shape: 'static,
{
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    let mut s = Session::new(k, opts, std_clock());
    let root = file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let main = file.file_name().unwrap().to_string_lossy().into_owned();
    s.set_resolver(Box::new(cad_session::FsResolver { root: root.to_path_buf() }), &main);
    s.post(ToEngine::SetSource { text, version: 1 });
    s.run_to_idle();
    let Some(d) = s.done() else { return Vec::new() };
    let Some(r) = d.ev.root() else { return Vec::new() };
    let parts: Vec<_> = d.ev.parts(r).iter().map(|p| (p.shape.clone(), p.solid)).collect();
    let k = s.engine_mut().kernel_mut();
    parts.iter().map(|(p, solid)| f(k, p, *solid)).collect()
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// One part: our mesh, the checks, the kernel's own mesh.
fn part<K: Kernel>(k: &mut K, s: &K::Shape, solid: bool, tol: f64, own: bool) -> Value {
    let t0 = Instant::now();
    let brep = match k.brep(s) {
        Ok(b) => b,
        Err(e) => return json!({ "error": format!("brep: {e}") }),
    };
    let brep_ms = ms(t0);
    let t1 = Instant::now();
    let t = match cad_tess::tessellate_with(&brep, &cad_tess::Options::new(tol)) {
        Ok(t) => t,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let tess_ms = ms(t1);
    let c = cad_tess::check(&t.mesh);
    let exact = if solid { k.mass_props(s).ok().map(|m| m.volume) } else { None };
    let mut v = json!({
        "faces": t.stats.faces,
        "failed_faces": t.stats.failed_faces.iter().map(|(id, w)| format!("{id}: {w}")).collect::<Vec<_>>(),
        "lost_constraints": t.stats.lost_constraints,
        "refined": t.stats.refined,
        "max_deviation": t.stats.max_deviation,
        "triangles": c.triangles,
        "watertight": c.watertight(),
        "boundary_edges": c.boundary_edges,
        "misoriented_edges": c.misoriented_edges,
        "nonmanifold_edges": c.nonmanifold_edges,
        "degenerate": c.degenerate,
        "volume": c.volume,
        "exact_volume": exact,
        // |V_mesh − V_exact| / (tol · area): within 1 passes
        "volume_ratio": exact.map(|e| (c.volume - e).abs() / (tol * c.area).max(1e-300)),
        "brep_ms": brep_ms,
        "ms": tess_ms,
    });
    if own {
        let t2 = Instant::now();
        if let Ok(m) = k.tessellate(s, tol) {
            let own_ms = ms(t2);
            let oc = cad_tess::check(&m);
            v["own"] = json!({
                "triangles": oc.triangles,
                "watertight": oc.watertight(),
                "volume_ratio": exact.map(|e| (oc.volume - e).abs() / (tol * oc.area).max(1e-300)),
                "ms": own_ms,
            });
        }
    }
    v
}

fn run<K: Kernel + 'static>(k: K, file: &Path, tol: f64, own: bool) -> Vec<Value>
where
    K::Shape: 'static,
{
    with_parts(k, file, |k, s, solid| part(k, s, solid, tol, own))
}

fn dump<K: Kernel + 'static>(k: K, file: &Path) -> Vec<Value>
where
    K::Shape: 'static,
{
    with_parts(k, file, |k, s, _| k.brep(s).map(|b| serde_json::to_value(b).unwrap()).unwrap_or(Value::Null))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut kernel = "smlib".to_string();
    let mut tol = 0.05;
    let mut out: Option<String> = None;
    let mut do_dump = false;
    let mut files: Vec<PathBuf> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--kernel" => kernel = args.next().unwrap(),
            "--tol" => tol = args.next().unwrap().parse().unwrap(),
            "--json" => out = args.next(),
            "--dump" => do_dump = true,
            _ => files.push(PathBuf::from(a)),
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if files.is_empty() {
        files = corpus(&root);
    }
    let kernels: Vec<&str> = match kernel.as_str() {
        "both" => vec!["smlib", "solidcore"],
        k => vec![k],
    };
    if do_dump {
        let f = &files[0];
        let v = if kernels[0] == "solidcore" {
            dump(kernel_solidcore::SolidcoreKernel::new(), f)
        } else {
            dump(kernel_smlib::SmlibKernel::new(), f)
        };
        println!("{}", serde_json::to_string(&v).unwrap());
        return;
    }
    let mut rows = Vec::new();
    for f in &files {
        let name = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        for kn in &kernels {
            let t0 = Instant::now();
            let parts = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if *kn == "solidcore" {
                    run(kernel_solidcore::SolidcoreKernel::new(), f, tol, false)
                } else {
                    run(kernel_smlib::SmlibKernel::new(), f, tol, true)
                }
            }))
            .unwrap_or_else(|_| vec![json!({ "error": "panic" })]);
            let ok = parts
                .iter()
                .all(|p| p["watertight"] == true && p["failed_faces"].as_array().is_some_and(|a| a.is_empty()));
            let tris: u64 = parts.iter().filter_map(|p| p["triangles"].as_u64()).sum();
            let own: u64 = parts.iter().filter_map(|p| p["own"]["triangles"].as_u64()).sum();
            let ratio = parts.iter().filter_map(|p| p["volume_ratio"].as_f64()).fold(0.0, f64::max);
            let own_ratio = parts.iter().filter_map(|p| p["own"]["volume_ratio"].as_f64()).fold(0.0, f64::max);
            let tms: f64 = parts.iter().filter_map(|p| p["ms"].as_f64()).sum();
            let oms: f64 = parts.iter().filter_map(|p| p["own"]["ms"].as_f64()).sum();
            let err: Vec<String> = parts
                .iter()
                .filter_map(|p| {
                    p["error"]
                        .as_str()
                        .map(String::from)
                        .or_else(|| p["failed_faces"].as_array().filter(|a| !a.is_empty()).map(|a| format!("{a:?}")))
                })
                .collect();
            eprintln!(
                "{name:60} {kn:9} parts {:2} {} tris {tris:7} (own {own:7}) vol {ratio:6.3} (own {own_ratio:6.3}) ms {tms:8.1} (own {oms:8.1}) total {:.0} {}",
                parts.len(),
                if ok { "ok  " } else { "FAIL" },
                ms(t0),
                err.join("; ")
            );
            rows.push(json!({ "file": name, "kernel": kn, "parts": parts }));
        }
    }
    if let Some(o) = out {
        std::fs::write(o, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    }
}
