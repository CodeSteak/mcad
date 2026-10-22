//! Engine core: evaluates a [`cad_ir::Tree`] into kernel shapes, generic over
//! [`cad_kernel::Kernel`].
//!
//! * **Evaluation** bottom-up (post-order). Every node's result is expressed in the node's own
//!   system (`Node.world`); a parent takes a child's result through `child.local`. Transform
//!   nodes therefore need no extra work: their children carry the matrix in `local`.
//! * **Cache:** every node has a content [`CacheKey`] (operation, resolved parameters,
//!   `$fillet`/`$chamfer`, keys/relative matrices/modifier flags of the children, kernel id;
//!   never the world matrix). Node results (shapes, diagnostics, hints, selected edges, bounds)
//!   live in an LRU owned by the [`Engine`] and survive evaluations.
//! * **Provenance:** the engine assigns face tags. A tag is an interned index of the creating
//!   node's key (stable for equal content for the engine's lifetime). After an evaluation
//!   [`Evaluation::tag_nodes`] maps tags to the nodes of that tree (several rows for nodes with
//!   equal content). Each result also carries the set of tags it can contain, which assigns
//!   faces to children (new edges of `difference(fillet = r)`).
//! * **Stepwise:** [`Engine::start`] returns a [`Job`]; [`Job::step`] runs until a [`Budget`]
//!   (kernel calls or milliseconds from an injected clock) is used up. A step may stop between
//!   any two kernel calls; the calls already made for the interrupted node are replayed on the
//!   next step, finished nodes go to the cache. Dropping the job aborts the evaluation.
//! * **Failures** follow the spec: a failing operation yields its inputs without the
//!   operation (fillet → body without fillet), with an error diagnostic at the node; the
//!   evaluation continues.
//!
//! 2D subtrees under `linear_extrude`/`rotate_extrude`/`sweep` are "lifted": every leaf profile
//! becomes a solid with its accumulated 2D frame and the 2D booleans are rebuilt as 3D booleans
//! (see `profile.rs`). 2D nodes on their own are shown as filled polygons
//! ([`Engine::tessellate`]).

mod cache;
pub mod counting;
mod eval;
pub mod export;
pub mod key;
mod profile;

use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use cad_ir::{Dim, NodeIdx, Op, Tree};
use cad_kernel::{Kernel, MassProps, Mesh, Tag};
use cad_syntax::Span;
use cad_syntax::ast::StmtId;
use glam::{DMat4, DVec3};
use serde::Serialize;

pub use cache::Lru;
pub use counting::CountingKernel;
pub use key::{CacheKey, KeyHasher};

/// Default display tolerance in mm when neither the call nor `$tol` gives one.
pub const DEFAULT_TOL: f64 = 0.05;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum number of cached node results.
    pub cache_entries: usize,
    /// Maximum estimated size of cached meshes in bytes.
    pub mesh_cache_bytes: usize,
    /// Interior sample points per edge for zone selection and new-edge detection.
    pub edge_samples: u32,
    /// Sample points per face for `shell` zones.
    pub face_samples: u32,
    pub default_tol: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            cache_entries: 4096,
            mesh_cache_bytes: 256 << 20,
            edge_samples: 8,
            face_samples: 4,
            default_tol: DEFAULT_TOL,
        }
    }
}

/// Monotonic clock in milliseconds (injected; `std::time::Instant` panics on
/// wasm32-unknown-unknown — use `performance.now()` there).
pub type Clock = Box<dyn Fn() -> f64>;

/// A clock based on `std::time::Instant` (not for wasm32-unknown-unknown).
#[cfg(not(target_arch = "wasm32"))]
pub fn std_clock() -> Clock {
    let t0 = std::time::Instant::now();
    Box::new(move || t0.elapsed().as_secs_f64() * 1000.0)
}

/// Limit for one [`Job::step`]. A step always makes progress (at least one kernel call or
/// one finished node); `None` fields are unlimited.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Budget {
    pub max_ops: Option<u64>,
    /// Needs a clock ([`Engine::set_clock`]); ignored without one.
    pub max_ms: Option<f64>,
}

impl Budget {
    pub const UNLIMITED: Budget = Budget { max_ops: None, max_ms: None };

    pub fn ops(n: u64) -> Self {
        Budget { max_ops: Some(n), max_ms: None }
    }

    pub fn ms(ms: f64) -> Self {
        Budget { max_ops: None, max_ms: Some(ms) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity {
    Warning,
    Error,
}

/// Diagnostic as stored in a (cached) node result; [`Diagnostic`] adds the node's position.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Diag {
    pub severity: Severity,
    pub message: String,
    pub edges: Vec<Vec<DVec3>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub node: NodeIdx,
    pub stmt: StmtId,
    pub span: Span,
    /// Affected edges as polylines in the node's own system (e.g. the edge a fillet failed on).
    pub edges: Vec<Vec<DVec3>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Status {
    Ok,
    Warning,
    Error,
    /// Disabled (`*`) or not reachable from the root: not evaluated.
    Skipped,
}

/// One solid (or open surface) of a node result, in the node's own system.
#[derive(Debug, Clone)]
pub struct Part<S> {
    pub shape: S,
    /// `false` for open surfaces (`surface`), which must not enter booleans.
    pub solid: bool,
}

/// Result of one node (cached by key).
#[derive(Debug)]
pub(crate) struct NodeOutput<S> {
    pub parts: Vec<Part<S>>,
    /// Sorted tags that faces of `parts` can carry.
    pub tags: Rc<[Tag]>,
    pub dim: Option<Dim>,
    pub bounds: Option<(DVec3, DVec3)>,
    pub diags: Vec<Diag>,
    pub hints: Vec<String>,
    pub selected_edges: Vec<Vec<DVec3>>,
    /// Sample points of faces opened by `shell` (node system).
    pub opened_faces: Vec<Vec<DVec3>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeReport {
    pub node: NodeIdx,
    pub stmt: StmtId,
    pub span: Span,
    pub inst_key: u64,
    pub label: String,
    pub key: CacheKey,
    pub tag: Tag,
    pub status: Status,
    pub diagnostics: Vec<Diagnostic>,
    /// Inline hints, e.g. "4 Kanten".
    pub hints: Vec<String>,
    /// Edges selected by fillet/chamfer (zone block, boolean parameter, `$fillet`), as polylines
    /// in the node's own system.
    pub selected_edges: Vec<Vec<DVec3>>,
    pub opened_faces: usize,
    /// Bounds in the node's own system.
    pub bounds: Option<(DVec3, DVec3)>,
    pub dim: Option<Dim>,
    pub parts: usize,
    /// Result came from the cache.
    pub cached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NodeRef {
    pub node: NodeIdx,
    pub inst_key: u64,
    pub stmt: StmtId,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct EvalStats {
    pub nodes: usize,
    pub cache_hits: usize,
    pub kernel_calls: u64,
    pub steps: u32,
}

/// Finished evaluation of one tree.
pub struct Evaluation<S> {
    pub tree: Rc<Tree>,
    /// Indexed by [`NodeIdx`].
    pub reports: Vec<NodeReport>,
    /// Tag → nodes of this tree whose key the tag was interned for.
    pub tag_nodes: BTreeMap<Tag, Vec<NodeRef>>,
    pub stats: EvalStats,
    outputs: Vec<Option<Rc<NodeOutput<S>>>>,
}

impl<S> Evaluation<S> {
    pub fn root(&self) -> Option<NodeIdx> {
        self.tree.root
    }

    pub fn report(&self, n: NodeIdx) -> &NodeReport {
        &self.reports[n as usize]
    }

    /// Parts of a node's result in its own system (empty for 2D nodes and skipped nodes).
    pub fn parts(&self, n: NodeIdx) -> &[Part<S>] {
        self.outputs[n as usize].as_ref().map(|o| o.parts.as_slice()).unwrap_or(&[])
    }

    /// Nodes that created faces with this tag.
    pub fn nodes_for_tag(&self, tag: Tag) -> &[NodeRef] {
        self.tag_nodes.get(&tag).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// All diagnostics in node order.
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.reports.iter().flat_map(|r| r.diagnostics.iter())
    }

    /// Nodes whose report has a status other than `Skipped`.
    pub fn evaluated(&self) -> impl Iterator<Item = &NodeReport> {
        self.reports.iter().filter(|r| r.status != Status::Skipped)
    }
}

/// Result of [`Job::step`].
pub enum Step<S> {
    Pending { done: usize, total: usize },
    Done(Evaluation<S>),
}

impl<S> Step<S> {
    pub fn is_done(&self) -> bool {
        matches!(self, Step::Done(_))
    }
}

/// Interned tags: tag `i + 1` belongs to `keys[i]`. Tag 0 is never assigned.
#[derive(Default)]
struct TagTable {
    by_key: HashMap<CacheKey, Tag>,
    keys: Vec<CacheKey>,
}

impl TagTable {
    fn intern(&mut self, k: CacheKey) -> Tag {
        if let Some(&t) = self.by_key.get(&k) {
            return t;
        }
        self.keys.push(k);
        let t = self.keys.len() as Tag;
        self.by_key.insert(k, t);
        t
    }
}

pub struct Engine<K: Kernel> {
    kernel: K,
    config: EngineConfig,
    cache: Lru<CacheKey, Rc<NodeOutput<K::Shape>>>,
    meshes: Lru<(CacheKey, u64), Rc<Mesh>>,
    tags: TagTable,
    clock: Option<Clock>,
    kernel_calls: u64,
}

/// An evaluation in progress. Owns its tree; drop it to abort.
pub struct Job<S> {
    tree: Rc<Tree>,
    keys: Vec<CacheKey>,
    tags: Vec<Tag>,
    order: Vec<NodeIdx>,
    pos: usize,
    outputs: Vec<Option<Rc<NodeOutput<S>>>>,
    cached: Vec<bool>,
    /// Kernel results of the interrupted node, replayed in call order.
    replay: Vec<Rc<dyn Any>>,
    stats: EvalStats,
}

impl<S> Job<S> {
    pub fn total(&self) -> usize {
        self.order.len()
    }

    pub fn done(&self) -> usize {
        self.pos
    }

    /// Node currently being evaluated (for a watchdog), if any.
    pub fn current(&self) -> Option<NodeIdx> {
        self.order.get(self.pos).copied()
    }

    /// Cache key of a node of this job's tree.
    pub fn key(&self, n: NodeIdx) -> CacheKey {
        self.keys[n as usize]
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

impl<S: Clone + 'static> Job<S> {
    /// Runs until the budget is used up or the evaluation is finished.
    pub fn step<K: Kernel<Shape = S>>(&mut self, e: &mut Engine<K>, budget: Budget) -> Step<S> {
        self.stats.steps += 1;
        let deadline = match (budget.max_ms, &e.clock) {
            (Some(ms), Some(c)) => Some(c() + ms),
            _ => None,
        };
        let mut ops: u64 = 0;
        while self.pos < self.order.len() {
            let idx = self.order[self.pos];
            let key = self.keys[idx as usize];
            if self.replay.is_empty()
                && let Some(o) = e.cache.get(&key)
            {
                self.outputs[idx as usize] = Some(o);
                self.cached[idx as usize] = true;
                self.stats.cache_hits += 1;
                self.stats.nodes += 1;
                self.pos += 1;
                continue;
            }
            let res = {
                let mut run = eval::Run {
                    k: &mut e.kernel,
                    replay: &mut self.replay,
                    cursor: 0,
                    ops: &mut ops,
                    max_ops: budget.max_ops,
                    deadline,
                    clock: e.clock.as_deref(),
                };
                let ctx = eval::Ctx {
                    tree: &self.tree,
                    keys: &self.keys,
                    tags: &self.tags,
                    outputs: &self.outputs,
                    config: &e.config,
                };
                eval::eval_node(&mut run, &ctx, idx)
            };
            match res {
                Ok(out) => {
                    let o = Rc::new(out);
                    e.cache.insert(key, o.clone(), 1);
                    self.outputs[idx as usize] = Some(o);
                    self.replay.clear();
                    self.stats.nodes += 1;
                    self.pos += 1;
                }
                Err(eval::Interrupt) => {
                    self.stats.kernel_calls += ops;
                    e.kernel_calls += ops;
                    return Step::Pending { done: self.pos, total: self.order.len() };
                }
            }
            if self.pos < self.order.len() && eval::exhausted(ops, budget.max_ops, deadline, e.clock.as_deref()) {
                self.stats.kernel_calls += ops;
                e.kernel_calls += ops;
                return Step::Pending { done: self.pos, total: self.order.len() };
            }
        }
        self.stats.kernel_calls += ops;
        e.kernel_calls += ops;
        Step::Done(self.finish())
    }

    fn finish(&self) -> Evaluation<S> {
        let tree = &self.tree;
        let mut reports = Vec::with_capacity(tree.nodes.len());
        let mut tag_nodes: BTreeMap<Tag, Vec<NodeRef>> = BTreeMap::new();
        for (i, n) in tree.nodes.iter().enumerate() {
            let idx = i as NodeIdx;
            let out = self.outputs[i].as_ref();
            let mut status = if out.is_some() { Status::Ok } else { Status::Skipped };
            let mut diagnostics = Vec::new();
            if let Some(o) = out {
                for d in &o.diags {
                    match d.severity {
                        Severity::Error => status = Status::Error,
                        Severity::Warning if status == Status::Ok => status = Status::Warning,
                        _ => {}
                    }
                    diagnostics.push(Diagnostic {
                        severity: d.severity,
                        message: d.message.clone(),
                        node: idx,
                        stmt: n.stmt,
                        span: n.span,
                        edges: d.edges.clone(),
                    });
                }
                tag_nodes.entry(self.tags[i]).or_default().push(NodeRef {
                    node: idx,
                    inst_key: n.inst_key,
                    stmt: n.stmt,
                    span: n.span,
                });
            }
            reports.push(NodeReport {
                node: idx,
                stmt: n.stmt,
                span: n.span,
                inst_key: n.inst_key,
                label: n.op.label(),
                key: self.keys[i],
                tag: self.tags[i],
                status,
                diagnostics,
                hints: out.map(|o| o.hints.clone()).unwrap_or_default(),
                selected_edges: out.map(|o| o.selected_edges.clone()).unwrap_or_default(),
                opened_faces: out.map(|o| o.opened_faces.len()).unwrap_or(0),
                bounds: out.and_then(|o| o.bounds),
                dim: out.and_then(|o| o.dim),
                parts: out.map(|o| o.parts.len()).unwrap_or(0),
                cached: self.cached[i],
            });
        }
        Evaluation {
            tree: self.tree.clone(),
            reports,
            tag_nodes,
            stats: self.stats,
            outputs: self.outputs.clone(),
        }
    }
}

impl<K: Kernel> Engine<K>
where
    K::Shape: 'static,
{
    pub fn new(kernel: K) -> Self {
        Self::with_config(kernel, EngineConfig::default())
    }

    pub fn with_config(kernel: K, config: EngineConfig) -> Self {
        Engine {
            kernel,
            cache: Lru::new(config.cache_entries),
            meshes: Lru::new(config.mesh_cache_bytes),
            config,
            tags: TagTable::default(),
            clock: None,
            kernel_calls: 0,
        }
    }

    /// Clock for time budgets, in milliseconds.
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = Some(clock);
    }

    pub fn kernel(&self) -> &K {
        &self.kernel
    }

    pub fn kernel_mut(&mut self) -> &mut K {
        &mut self.kernel
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Changes the cache limits (evicts as needed).
    pub fn set_cache_limits(&mut self, entries: usize, mesh_bytes: usize) {
        self.config.cache_entries = entries;
        self.config.mesh_cache_bytes = mesh_bytes;
        self.cache.set_limit(entries);
        self.meshes.set_limit(mesh_bytes);
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.meshes.clear();
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Kernel calls made by evaluations of this engine (not counting tessellation).
    pub fn kernel_calls(&self) -> u64 {
        self.kernel_calls
    }

    /// Key a tag was interned for.
    pub fn tag_key(&self, tag: Tag) -> Option<CacheKey> {
        self.tags.keys.get((tag as usize).wrapping_sub(1)).copied()
    }

    /// Starts an evaluation. Computes keys and the evaluation order; no kernel calls.
    pub fn start(&mut self, tree: Tree) -> Job<K::Shape> {
        let tree = Rc::new(tree);
        let n = tree.nodes.len();
        let order = eval::post_order(&tree);
        let keys = eval::compute_keys(&tree, self.kernel.id());
        let tags = keys.iter().map(|&k| self.tags.intern(k)).collect();
        Job {
            tree,
            keys,
            tags,
            order,
            pos: 0,
            outputs: vec![None; n],
            cached: vec![false; n],
            replay: Vec::new(),
            stats: EvalStats::default(),
        }
    }

    /// Evaluates a tree to completion.
    pub fn evaluate(&mut self, tree: Tree) -> Evaluation<K::Shape> {
        let mut job = self.start(tree);
        loop {
            if let Step::Done(ev) = job.step(self, Budget::UNLIMITED) {
                return ev;
            }
        }
    }

    fn tol_for(&self, ev: &Evaluation<K::Shape>, node: NodeIdx, tol: Option<f64>) -> f64 {
        let t = tol.or(ev.tree.node(node).tol).unwrap_or(self.config.default_tol);
        if t > 0.0 { t } else { self.config.default_tol }
    }

    /// Mesh of a node in its own system (place it with `Node.world`). 2D nodes give a filled
    /// outline in their xy-plane. `tol`: chordal tolerance in mm (default `$tol` of the node or
    /// [`EngineConfig::default_tol`]). Meshes are cached per (key, tolerance).
    pub fn tessellate(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        tol: Option<f64>,
    ) -> Result<Rc<Mesh>, String> {
        let tol = self.tol_for(ev, node, tol);
        let Some(out) = ev.outputs.get(node as usize).and_then(|o| o.clone()) else {
            return Err(format!("Knoten {node} wurde nicht ausgewertet"));
        };
        let mkey = (ev.reports[node as usize].key, key::canon_f64(tol));
        if let Some(m) = self.meshes.get(&mkey) {
            return Ok(m);
        }
        let mesh = if out.dim == Some(Dim::Two) {
            let mut warn = Vec::new();
            let expr = profile::lift(&ev.tree, node, DMat4::IDENTITY, &mut warn)?;
            let tag_of = |n: NodeIdx| ev.reports[n as usize].tag;
            profile::region_mesh(&expr, tol, &tag_of)
        } else {
            let mut mesh = Mesh::default();
            for p in &out.parts {
                let m = self.kernel.tessellate(&p.shape, tol).map_err(|e| e.to_string())?;
                append_offset(&mut mesh, &m);
            }
            mesh
        };
        let cost = mesh_bytes(&mesh);
        let mesh = Rc::new(mesh);
        self.meshes.insert(mkey, mesh.clone(), cost);
        Ok(mesh)
    }

    /// Mesh of the root (empty mesh if there is no root).
    pub fn tessellate_root(&mut self, ev: &Evaluation<K::Shape>, tol: Option<f64>) -> Result<Rc<Mesh>, String> {
        match ev.root() {
            Some(r) => self.tessellate(ev, r, tol),
            None => Ok(Rc::new(Mesh::default())),
        }
    }

    /// Binary STL of the root (fine tolerance by default: `tol` or 0.01 mm).
    pub fn export_stl(&mut self, ev: &Evaluation<K::Shape>, tol: Option<f64>) -> Result<Vec<u8>, String> {
        let m = self.export_mesh(ev, tol)?;
        Ok(export::stl_binary(&m, "cad-engine"))
    }

    /// 3MF package of the root.
    pub fn export_3mf(&mut self, ev: &Evaluation<K::Shape>, tol: Option<f64>) -> Result<Vec<u8>, String> {
        let m = self.export_mesh(ev, tol)?;
        Ok(export::threemf(&m))
    }

    fn export_mesh(&mut self, ev: &Evaluation<K::Shape>, tol: Option<f64>) -> Result<Rc<Mesh>, String> {
        let root = ev.root().ok_or("kein Ergebnis")?;
        if ev.reports[root as usize].dim == Some(Dim::Two) {
            return Err("2D-Ergebnis kann nicht als Körper exportiert werden".into());
        }
        self.tessellate(ev, root, Some(tol.unwrap_or(0.01)))
    }

    /// Volume, area and centroid of a node's result (sum over its parts).
    pub fn mass_props(&mut self, ev: &Evaluation<K::Shape>, node: NodeIdx) -> Result<MassProps, String> {
        let mut acc = MassProps::default();
        let mut c = DVec3::ZERO;
        for p in ev.parts(node) {
            let m = self.kernel.mass_props(&p.shape).map_err(|e| e.to_string())?;
            acc.volume += m.volume;
            acc.area += m.area;
            c += m.centroid * m.volume;
        }
        if acc.volume.abs() > 0.0 {
            acc.centroid = c / acc.volume;
        }
        Ok(acc)
    }

    /// Kernel shape tags of a node mapped to nodes of the evaluation, per triangle of `mesh`.
    pub fn triangle_nodes<'e>(&self, ev: &'e Evaluation<K::Shape>, mesh: &Mesh) -> Vec<&'e [NodeRef]> {
        mesh.tri_tags.iter().map(|&t| ev.nodes_for_tag(t)).collect()
    }
}

/// Appends `m`, offsetting face and edge ids so they stay unique.
fn append_offset(dst: &mut Mesh, m: &Mesh) {
    let face_off = dst.tri_faces.iter().max().map(|v| v + 1).unwrap_or(0);
    let edge_off = dst.edge_ids.iter().max().map(|v| v + 1).unwrap_or(0);
    let nf = dst.tri_faces.len();
    let ne = dst.edge_ids.len();
    dst.append_transformed(m, &DMat4::IDENTITY);
    for f in &mut dst.tri_faces[nf..] {
        *f += face_off;
    }
    for e in &mut dst.edge_ids[ne..] {
        *e += edge_off;
    }
}

fn mesh_bytes(m: &Mesh) -> usize {
    4 * (m.positions.len()
        + m.normals.len()
        + m.indices.len()
        + m.tri_tags.len()
        + m.tri_faces.len()
        + m.edge_segments.len()
        + m.edge_ids.len())
        + 64
}

/// `true` if a node takes no part in its parent's result (modifiers `*`, `%`, zones).
pub fn excluded(tree: &Tree, n: NodeIdx) -> bool {
    let f = tree.node(n).flags;
    f.disabled || f.background || f.zone
}

/// `true` for nodes whose op is a profile operation (2D children).
pub(crate) fn is_profile_op(op: &Op) -> bool {
    matches!(op, Op::LinearExtrude { .. } | Op::RotateExtrude { .. } | Op::Loft { .. } | Op::Sweep { .. })
}
