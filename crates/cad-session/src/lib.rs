//! A CAD session: source text + version → parse → language evaluation (file or module
//! context with overrides) → stepwise kernel job → scene, meshes, edits, exports.
//!
//! Generic over [`Kernel`]; the same session runs in the browser worker (`cad-wasm`), in the
//! CLI and in tests. Messages are the serde types of [`protocol`].
//!
//! * [`Session::post`] handles one UI message and answers at once. It is cheap: parsing and
//!   the language evaluation take milliseconds; no kernel work happens here.
//! * [`Session::step`] does the kernel work (evaluation job, then queued meshes and exports)
//!   until the time budget is used up. Call it while [`Session::busy`].
//! * A new `SetSource`/`SetContext` drops the running job; finished nodes stay in the engine
//!   cache, so the next job only recomputes the path to the root.
//! * Syntax errors: only `Diagnostics` are sent, the last scene stays valid (Spec: Fehler).
//! * While a handle is dragged, meshes are tessellated with a coarse tolerance
//!   ([`SessionOptions::coarse_factor`] × the fine one), afterwards fine again. Jobs started
//!   during a drag run in the engine's draft mode (slow fillets/chamfers left out, see
//!   `cad_engine::Engine::set_draft`); `DragEnd` without a new source re-evaluates in full.
//! * After every finished job the root mesh is sent automatically
//!   ([`SessionOptions::auto_root_mesh`]) unless it was already sent with that key and
//!   tolerance class.

pub mod convert;
pub mod protocol;

use std::collections::VecDeque;
use std::rc::Rc;

use cad_engine::{Budget, Engine, EngineConfig, Evaluation, Job, Step};
use cad_eval::{Context, DragOptions, EvalOptions, EvalOutput};
use cad_ir::NodeIdx;
use cad_kernel::{Kernel, MassProps, Mesh};

use convert::{KernelInfo, SceneInput, Source};
use protocol::{self as p, FromEngine, ToEngine};

pub use protocol::PROTOCOL_VERSION;

/// Monotonic clock in milliseconds.
pub type SharedClock = Rc<dyn Fn() -> f64>;

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub eval: EvalOptions,
    pub drag: DragOptions,
    pub engine: EngineConfig,
    /// Coarse tolerance = fine tolerance × this factor (during drags).
    pub coarse_factor: f64,
    /// Send the root mesh after every finished job.
    pub auto_root_mesh: bool,
    /// Tolerance of exports in mm.
    pub export_tol: f64,
}

impl Default for SessionOptions {
    fn default() -> Self {
        SessionOptions {
            eval: EvalOptions::default(),
            drag: DragOptions::default(),
            engine: EngineConfig::default(),
            coarse_factor: 10.0,
            auto_root_mesh: true,
            export_tol: 0.01,
        }
    }
}

/// The last source that parsed, with its language evaluation.
pub struct Current {
    pub version: u64,
    pub src: Source,
    pub out: EvalOutput,
    pub eval_ms: f64,
    /// Syntax and evaluation diagnostics (protocol form).
    pub lang_diagnostics: Vec<p::Diagnostic>,
}

/// A finished kernel evaluation of [`Current`] at `version`.
pub struct Done<S> {
    pub version: u64,
    pub ev: Evaluation<S>,
    pub scene: p::Scene,
}

struct Running<S> {
    job: Job<S>,
    version: u64,
    kernel_ms: f64,
}

#[derive(Debug, Clone)]
struct MeshReq {
    nodes: Vec<NodeIdx>,
    coarse: bool,
    version: u64,
}

pub struct Session<K: Kernel> {
    engine: Engine<K>,
    opts: SessionOptions,
    clock: SharedClock,
    /// Latest source text (may have syntax errors).
    text: String,
    version: u64,
    has_source: bool,
    context: Context,
    cur: Option<Current>,
    running: Option<Running<K::Shape>>,
    done: Option<Done<K::Shape>>,
    drag: Option<u64>,
    mesh_queue: VecDeque<MeshReq>,
    export: Option<p::ExportFormat>,
    /// (key, coarse) of the root mesh sent last.
    last_root_mesh: Option<(String, bool)>,
}

impl<K: Kernel> Session<K>
where
    K::Shape: 'static,
{
    pub fn new(kernel: K, opts: SessionOptions, clock: SharedClock) -> Self {
        let mut engine = Engine::with_config(kernel, opts.engine.clone());
        let c = clock.clone();
        engine.set_clock(Box::new(move || c()));
        Session {
            engine,
            opts,
            clock,
            text: String::new(),
            version: 0,
            has_source: false,
            context: Context::File,
            cur: None,
            running: None,
            done: None,
            drag: None,
            mesh_queue: VecDeque::new(),
            export: None,
            last_root_mesh: None,
        }
    }

    pub fn options(&self) -> &SessionOptions {
        &self.opts
    }

    pub fn engine(&self) -> &Engine<K> {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut Engine<K> {
        &mut self.engine
    }

    /// The last source that parsed and its language evaluation.
    pub fn current(&self) -> Option<&Current> {
        self.cur.as_ref()
    }

    /// The last finished kernel evaluation.
    pub fn done(&self) -> Option<&Done<K::Shape>> {
        self.done.as_ref()
    }

    /// Kernel job, mesh or export work pending: call [`Session::step`].
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.mesh_queue.is_empty() || (self.export.is_some() && self.done.is_some())
    }

    pub fn drag_active(&self) -> bool {
        self.drag.is_some()
    }

    /// Handles one message from the UI.
    pub fn post(&mut self, msg: ToEngine) -> Vec<FromEngine> {
        match msg {
            ToEngine::SetSource { text, version } => {
                self.text = text;
                self.version = version;
                self.has_source = true;
                self.reevaluate()
            }
            ToEngine::SetContext { module, overrides } => {
                self.context = match module {
                    None => Context::File,
                    Some(name) => Context::Module {
                        name,
                        overrides: overrides.iter().map(|(k, v)| (k.clone(), convert::pvalue(v))).collect(),
                    },
                };
                if self.has_source { self.reevaluate() } else { Vec::new() }
            }
            ToEngine::RequestMeshes { nodes, coarse, version } => {
                let Some(d) = &self.done else {
                    return vec![self.error("keine Auswertung vorhanden", None)];
                };
                if version.is_some_and(|v| v != d.version) {
                    return Vec::new();
                }
                let coarse = coarse.unwrap_or(self.drag.is_some());
                self.mesh_queue.push_back(MeshReq { nodes, coarse, version: d.version });
                Vec::new()
            }
            ToEngine::DragStart { handle } => {
                self.drag = Some(handle);
                self.snap_targets(handle)
            }
            ToEngine::Drag { handle, target } => {
                self.drag = Some(handle);
                let mut res = self.drag_edits(handle, &target);
                if res.is_empty() {
                    // Always answer: the UI keeps at most one drag in flight.
                    res.push(FromEngine::Edits { version: self.cur.as_ref().map_or(self.version, |c| c.version), edits: Vec::new() });
                }
                res
            }
            ToEngine::DragEnd { handle, target } => {
                self.drag = None;
                let res = match target {
                    Some(t) => self.drag_edits(handle, &t),
                    None => Vec::new(),
                };
                let draft = self.running.as_ref().is_some_and(|r| r.job.suspended_blends() > 0)
                    || self.done.as_ref().is_some_and(|d| d.ev.draft && d.ev.stats.suspended_blends > 0);
                if res.is_empty() && draft && self.has_source {
                    // No new source follows, but the scene left blends out: evaluate in full.
                    return self.reevaluate();
                }
                if res.is_empty() {
                    // No new source follows: refine the coarse root mesh of the current state.
                    if let Some(d) = &self.done
                        && self.running.is_none()
                        && self.last_root_mesh.as_ref().is_some_and(|(_, c)| *c)
                        && let Some(root) = d.ev.root()
                    {
                        self.mesh_queue.push_back(MeshReq { nodes: vec![root], coarse: false, version: d.version });
                    }
                }
                res
            }
            ToEngine::SnapTargets { handle } => self.snap_targets(handle),
            ToEngine::SetDragOptions { step_mm, step_deg } => {
                let ok = |v: f64| v.is_finite() && v >= 0.0;
                if ok(step_mm) {
                    self.opts.drag.step_mm = step_mm;
                }
                if ok(step_deg) {
                    self.opts.drag.step_deg = step_deg;
                }
                Vec::new()
            }
            ToEngine::SetDefault { module, param, value } => {
                let (file, _) = cad_syntax::parse(&self.text);
                let text = convert::value_text(&value);
                match cad_eval::set_default_edit(&self.text, &file, &module, &param, &text) {
                    Some(e) => {
                        let src = Source::new(self.text.clone());
                        vec![FromEngine::Edits { version: self.version, edits: vec![src.edit(&e)] }]
                    }
                    None => vec![self.error(&format!("Parameter {param} von Modul {module} nicht gefunden"), None)],
                }
            }
            ToEngine::Export { format } => {
                if self.done.is_none() && self.running.is_none() {
                    return vec![self.error("keine Auswertung vorhanden", None)];
                }
                self.export = Some(format);
                Vec::new()
            }
            ToEngine::Cancel => {
                self.running = None;
                self.mesh_queue.clear();
                self.export = None;
                Vec::new()
            }
            ToEngine::BlockKeys { keys } => {
                let mut ok = Vec::new();
                let mut bad = Vec::new();
                for k in keys {
                    match cad_engine::CacheKey::from_hex(&k) {
                        Some(key) => ok.push(key),
                        None => bad.push(k),
                    }
                }
                self.engine.block_keys(ok);
                if bad.is_empty() { Vec::new() } else { vec![self.error(&format!("ungültige Schlüssel: {}", bad.join(", ")), None)] }
            }
        }
    }

    fn error(&self, msg: &str, node: Option<u32>) -> FromEngine {
        FromEngine::Error { message: msg.to_string(), version: Some(self.version), node }
    }

    fn eval_options(&self) -> EvalOptions {
        EvalOptions { context: self.context.clone(), ..self.opts.eval.clone() }
    }

    /// Parse + evaluate the latest text; start a kernel job.
    fn reevaluate(&mut self) -> Vec<FromEngine> {
        let t0 = (self.clock)();
        let (file, parse_diags) = cad_syntax::parse(&self.text);
        let src = Source::new(self.text.clone());
        if parse_diags.iter().any(|d| d.severity == cad_syntax::Severity::Error) {
            // Keep the last scene and the running job (it belongs to the last valid source).
            return vec![FromEngine::Diagnostics {
                version: self.version,
                items: convert::lang_diagnostics(&src, &parse_diags, p::DiagSource::Syntax),
            }];
        }
        let mut out = cad_eval::eval(&self.text, &file, &self.eval_options());
        let eval_ms = (self.clock)() - t0;
        let mut items = convert::lang_diagnostics(&src, &parse_diags, p::DiagSource::Syntax);
        items.extend(convert::lang_diagnostics(&src, &out.diagnostics, p::DiagSource::Eval));
        let mut diags = parse_diags;
        diags.append(&mut out.diagnostics);
        out.diagnostics = diags;

        self.running = None;
        self.engine.set_draft(self.drag.is_some());
        let job = self.engine.start(out.tree.clone());
        let keys: Vec<_> = (0..job.tree().nodes.len() as u32).map(|n| job.key(n)).collect();
        let tags: Vec<u32> = (0..job.tree().nodes.len() as u32).map(|n| job.tag(n)).collect();
        let cur = Current { version: self.version, src, out, eval_ms, lang_diagnostics: items.clone() };
        let scene = convert::scene(SceneInput {
            version: self.version,
            src: &cur.src,
            out: &cur.out,
            kernel: KernelInfo::<K::Shape>::Pending { keys: &keys, tags: &tags },
            stats: p::Stats { eval_ms, eval_steps: cur.out.steps as u32, ..Default::default() },
        });
        let progress = FromEngine::Progress {
            version: self.version,
            done: 0,
            total: job.total() as u32,
            current: job.current().map(|n| node_ref(&cur, n, &job)),
        };
        self.cur = Some(cur);
        self.running = Some(Running { job, version: self.version, kernel_ms: 0.0 });
        vec![FromEngine::Diagnostics { version: self.version, items }, FromEngine::Scene(Box::new(scene)), progress]
    }

    fn snap_targets(&self, handle: u64) -> Vec<FromEngine> {
        let Some(cur) = &self.cur else { return Vec::new() };
        let targets = cad_eval::snap_targets(&cur.out, handle)
            .into_iter()
            .map(|t| p::SnapTarget {
                text: t.text,
                snap: snap_ref_to_p(&t.r#ref),
                local: convert::v3(t.local),
                world: convert::v3(t.world),
                result_world: convert::v3(t.result_world),
            })
            .collect();
        vec![FromEngine::SnapTargets { version: cur.version, handle, targets }]
    }

    fn drag_edits(&self, handle: u64, target: &p::DragTarget) -> Vec<FromEngine> {
        let Some(cur) = &self.cur else { return Vec::new() };
        let t = match target {
            p::DragTarget::Point { pos } => {
                cad_eval::DragTarget::Free(cad_eval::DragValue::Point(glam::DVec3::from_array(pos.0)))
            }
            p::DragTarget::Value { value } => cad_eval::DragTarget::Free(cad_eval::DragValue::Scalar(*value)),
            p::DragTarget::Snap { snap } => cad_eval::DragTarget::Snap(snap_ref_from_p(snap)),
        };
        let edits = cad_eval::drag_edits(&cur.out, &cur.src.text, handle, &t, &self.opts.drag);
        if edits.is_empty() {
            return Vec::new();
        }
        vec![FromEngine::Edits { version: cur.version, edits: edits.iter().map(|e| cur.src.edit(e)).collect() }]
    }

    /// Kernel work until `budget_ms` is used up (`None`: until idle). Always makes progress.
    pub fn step(&mut self, budget_ms: Option<f64>) -> Vec<FromEngine> {
        let t0 = (self.clock)();
        let deadline = budget_ms.map(|b| t0 + b);
        if let Some(r) = &mut self.running {
            let budget = budget_ms.map(Budget::ms).unwrap_or(Budget::UNLIMITED);
            let st = r.job.step(&mut self.engine, budget);
            r.kernel_ms += (self.clock)() - t0;
            return match st {
                Step::Pending { done, total } => {
                    let cur = self.cur.as_ref().expect("running job without source");
                    vec![FromEngine::Progress {
                        version: r.version,
                        done: done as u32,
                        total: total as u32,
                        current: r.job.current().map(|n| node_ref(cur, n, &r.job)),
                    }]
                }
                Step::Done(ev) => {
                    let r = self.running.take().expect("running");
                    self.finish(ev, r)
                }
            };
        }
        let mut res = Vec::new();
        let mut items: Vec<p::MeshData> = Vec::new();
        let mut items_version = 0;
        while let Some(req) = self.mesh_queue.pop_front() {
            if !items.is_empty() && items_version != req.version {
                self.mesh_queue.push_front(req);
                break;
            }
            items_version = req.version;
            match self.meshes(&req) {
                Ok(mut m) => items.append(&mut m),
                Err(e) => res.push(self.error(&e, None)),
            }
            if deadline.is_some_and(|d| (self.clock)() >= d) {
                break;
            }
        }
        if !items.is_empty() {
            res.push(FromEngine::Meshes { version: items_version, items });
        }
        if res.is_empty()
            && let Some(fmt) = self.export.take()
        {
            res.push(self.do_export(fmt));
        }
        res
    }

    /// Runs [`Session::step`] until idle and collects all answers.
    pub fn run_to_idle(&mut self) -> Vec<FromEngine> {
        let mut res = Vec::new();
        while self.busy() {
            res.extend(self.step(None));
        }
        res
    }

    fn finish(&mut self, ev: Evaluation<K::Shape>, r: Running<K::Shape>) -> Vec<FromEngine> {
        let cur = self.cur.as_ref().expect("finished job without source");
        let stats = p::Stats {
            eval_ms: cur.eval_ms,
            kernel_ms: r.kernel_ms,
            nodes: ev.stats.nodes as u32,
            cache_hits: ev.stats.cache_hits as u32,
            kernel_calls: ev.stats.kernel_calls as u32,
            steps: ev.stats.steps,
            eval_steps: cur.out.steps as u32,
            suspended_blends: ev.stats.suspended_blends as u32,
        };
        let scene = convert::scene(SceneInput {
            version: r.version,
            src: &cur.src,
            out: &cur.out,
            kernel: KernelInfo::Done(&ev),
            stats,
        });
        let mut items = cur.lang_diagnostics.clone();
        items.extend(convert::kernel_diagnostics(&cur.src, &ev));
        let res =
            vec![FromEngine::Diagnostics { version: r.version, items }, FromEngine::Scene(Box::new(scene.clone()))];
        let root = ev.root();
        self.done = Some(Done { version: r.version, ev, scene });
        if self.opts.auto_root_mesh
            && let Some(root) = root
        {
            let coarse = self.drag.is_some();
            let key = self.done.as_ref().and_then(|d| d.scene.nodes[root as usize].mesh_key.clone());
            let already = key.is_some_and(|k| self.last_root_mesh.as_ref() == Some(&(k, coarse)));
            if !already {
                self.mesh_queue.push_front(MeshReq { nodes: vec![root], coarse, version: r.version });
            }
        }
        res
    }

    /// Display tolerance of a node.
    pub fn tolerance(&self, node: NodeIdx, coarse: bool) -> f64 {
        let fine = self
            .done
            .as_ref()
            .and_then(|d| d.ev.tree.nodes.get(node as usize))
            .and_then(|n| n.tol)
            .filter(|t| *t > 0.0)
            .unwrap_or(self.engine.config().default_tol);
        if coarse { fine * self.opts.coarse_factor } else { fine }
    }

    /// Mesh of a node of the last finished evaluation (node system).
    pub fn mesh(&mut self, node: NodeIdx, coarse: bool) -> Result<Rc<Mesh>, String> {
        let tol = self.tolerance(node, coarse);
        let d = self.done.as_ref().ok_or("keine Auswertung vorhanden")?;
        if node as usize >= d.ev.tree.nodes.len() {
            return Err(format!("Knoten {node} existiert nicht"));
        }
        self.engine.tessellate(&d.ev, node, Some(tol))
    }

    /// Volume, area, centroid of a node of the last finished evaluation.
    pub fn mass_props(&mut self, node: NodeIdx) -> Result<MassProps, String> {
        let d = self.done.as_ref().ok_or("keine Auswertung vorhanden")?;
        self.engine.mass_props(&d.ev, node)
    }

    fn meshes(&mut self, req: &MeshReq) -> Result<Vec<p::MeshData>, String> {
        let Some(d) = &self.done else { return Ok(Vec::new()) };
        if d.version != req.version {
            return Ok(Vec::new());
        }
        let root = d.ev.root();
        let mut out: Vec<p::MeshData> = Vec::new();
        for &n in &req.nodes {
            let Some(key) =
                self.done.as_ref().and_then(|d| d.scene.nodes.get(n as usize)).and_then(|s| s.mesh_key.clone())
            else {
                continue;
            };
            if let Some(m) = out.iter_mut().find(|m| m.key == key) {
                m.nodes.push(n);
                continue;
            }
            let tol = self.tolerance(n, req.coarse);
            let mesh = self.mesh(n, req.coarse)?;
            if Some(n) == root {
                self.last_root_mesh = Some((key.clone(), req.coarse));
            }
            out.push(p::MeshData {
                node: n,
                nodes: vec![n],
                key,
                coarse: req.coarse,
                tol,
                positions: mesh.positions.clone(),
                normals: mesh.normals.clone(),
                indices: mesh.indices.clone(),
                tri_tags: mesh.tri_tags.clone(),
                tri_faces: mesh.tri_faces.clone(),
                edge_segments: mesh.edge_segments.clone(),
                edge_ids: mesh.edge_ids.clone(),
            });
        }
        Ok(out)
    }

    fn do_export(&mut self, format: p::ExportFormat) -> FromEngine {
        match self.export_bytes(format) {
            Ok((bytes, triangles)) => FromEngine::ExportResult { format, bytes, triangles },
            Err(e) => self.error(&e, None),
        }
    }

    /// Export of the root of the last finished evaluation: (bytes, triangle count).
    pub fn export_bytes(&mut self, format: p::ExportFormat) -> Result<(Vec<u8>, u32), String> {
        let tol = Some(self.opts.export_tol);
        let d = self.done.as_ref().ok_or("keine Auswertung vorhanden")?;
        let root = d.ev.root().ok_or("kein Ergebnis")?;
        let bytes = match format {
            p::ExportFormat::Stl => self.engine.export_stl(&d.ev, tol)?,
            p::ExportFormat::ThreeMf => self.engine.export_3mf(&d.ev, tol)?,
        };
        let tris = self.engine.tessellate(&d.ev, root, tol)?.triangle_count() as u32;
        Ok((bytes, tris))
    }
}

fn node_ref<S>(cur: &Current, n: NodeIdx, job: &Job<S>) -> p::NodeRef {
    let node = cur.out.tree.node(n);
    p::NodeRef {
        node: n,
        stmt: convert::stmt(node.stmt),
        range: cur.src.range(node.span),
        key: Some(job.key(n).to_hex()),
    }
}

fn snap_ref_to_p(r: &cad_eval::SnapRef) -> p::SnapRef {
    match r {
        cad_eval::SnapRef::Grid { grid, planes } => p::SnapRef::Grid { grid: *grid, planes: planes.clone() },
        cad_eval::SnapRef::Anchor { inst, name } => p::SnapRef::Anchor { inst: *inst, name: name.clone() },
        cad_eval::SnapRef::Plane { grid, plane } => p::SnapRef::Plane { grid: *grid, plane: plane.clone() },
    }
}

fn snap_ref_from_p(r: &p::SnapRef) -> cad_eval::SnapRef {
    match r {
        p::SnapRef::Grid { grid, planes } => cad_eval::SnapRef::Grid { grid: *grid, planes: planes.clone() },
        p::SnapRef::Anchor { inst, name } => cad_eval::SnapRef::Anchor { inst: *inst, name: name.clone() },
        p::SnapRef::Plane { grid, plane } => cad_eval::SnapRef::Plane { grid: *grid, plane: plane.clone() },
    }
}

/// Clock based on `std::time::Instant` (not for wasm32-unknown-unknown).
#[cfg(not(target_arch = "wasm32"))]
pub fn std_clock() -> SharedClock {
    let t0 = std::time::Instant::now();
    Rc::new(move || t0.elapsed().as_secs_f64() * 1000.0)
}
