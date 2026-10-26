//! Checks the memory estimate of cached node results (cache limits of the CLI daemon): each
//! model is evaluated with a fresh engine, once with the topology estimate alone
//! (`Kernel::shape_bytes`) and once with the heap probe of the daemon
//! (`Engine::set_heap_probe`, glibc `mallinfo2`). After the evaluation the session is ended
//! and the engine dropped; the heap bytes this frees are what the cache really held. Prints
//! one JSON object per model and a summary (test report 10).
//!
//!   cargo run --release -p cad-cli --example cache_size -- tests/models/spec/*.mcad …

use std::collections::HashSet;
use std::path::Path;

use cad_session::protocol::ToEngine;
use cad_session::{FsResolver, Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

fn heap() -> usize {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: plain glibc statistics call.
    unsafe {
        let m = libc::mallinfo2();
        m.uordblks + m.hblkhd
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    0
}

struct Row {
    breps: usize,
    faces: usize,
    edges: usize,
    /// heap freed by dropping the engine (cache contents)
    freed: usize,
    estimate: usize,
}

fn run(path: &Path, probe: bool) -> Option<Row> {
    let text = std::fs::read_to_string(path).ok()?;
    let k = SmlibKernel::new();
    let mut s =
        Session::new(SmlibKernel::new(), SessionOptions { auto_root_mesh: false, ..Default::default() }, std_clock());
    if probe {
        s.engine_mut().set_heap_probe(Some(Box::new(heap)));
    }
    let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    s.set_resolver(Box::new(FsResolver { root }), &path.file_name()?.to_string_lossy());
    s.post(ToEngine::SetSource { text, version: 1 });
    s.run_to_idle();
    let d = s.done()?;
    let (mut breps, mut faces, mut edges) = (0, 0, 0);
    let mut seen = HashSet::new();
    for n in 0..d.ev.tree.nodes.len() as u32 {
        for p in d.ev.parts(n) {
            if seen.insert(p.shape.raw()) {
                breps += 1;
                faces += k.face_count(&p.shape);
                edges += k.edge_count(&p.shape);
            }
        }
    }
    let estimate = s.engine().cache_stats().node_bytes;
    let engine = s.into_engine();
    let with = heap();
    drop(engine);
    let freed = with.saturating_sub(heap());
    Some(Row { breps, faces, edges, freed, estimate })
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            // initialise SMLib's global state first (not part of any model)
            let _ = run(Path::new(&files[0]), false);
            let mut tot = [0usize; 6];
            let mut n = 0;
            for f in &files {
                let (Some(a), Some(b)) = (run(Path::new(f), false), run(Path::new(f), true)) else { continue };
                if a.breps == 0 {
                    continue;
                }
                n += 1;
                println!(
                    "{{\"file\": {f:?}, \"breps\": {}, \"faces\": {}, \"edges\": {}, \"freed\": {}, \"estimate_topology\": {}, \"estimate_probe\": {}}}",
                    a.breps, a.faces, a.edges, b.freed, a.estimate, b.estimate
                );
                for (i, v) in [a.breps, a.faces, a.edges, b.freed, a.estimate, b.estimate].into_iter().enumerate() {
                    tot[i] += v;
                }
            }
            println!(
                "{{\"summary\": true, \"models\": {n}, \"breps\": {}, \"faces\": {}, \"edges\": {}, \"freed\": {}, \"estimate_topology\": {}, \"estimate_probe\": {}}}",
                tot[0], tot[1], tot[2], tot[3], tot[4], tot[5]
            );
        })
        .unwrap()
        .join();
}
