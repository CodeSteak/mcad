//! `mcad diff-reference REFERENCE [files…] [--json]`: evaluates each file of a reference file
//! (`tests/kernel-reference.json`: the per-node results SMLib, the former reference kernel, gave
//! before it was removed) with solidcore and compares every node against the frozen results
//! (plan sections 2.3 and 8, phase 5.2).
//!
//! Files are evaluated like the other commands do (`include`/`use`/`import` relative to the file).
//! Per node of the reference: status, volume, number of faces and edges of its result (exactly
//! equal), and the edges selected by fillet/chamfer (count and points, 1e-6 of the size).
//! Volumes ([`volume_check`]): relative 1e-6; else, where the exact volume is known (recorded
//! with the reference from the parameters of the operations: primitives, plain extrusions,
//! revolutions and their affine images), solidcore at most as far from it as the reference; else
//! within the summed deviations of both from the exact volumes of the operands; else, for curved
//! results, 3e-5 (both integrate over curved faces, and SMLib fitted skins and sweeps less
//! closely: its sphere r 5 is 7.9e-7 below the exact volume, its twisted bar 1.5e-5 above); else,
//! for a result with a fillet, chamfer, thicken, shell or offset in its history, 8 % of the
//! volume those operations changed plus 4e-6 of the node (both fit the blend and offset
//! surfaces; the `curved` rule does not apply there), where the B-reps' kinds agree only if
//! their edge and face positions agree as well (the reference holds the B-rep of every blended
//! node).
//! Selected edges match in any order, by all their sample points (either direction; another
//! parameterisation or start point within the chordal sag of the other's samples).
//! "Coverage" counts per operation (node label) how many nodes solidcore evaluated like the
//! reference. Verdicts: `same`; `unsupported` (solidcore reports the operation, or a query of its
//! result — faces, edges, volume —, as not supported, also in a warning such as "B-rep from the
//! mesh failed", or a variable radius it cannot build); `blocked` (differs only because an input
//! node is unsupported or failed); `differ`; `failed` (solidcore fails where the reference
//! succeeds); `skipped` (not evaluated in the reference either).
//!
//! JSON: `{ schema_version, command: "diff-reference", ok, exit_code, reference, reference_kernel,
//!   solidcore_rev, files: [{ file, nodes: [NodeDiff], same, unsupported, blocked, differ,
//!   failed, cost, mesh }], coverage: { label: { nodes, same, unsupported, … } }, summary }`
//! with `NodeDiff = { idx, label, line,
//!   verdict: "same"|"unsupported"|"blocked"|"differ"|"failed"|"skipped",
//!   reference: Metrics, solidcore: Metrics, volume_check: VolumeCheck|null, notes: [string] }`,
//! `Metrics = { status, volume, exact_volume, curved, faces, edges, selected_edges, message,
//! blend_change, brep }`, `VolumeCheck = { rule: "relative"|"exact"|"operands"|"curved"|"blend"|"none",
//! tolerance, difference, exact, reference_error, solidcore_error }`; `summary.volume_rules`:
//! same nodes per rule. Cost per file and in the summary (sums; heap: maximum): `{ eval_ms,
//! query_ms, heap_bytes, copies, copy_ms }` (plan 0.5).
//! With `--mesh` also the mesh of each file's result (`diff/mesh.rs`, plan 2.7): `files[].mesh`,
//! `summary.mesh`; they do not change the exit code.
//! Exit code 0 if every node of every file is the same, 1 otherwise, 2 usage error.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cad_i18n::{tr, trs};
use cad_kernel::Kernel;
use cad_session::protocol::{self as p, ToEngine};
use cad_session::{PROTOCOL_VERSION, Session, SessionOptions, std_clock};
use kernel_solidcore::SolidcoreKernel;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Fail, Outcome};

mod brep;
mod mesh;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub status: String,
    pub volume: Option<f64>,
    /// Exact volume from the parameters (primitives, plain extrusions, revolutions and their
    /// affine images), `None` where unknown; recorded with the reference.
    pub exact_volume: Option<f64>,
    /// Some edge of the result is not straight (so some face is curved and its volume integral
    /// is approximate).
    pub curved: bool,
    pub faces: Option<usize>,
    pub edges: Option<usize>,
    pub selected_edges: Vec<Vec<[f64; 3]>>,
    pub message: Option<String>,
    /// The volume the blends and offsets in the history of the node's shapes changed (rule
    /// `blend`; NaN where a volume failed); recorded with the reference.
    pub blend_change: f64,
    /// The neutral B-rep of the node's parts (`Kernel::brep`) where the reference has one (the
    /// blended nodes).
    pub brep: Option<brep::Summary>,
    #[serde(skip)]
    pub unsupported: bool,
}

/// How the volumes of a node were compared (see [`volume_check`]).
#[derive(Debug, Clone, Serialize)]
pub struct VolumeCheck {
    /// `relative`: |reference − solidcore| ≤ 1e-6 |reference|; `exact`: solidcore at most as
    /// far from the exact volume as the reference (or within 1e-6 of it); `operands`: the
    /// difference is within the summed deviations of both from the exact volumes of the
    /// operands; `curved`: curved faces, within 3e-5 (of the reference and of an exact value);
    /// `blend`: within the share of the volume the blends changed; `none`: none of them.
    pub rule: &'static str,
    /// Allowed difference (absolute, mm³) of the rule that applied, or of `relative`.
    pub tolerance: f64,
    pub difference: f64,
    pub exact: Option<f64>,
    /// |volume − exact| / exact.
    pub reference_error: Option<f64>,
    pub solidcore_error: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDiff {
    pub idx: u32,
    pub label: String,
    pub line: u32,
    pub verdict: &'static str,
    pub reference: Metrics,
    pub solidcore: Metrics,
    /// Present when both gave a volume.
    pub volume_check: Option<VolumeCheck>,
    pub notes: Vec<String>,
}

/// The frozen per-node results of the reference kernel (`tests/kernel-reference.json`): the
/// [`Metrics`] `diff-reference` compares, per node of each file, file paths relative to the
/// reference file's directory, one node per line.
#[derive(Debug, Clone, Deserialize)]
pub struct Reference {
    pub schema: u32,
    /// Kernel the results come from.
    pub kernel_id: String,
    pub files: Vec<RefFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefFile {
    pub file: String,
    pub nodes: Vec<RefNode>,
}

/// One node: its place in the tree and the reference kernel's metrics.
#[derive(Debug, Clone, Deserialize)]
pub struct RefNode {
    pub idx: u32,
    pub label: String,
    pub line: u32,
    #[serde(default)]
    pub children: Vec<u32>,
    pub status: String,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub exact_volume: Option<f64>,
    #[serde(default)]
    pub curved: bool,
    #[serde(default)]
    pub faces: Option<usize>,
    #[serde(default)]
    pub edges: Option<usize>,
    #[serde(default)]
    pub selected_edges: Vec<Vec<[f64; 3]>>,
    #[serde(default)]
    pub message: Option<String>,
    /// [`Metrics::blend_change`]; `null` for NaN (a volume of the history failed).
    #[serde(default, deserialize_with = "nan_as_null")]
    pub blend_change: f64,
    #[serde(default)]
    pub brep: Option<RefBrep>,
}

/// A B-rep summary with what [`brep::compare`] matches.
#[derive(Debug, Clone, Deserialize)]
pub struct RefBrep {
    #[serde(flatten)]
    pub summary: brep::Summary,
    pub shape: brep::Shape,
}

fn nan_as_null<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
}

impl RefNode {
    fn row(&self) -> NodeRow {
        (self.idx, self.label.clone(), self.line, self.children.clone())
    }

    fn metrics(&self) -> Metrics {
        Metrics {
            status: self.status.clone(),
            volume: self.volume,
            exact_volume: self.exact_volume,
            curved: self.curved,
            faces: self.faces,
            edges: self.edges,
            selected_edges: self.selected_edges.clone(),
            message: self.message.clone(),
            blend_change: self.blend_change,
            brep: self.brep.clone().map(|b| brep::Summary { shape: b.shape, ..b.summary }),
            unsupported: self.message.as_deref().is_some_and(is_unsupported),
        }
    }
}

/// `file` relative to `dir` (both made absolute; `/` as separator).
fn relative_to(file: &Path, dir: &Path) -> Result<String, Fail> {
    let abs = |p: &Path| std::fs::canonicalize(p).map_err(|e| Fail::Usage(format!("{}: {e}", p.display())));
    let (f, d) = (abs(file)?, abs(dir)?);
    let rel = f.strip_prefix(&d).map_err(|_| {
        Fail::Usage(tr!(
            "{}: liegt nicht unter dem Verzeichnis der Referenzdatei {}",
            "{}: not below the directory of the reference file {}",
            file.display(),
            dir.display()
        ))
    })?;
    Ok(rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// Reads a reference file.
pub fn read_reference(path: &Path) -> Result<Reference, Fail> {
    let text = std::fs::read_to_string(path).map_err(|e| Fail::Usage(format!("{}: {e}", path.display())))?;
    let r: Reference = serde_json::from_str(&text).map_err(|e| {
        Fail::Usage(tr!("{}: keine Referenzdatei: {e}", "{}: not a reference file: {e}", path.display()))
    })?;
    if r.schema != 1 {
        return Err(Fail::Usage(tr!(
            "{}: Referenzdatei mit Schema {}, erwartet 1",
            "{}: reference file of schema {}, expected 1",
            path.display(),
            r.schema
        )));
    }
    Ok(r)
}

/// A kernel message naming an operation the kernel cannot do (verdict `unsupported`): the
/// kernel's `Unsupported` error, or the engine's diagnostic for a variable radius it cannot
/// build (both languages).
fn is_unsupported(m: &str) -> bool {
    m.contains(UNSUPPORTED)
        || m.contains("variable radius is not supported")
        || m.contains("variabler Radius wird nicht unterstützt")
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
/// Results with a fillet, chamfer, thicken, shell or offset at or below the node: both kernels
/// fit the blend and offset surfaces, SMLib less closely (`pillow_block`: its chamfers of 19 edges
/// remove 39.15 mm³, by Pappus 41.40, solidcore 41.39: 5.4 % of the change). The tolerance is
/// relative to the volume those operations changed (recorded with the reference, summed over
/// the node's history), not to the node; plus [`REL_VOLUME_BLEND_REST`] of a curved node for the
/// integration of the rest. A wrong radius changes the volume by less than that (a cube with
/// `$fillet = 1` at × 1.05): where both B-reps have the same kinds, their edge and face positions
/// must agree as well (they do to 1e-12, OCCT: both kernels build the same blend surfaces; × 1.02
/// is caught); elsewhere [`REL_VOLUME_BLEND_UNCHECKED`].
const REL_VOLUME_BLEND: f64 = 0.08;
/// Rule `blend` where the B-reps' kinds differ (no position check): 5 % of the change (largest
/// need in the corpus 3.7 %, `key_rack`).
const REL_VOLUME_BLEND_UNCHECKED: f64 = 0.05;
/// The part of rule `blend` for the unblended rest of a curved node (curved primitives agree
/// within 4.3e-6); planar rests agree within [`REL_VOLUME`].
const REL_VOLUME_BLEND_REST: f64 = 4e-6;
/// Rule `operands` at most this, relative to the node's volume: the operands' deviations
/// summed can exceed a small result many times (a 1 mm cube on a sphere of r 1000).
const REL_VOLUME_OPERANDS: f64 = 1e-4;

/// One evaluation: (index, label, line, children) and metrics per scene node.
struct Run {
    nodes: Vec<NodeRow>,
    metrics: Vec<Metrics>,
    cost: Cost,
    /// The meshes of the root's parts (`--mesh`).
    mesh: Option<mesh::Parts>,
}

/// Cost of one evaluation (plan 0.5).
#[derive(Debug, Clone, Copy, Default, Serialize)]
struct Cost {
    /// Evaluation of the file (language and kernel operations), ms.
    eval_ms: f64,
    /// The queries of `diff-reference` itself (faces, edges, volume of every node), ms.
    query_ms: f64,
    /// Heap held after the evaluation and the queries (glibc `mallinfo2`), bytes; `null`
    /// elsewhere.
    heap_bytes: Option<usize>,
    /// B-rep copies of the kernel (`SmApiBrepCopy`) and their time in ms.
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

/// Evaluates `text` with solidcore and measures every node; the B-rep of the node at position
/// `i` of the scene where `with_brep[i]` (the blended nodes of the reference).
fn evaluate(text: &str, file: &Path, with_brep: &[bool], mesh_tol: Option<f64>) -> Run {
    let heap0 = crate::daemon::heap_in_use();
    let mut opts = SessionOptions { auto_root_mesh: false, ..Default::default() };
    opts.eval.max_stack_bytes = 192 << 20;
    let mut s = Session::new(SolidcoreKernel::new(), opts, std_clock());
    crate::kernel::use_jobs(s.engine_mut(), crate::kernel::jobs());
    // `include`/`use` relative to the file, as in the other commands
    let root = file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let main = file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    s.set_resolver(Box::new(cad_session::FsResolver { root: root.to_path_buf() }), &main);
    let t0 = std::time::Instant::now();
    s.post(ToEngine::SetSource { text: text.to_string(), version: 1 });
    s.run_to_idle();
    let mut cost = Cost { eval_ms: ms(t0.elapsed()), ..Default::default() };
    let t1 = std::time::Instant::now();
    let Some(d) = s.done() else { return Run { nodes: Vec::new(), metrics: Vec::new(), cost, mesh: None } };
    let scene = d.scene.clone();
    let mut nodes = Vec::new();
    let mut out = Vec::new();
    for (i, n) in scene.nodes.iter().enumerate() {
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
            .or_else(|| n.messages.iter().find(|m| m.severity == p::Severity::Warning && is_unsupported(&m.message)))
            .map(|m| m.message.clone());
        let unsupported = message.as_deref().is_some_and(is_unsupported);
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
            // blended nodes: their verdict compares the positions (see `blend_tolerance`)
            if with_brep.get(i).copied().unwrap_or(false) && !meshes && !parts.is_empty() {
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
                m.unsupported = is_unsupported(&e);
                m.message = Some(e);
            }
        }
        out.push(m);
    }
    cost.query_ms = ms(t1.elapsed());
    let (n, t) = s.engine_mut().kernel_mut().copy_stats();
    (cost.copies, cost.copy_ms) = (n, ms(t));
    cost.heap_bytes = crate::daemon::heap_in_use().zip(heap0).map(|(a, b)| a.saturating_sub(b));
    let mesh = mesh_tol.map(|tol| {
        let root = s.done().and_then(|d| d.ev.root());
        let shapes: Vec<_> = s
            .done()
            .zip(root)
            .map(|(d, r)| d.ev.parts(r).iter().map(|p| (p.shape.clone(), p.solid)).collect())
            .unwrap_or_default();
        mesh::mesh_parts(s.engine_mut().kernel_mut(), &shapes, tol)
    });
    Run { nodes, metrics: out, cost, mesh }
}

/// Compares the volumes `x` (reference) and `y` (solidcore). `budget`: summed deviation of both
/// kernels from the exact volumes of the operands (see `command`). The rules in order:
/// `relative` (1e-6), `exact` (solidcore at most as far from the exact volume as SMLib, at least
/// 1e-6), `operands` (no exact value, difference ≤ budget, at most 1e-4 of the volume, + 1e-6),
/// `curved` (curved faces:
/// difference ≤ 3e-5, and solidcore within 3e-5 of an exact value), `blend` (a fillet or chamfer
/// in the node's history: difference ≤ `blend`, see [`blends`], [`blend_tolerance`]); else
/// `none`.
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
        reference_error: err(x),
        solidcore_error: err(y),
    };
    if difference <= rel {
        return c;
    }
    if let (Some(e), Some(ea), Some(eb)) = (exact, c.reference_error, c.solidcore_error) {
        let allowed = ea.max(REL_VOLUME);
        if eb <= allowed {
            c.rule = "exact";
            c.tolerance = allowed * e;
            return c;
        }
    }
    if let Some(b) = budget
        && exact.is_none()
        && difference <= b.min(REL_VOLUME_OPERANDS * x.abs().max(y.abs())) + rel
    {
        c.rule = "operands";
        c.tolerance = b.min(REL_VOLUME_OPERANDS * x.abs().max(y.abs())) + rel;
        return c;
    }
    // with an exact value solidcore must also be within 3e-5 of it
    let near_exact = c.solidcore_error.is_none_or(|e| e <= REL_VOLUME_CURVED);
    // not for blended results: their tolerance is rule `blend`'s, from the volume the blends changed
    if a.curved && blend.is_none() && near_exact && difference <= REL_VOLUME_CURVED * x.abs() {
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

/// Per node whose shapes SMLib blended (fillet, chamfer) or offset (`thicken`, `shell`,
/// `offset`; SMLib approximates offset surfaces as B-splines: a thickened B-spline sheet in
/// `freeform_lid` is 3.3e-5 off a numerical integration, solidcore 1.6e-6) somewhere in their
/// history: the tolerance of rule `blend`, [`REL_VOLUME_BLEND`] of the volume those operations
/// changed plus [`REL_VOLUME_BLEND_REST`] of the node; `None` without one (or where a volume of
/// that history failed).
fn blends(a: &[Metrics]) -> Vec<Option<Blend>> {
    a.iter()
        .map(|m| {
            let c = m.blend_change;
            let rest = if m.curved { REL_VOLUME_BLEND_REST * m.volume.unwrap_or(0.0).abs() } else { 0.0 };
            (c.is_finite() && c > 0.0).then_some(Blend { change: c, rest })
        })
        .collect()
}

/// Rule `blend` of a node: the volume its blends changed and the allowance for the rest.
#[derive(Debug, Clone, Copy)]
struct Blend {
    change: f64,
    rest: f64,
}

/// Rule `blend`'s tolerance from the B-reps of both kernels: `None` if the positions differ
/// where the kinds agree (then the volume cannot excuse the node).
fn blend_tolerance(b: Blend, a: &Metrics, c: &Metrics, notes: &mut Vec<String>) -> Option<f64> {
    let cmp = match (&a.brep, &c.brep) {
        (Some(x), Some(y)) if x.error.is_none() && y.error.is_none() => Some(brep::compare(x, y)),
        _ => None,
    };
    match cmp {
        Some(k) if k.kinds_agree && !k.positions_agree => {
            notes.push(format!("blend: B-reps at other positions ({})", k.notes.join("; ")));
            None
        }
        Some(k) if k.kinds_agree => Some(REL_VOLUME_BLEND * b.change + b.rest),
        _ => Some(REL_VOLUME_BLEND_UNCHECKED * b.change + b.rest),
    }
}

/// The same edge, sampled: every point within `tol`, in order or reversed; else (another
/// parameterisation of the same curve, e.g. a B-spline for a circle, or a closed edge from
/// another start point) every point of each within `tol` plus twice the chordal sag of the other's
/// polyline (estimated from its points) of that polyline, and the ends equal unless both are closed.
/// Limit: in that second case an edge within about that sag passes too (18 samples of a circle
/// r 10: 0.19 mm, so r 10.1 or an offset of 0.2 mm along the axis is not told apart); the face
/// and edge counts and the volume of the node still are compared exactly.
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

/// Sums of the mesh checks (`--mesh`) over the files.
#[derive(Debug, Clone, Default, Serialize)]
struct MeshTotals {
    files: usize,
    verdicts: BTreeMap<&'static str, usize>,
    /// Files whose solid parts all meshed watertight.
    watertight: usize,
    triangles: usize,
    ms: f64,
    brep_ms: f64,
    surface_max: f64,
    chord_max: f64,
    /// Largest `|V − exact| / (tol · area)`.
    volume_ratio_max: f64,
}

impl MeshTotals {
    fn add(&mut self, c: &mesh::Check) {
        *self.verdicts.entry(c.verdict).or_insert(0) += 1;
        if c.verdict == "skipped" {
            return;
        }
        let m = &c.metrics;
        self.files += 1;
        self.watertight += usize::from(m.watertight && m.error.is_none());
        self.triangles += m.triangles;
        self.ms += m.ms;
        self.brep_ms += m.brep_ms;
        self.surface_max = self.surface_max.max(m.surface.map_or(0.0, |s| s.max));
        self.chord_max = self.chord_max.max(m.chord.map_or(0.0, |s| s.max));
        self.volume_ratio_max = self.volume_ratio_max.max(m.volume_ratio.unwrap_or(0.0));
    }
}

fn compare(
    a: &Metrics,
    b: &Metrics,
    budget: Option<f64>,
    blend: Option<Blend>,
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
    blend: Option<Blend>,
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
        notes.push("the reference fails, solidcore not".into());
        return "differ";
    }
    match (a.volume, b.volume) {
        (Some(x), Some(y)) => {
            let blend = blend.and_then(|bl| blend_tolerance(bl, a, b, notes));
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

/// Smallest `--mesh-tol` (as `measure --mesh-tol`).
pub const MIN_MESH_TOL: f64 = 1e-4;
/// Largest `--mesh-tol`: coarser meshes are a few triangles per face, their check says
/// nothing about the tessellator.
pub const MAX_MESH_TOL: f64 = 10.0;

pub fn command(reference: &Path, files: &[PathBuf], mesh_tol: Option<f64>) -> Result<Outcome, Fail> {
    if let Some(t) = mesh_tol {
        if !t.is_finite() {
            return Err(Fail::Usage(tr!(
                "--mesh-tol {t} ist keine endliche Zahl",
                "--mesh-tol {t} is not a finite number"
            )));
        }
        if t < MIN_MESH_TOL {
            return Err(Fail::Usage(tr!(
                "--mesh-tol {t} ist kleiner als {MIN_MESH_TOL} mm",
                "--mesh-tol {t} is below {MIN_MESH_TOL} mm"
            )));
        }
        if t > MAX_MESH_TOL {
            return Err(Fail::Usage(tr!(
                "--mesh-tol {t} ist größer als {MAX_MESH_TOL} mm (gröbere Netze sind wenige Dreiecke je \
                 Fläche, ihre Prüfung sagt nichts)",
                "--mesh-tol {t} is above {MAX_MESH_TOL} mm (coarser meshes are a few triangles per \
                 face, their check says nothing)"
            )));
        }
    }
    let r = read_reference(reference)?;
    let dir = reference.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    // the reference's files, or those of `files` (which it must hold)
    let mut chosen: Vec<(PathBuf, &RefFile)> = Vec::new();
    if files.is_empty() {
        chosen.extend(r.files.iter().map(|f| (dir.join(&f.file), f)));
    }
    for f in files {
        let rel = relative_to(f, &dir)?;
        let Some(rf) = r.files.iter().find(|x| x.file == rel) else {
            return Err(Fail::Usage(tr!(
                "{}: nicht in der Referenzdatei {}",
                "{}: not in the reference file {}",
                f.display(),
                reference.display()
            )));
        };
        chosen.push((f.clone(), rf));
    }
    if chosen.is_empty() {
        return Err(Fail::Usage(tr!("keine Dateien in {}", "no files in {}", reference.display())));
    }
    let mut out_files = Vec::new();
    let mut coverage: BTreeMap<String, BTreeMap<&'static str, usize>> = BTreeMap::new();
    let mut text = String::new();
    let mut all_same = true;
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();
    // same nodes per volume rule
    let mut rules: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut cost = Cost::default();
    let mut meshes = MeshTotals::default();
    for (f, rf) in &chosen {
        let src = std::fs::read_to_string(f).map_err(|e| Fail::Usage(format!("{}: {e}", f.display())))?;
        let nodes: Vec<NodeRow> = rf.nodes.iter().map(RefNode::row).collect();
        let a: Vec<Metrics> = rf.nodes.iter().map(RefNode::metrics).collect();
        let with_brep: Vec<bool> = a.iter().map(|m| m.brep.is_some()).collect();
        let solid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| evaluate(&src, f, &with_brep, mesh_tol)));
        let rb = solid.ok();
        let mut diffs = Vec::new();
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let pos: BTreeMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();
        // solidcore's metrics per node of the reference's tree
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
        let budget = budgets(&nodes, &pos, &a, &b);
        let blend = blends(&a);
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
                matches!(own[i].0, "differ" | "failed") && broken(scope, &nodes, &pos, &own)
            })
            .collect();
        for (i, ((idx, label, line, _), (verdict, mb, notes, volume_check))) in nodes.iter().zip(own).enumerate() {
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
                reference: a[i].clone(),
                solidcore: mb,
                volume_check,
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
        let mesh_check = mesh_tol.map(|tol| match rb.as_ref().and_then(|r| r.mesh.as_ref()) {
            Some(b) => mesh::check(b, tol),
            None => mesh::Check {
                tol,
                verdict: "failed",
                notes: vec!["solidcore: evaluation aborted".into()],
                ..Default::default()
            },
        });
        if let Some(c) = &mesh_check {
            meshes.add(c);
            if !matches!(c.verdict, "ok" | "skipped") || !c.notes.is_empty() {
                text.push_str(&tr!("  Netz [{}]: {}\n", "  mesh [{}]: {}\n", c.verdict, c.notes.join("; ")));
            }
        }
        let cb = rb.as_ref().map(|r| r.cost);
        if let Some(c) = &cb {
            cost.add(c);
        }
        out_files.push(json!({
            "file": name,
            "cost": cb,
            "nodes": diffs,
            "mesh": mesh_check,
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
         Rundung {}\n",
        "Volume of the same nodes: relative 1e-6 {}, exact value {}, operands {}, curved 3e-5 {}, blend {}\n",
        g("relative"),
        g("exact"),
        g("operands"),
        g("curved"),
        g("blend")
    ));
    if let Some(tol) = mesh_tol {
        let m = &meshes;
        let g = |k: &str| m.verdicts.get(k).copied().unwrap_or(0);
        text.push_str(&tr!(
            "Netze (tol {}): ok {}, abweichend {}, gescheitert {}; wasserdicht {} von {}; Sehnenfehler max {} mm; \
             Dreiecke {}; Zeit {} ms (davon B-Rep {} ms)\n",
            "Meshes (tol {}): ok {}, differ {}, failed {}; watertight {} of {}; chord error max {} mm; \
             triangles {}; time {} ms (B-rep {} ms of it)\n",
            crate::num(tol),
            g("ok"),
            g("differ"),
            g("failed"),
            m.watertight,
            m.files,
            crate::num(m.chord_max),
            m.triangles,
            crate::num(m.ms),
            crate::num(m.brep_ms)
        ));
    }
    text.push_str(&tr!(
        "Zeit: Auswertung {} ms, Abfragen {} ms, {} Kopien in {} ms\n",
        "Time: evaluation {} ms, queries {} ms, {} copies in {} ms\n",
        crate::num(cost.eval_ms),
        crate::num(cost.query_ms),
        cost.copies,
        crate::num(cost.copy_ms)
    ));
    let j = json!({
        "schema_version": PROTOCOL_VERSION,
        "command": "diff-reference",
        "ok": all_same,
        "exit_code": 0,
        "reference": reference.display().to_string(),
        "reference_kernel": r.kernel_id,
        "solidcore_rev": kernel_solidcore::SOLIDCORE_REV,
        "files": out_files,
        "coverage": coverage,
        "summary": { "nodes": n, "same": totals.get("same").unwrap_or(&0),
                     "unsupported": totals.get("unsupported").unwrap_or(&0),
                     "blocked": totals.get("blocked").unwrap_or(&0),
                     "differ": totals.get("differ").unwrap_or(&0),
                     "failed": totals.get("failed").unwrap_or(&0),
                     "volume_rules": rules,
                     "mesh": mesh_tol.map(|_| &meshes),
                     "cost": cost },
    });
    Ok(Outcome { json: j, text, errors: !all_same, exit: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule `blend` scales with the volume the blends changed, not with the node: a box edge of
    /// 100 with all 12 edges filleted r 1 changes 12 · 100 · (1 − π/4) ≈ 258 mm³; a radius 5 %
    /// too large changes 10 % more and is caught. Without B-reps to compare (or with other
    /// kinds) 5 % of the change; with the same kinds the positions decide.
    #[test]
    fn blend_tolerance_follows_the_blended_volume() {
        let change = 12.0 * 100.0 * (1.0 - std::f64::consts::PI / 4.0);
        let v = 1e6 - change;
        let a = Metrics { volume: Some(v), curved: true, blend_change: change, ..Default::default() };
        let bl = blends(std::slice::from_ref(&a))[0].unwrap();
        let mut notes = Vec::new();
        let tol = blend_tolerance(bl, &a, &a, &mut notes);
        assert!(tol.is_some() && notes.is_empty());
        let wrong = 1e6 - change * 1.05 * 1.05;
        assert_eq!(volume_check(v, wrong, &a, None, tol).rule, "none");
        assert_eq!(volume_check(v, 1e6 - change * 1.04, &a, None, tol).rule, "blend");
        assert_eq!(volume_check(v, 1e6 - change * 1.08, &a, None, tol).rule, "none");
        // planar results get no allowance for the rest
        let p = Metrics { curved: false, ..a.clone() };
        assert_eq!(blends(std::slice::from_ref(&p))[0].unwrap().rest, 0.0);
        // without a blend the curved rule takes the same difference within 3e-5 of the node
        let c = Metrics { volume: Some(v), curved: true, ..Default::default() };
        assert_eq!(volume_check(v, wrong, &c, None, None).rule, "curved");
    }
}
