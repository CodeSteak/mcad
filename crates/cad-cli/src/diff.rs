//! `cad diff-kernels <files…> [--json]`: evaluates each file with SMLib and with solidcore and
//! compares every node (plan sections 2.3 and 8, acceptance test of the switch to solidcore).
//!
//! Per node evaluated by SMLib: status, volume (relative 1e-6), number of faces and edges of its
//! result, and the edges selected by fillet/chamfer (count and points, 1e-6 of the size).
//! "Coverage" counts per operation (node label) how many nodes solidcore evaluated like SMLib.
//! Verdicts: `same`; `unsupported` (solidcore reports the operation as not supported);
//! `blocked` (differs only because an input node is unsupported or failed); `differ`; `failed`
//! (solidcore fails where SMLib succeeds); `skipped` (not evaluated by SMLib either).
//!
//! JSON: `{ schema_version, command: "diff-kernels", ok, exit_code, solidcore_rev,
//!   files: [{ file, nodes: [NodeDiff], same, smlib_ok, solidcore_ok, unsupported, differ }],
//!   coverage: { label: { nodes, same, unsupported, differ, failed } }, summary }`
//! with `NodeDiff = { idx, label, line,
//!   verdict: "same"|"unsupported"|"blocked"|"differ"|"failed"|"skipped",
//!   smlib: Metrics, solidcore: Metrics, notes: [string] }` and
//! `Metrics = { status, volume, faces, edges, selected_edges, message }`.
//! Exit code 0 if every node of every file is the same, 1 otherwise, 2 usage error.

use std::collections::BTreeMap;
use std::path::PathBuf;

use cad_kernel::Kernel;
use cad_session::protocol::{self as p, ToEngine};
use cad_session::{PROTOCOL_VERSION, Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;
use kernel_solidcore::SolidcoreKernel;
use serde::Serialize;
use serde_json::json;

use crate::{Fail, Outcome};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub status: String,
    pub volume: Option<f64>,
    pub faces: Option<usize>,
    pub edges: Option<usize>,
    pub selected_edges: Vec<Vec<[f64; 3]>>,
    pub message: Option<String>,
    #[serde(skip)]
    pub unsupported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDiff {
    pub idx: u32,
    pub label: String,
    pub line: u32,
    pub verdict: &'static str,
    pub smlib: Metrics,
    pub solidcore: Metrics,
    pub notes: Vec<String>,
}

const UNSUPPORTED: &str = "not supported by this kernel";

/// Evaluates `text` with `kernel` and measures every node.
/// (index, label, line, children) per scene node.
type NodeRow = (u32, String, u32, Vec<u32>);

fn evaluate<K: Kernel + 'static>(kernel: K, text: &str) -> (Vec<NodeRow>, Vec<Metrics>)
where
    K::Shape: 'static,
{
    let mut opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    opts.eval.max_stack_bytes = 192 << 20;
    let mut s = Session::new(kernel, opts, std_clock());
    s.post(ToEngine::SetSource { text: text.to_string(), version: 1 });
    s.run_to_idle();
    let Some(d) = s.done() else { return (Vec::new(), Vec::new()) };
    let scene = d.scene.clone();
    let mut nodes = Vec::new();
    let mut out = Vec::new();
    for n in &scene.nodes {
        nodes.push((n.idx, n.label.clone(), n.range.line, n.children.clone()));
        let status = match n.status {
            p::NodeStatus::Ok => "ok",
            p::NodeStatus::Warning => "warning",
            p::NodeStatus::Error => "error",
            p::NodeStatus::Skipped => "skipped",
            p::NodeStatus::Pending => "pending",
        };
        let message = n.messages.iter().find(|m| m.severity == p::Severity::Error).map(|m| m.message.clone());
        let unsupported = message.as_deref().is_some_and(|m| m.contains(UNSUPPORTED));
        let mut m = Metrics {
            status: status.into(),
            message,
            unsupported,
            selected_edges: n.selected_edges.iter().map(|e| e.iter().map(|v| [v.0[0], v.0[1], v.0[2]]).collect()).collect(),
            ..Default::default()
        };
        if matches!(n.status, p::NodeStatus::Ok | p::NodeStatus::Warning) {
            let parts: Vec<_> = s.done().map(|d| d.ev.parts(n.idx).to_vec()).unwrap_or_default();
            let k = s.engine_mut().kernel_mut();
            let mut faces = 0;
            let mut edges = 0;
            let mut ok = true;
            for part in &parts {
                match (k.faces(&part.shape, 0), k.edges(&part.shape, 0)) {
                    (Ok(f), Ok(e)) => {
                        faces += f.len();
                        edges += e.len();
                    }
                    _ => ok = false,
                }
            }
            if ok && !parts.is_empty() {
                m.faces = Some(faces);
                m.edges = Some(edges);
            }
            m.volume = s.mass_props(n.idx).ok().map(|mp| mp.volume);
        }
        out.push(m);
    }
    (nodes, out)
}

fn compare(a: &Metrics, b: &Metrics, notes: &mut Vec<String>) -> &'static str {
    if a.status == "skipped" || a.status == "pending" {
        return "skipped";
    }
    if b.unsupported {
        return "unsupported";
    }
    if a.status == "error" && b.status == "error" {
        return "same";
    }
    if b.status == "error" {
        notes.push(format!("solidcore: {}", b.message.clone().unwrap_or_default()));
        return "failed";
    }
    if a.status == "error" {
        notes.push("SMLib fails, solidcore not".into());
        return "differ";
    }
    match (a.volume, b.volume) {
        (Some(x), Some(y)) if (x - y).abs() > 1e-6 * x.abs().max(1e-9) => notes.push(format!("volume {x} vs {y}")),
        (Some(_), None) => notes.push("solidcore: no volume".into()),
        _ => {}
    }
    if a.faces != b.faces {
        notes.push(format!("faces {:?} vs {:?}", a.faces, b.faces));
    }
    if a.edges != b.edges {
        notes.push(format!("edges {:?} vs {:?}", a.edges, b.edges));
    }
    if a.selected_edges.len() != b.selected_edges.len() {
        notes.push(format!("selected edges {} vs {}", a.selected_edges.len(), b.selected_edges.len()));
    } else {
        let size = a
            .selected_edges
            .iter()
            .flatten()
            .flat_map(|p| p.iter().map(|x| x.abs()))
            .fold(1.0f64, f64::max);
        let close = a.selected_edges.iter().zip(&b.selected_edges).all(|(x, y)| {
            let (ea, eb) = (x.first().zip(x.last()), y.first().zip(y.last()));
            match (ea, eb) {
                (Some((a0, a1)), Some((b0, b1))) => {
                    let d = |p: &[f64; 3], q: &[f64; 3]| p.iter().zip(q).map(|(u, v)| (u - v).abs()).fold(0.0, f64::max);
                    let tol = 1e-6 * size;
                    (d(a0, b0) <= tol && d(a1, b1) <= tol) || (d(a0, b1) <= tol && d(a1, b0) <= tol)
                }
                _ => true,
            }
        });
        if !close {
            notes.push("selected edges at other positions".into());
        }
    }
    if notes.is_empty() { "same" } else { "differ" }
}

pub fn command(files: &[PathBuf]) -> Result<Outcome, Fail> {
    if files.is_empty() {
        return Err(Fail::Usage("diff-kernels: keine Dateien".into()));
    }
    let mut out_files = Vec::new();
    let mut coverage: BTreeMap<String, BTreeMap<&'static str, usize>> = BTreeMap::new();
    let mut text = String::new();
    let mut all_same = true;
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();
    if !SolidcoreKernel::available() {
        return Err(Fail::Usage(
            "diff-kernels braucht solidcore: cad mit `--features solidcore` bauen \
             (cargo build -p cad-cli --features solidcore)"
                .into(),
        ));
    }
    for f in files {
        let src = std::fs::read_to_string(f).map_err(|e| Fail::Usage(format!("{}: {e}", f.display())))?;
        let (nodes, a) = evaluate(SmlibKernel::new(), &src);
        let solid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| evaluate(SolidcoreKernel::new(), &src)));
        let (nodes_b, b) = solid.unwrap_or_default();
        let mut diffs = Vec::new();
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        // own verdict per node, then "blocked" where a descendant is unsupported or failed
        let mut own = Vec::with_capacity(nodes.len());
        for (i, (idx, label, _, _)) in nodes.iter().enumerate() {
            let mut notes = Vec::new();
            let mb = if nodes_b.get(i).is_some_and(|n| n.0 == *idx && n.1 == *label) {
                b[i].clone()
            } else {
                notes.push("solidcore: evaluation aborted (panic or other tree)".into());
                Metrics { status: "error".into(), ..Default::default() }
            };
            let verdict = compare(&a[i], &mb, &mut notes);
            own.push((verdict, mb, notes));
        }
        let pos: BTreeMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();
        fn broken(i: usize, nodes: &[NodeRow], pos: &BTreeMap<u32, usize>, own: &[(&str, Metrics, Vec<String>)]) -> bool {
            nodes[i].3.iter().filter_map(|c| pos.get(c)).any(|&c| {
                matches!(own[c].0, "unsupported" | "failed") || broken(c, nodes, pos, own)
            })
        }
        // a zone selects edges of its sibling (the fillet/chamfer target): look at the parent
        let parent: BTreeMap<u32, usize> =
            nodes.iter().enumerate().flat_map(|(i, n)| n.3.iter().map(move |c| (*c, i))).collect();
        let blocked: Vec<bool> = (0..nodes.len())
            .map(|i| {
                let scope = if nodes[i].1 == "zone" { parent.get(&nodes[i].0).copied().unwrap_or(i) } else { i };
                matches!(own[i].0, "differ" | "failed") && broken(scope, &nodes, &pos, &own)
            })
            .collect();
        for (i, ((idx, label, line, _), (verdict, mb, notes))) in nodes.iter().zip(own).enumerate() {
            let (verdict, notes) = if blocked[i] {
                ("blocked", vec!["input from an unsupported or failed node".to_string()])
            } else {
                (verdict, notes)
            };
            if verdict != "skipped" {
                *counts.entry(verdict).or_insert(0) += 1;
                *totals.entry(verdict).or_insert(0) += 1;
                let c = coverage.entry(label.clone()).or_default();
                *c.entry("nodes").or_insert(0) += 1;
                *c.entry(verdict).or_insert(0) += 1;
            }
            if verdict != "same" && verdict != "skipped" {
                all_same = false;
            }
            diffs.push(NodeDiff {
                idx: *idx,
                label: label.clone(),
                line: *line,
                verdict,
                smlib: a[i].clone(),
                solidcore: mb,
                notes,
            });
        }
        let name = f.display().to_string();
        text.push_str(&format!(
            "{name}: {} Knoten, gleich {}, unsupported {}, blockiert {}, abweichend {}, gescheitert {}\n",
            diffs.iter().filter(|d| d.verdict != "skipped").count(),
            counts.get("same").unwrap_or(&0),
            counts.get("unsupported").unwrap_or(&0),
            counts.get("blocked").unwrap_or(&0),
            counts.get("differ").unwrap_or(&0),
            counts.get("failed").unwrap_or(&0),
        ));
        for d in diffs.iter().filter(|d| d.verdict == "differ" || d.verdict == "failed") {
            text.push_str(&format!("  Zeile {} {} [{}]: {}\n", d.line, d.label, d.verdict, d.notes.join("; ")));
        }
        out_files.push(json!({
            "file": name,
            "nodes": diffs,
            "same": counts.get("same").unwrap_or(&0),
            "unsupported": counts.get("unsupported").unwrap_or(&0),
            "blocked": counts.get("blocked").unwrap_or(&0),
            "differ": counts.get("differ").unwrap_or(&0),
            "failed": counts.get("failed").unwrap_or(&0),
        }));
    }
    text.push_str("\nAbdeckung solidcore je Operation (gleich / Knoten; unsupported, blockiert, abweichend):\n");
    for (label, c) in &coverage {
        let g = |k: &str| c.get(k).copied().unwrap_or(0);
        text.push_str(&format!(
            "  {label:<20} {:>4} / {:<4} {:>4} {:>4} {:>4}\n",
            g("same"),
            g("nodes"),
            g("unsupported"),
            g("blocked"),
            g("differ") + g("failed")
        ));
    }
    let n: usize = ["same", "unsupported", "blocked", "differ", "failed"].iter().map(|k| totals.get(k).unwrap_or(&0)).sum();
    text.push_str(&format!(
        "\nGesamt: {n} Knoten, gleich {}, unsupported {}, blockiert {}, abweichend {}, gescheitert {}\n",
        totals.get("same").unwrap_or(&0),
        totals.get("unsupported").unwrap_or(&0),
        totals.get("blocked").unwrap_or(&0),
        totals.get("differ").unwrap_or(&0),
        totals.get("failed").unwrap_or(&0)
    ));
    let j = json!({
        "schema_version": PROTOCOL_VERSION,
        "command": "diff-kernels",
        "ok": all_same,
        "exit_code": 0,
        "solidcore_rev": kernel_solidcore::SOLIDCORE_REV,
        "solidcore_available": SolidcoreKernel::available(),
        "files": out_files,
        "coverage": coverage,
        "summary": { "nodes": n, "same": totals.get("same").unwrap_or(&0),
                     "unsupported": totals.get("unsupported").unwrap_or(&0),
                     "blocked": totals.get("blocked").unwrap_or(&0),
                     "differ": totals.get("differ").unwrap_or(&0),
                     "failed": totals.get("failed").unwrap_or(&0) },
    });
    Ok(Outcome { json: j, text, errors: !all_same })
}
