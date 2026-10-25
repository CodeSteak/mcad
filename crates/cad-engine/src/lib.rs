//! Engine core: evaluates a [`cad_ir::Tree`] into kernel shapes, generic over
//! [`cad_kernel::Kernel`].
//!
//! * **Evaluation** bottom-up (post-order). Every node's result is expressed in the node's own
//!   system (`Node.world`); a parent takes a child's result through `child.local`. Transform
//!   nodes therefore need no extra work: their children carry the matrix in `local`.
//! * **Cache:** every node has a content [`CacheKey`] (operation, resolved parameters,
//!   `$fillet`/`$chamfer`, keys/relative matrices/modifier flags of the children, kernel id;
//!   never the world matrix). Node results (shapes, diagnostics, hints, selected edges, bounds)
//!   and meshes (also mass properties and STEP B-reps) live in one LRU owned by the [`Engine`]
//!   and survive evaluations. It has two limits ([`EngineConfig::cache_bytes`],
//!   [`EngineConfig::cache_entries`]); the size of a node result is estimated with
//!   [`Kernel::shape_bytes`] or measured ([`Engine::set_heap_probe`]), that of a mesh from its
//!   arrays.
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
//! * **Draft mode** ([`Engine::set_draft`], while a handle is dragged): blend nodes whose last
//!   fillet/chamfer took longer than [`EngineConfig::drag_blend_budget_ms`] (and whose result
//!   is not cached) are left out; the node shows the body without the blend and the hint
//!   [`SUSPENDED_HINT`]. Such nodes and their ancestors get marked keys (never shared with full
//!   results) and run with [`Kernel::set_draft`]. The next evaluation outside draft mode
//!   computes everything again.
//! * **Blocked keys** ([`Engine::block_keys`], watchdog): a node whose key is blocked is not
//!   evaluated; it yields nothing and the error [`BLOCKED_MESSAGE`]. A changed parameter
//!   changes the key, so the node is computed again.
//!
//! Known limits: the tag interning table only grows (one entry per distinct node content
//! seen by this engine); edges between faces of two equal-content children count as old
//! edges (same tag); a node's evaluation is the unit of caching (a step may stop inside a
//! node, its finished calls are replayed, not cached); circles under non-uniform 2D scale are
//! approximated by splines; the 2D display uses Catmull–Rom splines, not the kernel's curve.
//!
//! 2D subtrees under `linear_extrude`/`rotate_extrude`/`sweep` are "lifted": every leaf profile
//! becomes a solid with its accumulated 2D frame and the 2D booleans are rebuilt as 3D booleans
//! (see `profile.rs`). 2D nodes on their own are shown as filled polygons
//! ([`Engine::tessellate`]).

pub mod builder;
mod cache;
pub mod color_export;
pub mod colors;
pub mod counting;
mod eval;
pub mod export;
mod hull3d;
pub mod key;
mod op2d;
pub mod parts;
mod profile;

pub use profile::fill_loops;

pub use cad_step::{Schema as StepSchema, StepOptions, StepStats};
use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use cad_ir::{Dim, NodeIdx, Op, Tree};
use cad_kernel::{Kernel, MassProps, Mesh, Tag};
use cad_syntax::Span;
use cad_syntax::ast::StmtId;
use glam::{DMat4, DVec3};
use serde::Serialize;

pub use cache::Lru;
pub use counting::CountingKernel;
pub use eval::SUSPENDED_HINT;
pub use key::{CacheKey, KeyHasher};
pub use parts::{NodeFormat, part_file_name};

/// Error of a node whose key is blocked (see [`Engine::block_keys`]).
pub const BLOCKED_MESSAGE: &str = "Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung";

/// Default display tolerance in mm when neither the call nor `$tol` gives one.
pub const DEFAULT_TOL: f64 = 0.05;

/// Default limit of the cache in estimated bytes (node results and meshes): 512 MiB.
pub const DEFAULT_CACHE_BYTES: usize = 512 << 20;

/// What the cache holds ([`Engine::cache_stats`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CacheStats {
    /// Node results.
    pub nodes: usize,
    /// Meshes (per key and tolerance).
    pub meshes: usize,
    /// Estimated bytes of the node results.
    pub node_bytes: usize,
    /// Estimated bytes of the meshes.
    pub mesh_bytes: usize,
    pub limit_bytes: usize,
    pub max_entries: usize,
    /// Lookups that found an entry / found none (node results and meshes).
    pub hits: u64,
    pub misses: u64,
    /// Entries dropped because a limit was exceeded.
    pub evictions: u64,
}

/// Key of a cache entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Slot {
    Node(CacheKey),
    /// Mesh of a node result at a tolerance (canonical bits).
    Mesh(CacheKey, u64),
    /// Mass properties of a node result.
    Mass(CacheKey),
    /// Neutral B-rep of part `u32` of a node result, placed with a matrix (hash).
    Brep(CacheKey, u64, u32),
    /// Colour regions of a node result (key from `colors.rs`).
    Regions(CacheKey),
    /// Data of a caller (e.g. assertion checks of the session), by a key it computes.
    Aux(CacheKey),
}

impl Slot {
    fn key(&self) -> &CacheKey {
        match self {
            Slot::Node(k) | Slot::Mesh(k, _) | Slot::Mass(k) | Slot::Brep(k, ..) | Slot::Regions(k) | Slot::Aux(k) => k,
        }
    }
}

enum Cached<S> {
    Node(Rc<NodeOutput<S>>),
    Mesh(Rc<Mesh>),
    Mass(MassProps),
    Brep(Rc<cad_kernel::brep::Brep>),
    Regions(Rc<colors::ColorRegions<S>>),
    Aux(Rc<dyn Any>),
}

impl<S> Clone for Cached<S> {
    fn clone(&self) -> Self {
        match self {
            Cached::Node(o) => Cached::Node(o.clone()),
            Cached::Mesh(m) => Cached::Mesh(m.clone()),
            Cached::Mass(m) => Cached::Mass(*m),
            Cached::Brep(b) => Cached::Brep(b.clone()),
            Cached::Regions(r) => Cached::Regions(r.clone()),
            Cached::Aux(a) => Cached::Aux(a.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum number of cache entries (node results and meshes).
    pub cache_entries: usize,
    /// Maximum estimated size of the cache in bytes (node results and meshes together).
    pub cache_bytes: usize,
    /// Interior sample points per edge for zone selection and new-edge detection.
    pub edge_samples: u32,
    /// Sample points per face for `shell` zones.
    pub face_samples: u32,
    pub default_tol: f64,
    /// Draft mode: blends whose last kernel time exceeded this many ms are left out
    /// (`None`: never).
    pub drag_blend_budget_ms: Option<f64>,
    /// Keys blocked from the start (watchdog after a worker restart), see
    /// [`Engine::block_keys`].
    pub blocked_keys: Vec<CacheKey>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            cache_entries: 16384,
            cache_bytes: DEFAULT_CACHE_BYTES,
            edge_samples: 16,
            face_samples: 4,
            default_tol: DEFAULT_TOL,
            drag_blend_budget_ms: Some(25.0),
            blocked_keys: Vec::new(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum Severity {
    Warning,
    #[default]
    Error,
}

/// Diagnostic as stored in a (cached) node result; [`Diagnostic`] adds the node's position.
/// With `at`, the message is a template: `{at}` stands for the place (written in context
/// coordinates when the evaluation finishes), `{nodes}` for the operands `nodes`.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Diag {
    pub severity: Severity,
    pub message: String,
    pub edges: Vec<Vec<DVec3>>,
    /// Place of the problem in the node's own system.
    pub at: Option<DVec3>,
    /// Nodes the message is about (operands of a Boolean that touch at `at`).
    pub nodes: Vec<NodeIdx>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The text, operands named by their label (`{nodes}` of `template` filled with
    /// [`operand_list`] of the labels).
    pub message: String,
    pub node: NodeIdx,
    pub stmt: StmtId,
    pub span: Span,
    /// Affected edges as polylines in the node's own system (e.g. the edge a fillet failed on,
    /// the edges along which the operands of a Boolean touch tangentially).
    pub edges: Vec<Vec<DVec3>>,
    /// Place of the problem in context coordinates (`NodeReport::display_world`), e.g. where
    /// the operands of a Boolean touch.
    pub at: Option<DVec3>,
    /// The nodes the message names (operands of a Boolean touching at `at`).
    pub nodes: Vec<NodeIdx>,
    /// The message with the placeholder `{nodes}` for the operands, for callers that name
    /// them with their source lines (`None`: no placeholder).
    pub template: Option<String>,
}

/// Name of what node `n` places: through transforms, colours and groups with one child to the
/// object; a named instance by its name, a module call by the module (`dom`, `cylinder`).
pub fn object_label(tree: &Tree, n: NodeIdx) -> String {
    let mut m = n;
    while matches!(tree.node(m).op, Op::Transform { .. } | Op::Color { .. } | Op::Group) {
        let kids: Vec<NodeIdx> = tree.solid_children(m).collect();
        let [k] = kids[..] else { break };
        m = k;
    }
    match &tree.node(m).op {
        Op::ModuleCall { instance: Some(i), .. } => i.clone(),
        op => op.label(),
    }
}

/// "a und b", "a, b und c" (German list of operands).
pub fn operand_list(names: &[String]) -> String {
    match names {
        [] => "die Operanden".into(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} und {last}", rest.join(", ")),
    }
}

/// `[x, y, z]` with at most 4 decimals.
pub(crate) fn point_text(p: DVec3) -> String {
    let n = |x: f64| {
        let s = format!("{:.4}", x);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" { "0".to_string() } else { s.to_string() }
    };
    format!("[{}, {}, {}]", n(p.x), n(p.y), n(p.z))
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

/// A mesh of a node result (`import("x.stl")` without B-rep), in the node's own system. Meshes
/// pass through groups, transforms, colours and plain unions next to the B-rep parts; other
/// operations report them (see `eval.rs`).
#[derive(Debug, Clone)]
pub struct MeshPart {
    pub mesh: std::sync::Arc<cad_kernel::trimesh::TriMesh>,
    /// Mesh coordinates → node system.
    pub place: DMat4,
    /// Tag of the importing node (all triangles).
    pub tag: Tag,
}

impl MeshPart {
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        self.mesh.bounds(&self.place)
    }

    /// The same mesh placed by `m` after its placement.
    pub fn placed(&self, m: &DMat4) -> MeshPart {
        MeshPart { mesh: self.mesh.clone(), place: *m * self.place, tag: self.tag }
    }

    /// Closed (a solid without B-rep) or an open surface.
    pub fn solid(&self) -> bool {
        self.mesh.closed
    }

    /// Display mesh in the node system (face ids of the mesh's faces).
    pub fn display(&self) -> Mesh {
        self.mesh.display(&self.place, self.tag)
    }

    /// Triangles in the node system placed by `m`.
    pub fn triangles(&self, m: &DMat4) -> Vec<[DVec3; 3]> {
        let t = *m * self.place;
        let q: Vec<DVec3> = self.mesh.points.iter().map(|p| t.transform_point3(*p)).collect();
        self.mesh.tris.iter().map(|tr| tr.map(|i| q[i as usize])).collect()
    }
}

/// One part of a node prepared for measuring ([`Engine::measure_parts`]).
#[derive(Debug, Clone)]
pub struct MeasurePart {
    /// Index in [`Evaluation::parts`].
    pub part: usize,
    /// Context coordinates.
    pub brep: Rc<cad_kernel::brep::Brep>,
    /// Context coordinates, face ids as in `brep`.
    pub mesh: Rc<Mesh>,
    pub solid: bool,
    /// Volume, area, centroid (solids).
    pub mass: Option<MassProps>,
}

/// Result of one node (cached by key).
#[derive(Debug)]
pub(crate) struct NodeOutput<S> {
    pub parts: Vec<Part<S>>,
    /// Meshes without B-rep (after `parts` in meshes, face ids and measuring).
    pub meshes: Vec<MeshPart>,
    /// Sorted tags that faces of `parts` can carry.
    pub tags: Rc<[Tag]>,
    pub dim: Option<Dim>,
    pub bounds: Option<(DVec3, DVec3)>,
    pub diags: Vec<Diag>,
    pub hints: Vec<String>,
    pub selected_edges: Vec<Vec<DVec3>>,
    /// Guide lines of 2D operations (`"source"`, `"result"`, `"centerline"`), node system.
    pub guides: Vec<(&'static str, Vec<DVec3>)>,
    /// Sample points of faces opened by `shell` (node system).
    pub opened_faces: Vec<Vec<DVec3>>,
    /// Time of the node's fillet/chamfer kernel calls (ms, 0 without a clock).
    pub blend_ms: f64,
    /// The blend was left out (draft mode).
    pub suspended: bool,
    /// `parts` are the operands of a Boolean that failed here or below (reported there): a
    /// union of them in a parent fails again and is not reported a second time.
    pub unmerged: bool,
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
    /// Guide lines of 2D operations: `"source"` (input contour of `offset`), `"result"` (offset
    /// contour), `"centerline"` (path of `stroke`), polylines in the node's own system.
    pub guides: Vec<(String, Vec<DVec3>)>,
    pub opened_faces: usize,
    /// Bounds in the node's own system.
    pub bounds: Option<(DVec3, DVec3)>,
    pub dim: Option<Dim>,
    pub parts: usize,
    /// Meshes without B-rep in the result ([`Evaluation::meshes`]).
    pub meshes: usize,
    /// Result came from the cache.
    pub cached: bool,
    /// Draft mode left this node's fillet/chamfer out (hint [`SUSPENDED_HINT`]).
    pub blend_suspended: bool,
    /// The node's key is blocked ([`Engine::block_keys`]): not evaluated, error
    /// [`BLOCKED_MESSAGE`].
    pub blocked: bool,
    /// Where the UI places this node's mesh: `Node.world` relative to the evaluated root.
    /// Equals `Node.world` unless a `!` modifier made a nested node the root, whose
    /// enclosing transforms are ignored (as in OpenSCAD).
    pub display_world: DMat4,
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
    /// Blend nodes left out in draft mode.
    pub suspended_blends: usize,
    /// Nodes not evaluated because their key is blocked.
    pub blocked: usize,
}

/// Finished evaluation of one tree.
pub struct Evaluation<S> {
    pub tree: Rc<Tree>,
    /// Indexed by [`NodeIdx`].
    pub reports: Vec<NodeReport>,
    /// Tag → nodes of this tree whose key the tag was interned for.
    pub tag_nodes: BTreeMap<Tag, Vec<NodeRef>>,
    pub stats: EvalStats,
    /// Evaluated in draft mode ([`Engine::set_draft`]).
    pub draft: bool,
    outputs: Vec<Option<Rc<NodeOutput<S>>>>,
}

impl<S> Evaluation<S> {
    pub fn root(&self) -> Option<NodeIdx> {
        self.tree.root
    }

    /// `true` if the root is a nested node (`!` modifier): transforms above it are ignored
    /// and [`NodeReport::display_world`] is relative to it.
    pub fn root_is_subtree(&self) -> bool {
        self.tree.root.is_some_and(|r| self.tree.node(r).parent.is_some())
    }

    pub fn report(&self, n: NodeIdx) -> &NodeReport {
        &self.reports[n as usize]
    }

    /// Parts of a node's result in its own system (empty for 2D nodes and skipped nodes).
    pub fn parts(&self, n: NodeIdx) -> &[Part<S>] {
        self.outputs[n as usize].as_ref().map(|o| o.parts.as_slice()).unwrap_or(&[])
    }

    /// Meshes without B-rep of a node's result in its own system.
    pub fn meshes(&self, n: NodeIdx) -> &[MeshPart] {
        self.outputs[n as usize].as_ref().map(|o| o.meshes.as_slice()).unwrap_or(&[])
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
    cache: Lru<Slot, Cached<K::Shape>>,
    tags: TagTable,
    clock: Option<Clock>,
    kernel_calls: u64,
    draft: bool,
    /// Last measured fillet/chamfer time per blend node (`inst_key`), for draft mode.
    blend_ms: HashMap<u64, f64>,
    blocked: HashSet<CacheKey>,
    heap_probe: Option<HeapProbe>,
}

/// Bytes in use on the heap (e.g. glibc `mallinfo2`), see [`Engine::set_heap_probe`].
pub type HeapProbe = Box<dyn Fn() -> usize>;

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
    draft: bool,
    /// Per node: blend left out (draft mode).
    skip: Vec<bool>,
    /// Per node: draft result (a skipped blend in its subtree); evaluated with
    /// `Kernel::set_draft(true)`.
    tainted: Vec<bool>,
    blocked: Vec<bool>,
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

    /// Face tag of a node of this job's tree (known before the node is evaluated).
    pub fn tag(&self, n: NodeIdx) -> Tag {
        self.tags[n as usize]
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Started in draft mode.
    pub fn draft(&self) -> bool {
        self.draft
    }

    /// Blend nodes this job leaves out (draft mode).
    pub fn suspended_blends(&self) -> usize {
        self.skip.iter().filter(|&&s| s).count()
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
            if self.replay.is_empty() && e.blocked.contains(&key) {
                self.blocked[idx as usize] = true;
                self.outputs[idx as usize] = Some(Rc::new(self.blocked_output(idx)));
                self.stats.blocked += 1;
                self.stats.nodes += 1;
                self.pos += 1;
                continue;
            }
            if self.replay.is_empty()
                && let Some(o) = e.cached_node(&key)
            {
                self.outputs[idx as usize] = Some(o);
                self.cached[idx as usize] = true;
                self.stats.cache_hits += 1;
                self.stats.nodes += 1;
                self.pos += 1;
                continue;
            }
            // heap in use before a fresh node evaluation (not for a replayed one)
            let heap0 = if self.replay.is_empty() { e.heap_probe.as_ref().map(|p| p()) } else { None };
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
                    tags: &self.tags,
                    outputs: &self.outputs,
                    config: &e.config,
                    skip: &self.skip,
                };
                let tainted = self.tainted[idx as usize];
                if tainted {
                    run.k.set_draft(true);
                }
                let r = eval::eval_node(&mut run, &ctx, idx);
                if tainted {
                    run.k.set_draft(false);
                }
                r
            };
            match res {
                Ok(out) => {
                    if out.blend_ms > 0.0 {
                        e.blend_ms.insert(self.tree.node(idx).inst_key, out.blend_ms);
                    }
                    if out.suspended {
                        self.stats.suspended_blends += 1;
                    }
                    let o = Rc::new(out);
                    // What the node kept on the heap (its new shapes), if measured: the
                    // topology estimate can be off by a factor of 10 for freeform geometry.
                    let measured = heap0.and_then(|h0| e.heap_probe.as_ref().map(|p| p().saturating_sub(h0)));
                    // (an imported mesh was allocated before, by the language evaluation)
                    let cost = match measured {
                        Some(m) => m.max(512) + own_mesh_bytes(&o),
                        None => e.output_bytes(&o),
                    };
                    e.cache.insert(Slot::Node(key), Cached::Node(o.clone()), cost);
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

    fn blocked_output(&self, idx: NodeIdx) -> NodeOutput<S> {
        let dim = self.tree.dim(idx);
        let two_d = dim == Some(Dim::Two) && !is_profile_op(&self.tree.node(idx).op);
        NodeOutput {
            parts: Vec::new(),
            meshes: Vec::new(),
            tags: vec![self.tags[idx as usize]].into(),
            dim: if two_d { dim } else { Some(Dim::Three) },
            bounds: None,
            diags: vec![Diag { severity: Severity::Error, message: BLOCKED_MESSAGE.into(), ..Default::default() }],
            hints: Vec::new(),
            selected_edges: Vec::new(),
            guides: Vec::new(),
            opened_faces: Vec::new(),
            blend_ms: 0.0,
            suspended: false,
            unmerged: false,
        }
    }

    fn finish(&self) -> Evaluation<S> {
        let tree = &self.tree;
        let root_inv = tree.root.map(|r| tree.node(r).world.inverse()).unwrap_or(DMat4::IDENTITY);
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
                    let world = root_inv * n.world;
                    let at = d.at.map(|p| world.transform_point3(p));
                    let template = at.map(|p| d.message.replace("{at}", &point_text(p)));
                    let message = match &template {
                        Some(t) => {
                            let names: Vec<String> = d.nodes.iter().map(|&k| object_label(tree, k)).collect();
                            t.replace("{nodes}", &operand_list(&names))
                        }
                        None => d.message.clone(),
                    };
                    diagnostics.push(Diagnostic {
                        severity: d.severity,
                        message,
                        node: idx,
                        stmt: n.stmt,
                        span: n.span,
                        edges: d.edges.clone(),
                        at,
                        nodes: d.nodes.clone(),
                        template: template.filter(|t| t.contains("{nodes}")),
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
                guides: out
                    .map(|o| o.guides.iter().map(|(k, p)| (k.to_string(), p.clone())).collect())
                    .unwrap_or_default(),
                opened_faces: out.map(|o| o.opened_faces.len()).unwrap_or(0),
                bounds: out.and_then(|o| o.bounds),
                dim: out.and_then(|o| o.dim),
                parts: out.map(|o| o.parts.len()).unwrap_or(0),
                meshes: out.map(|o| o.meshes.len()).unwrap_or(0),
                cached: self.cached[i],
                blend_suspended: out.is_some_and(|o| o.suspended),
                blocked: self.blocked[i],
                display_world: root_inv * n.world,
            });
        }
        Evaluation {
            tree: self.tree.clone(),
            reports,
            tag_nodes,
            stats: self.stats,
            draft: self.draft,
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
            cache: Lru::with_limits(config.cache_bytes, config.cache_entries),
            blocked: config.blocked_keys.iter().copied().collect(),
            config,
            tags: TagTable::default(),
            clock: None,
            kernel_calls: 0,
            draft: false,
            blend_ms: HashMap::new(),
            heap_probe: None,
        }
    }

    /// Draft mode for evaluations started from now on (see the crate docs). Needs a clock
    /// ([`Engine::set_clock`]) to measure blends.
    pub fn set_draft(&mut self, draft: bool) {
        self.draft = draft;
    }

    pub fn draft(&self) -> bool {
        self.draft
    }

    /// Blocks nodes with these keys (watchdog: the node hung the kernel). Evaluations started
    /// from now on do not compute them and report [`BLOCKED_MESSAGE`].
    pub fn block_keys(&mut self, keys: impl IntoIterator<Item = CacheKey>) {
        self.blocked.extend(keys);
    }

    /// Lifts all blocks.
    pub fn unblock_all(&mut self) {
        self.blocked.clear();
    }

    pub fn blocked_keys(&self) -> impl Iterator<Item = &CacheKey> {
        self.blocked.iter()
    }

    /// Measures the cache cost of node results from the heap: bytes in use after minus before
    /// evaluating a node, instead of the estimate from [`Kernel::shape_bytes`]. For a process
    /// with one engine thread (the CLI daemon); other threads' allocations count too.
    pub fn set_heap_probe(&mut self, probe: Option<HeapProbe>) {
        self.heap_probe = probe;
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

    /// Changes the cache limits (evicts as needed): entries, estimated bytes.
    pub fn set_cache_limits(&mut self, entries: usize, bytes: usize) {
        self.config.cache_entries = entries;
        self.config.cache_bytes = bytes;
        self.cache.set_limits(bytes, entries);
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Number of cached node results.
    pub fn cache_len(&self) -> usize {
        self.cache.entries().filter(|(k, _, _)| matches!(k, Slot::Node(_))).count()
    }

    /// Size, limits and counters of the cache.
    pub fn cache_stats(&self) -> CacheStats {
        let mut st = CacheStats {
            limit_bytes: self.cache.limit(),
            max_entries: self.cache.max_len(),
            hits: self.cache.hits,
            misses: self.cache.misses,
            evictions: self.cache.evictions,
            ..Default::default()
        };
        for (k, _, cost) in self.cache.entries() {
            match k {
                Slot::Node(_) => {
                    st.nodes += 1;
                    st.node_bytes += cost;
                }
                // meshes and derived data (mass properties, B-reps for STEP)
                Slot::Mesh(..) | Slot::Mass(_) | Slot::Brep(..) | Slot::Regions(_) | Slot::Aux(_) => {
                    st.meshes += 1;
                    st.mesh_bytes += cost;
                }
            }
        }
        st
    }

    /// Data a caller stored with [`Engine::aux_insert`] under `key` (`None` if missing,
    /// evicted or of another type).
    pub fn aux_get<T: 'static>(&mut self, key: &CacheKey) -> Option<Rc<T>> {
        match self.cache.get(&Slot::Aux(*key)) {
            Some(Cached::Aux(a)) => a.downcast::<T>().ok(),
            _ => None,
        }
    }

    /// Stores caller data in the cache (LRU with the node results; `cost`: estimated bytes).
    /// The key should be derived from node keys ([`KeyHasher`]), so a result is never stale.
    pub fn aux_insert<T: 'static>(&mut self, key: CacheKey, value: Rc<T>, cost: usize) {
        self.cache.insert(Slot::Aux(key), Cached::Aux(value), cost);
    }

    /// Removes the node results and meshes of these keys from the cache.
    pub fn evict_keys(&mut self, keys: &HashSet<CacheKey>) -> usize {
        let before = self.cache.len();
        self.cache.retain(|k, _| !keys.contains(k.key()));
        before - self.cache.len()
    }

    fn cached_node(&mut self, key: &CacheKey) -> Option<Rc<NodeOutput<K::Shape>>> {
        match self.cache.get(&Slot::Node(*key)) {
            Some(Cached::Node(o)) => Some(o),
            _ => None,
        }
    }

    /// Estimated memory of a node result (shapes from [`Kernel::shape_bytes`]).
    fn output_bytes(&mut self, o: &NodeOutput<K::Shape>) -> usize {
        let pts = |v: &[Vec<DVec3>]| v.iter().map(|p| 24 * p.len() + 24).sum::<usize>();
        let mut b = 256 + std::mem::size_of::<NodeOutput<K::Shape>>() + 4 * o.tags.len();
        for p in &o.parts {
            b += 32 + self.kernel.shape_bytes(&p.shape);
        }
        b += own_mesh_bytes(o) + 96 * o.meshes.len();
        b += pts(&o.selected_edges) + pts(&o.opened_faces);
        b += o.guides.iter().map(|(_, p)| 24 * p.len() + 40).sum::<usize>();
        b += o.hints.iter().map(|h| h.len() + 24).sum::<usize>();
        b += o.diags.iter().map(|d| d.message.len() + 48 + pts(&d.edges)).sum::<usize>();
        b
    }

    /// Kernel calls made by evaluations of this engine (not counting tessellation).
    pub fn kernel_calls(&self) -> u64 {
        self.kernel_calls
    }

    /// Number of interned tags (distinct node contents this engine has seen; only grows).
    pub fn tag_count(&self) -> usize {
        self.tags.keys.len()
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
        let mut keys = eval::compute_keys(&tree, self.kernel.id(), &[]);
        // Draft mode: blends known to be slow whose results are not cached are left out.
        let mut skip = vec![false; n];
        if self.draft
            && let Some(budget) = self.config.drag_blend_budget_ms
        {
            for &i in &order {
                if eval::is_blend_node(&tree, i)
                    && self.blend_ms.get(&tree.node(i).inst_key).is_some_and(|&ms| ms > budget)
                    && !self.cache.contains(&Slot::Node(keys[i as usize]))
                {
                    skip[i as usize] = true;
                }
            }
        }
        let mut tainted = vec![false; n];
        if skip.iter().any(|&s| s) {
            keys = eval::compute_keys(&tree, self.kernel.id(), &skip);
            for &i in &order {
                // post-order: children before parents
                let t = skip[i as usize] || tree.node(i).children.iter().any(|&c| tainted[c as usize]);
                tainted[i as usize] = t;
            }
        }
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
            draft: self.draft,
            skip,
            tainted,
            blocked: vec![false; n],
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
        let mkey = Slot::Mesh(ev.reports[node as usize].key, key::canon_f64(tol));
        if let Some(Cached::Mesh(m)) = self.cache.get(&mkey) {
            return Ok(m);
        }
        let open = match &ev.tree.node(node).op {
            cad_ir::Op::Profile(p) => profile::open_sketch_mesh(p, tol),
            _ => None,
        };
        let mesh = if let Some(m) = open {
            m
        } else if out.dim == Some(Dim::Two) {
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
            for p in &out.meshes {
                append_offset(&mut mesh, &p.display());
            }
            mesh
        };
        let cost = mesh_bytes(&mesh);
        let mesh = Rc::new(mesh);
        self.cache.insert(mkey, Cached::Mesh(mesh.clone()), cost);
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

    /// STEP (exact B-rep from [`Kernel::brep`]) of the root, in the root's own system like the
    /// mesh exports. Faces get the colour of the node their tag belongs to (`color()`), unless
    /// `opts.tag_colors` is set.
    pub fn export_step(
        &mut self,
        ev: &Evaluation<K::Shape>,
        opts: &cad_step::StepOptions,
    ) -> Result<(Vec<u8>, cad_step::StepStats), String> {
        // The root's display system is the context system: the same writer as for parts.
        let root = ev.root().ok_or("kein Ergebnis")?;
        self.export_step_nodes(ev, &[root], opts)
    }

    fn export_mesh(&mut self, ev: &Evaluation<K::Shape>, tol: Option<f64>) -> Result<Rc<Mesh>, String> {
        let root = ev.root().ok_or("kein Ergebnis")?;
        if ev.reports[root as usize].dim == Some(Dim::Two) {
            return Err("2D-Ergebnis kann nicht als Körper exportiert werden".into());
        }
        self.tessellate(ev, root, Some(tol.unwrap_or(0.01)))
    }

    /// Volume, area and centroid of a node's result (sum over its parts; cached by key).
    pub fn mass_props(&mut self, ev: &Evaluation<K::Shape>, node: NodeIdx) -> Result<MassProps, String> {
        let slot = Slot::Mass(ev.reports[node as usize].key);
        if let Some(Cached::Mass(m)) = self.cache.get(&slot) {
            return Ok(m);
        }
        let m = self.mass_props_uncached(ev, node)?;
        self.cache.insert(slot, Cached::Mass(m), 128);
        Ok(m)
    }

    /// The shapes of a node for measuring (Plan 18): per part the neutral B-rep and a
    /// tessellation with tolerance `tol`, both placed with `world` (context coordinates; face
    /// ids of the mesh are those of the B-rep), whether it is a solid, and its mass
    /// properties. Not cached (the session keeps the result per node).
    pub fn measure_parts(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        world: &DMat4,
        tol: f64,
    ) -> Result<Vec<MeasurePart>, String> {
        let mut out = Vec::new();
        for (i, p) in ev.parts(node).iter().enumerate() {
            let placed;
            let s = if *world == DMat4::IDENTITY {
                &p.shape
            } else {
                placed = self.kernel.transform(&p.shape, world).map_err(|e| e.to_string())?;
                &placed
            };
            if self.kernel.is_empty(s) {
                continue;
            }
            let brep = self.kernel.brep(s).map_err(|e| e.to_string())?;
            let mesh = self.kernel.tessellate(s, tol).map_err(|e| e.to_string())?;
            let mass = if p.solid { self.kernel.mass_props(s).ok() } else { None };
            out.push(MeasurePart { part: i, brep: Rc::new(brep), mesh: Rc::new(mesh), solid: p.solid, mass });
        }
        // meshes: facetted B-reps (index after the B-rep parts)
        let n = ev.parts(node).len();
        for (i, p) in ev.meshes(node).iter().enumerate() {
            let m = *world * p.place;
            let mass = p.solid().then(|| p.mesh.mass_props(&m));
            out.push(MeasurePart {
                part: n + i,
                brep: Rc::new(p.mesh.brep(&m, p.tag)),
                mesh: Rc::new(p.mesh.display(&m, p.tag)),
                solid: p.solid(),
                mass,
            });
        }
        Ok(out)
    }

    /// The kept half `(q − point) · normal ≤ 0` of each solid part of a node placed with
    /// `world`, as neutral B-reps (`None`: nothing of the part stays). One kernel boolean per
    /// part (intersection with a box beyond the part); not cached.
    pub fn section_parts(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        world: &DMat4,
        point: DVec3,
        normal: DVec3,
    ) -> Result<Vec<(usize, Option<cad_kernel::brep::Brep>)>, String> {
        let n = normal.try_normalize().ok_or("Schnitt: Normale ist null")?;
        let x = n.any_orthonormal_vector();
        let y = n.cross(x);
        let frame = DMat4::from_cols(x.extend(0.0), y.extend(0.0), n.extend(0.0), point.extend(1.0));
        let mut out = Vec::new();
        for (i, p) in ev.parts(node).iter().enumerate() {
            if !p.solid {
                continue;
            }
            let placed;
            let s = if *world == DMat4::IDENTITY {
                &p.shape
            } else {
                placed = self.kernel.transform(&p.shape, world).map_err(|e| e.to_string())?;
                &placed
            };
            let Some((lo, hi)) = self.kernel.bounds(s).map_err(|e| e.to_string())? else { continue };
            let c = 0.5 * (lo + hi);
            let l = 2.0 * ((hi - lo).length() + (c - point).length()) + 1.0;
            let b =
                self.kernel.make_box(DVec3::new(-l, -l, -l), DVec3::new(l, l, 0.0), 0).map_err(|e| e.to_string())?;
            let b = self.kernel.transform(&b, &frame).map_err(|e| e.to_string())?;
            let cut = self
                .kernel
                .boolean(cad_kernel::BoolOp::Intersection, &[s.clone(), b], 0)
                .map_err(|e| format!("Schnitt im Kernel: {e}"))?;
            if self.kernel.is_empty(&cut) {
                out.push((i, None));
                continue;
            }
            out.push((i, Some(self.kernel.brep(&cut).map_err(|e| e.to_string())?)));
        }
        // closed meshes: the cap faces of the plane ∩ mesh (all `cad_measure` needs)
        let np = ev.parts(node).len();
        for (i, mp) in ev.meshes(node).iter().enumerate() {
            if !mp.solid() {
                continue;
            }
            let loops = mp.mesh.section_loops(&(*world * mp.place), point, n);
            let cap = (!loops.is_empty()).then(|| cad_kernel::trimesh::cap_brep(&loops, point, n, mp.tag));
            out.push((np + i, cap));
        }
        Ok(out)
    }

    /// Neutral B-rep of part `part` of a node, placed with `world` (cached by key and matrix).
    pub(crate) fn placed_brep(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        part: usize,
        world: &DMat4,
    ) -> Result<Rc<cad_kernel::brep::Brep>, String> {
        let slot = Slot::Brep(ev.reports[node as usize].key, KeyHasher::new().mat(world).finish().short(), part as u32);
        if let Some(Cached::Brep(b)) = self.cache.get(&slot) {
            return Ok(b);
        }
        let n = ev.parts(node).len();
        if part >= n {
            let mp = ev.meshes(node).get(part - n).ok_or("kein solcher Teil")?;
            let b = Rc::new(mp.mesh.brep(&(*world * mp.place), mp.tag));
            self.cache.insert(slot, Cached::Brep(b.clone()), brep_bytes(&b));
            return Ok(b);
        }
        let p = &ev.parts(node)[part];
        let b = if *world == DMat4::IDENTITY {
            self.kernel.brep(&p.shape)
        } else {
            let s = self.kernel.transform(&p.shape, world).map_err(|e| format!("STEP: {e}"))?;
            self.kernel.brep(&s)
        }
        .map_err(|e| format!("STEP: {e}"))?;
        let b = Rc::new(b);
        self.cache.insert(slot, Cached::Brep(b.clone()), brep_bytes(&b));
        Ok(b)
    }

    fn mass_props_uncached(&mut self, ev: &Evaluation<K::Shape>, node: NodeIdx) -> Result<MassProps, String> {
        let mut acc = MassProps::default();
        let mut c = DVec3::ZERO;
        for p in ev.parts(node) {
            let m = self.kernel.mass_props(&p.shape).map_err(|e| e.to_string())?;
            acc.volume += m.volume;
            acc.area += m.area;
            c += m.centroid * m.volume;
        }
        for p in ev.meshes(node) {
            let m = p.mesh.mass_props(&p.place);
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
pub(crate) fn append_offset(dst: &mut Mesh, m: &Mesh) {
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

/// Bytes of the imported mesh of an importing node (the data is shared by `Arc`: counted once,
/// at the node with the mesh's own tag and no children).
fn own_mesh_bytes<S>(o: &NodeOutput<S>) -> usize {
    o.meshes.iter().filter(|m| o.tags.len() == 1 && m.tag == o.tags[0]).map(|m| m.mesh.bytes()).sum()
}

/// Rough size of a neutral B-rep (points, knots, topology).
fn brep_bytes(b: &cad_kernel::brep::Brep) -> usize {
    use cad_kernel::brep::{Curve, Loop, Surface};
    let bs = |knots: usize, points: usize| 8 * knots + 32 * points + 64;
    let mut n = 256 + 24 * b.vertices.len();
    for e in &b.edges {
        n += 48
            + match &e.curve {
                Curve::BSpline(c) => bs(c.knots.len(), c.points.len()),
                _ => 96,
            };
    }
    for f in b.faces() {
        n += 160;
        n += match &f.surface {
            Surface::BSpline(s) => bs(s.knots_u.len() + s.knots_v.len(), s.points.len()),
            Surface::Revolution { curve: c, .. } | Surface::Extrusion { curve: c, .. } => {
                bs(c.knots.len(), c.points.len())
            }
            _ => 0,
        };
        for l in &f.loops {
            n += 32 + if let Loop::Edges(es) = l { 8 * es.len() } else { 0 };
        }
    }
    n
}

pub(crate) fn mesh_bytes(m: &Mesh) -> usize {
    4 * (m.positions.len()
        + m.normals.len()
        + m.indices.len()
        + m.tri_tags.len()
        + m.tri_faces.len()
        + m.edge_segments.len()
        + m.edge_ids.len())
        + 64
}

/// `true` for nodes whose op is a profile operation (2D children).
/// Exact region of a 2D node in its own system: loops of lines and arcs (outer loops
/// counter-clockwise, holes clockwise), as `offset`, `fillet2d`, `stroke` and 2D booleans make
/// them (Plan 17.3). B-splines enter as biarcs.
pub fn region_2d(tree: &Tree, idx: NodeIdx) -> Result<Vec<Vec<cad_kernel::Segment>>, String> {
    let mut warn = Vec::new();
    let e = profile::lift(tree, idx, DMat4::IDENTITY, &mut warn)?;
    op2d::expr_loops(&e)
}

pub(crate) fn is_profile_op(op: &Op) -> bool {
    matches!(op, Op::LinearExtrude { .. } | Op::RotateExtrude { .. } | Op::Loft { .. } | Op::Sweep { .. })
}
