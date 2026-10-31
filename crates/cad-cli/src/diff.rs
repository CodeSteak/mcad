//! `mcad diff-kernels <files…> [--json]`: evaluates each file with SMLib and with solidcore and
//! compares every node (plan sections 2.3 and 8, acceptance test of the switch to solidcore).
//!
//! Files are evaluated like the other commands do (`include`/`use`/`import` relative to the file).
//! Per node evaluated by SMLib: status, volume, number of faces and edges of its result (exactly
//! equal), and the edges selected by fillet/chamfer (count and points, 1e-6 of the size).
//! Volumes ([`volume_check`]): relative 1e-6; else, where the exact volume is known
//! (`diff/exact.rs`), solidcore at most as far from it as SMLib; else within the summed
//! deviations of both kernels from the exact volumes of the operands; else, for curved results,
//! 3e-5 (both integrate over curved faces, and SMLib fits skins and sweeps less closely: its
//! sphere r 5 is 7.9e-7 below the exact volume, its twisted bar 1.5e-5 above); else, for a
//! result with a fillet or chamfer at or below the node, 1e-4 of the blended bodies (both fit the
//! blend surfaces): of the volume of the nearest blend nodes at or below it, not of the node.
//! Selected edges match in any order, by all their sample points (either direction; another
//! parameterisation or start point within the chordal sag of the other's samples).
//! "Coverage" counts per operation (node label) how many nodes solidcore evaluated like SMLib.
//! Verdicts: `same`; `unsupported` (solidcore reports the operation, or a query of its result —
//! faces, edges, volume —, as not supported, also in a warning such as "B-rep from the mesh failed");
//! `blocked` (differs only because an input node is unsupported or failed); `differ`; `failed`
//! (solidcore fails where SMLib succeeds); `skipped` (not evaluated by SMLib either).
//!
//! JSON: `{ schema_version, command: "diff-kernels", ok, exit_code, solidcore_rev,
//!   files: [{ file, nodes: [NodeDiff], same, smlib_ok, solidcore_ok, unsupported, differ }],
//!   coverage: { label: { nodes, same, unsupported, differ, failed } }, summary }`
//! with `NodeDiff = { idx, label, line,
//!   verdict: "same"|"unsupported"|"blocked"|"differ"|"failed"|"skipped",
//!   smlib: Metrics, solidcore: Metrics, volume_check: VolumeCheck|null, notes: [string] }`,
//! `Metrics = { status, volume, exact_volume, curved, faces, edges, selected_edges, message }`,
//! `VolumeCheck = { rule: "relative"|"exact"|"operands"|"curved"|"blend"|"none", tolerance, difference,
//!   exact, smlib_error, solidcore_error }`; `summary.volume_rules`: same nodes per rule.
//! Cost per file (`smlib_cost`, `solidcore_cost`) and in the summary (sums; heap: maximum; plus
//! `eval_ratio`, `query_ratio` solidcore / SMLib): `{ eval_ms, query_ms, heap_bytes, copies,
//! copy_ms }` (plan 0.5).
//! With `--brep` also the neutral B-reps of the same nodes (`diff/brep.rs`): `NodeDiff.brep_check
//! = { verdict: "same"|"differ"|"unsupported", notes }`, `Metrics.brep` (counts per kind,
//! conventions not met), `summary.brep`: nodes per verdict. The B-rep check does not change the
//! node's verdict.
//! Exit code 0 if every node of every file is the same, 1 otherwise, 2 usage error.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cad_i18n::{tr, trs};
use cad_kernel::Kernel;
use cad_session::protocol::{self as p, ToEngine};
use cad_session::{PROTOCOL_VERSION, Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;
use kernel_solidcore::SolidcoreKernel;
use serde::Serialize;
use serde_json::json;

use crate::{Fail, Outcome};

mod brep;
mod exact;

use exact::Exact;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub status: String,
    pub volume: Option<f64>,
    /// Exact volume from the parameters (primitives, plain extrusions, revolutions and their
    /// affine images; `exact.rs`), `None` where unknown.
    pub exact_volume: Option<f64>,
    /// Some edge of the result is not straight (so some face is curved and its volume integral
    /// is approximate).
    pub curved: bool,
    pub faces: Option<usize>,
    pub edges: Option<usize>,
    pub selected_edges: Vec<Vec<[f64; 3]>>,
    pub message: Option<String>,
    /// The neutral B-rep of the node's parts (`Kernel::brep`), `None` without parts or for a mesh.
    pub brep: Option<brep::Summary>,
    #[serde(skip)]
    pub unsupported: bool,
}

/// The comparison of the neutral B-reps of a `same` node (`diff/brep.rs`).
#[derive(Debug, Clone, Serialize)]
pub struct BrepCheck {
    /// `same`, `differ` or `unsupported` (solidcore's `Kernel::brep` is not supported there).
    pub verdict: &'static str,
    pub notes: Vec<String>,
}

/// How the volumes of a node were compared (see [`volume_check`]).
#[derive(Debug, Clone, Serialize)]
pub struct VolumeCheck {
    /// `relative`: |smlib − solidcore| ≤ 1e-6 |smlib|; `exact`: solidcore at most as far from
    /// the exact volume as SMLib (or within 1e-6 of it); `operands`: the difference is within the
    /// summed deviations of both kernels from the exact volumes of the operands; `curved`: curved
    /// faces, within 3e-5 (of SMLib and of an exact value); `none`: none of them.
    pub rule: &'static str,
    /// Allowed difference (absolute, mm³) of the rule that applied, or of `relative`.
    pub tolerance: f64,
    pub difference: f64,
    pub exact: Option<f64>,
    /// |volume − exact| / exact.
    pub smlib_error: Option<f64>,
    pub solidcore_error: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDiff {
    pub idx: u32,
    pub label: String,
    pub line: u32,
    pub verdict: &'static str,
    pub smlib: Metrics,
    pub solidcore: Metrics,
    /// Present when both kernels gave a volume.
    pub volume_check: Option<VolumeCheck>,
    /// Present for `same` nodes whose B-reps both kernels were asked for.
    pub brep_check: Option<BrepCheck>,
    pub notes: Vec<String>,
}

const UNSUPPORTED: &str = "not supported by this kernel";

/// Volumes agree relatively (planar results, and any result without a better rule).
const REL_VOLUME: f64 = 1e-6;
/// Curved results without an exact value, relative: both kernels integrate the volume over
/// curved faces (cylinder r 5 h 10: SMLib 2.0e-7 below the exact volume, i.e. 1.6e-4 mm³;
/// sphere r 5: SMLib 7.9e-7 below, solidcore 1.8e-7 above), and SMLib's skin and sweep surfaces
/// are fitted less closely (twisted bar 8 × 8 × 20, exact 1280: SMLib 1.5e-5 above, solidcore
/// 6e-7). Largest difference in the corpus: 1.9e-5 (a loft in `planter`).
const REL_VOLUME_CURVED: f64 = 3e-5;
/// Results with a fillet or chamfer at or below the node, relative: both kernels fit the blend
/// surfaces, SMLib less closely (`pillow_block`: its chamfers of 19 edges remove 39.15 mm³, by
/// Pappus 41.40, solidcore 41.39; a cylinder rim chamfer: SMLib −1.2e-6, solidcore −5.7e-6 of the
/// exact volume). Largest difference in the corpus: 6.5e-5 (`pillow_block`). Relative to the
/// volume of the blend nodes (the nearest at or below a node, summed), so that unblended bodies
/// beside them in a group do not widen it ([`blends`]).
const REL_VOLUME_BLEND: f64 = 1e-4;

/// One evaluation: (index, label, line, children) and metrics per scene node.
struct Run {
    nodes: Vec<NodeRow>,
    metrics: Vec<Metrics>,
    cost: Cost,
}

/// Cost of one evaluation (plan 0.5).
#[derive(Debug, Clone, Copy, Default, Serialize)]
struct Cost {
    /// Evaluation of the file (language and kernel operations), ms.
    eval_ms: f64,
    /// The queries of `diff-kernels` itself (faces, edges, volume of every node), ms.
    query_ms: f64,
    /// Heap held after the evaluation and the queries (glibc `mallinfo2`, both kernels allocate
    /// with malloc), bytes; `null` elsewhere.
    heap_bytes: Option<usize>,
    /// B-rep copies of the kernel (solidcore: `SmApiBrepCopy`) and their time in ms.
    copies: u64,
    copy_ms: f64,
}

impl Cost {
    fn add(&mut self, o: &Cost) {
        self.eval_ms += o.eval_ms;
        self.query_ms += o.query_ms;
        self.copies += o.copies;
        self.copy_ms += o.copy_ms;
        self.heap_bytes = match (self.heap_bytes, o.heap_bytes) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Whether the sample points of an edge lie on one line (within 1e-9 of its length).
fn straight(pts: &[glam::DVec3]) -> bool {
    let (Some(&a), Some(&b)) = (pts.first(), pts.last()) else { return true };
    let d = b - a;
    let len = d.length();
    if len <= 1e-12 {
        // closed edge (full circle): straight only if all points coincide
        return pts.iter().all(|p| (*p - a).length() <= 1e-9);
    }
    pts.iter().all(|p| (*p - a).cross(d).length() / len <= 1e-9 * len)
}

/// (index, label, line, children) per scene node.
type NodeRow = (u32, String, u32, Vec<u32>);

/// Evaluates `text` with `kernel` and measures every node; `copies` reads the copy statistics
/// of the kernel.
fn evaluate<K: Kernel + 'static>(
    kernel: K,
    text: &str,
    file: &Path,
    copies: fn(&K) -> (u64, std::time::Duration),
    with_brep: bool,
) -> Run
where
    K::Shape: 'static,
{
    let heap0 = crate::daemon::heap_in_use();
    let mut opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    opts.eval.max_stack_bytes = 192 << 20;
    let mut s = Session::new(Exact(kernel), opts, std_clock());
    // `include`/`use` relative to the file, as in the other commands
    let root = file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let main = file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    s.set_resolver(Box::new(cad_session::FsResolver { root: root.to_path_buf() }), &main);
    let t0 = std::time::Instant::now();
    s.post(ToEngine::SetSource { text: text.to_string(), version: 1 });
    s.run_to_idle();
    let mut cost = Cost { eval_ms: ms(t0.elapsed()), ..Default::default() };
    let t1 = std::time::Instant::now();
    let Some(d) = s.done() else { return Run { nodes: Vec::new(), metrics: Vec::new(), cost } };
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
        // an error, else a warning that names an unsupported operation (e.g. STL import: "B-rep
        // from the mesh failed (… not supported …): continuing as a mesh")
        let message = n
            .messages
            .iter()
            .find(|m| m.severity == p::Severity::Error)
            .or_else(|| {
                n.messages.iter().find(|m| m.severity == p::Severity::Warning && m.message.contains(UNSUPPORTED))
            })
            .map(|m| m.message.clone());
        let unsupported = message.as_deref().is_some_and(|m| m.contains(UNSUPPORTED));
        let mut m = Metrics {
            status: status.into(),
            message,
            unsupported,
            selected_edges: n
                .selected_edges
                .iter()
                .map(|e| e.iter().map(|v| [v.0[0], v.0[1], v.0[2]]).collect())
                .collect(),
            ..Default::default()
        };
        if matches!(n.status, p::NodeStatus::Ok | p::NodeStatus::Warning) {
            let parts: Vec<_> = s.done().map(|d| d.ev.parts(n.idx).to_vec()).unwrap_or_default();
            let meshes = s.done().is_some_and(|d| !d.ev.meshes(n.idx).is_empty());
            if !meshes && !parts.is_empty() {
                m.exact_volume = parts.iter().map(|p| p.shape.1).sum();
            }
            let k = s.engine_mut().kernel_mut();
            let mut faces = 0;
            let mut edges = 0;
            let mut ok = true;
            // the first failed query; an unsupported query makes the node unsupported
            let mut query_err: Option<String> = None;
            for part in &parts {
                match (k.faces(&part.shape, 0), k.edges(&part.shape, 3)) {
                    (Ok(f), Ok(e)) => {
                        faces += f.len();
                        edges += e.len();
                        m.curved |= e.iter().any(|e| !straight(&e.points));
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        ok = false;
                        query_err.get_or_insert(tr!("Abfrage Flächen/Kanten: {e}", "query faces/edges: {e}"));
                    }
                }
            }
            if ok && !parts.is_empty() {
                m.faces = Some(faces);
                m.edges = Some(edges);
            }
            if with_brep && !meshes && !parts.is_empty() {
                let mut sum = brep::Summary::default();
                for part in &parts {
                    match k.brep(&part.shape) {
                        Ok(b) => sum.add(&b),
                        Err(e) => {
                            sum.unsupported = matches!(e, cad_kernel::KernelError::Unsupported(_));
                            sum.error = Some(e.to_string());
                            break;
                        }
                    }
                }
                m.brep = Some(sum);
            }
            match s.mass_props(n.idx) {
                Ok(mp) => m.volume = Some(mp.volume),
                Err(e) if !parts.is_empty() => {
                    query_err.get_or_insert(tr!("Abfrage Volumen: {e}", "query volume: {e}"));
                }
                Err(_) => {}
            }
            if let Some(e) = query_err
                && m.message.is_none()
            {
                m.unsupported = e.contains(UNSUPPORTED);
                m.message = Some(e);
            }
        }
        out.push(m);
    }
    cost.query_ms = ms(t1.elapsed());
    let (n, t) = copies(&s.engine_mut().kernel_mut().0);
    (cost.copies, cost.copy_ms) = (n, ms(t));
    cost.heap_bytes = crate::daemon::heap_in_use().zip(heap0).map(|(a, b)| a.saturating_sub(b));
    Run { nodes, metrics: out, cost }
}

/// Compares the volumes `x` (SMLib) and `y` (solidcore). `budget`: summed deviation of both
/// kernels from the exact volumes of the operands (see `command`). The rules in order:
/// `relative` (1e-6), `exact` (solidcore at most as far from the exact volume as SMLib, at least
/// 1e-6), `operands` (no exact value, difference ≤ budget + 1e-6), `curved` (curved faces:
/// difference ≤ 3e-5, and solidcore within 3e-5 of an exact value), `blend` (a fillet or chamfer
/// at or below the node: difference ≤ `blend`, 1e-4 of the blended bodies, see [`blends`]);
/// else `none`.
fn volume_check(x: f64, y: f64, a: &Metrics, budget: Option<f64>, blend: Option<f64>) -> VolumeCheck {
    let difference = (x - y).abs();
    let rel = REL_VOLUME * x.abs().max(1e-9);
    let exact = a.exact_volume.filter(|e| *e > 0.0);
    let err = |v: f64| exact.map(|e| (v - e).abs() / e);
    let mut c = VolumeCheck {
        rule: "relative",
        tolerance: rel,
        difference,
        exact,
        smlib_error: err(x),
        solidcore_error: err(y),
    };
    if difference <= rel {
        return c;
    }
    if let (Some(e), Some(ea), Some(eb)) = (exact, c.smlib_error, c.solidcore_error) {
        let allowed = ea.max(REL_VOLUME);
        if eb <= allowed {
            c.rule = "exact";
            c.tolerance = allowed * e;
            return c;
        }
    }
    if let Some(b) = budget
        && exact.is_none()
        && difference <= b + rel
    {
        c.rule = "operands";
        c.tolerance = b + rel;
        return c;
    }
    // with an exact value solidcore must also be within 3e-5 of it
    let near_exact = c.solidcore_error.is_none_or(|e| e <= REL_VOLUME_CURVED);
    if a.curved && near_exact && difference <= REL_VOLUME_CURVED * x.abs() {
        c.rule = "curved";
        c.tolerance = REL_VOLUME_CURVED * x.abs();
        return c;
    }
    if let Some(t) = blend
        && difference <= t
    {
        c.rule = "blend";
        c.tolerance = t;
        return c;
    }
    c.rule = "none";
    c
}

/// Per node: the summed deviation of both kernels from the exact volume, for a node with an
/// exact volume; else the sum over its children that have one (`None` if none has).
fn budgets(nodes: &[NodeRow], pos: &BTreeMap<u32, usize>, a: &[Metrics], b: &[Metrics]) -> Vec<Option<f64>> {
    fn go(
        i: usize,
        nodes: &[NodeRow],
        pos: &BTreeMap<u32, usize>,
        a: &[Metrics],
        b: &[Metrics],
        memo: &mut Vec<Option<Option<f64>>>,
    ) -> Option<f64> {
        if let Some(v) = memo[i] {
            return v;
        }
        let own = match (a[i].exact_volume, a[i].volume, b[i].volume) {
            (Some(e), Some(x), Some(y)) => Some((x - e).abs() + (y - e).abs()),
            _ => None,
        };
        let v = own.or_else(|| {
            let mut sum = None;
            for c in nodes[i].3.iter().filter_map(|c| pos.get(c)) {
                if let Some(v) = go(*c, nodes, pos, a, b, memo) {
                    *sum.get_or_insert(0.0) += v;
                }
            }
            sum
        });
        memo[i] = Some(v);
        v
    }
    let mut memo = vec![None; nodes.len()];
    (0..nodes.len()).map(|i| go(i, nodes, pos, a, b, &mut memo)).collect()
}

/// Per node where SMLib blends edges (fillet, chamfer) at the node or below it: the volume
/// tolerance of rule `blend`, 1e-4 of the volume of the blend nodes (the node itself if it
/// blends, else the sum over its children); `None` without a blend.
fn blends(nodes: &[NodeRow], pos: &BTreeMap<u32, usize>, a: &[Metrics]) -> Vec<Option<f64>> {
    fn go(
        i: usize,
        nodes: &[NodeRow],
        pos: &BTreeMap<u32, usize>,
        a: &[Metrics],
        memo: &mut Vec<Option<Option<f64>>>,
    ) -> Option<f64> {
        if let Some(v) = memo[i] {
            return v;
        }
        let v = if a[i].selected_edges.is_empty() {
            let mut sum = None;
            for c in nodes[i].3.iter().filter_map(|c| pos.get(c)) {
                if let Some(t) = go(*c, nodes, pos, a, memo) {
                    *sum.get_or_insert(0.0) += t;
                }
            }
            sum
        } else {
            Some(REL_VOLUME_BLEND * a[i].volume.unwrap_or(0.0).abs())
        };
        memo[i] = Some(v);
        v
    }
    let mut memo = vec![None; nodes.len()];
    (0..nodes.len()).map(|i| go(i, nodes, pos, a, &mut memo)).collect()
}

/// The same edge, sampled: every point within `tol`, in order or reversed; else (another
/// parameterisation of the same curve, e.g. a B-spline for a circle, or a closed edge from
/// another start point) every point of each within `tol` plus twice the chordal sag of the other's
/// polyline (estimated from its points) of that polyline, and the ends equal unless both are closed.
fn same_polyline(x: &[[f64; 3]], y: &[[f64; 3]], tol: f64) -> bool {
    use glam::DVec3;
    let (x, y): (Vec<DVec3>, Vec<DVec3>) =
        (x.iter().map(|p| DVec3::from(*p)).collect(), y.iter().map(|p| DVec3::from(*p)).collect());
    if x.is_empty() || y.is_empty() {
        return x.len() == y.len();
    }
    let d = |p: DVec3, q: DVec3| (p - q).abs().max_element();
    let n = x.len();
    if n == y.len() && ((0..n).all(|i| d(x[i], y[i]) <= tol) || (0..n).all(|i| d(x[i], y[n - 1 - i]) <= tol)) {
        return true;
    }
    let closed = |p: &[DVec3]| d(p[0], p[p.len() - 1]) <= tol;
    let ends = |p: &[DVec3], q: &[DVec3]| {
        (d(p[0], q[0]) <= tol && d(p[p.len() - 1], q[q.len() - 1]) <= tol)
            || (d(p[0], q[q.len() - 1]) <= tol && d(p[p.len() - 1], q[0]) <= tol)
    };
    if !(closed(&x) && closed(&y)) && !ends(&x, &y) {
        return false;
    }
    // sag of a polyline: a quarter of the largest deviation of a point from its neighbours' chord
    let sag = |p: &[DVec3]| {
        p.windows(3)
            .map(|w| {
                let (a, b, c) = (w[0], w[1], w[2]);
                let ac = c - a;
                let t = if ac.length_squared() > 0.0 {
                    ((b - a).dot(ac) / ac.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (b - (a + ac * t)).length() / 4.0
            })
            .fold(0.0, f64::max)
    };
    let near = |p: &[DVec3], q: &[DVec3]| {
        let lim = tol + 2.0 * sag(q);
        p.iter().all(|&a| {
            q.windows(2).any(|w| {
                let ab = w[1] - w[0];
                let t = if ab.length_squared() > 0.0 {
                    ((a - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (a - (w[0] + ab * t)).length() <= lim
            })
        })
    };
    near(&x, &y) && near(&y, &x)
}

fn compare(
    a: &Metrics,
    b: &Metrics,
    budget: Option<f64>,
    blend: Option<f64>,
    notes: &mut Vec<String>,
) -> (&'static str, Option<VolumeCheck>) {
    let mut vc = None;
    let verdict = compare_metrics(a, b, budget, blend, &mut vc, notes);
    (verdict, vc)
}

fn compare_metrics(
    a: &Metrics,
    b: &Metrics,
    budget: Option<f64>,
    blend: Option<f64>,
    vc: &mut Option<VolumeCheck>,
    notes: &mut Vec<String>,
) -> &'static str {
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
        (Some(x), Some(y)) => {
            let c = volume_check(x, y, a, budget, blend);
            if c.rule == "none" {
                let exact = c.exact.map(|e| format!(" (exact {e})")).unwrap_or_default();
                notes.push(format!("volume {x} vs {y}{exact}"));
            }
            *vc = Some(c);
        }
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
        let size = a.selected_edges.iter().flatten().flat_map(|p| p.iter().map(|x| x.abs())).fold(1.0f64, f64::max);
        // in any order (the kernels list a Boolean's edges in different orders), either direction
        let tol = 1e-6 * size;
        let mut free: Vec<bool> = vec![true; b.selected_edges.len()];
        let close = a.selected_edges.iter().all(|x| {
            let hit = (0..free.len()).find(|&j| free[j] && same_polyline(x, &b.selected_edges[j], tol));
            hit.map(|j| free[j] = false).is_some()
        });
        if !close {
            notes.push("selected edges at other positions".into());
        }
    }
    if notes.is_empty() { "same" } else { "differ" }
}

pub fn command(files: &[PathBuf], brep: bool) -> Result<Outcome, Fail> {
    if files.is_empty() {
        return Err(Fail::Usage(tr!("diff-kernels: keine Dateien", "diff-kernels: no files")));
    }
    let mut out_files = Vec::new();
    let mut coverage: BTreeMap<String, BTreeMap<&'static str, usize>> = BTreeMap::new();
    let mut text = String::new();
    let mut all_same = true;
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();
    // same nodes per volume rule
    let mut rules: BTreeMap<&'static str, usize> = BTreeMap::new();
    // B-rep comparison of the same nodes, per verdict
    let mut brep_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let (mut cost_a, mut cost_b) = (Cost::default(), Cost::default());
    if !SolidcoreKernel::available() {
        return Err(Fail::Usage(tr!(
            "diff-kernels braucht solidcore: mcad mit `--features solidcore` bauen \
             (cargo build -p cad-cli --features solidcore)",
            "diff-kernels needs solidcore: build mcad with `--features solidcore` \
             (cargo build -p cad-cli --features solidcore)"
        )));
    }
    for f in files {
        let src = std::fs::read_to_string(f).map_err(|e| Fail::Usage(format!("{}: {e}", f.display())))?;
        let ra = evaluate(SmlibKernel::new(), &src, f, |_| (0, std::time::Duration::ZERO), brep);
        let solid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            evaluate(SolidcoreKernel::new(), &src, f, SolidcoreKernel::copy_stats, brep)
        }));
        let rb = solid.ok();
        let (nodes, a) = (&ra.nodes, &ra.metrics);
        let mut diffs = Vec::new();
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let pos: BTreeMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();
        // solidcore's metrics per node of the SMLib tree
        let aborted: Vec<bool> = (0..nodes.len())
            .map(|i| {
                !rb.as_ref().is_some_and(|r| r.nodes.get(i).is_some_and(|n| n.0 == nodes[i].0 && n.1 == nodes[i].1))
            })
            .collect();
        let b: Vec<Metrics> = (0..nodes.len())
            .map(|i| match &rb {
                Some(r) if !aborted[i] => r.metrics[i].clone(),
                _ => Metrics { status: "error".into(), ..Default::default() },
            })
            .collect();
        let budget = budgets(nodes, &pos, a, &b);
        let blend = blends(nodes, &pos, a);
        // own verdict per node, then "blocked" where a descendant is unsupported or failed
        let mut own = Vec::with_capacity(nodes.len());
        for i in 0..nodes.len() {
            let mut notes = Vec::new();
            if aborted[i] {
                notes.push("solidcore: evaluation aborted (panic or other tree)".into());
            }
            let (verdict, vc) = compare(&a[i], &b[i], budget[i], blend[i], &mut notes);
            own.push((verdict, b[i].clone(), notes, vc));
        }
        fn broken(
            i: usize,
            nodes: &[NodeRow],
            pos: &BTreeMap<u32, usize>,
            own: &[(&str, Metrics, Vec<String>, Option<VolumeCheck>)],
        ) -> bool {
            nodes[i]
                .3
                .iter()
                .filter_map(|c| pos.get(c))
                .any(|&c| matches!(own[c].0, "unsupported" | "failed") || broken(c, nodes, pos, own))
        }
        // a zone selects edges of its sibling (the fillet/chamfer target): look at the parent
        let parent: BTreeMap<u32, usize> =
            nodes.iter().enumerate().flat_map(|(i, n)| n.3.iter().map(move |c| (*c, i))).collect();
        let blocked: Vec<bool> = (0..nodes.len())
            .map(|i| {
                let scope = if nodes[i].1 == "zone" { parent.get(&nodes[i].0).copied().unwrap_or(i) } else { i };
                matches!(own[i].0, "differ" | "failed") && broken(scope, nodes, &pos, &own)
            })
            .collect();
        for (i, ((idx, label, line, _), (verdict, mb, notes, volume_check))) in nodes.iter().zip(own).enumerate() {
            let (verdict, notes) = if blocked[i] {
                ("blocked", vec!["input from an unsupported or failed node".to_string()])
            } else {
                (verdict, notes)
            };
            let brep_check = match (&a[i].brep, &mb.brep) {
                (Some(x), Some(y)) if verdict == "same" && x.error.is_none() => Some(if let Some(e) = &y.error {
                    BrepCheck { verdict: if y.unsupported { "unsupported" } else { "differ" }, notes: vec![e.clone()] }
                } else {
                    let n = brep::compare(x, y);
                    BrepCheck { verdict: if n.is_empty() { "same" } else { "differ" }, notes: n }
                }),
                _ => None,
            };
            if let Some(c) = &brep_check {
                *brep_counts.entry(c.verdict).or_insert(0) += 1;
            }
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
            if verdict == "same"
                && let Some(c) = &volume_check
            {
                *rules.entry(c.rule).or_insert(0) += 1;
            }
            diffs.push(NodeDiff {
                idx: *idx,
                label: label.clone(),
                line: *line,
                verdict,
                smlib: a[i].clone(),
                solidcore: mb,
                volume_check,
                brep_check,
                notes,
            });
        }
        let name = f.display().to_string();
        text.push_str(&tr!(
            "{name}: {} Knoten, gleich {}, unsupported {}, blockiert {}, abweichend {}, gescheitert {}\n",
            "{name}: {} nodes, same {}, unsupported {}, blocked {}, differ {}, failed {}\n",
            diffs.iter().filter(|d| d.verdict != "skipped").count(),
            counts.get("same").unwrap_or(&0),
            counts.get("unsupported").unwrap_or(&0),
            counts.get("blocked").unwrap_or(&0),
            counts.get("differ").unwrap_or(&0),
            counts.get("failed").unwrap_or(&0),
        ));
        for d in diffs.iter().filter(|d| d.verdict == "differ" || d.verdict == "failed") {
            text.push_str(&tr!(
                "  Zeile {} {} [{}]: {}\n",
                "  line {} {} [{}]: {}\n",
                d.line,
                d.label,
                d.verdict,
                d.notes.join("; ")
            ));
        }
        for d in diffs.iter() {
            if let Some(c) = d.brep_check.as_ref().filter(|c| c.verdict == "differ") {
                text.push_str(&tr!(
                    "  Zeile {} {} [B-Rep]: {}\n",
                    "  line {} {} [B-rep]: {}\n",
                    d.line,
                    d.label,
                    c.notes.join("; ")
                ));
            }
        }
        let cb = rb.as_ref().map(|r| r.cost);
        cost_a.add(&ra.cost);
        if let Some(c) = &cb {
            cost_b.add(c);
        }
        out_files.push(json!({
            "file": name,
            "smlib_cost": ra.cost,
            "solidcore_cost": cb,
            "nodes": diffs,
            "same": counts.get("same").unwrap_or(&0),
            "unsupported": counts.get("unsupported").unwrap_or(&0),
            "blocked": counts.get("blocked").unwrap_or(&0),
            "differ": counts.get("differ").unwrap_or(&0),
            "failed": counts.get("failed").unwrap_or(&0),
        }));
    }
    text.push_str(trs!(
        "\nAbdeckung solidcore je Operation (gleich / Knoten; unsupported, blockiert, abweichend):\n",
        "\nCoverage of solidcore per operation (same / nodes; unsupported, blocked, differ):\n"
    ));
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
    let n: usize =
        ["same", "unsupported", "blocked", "differ", "failed"].iter().map(|k| totals.get(k).unwrap_or(&0)).sum();
    text.push_str(&tr!(
        "\nGesamt: {n} Knoten, gleich {}, unsupported {}, blockiert {}, abweichend {}, gescheitert {}\n",
        "\nTotal: {n} nodes, same {}, unsupported {}, blocked {}, differ {}, failed {}\n",
        totals.get("same").unwrap_or(&0),
        totals.get("unsupported").unwrap_or(&0),
        totals.get("blocked").unwrap_or(&0),
        totals.get("differ").unwrap_or(&0),
        totals.get("failed").unwrap_or(&0)
    ));
    let g = |k: &str| rules.get(k).copied().unwrap_or(0);
    text.push_str(&tr!(
        "Volumen der gleichen Knoten: relativ 1e-6 {}, exakter Wert {}, Operanden {}, gekrümmt 3e-5 {}, \
         Rundung 1e-4 {}\n",
        "Volume of the same nodes: relative 1e-6 {}, exact value {}, operands {}, curved 3e-5 {}, blend 1e-4 {}\n",
        g("relative"),
        g("exact"),
        g("operands"),
        g("curved"),
        g("blend")
    ));
    let gb = |k: &str| brep_counts.get(k).copied().unwrap_or(0);
    if brep {
        text.push_str(&tr!(
            "B-Rep der gleichen Knoten: gleich {}, abweichend {}, unsupported {}\n",
            "B-rep of the same nodes: same {}, differ {}, unsupported {}\n",
            gb("same"),
            gb("differ"),
            gb("unsupported")
        ));
    }
    let ratio = |a: f64, b: f64| if a > 0.0 { b / a } else { 0.0 };
    text.push_str(&tr!(
        "Zeit: SMLib Auswertung {} ms, Abfragen {} ms; solidcore Auswertung {} ms ({}×), Abfragen {} ms ({}×), \
         {} Kopien in {} ms\n",
        "Time: SMLib evaluation {} ms, queries {} ms; solidcore evaluation {} ms ({}×), queries {} ms ({}×), \
         {} copies in {} ms\n",
        crate::num(cost_a.eval_ms),
        crate::num(cost_a.query_ms),
        crate::num(cost_b.eval_ms),
        crate::num(ratio(cost_a.eval_ms, cost_b.eval_ms)),
        crate::num(cost_b.query_ms),
        crate::num(ratio(cost_a.query_ms, cost_b.query_ms)),
        cost_b.copies,
        crate::num(cost_b.copy_ms)
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
                     "failed": totals.get("failed").unwrap_or(&0),
                     "volume_rules": rules,
                     "brep": brep_counts,
                     "smlib_cost": cost_a, "solidcore_cost": cost_b,
                     "eval_ratio": ratio(cost_a.eval_ms, cost_b.eval_ms),
                     "query_ratio": ratio(cost_a.query_ms, cost_b.query_ms) },
    });
    Ok(Outcome { json: j, text, errors: !all_same, exit: None })
}
