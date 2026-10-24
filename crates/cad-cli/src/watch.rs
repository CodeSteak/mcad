//! `cad watch`: watches the main file and every project file (`include`/`use`), evaluates
//! again on each change in the same session (the engine cache stays, so only changed nodes
//! reach the kernel) and prints one line per run: JSON (`--json`, one object per line) or
//! text. `--render bild.png` writes the image again after each run (all `render` options).
//! Ends with Ctrl+C, `--once-after-change` (after the first run caused by a change) or
//! `--timeout` (seconds without a change).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cad_session::PROTOCOL_VERSION;
use cad_session::protocol as p;
use clap::Args;
use notify::{RecursiveMode, Watcher};
use serde_json::{Value as Json, json};

use crate::render::{Plan, ViewArgs, bounds_json, render_model};
use crate::{Common, Fail, Model, Outcome, load, num};

#[derive(Args, Clone, Debug)]
pub(crate) struct WatchArgs {
    /// Rewrite this image on every evaluation (options as for `render`).
    #[arg(long, value_name = "IMAGE.png")]
    pub render: Option<PathBuf>,
    /// Exit after the first evaluation triggered by a change.
    #[arg(long)]
    pub once_after_change: bool,
    /// Exit when no change came for this many seconds.
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<f64>,
    /// Collect changes for this long (ms) before evaluating again.
    #[arg(long, default_value_t = 60, value_name = "MS")]
    pub debounce: u64,
    #[command(flatten)]
    pub v: ViewArgs,
}

/// Absolute paths of the main file and all project files of the last evaluation.
fn project_files(m: &Model, main: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    out.insert(std::path::absolute(main).unwrap_or_else(|_| main.to_path_buf()));
    if let Some(cur) = m.session.current() {
        // included/used files and data files (`import("logo.svg")`, fonts), also missing ones
        let paths = cur.proj.files.iter().skip(1).map(|f| &f.path).chain(cur.proj.assets.keys());
        for f in paths {
            let p = m.root.join(f);
            out.insert(std::path::absolute(&p).unwrap_or(p));
        }
    }
    out
}

struct Run<'a> {
    n: u64,
    trigger: &'a str,
    changed: Vec<String>,
    total_ms: f64,
}

fn report(m: &mut Model, plan: Option<&(Plan, PathBuf)>, r: Run) -> (Json, String, bool) {
    let errors = m.diagnostics.iter().filter(|d| d.severity == p::Severity::Error).count();
    let warnings = m.diagnostics.iter().filter(|d| d.severity == p::Severity::Warning).count();
    let mut j = json!({
        "schema_version": PROTOCOL_VERSION,
        "command": "watch",
        "run": r.n,
        "trigger": r.trigger,
        "changed": r.changed,
        "file": m.file,
        "ok": errors == 0,
        "exit_code": if errors > 0 { 1 } else { 0 },
        "context": m.context,
        "errors": errors,
        "warnings": warnings,
        "diagnostics": m.diagnostics,
    });
    let mut t =
        format!("[{}] {}: {errors} Fehler, {warnings} Warnungen, Auswertung {} ms", r.n, m.file, num(r.total_ms));
    let mut timings = json!({ "total_ms": r.total_ms });
    if let Some(sc) = m.scene.clone() {
        timings["eval_ms"] = json!(sc.stats.eval_ms);
        timings["kernel_ms"] = json!(sc.stats.kernel_ms);
        j["stats"] = json!({ "nodes": sc.stats.nodes, "cache_hits": sc.stats.cache_hits, "kernel_calls": sc.stats.kernel_calls });
        t.push_str(&format!(
            " (Kernel {} ms, {}/{} Knoten aus dem Cache)",
            num(sc.stats.kernel_ms),
            sc.stats.cache_hits,
            sc.stats.nodes
        ));
        let root = sc.root.filter(|&r| sc.nodes[r as usize].dim == Some(p::Dim::Three));
        let volume = root.and_then(|r| m.session.mass_props(r).ok()).map(|mp| mp.volume);
        let bounds = sc.root.and_then(|r| {
            let n = &sc.nodes[r as usize];
            n.bounds.as_ref().map(|b| crate::world_bounds(b, &crate::mat(&n.display_world)))
        });
        j["volume"] = json!(volume);
        j["bounds"] = bounds_json(&bounds);
        j["parts"] = json!(
            sc.parts
                .iter()
                .map(|pt| json!({ "name": pt.name, "volume": pt.volume, "bounds": bounds_json(&pt.bounds) }))
                .collect::<Vec<_>>()
        );
        if let Some(v) = volume {
            t.push_str(&format!(", Volumen {} mm³", num(v)));
        }
        if !sc.parts.is_empty() {
            t.push_str(&format!(", {} Bauteile", sc.parts.len()));
        }
        if let Some((plan, out)) = plan {
            let t0 = Instant::now();
            match render_model(m, plan, out) {
                Ok((rj, _)) => {
                    j["render"] = json!({ "output": rj["output"], "width": rj["width"], "height": rj["height"] });
                    t.push_str(&format!(", Bild {}", out.display()));
                }
                Err(Fail::Usage(e) | Fail::Kernel(e)) => {
                    j["render"] = json!({ "error": e });
                    t.push_str(&format!(", Bild: {e}"));
                }
            }
            let render_ms = t0.elapsed().as_secs_f64() * 1000.0;
            timings["render_ms"] = json!(render_ms);
            timings["total_ms"] = json!(r.total_ms + render_ms);
        }
    } else {
        j["render"] = Json::Null;
    }
    j["timings"] = timings;
    if !r.changed.is_empty() {
        t.push_str(&format!(" – geändert: {}", r.changed.join(", ")));
    }
    t.push('\n');
    t.push_str(&m.diagnostics_text());
    (j, t, errors > 0)
}

fn emit(json_mode: bool, j: &Json, t: &str) {
    if json_mode {
        println!("{}", serde_json::to_string(j).unwrap());
    } else {
        print!("{t}");
    }
}

pub(crate) fn command(c: &Common, a: &WatchArgs) -> Result<Outcome, Fail> {
    if c.file.as_os_str() == "-" {
        return Err(Fail::Usage("watch braucht eine Datei (nicht stdin)".into()));
    }
    let plan = match &a.render {
        Some(out) => Some((Plan::from_args(&a.v)?, out.clone())),
        None => None,
    };
    let t0 = Instant::now();
    let first_line = plan.as_ref().and_then(|(p, _)| p.lines.first().copied());
    let mut m = load(c, first_line, a.v.whole_file, true)?;
    let (j, t, mut errors) = report(
        &mut m,
        plan.as_ref(),
        Run { n: 1, trigger: "start", changed: Vec::new(), total_ms: t0.elapsed().as_secs_f64() * 1000.0 },
    );
    emit(c.json, &j, &t);

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx).map_err(|e| Fail::Usage(format!("Dateibeobachtung: {e}")))?;
    let mut files = project_files(&m, &c.file);
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    let mut sync_dirs = |files: &BTreeSet<PathBuf>, watcher: &mut notify::RecommendedWatcher| {
        for f in files {
            let d = f.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
            if !dirs.contains(&d) && watcher.watch(&d, RecursiveMode::NonRecursive).is_ok() {
                dirs.insert(d);
            }
        }
    };
    sync_dirs(&files, &mut watcher);
    let mut run = 1;
    let mut last_change = Instant::now();
    loop {
        let wait = match a.timeout {
            Some(s) => {
                let left = Duration::from_secs_f64(s.max(0.0)).saturating_sub(last_change.elapsed());
                if left.is_zero() {
                    break;
                }
                left
            }
            None => Duration::from_secs(3600),
        };
        let ev = match rx.recv_timeout(wait) {
            Ok(ev) => ev,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut changed: BTreeSet<PathBuf> = BTreeSet::new();
        let take = |ev: notify::Result<notify::Event>, changed: &mut BTreeSet<PathBuf>| {
            if let Ok(ev) = ev
                && !matches!(ev.kind, notify::EventKind::Access(_))
            {
                for p in ev.paths {
                    let p = std::path::absolute(&p).unwrap_or(p);
                    if files.contains(&p) {
                        changed.insert(p);
                    }
                }
            }
        };
        take(ev, &mut changed);
        if changed.is_empty() {
            continue;
        }
        std::thread::sleep(Duration::from_millis(a.debounce));
        while let Ok(ev) = rx.try_recv() {
            take(ev, &mut changed);
        }
        last_change = Instant::now();
        // editors save atomically (write + rename): retry briefly while the file is missing
        let mut text = None;
        for _ in 0..20 {
            if let Ok(s) = std::fs::read_to_string(&c.file) {
                text = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let Some(text) = text else {
            eprintln!("cad watch: {} nicht lesbar", c.file.display());
            continue;
        };
        run += 1;
        let total = m.reload(text);
        let names: Vec<String> = changed
            .iter()
            .map(|p| {
                let rel = p.strip_prefix(std::path::absolute(&m.root).unwrap_or_default()).unwrap_or(p);
                rel.display().to_string()
            })
            .collect();
        let (j, t, e) =
            report(&mut m, plan.as_ref(), Run { n: run, trigger: "change", changed: names, total_ms: total });
        errors = e;
        emit(c.json, &j, &t);
        files = project_files(&m, &c.file);
        sync_dirs(&files, &mut watcher);
        if a.once_after_change {
            break;
        }
    }
    Ok(Outcome { json: Json::Null, text: String::new(), errors })
}
