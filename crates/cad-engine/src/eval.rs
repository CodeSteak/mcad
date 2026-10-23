//! Evaluation of single nodes, cache keys and evaluation order.

use std::any::Any;
use std::rc::Rc;

use cad_ir::{BlendKind, Dim, Law, NodeIdx, Op, Prim, Select, Tree, ZoneShape};
use cad_kernel::{BoolOp, Class, EdgeInfo, FaceInfo, KResult, Kernel, KernelError, Loop, PlacedLoop, Tag};
use glam::{DMat4, DQuat, DVec2, DVec3};

use crate::key::{CacheKey, KeyHasher};
use crate::profile::{self, Expr2, Leaf};
use crate::{Diag, EngineConfig, NodeOutput, Part, Severity, is_profile_op};

/// The step budget is used up; the node is re-run (with replay) on the next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Interrupt;

pub(crate) fn exhausted(
    ops: u64,
    max_ops: Option<u64>,
    deadline: Option<f64>,
    clock: Option<&dyn Fn() -> f64>,
) -> bool {
    if ops == 0 {
        return false;
    }
    if let Some(m) = max_ops
        && ops >= m
    {
        return true;
    }
    if let (Some(d), Some(c)) = (deadline, clock) {
        return c() >= d;
    }
    false
}

/// Kernel access with budget checks and replay of an interrupted node's calls.
pub(crate) struct Run<'a, K: Kernel> {
    pub k: &'a mut K,
    pub replay: &'a mut Vec<Rc<dyn Any>>,
    pub cursor: usize,
    pub ops: &'a mut u64,
    pub max_ops: Option<u64>,
    pub deadline: Option<f64>,
    pub clock: Option<&'a dyn Fn() -> f64>,
}

impl<K: Kernel> Run<'_, K> {
    fn call<T: Clone + 'static>(&mut self, f: impl FnOnce(&mut K) -> KResult<T>) -> Result<KResult<T>, Interrupt> {
        if self.cursor < self.replay.len() {
            let v = self.replay[self.cursor]
                .downcast_ref::<KResult<T>>()
                .expect("replayed kernel call has a different type (non-deterministic node evaluation)")
                .clone();
            self.cursor += 1;
            return Ok(v);
        }
        if exhausted(*self.ops, self.max_ops, self.deadline, self.clock) {
            return Err(Interrupt);
        }
        let r = f(self.k);
        *self.ops += 1;
        self.replay.push(Rc::new(r.clone()));
        self.cursor += 1;
        Ok(r)
    }

    /// Like [`Run::call`], plus the call's duration in ms (0 without a clock); a replayed
    /// call returns the duration measured when it ran.
    fn call_timed<T: Clone + 'static>(
        &mut self,
        f: impl FnOnce(&mut K) -> KResult<T>,
    ) -> Result<(KResult<T>, f64), Interrupt> {
        let clock = self.clock;
        let r = self.call(|k| {
            let t0 = clock.map_or(0.0, |c| c());
            let r = f(k);
            Ok((r, clock.map_or(0.0, |c| c() - t0)))
        })?;
        Ok(r.unwrap_or_else(|e| (Err(e), 0.0)))
    }
}

pub(crate) struct Ctx<'a, S> {
    pub tree: &'a Tree,
    pub tags: &'a [Tag],
    pub outputs: &'a [Option<Rc<NodeOutput<S>>>],
    pub config: &'a EngineConfig,
    /// Per node: leave the blend out (draft evaluation while dragging, see `Engine::start`).
    pub skip: &'a [bool],
}

/// Hint of a blend node whose rounding/chamfer was left out while dragging.
pub const SUSPENDED_HINT: &str = "Verrundung beim Ziehen ausgesetzt";

// ---------------------------------------------------------------- order and keys

/// Post-order from the root; disabled subtrees are left out.
pub(crate) fn post_order(tree: &Tree) -> Vec<NodeIdx> {
    let mut out = Vec::new();
    let Some(root) = tree.root else { return out };
    if tree.node(root).flags.disabled {
        return out;
    }
    let mut stack = vec![(root, 0usize)];
    while let Some((n, i)) = stack.pop() {
        let ch = &tree.node(n).children;
        if i < ch.len() {
            stack.push((n, i + 1));
            let c = ch[i];
            if !tree.node(c).flags.disabled {
                stack.push((c, 0));
            }
        } else {
            out.push(n);
        }
    }
    out
}

fn is_identity(m: &DMat4) -> bool {
    m.abs_diff_eq(DMat4::IDENTITY, 1e-12)
}

/// Blend/shell block a zone belongs to: (block, body child, matrix block system → zone system
/// as product of the locals on the path below the block).
pub(crate) fn zone_block(tree: &Tree, zone: NodeIdx) -> Option<(NodeIdx, NodeIdx, DMat4)> {
    let mut path = DMat4::IDENTITY; // zone system → system of `cur`'s parent... accumulated
    let mut cur = zone;
    loop {
        let n = tree.node(cur);
        path = n.local * path;
        let p = n.parent?;
        let pn = tree.node(p);
        if matches!(pn.op, Op::Blend { .. } | Op::Shell { .. }) && pn.children.first() != Some(&cur) {
            let body = *pn.children.first()?;
            return Some((p, body, path));
        }
        cur = p;
    }
}

/// `true` for nodes that round or chamfer edges (candidates for skipping while dragging).
pub(crate) fn is_blend_node(tree: &Tree, n: NodeIdx) -> bool {
    let node = tree.node(n);
    match &node.op {
        Op::Blend { .. } => true,
        Op::Boolean { fillet, chamfer, fillet_law, .. } => {
            fillet.is_some_and(|r| r > 0.0) || chamfer.is_some_and(|c| c > 0.0) || fillet_law.is_some()
        }
        Op::Prim { .. } => node.blend.is_some_and(|(_, r)| r > 0.0),
        _ => false,
    }
}

/// Keys of all nodes (zero key for nodes not reachable from the root). Nodes with `skip[n]`
/// (blends left out while dragging) get a marked key, which changes the keys of all their
/// ancestors as well: such draft results never share cache entries with full results.
pub(crate) fn compute_keys(tree: &Tree, kernel_id: &str, skip: &[bool]) -> Vec<CacheKey> {
    let mut keys = vec![CacheKey([0; 32]); tree.nodes.len()];
    // Post-order over everything reachable, including disabled subtrees (cheap, keeps the
    // order simple); children before parents, earlier siblings before later ones.
    let Some(root) = tree.root else { return keys };
    let mut order = Vec::new();
    let mut stack = vec![(root, 0usize)];
    while let Some((n, i)) = stack.pop() {
        let ch = &tree.node(n).children;
        if i < ch.len() {
            stack.push((n, i + 1));
            stack.push((ch[i], 0));
        } else {
            order.push(n);
        }
    }
    for n in order {
        let node = tree.node(n);
        let mut h = KeyHasher::new();
        h.str("cad-engine/1").str(kernel_id);
        match &node.op {
            // Colour and filament do not change the geometry: a new colour or slot recomputes
            // nothing (colour regions have their own key, see `colors.rs`).
            Op::Color { .. } => h.str("color"),
            op => h.value(op),
        };
        h.value(&node.blend);
        if skip.get(n as usize).copied().unwrap_or(false) {
            h.str("draft: blend suspended");
        }
        if let Op::Zone(_) = node.op {
            match zone_block(tree, n) {
                Some((_, body, path)) => {
                    let rel = path.inverse() * tree.node(body).local;
                    h.tag(1).key(&keys[body as usize]).mat(&rel);
                }
                None => {
                    h.tag(0);
                }
            }
        }
        for (i, &c) in node.children.iter().enumerate() {
            let cn = tree.node(c);
            let f = cn.flags;
            h.u64(i as u64);
            if f.disabled {
                h.tag(b'*');
                continue;
            }
            if f.background {
                h.tag(b'%');
                continue;
            }
            h.tag(if f.zone { b'z' } else { b'c' });
            h.key(&keys[c as usize]).mat(&cn.local);
        }
        keys[n as usize] = h.finish();
    }
    keys
}

// ---------------------------------------------------------------- node evaluation

/// Do all interior sample points of an edge (and its midpoint between the ends) lie on one of
/// the polylines (within 1e-6 of the model size)?
fn on_polylines(points: &[DVec3], lines: &[Vec<DVec3>]) -> bool {
    if points.len() < 2 || lines.is_empty() {
        return false;
    }
    let (mut lo, mut hi) = (points[0], points[0]);
    for p in points.iter().chain(lines.iter().flatten()) {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let tol = 1e-6 * (hi - lo).length().max(1.0);
    let dist = |q: DVec3, l: &[DVec3]| {
        l.windows(2)
            .map(|w| {
                let ab = w[1] - w[0];
                let t = if ab.length_squared() > 0.0 {
                    ((q - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (w[0] + ab * t - q).length()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let mut probes: Vec<DVec3> = points[1..points.len() - 1].to_vec();
    if probes.is_empty() {
        probes.push((points[0] + points[points.len() - 1]) * 0.5);
    }
    // all probes on the same old edge (a curved old edge's polyline is a chord: only exact for
    // straight edges, which is where merged coplanar faces occur)
    lines.iter().any(|l| probes.iter().all(|&q| dist(q, l) <= tol))
}

/// Number for hints and messages: at most 3 decimals, no trailing zeros.
fn fmt_num(x: f64) -> String {
    let s = format!("{:.3}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// `2–4` (en dash), or `3` for a constant radius.
fn radius_range(lo: f64, hi: f64) -> String {
    if fmt_num(lo) == fmt_num(hi) { fmt_num(lo) } else { format!("{}–{}", fmt_num(lo), fmt_num(hi)) }
}

/// Stations of a radius law along an edge with these sample points: about one per 1.5 mm of
/// length, at least 4 (a cubic law), at most 32.
fn law_station_count(points: &[DVec3]) -> u32 {
    let len: f64 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    ((len / 1.5).round() as u32 + 1).clamp(4, 32)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

pub(crate) fn transform_box(b: (DVec3, DVec3), m: &DMat4) -> (DVec3, DVec3) {
    let (lo, hi) = b;
    let mut rlo = DVec3::splat(f64::INFINITY);
    let mut rhi = DVec3::splat(f64::NEG_INFINITY);
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let p = m.transform_point3(c);
        rlo = rlo.min(p);
        rhi = rhi.max(p);
    }
    (rlo, rhi)
}

pub(crate) fn merge_box(a: Option<(DVec3, DVec3)>, b: Option<(DVec3, DVec3)>) -> Option<(DVec3, DVec3)> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
    }
}

struct NodeEval<'r, 'a, 'c, K: Kernel> {
    run: &'r mut Run<'a, K>,
    ctx: &'c Ctx<'c, K::Shape>,
    idx: NodeIdx,
    tag: Tag,
    diags: Vec<Diag>,
    hints: Vec<String>,
    selected: Vec<Vec<DVec3>>,
    opened: Vec<Vec<DVec3>>,
    /// Time of this node's fillet/chamfer kernel calls (ms).
    blend_ms: f64,
    suspended: bool,
}

type R<T> = Result<T, Interrupt>;

pub(crate) fn eval_node<K: Kernel>(
    run: &mut Run<'_, K>,
    ctx: &Ctx<'_, K::Shape>,
    idx: NodeIdx,
) -> Result<NodeOutput<K::Shape>, Interrupt>
where
    K::Shape: 'static,
{
    let mut ev = NodeEval {
        run,
        ctx,
        idx,
        tag: ctx.tags[idx as usize],
        diags: Vec::new(),
        hints: Vec::new(),
        selected: Vec::new(),
        opened: Vec::new(),
        blend_ms: 0.0,
        suspended: false,
    };
    let tree = ctx.tree;
    let node = tree.node(idx);
    let dim = tree.dim(idx);

    // Tags this result can carry: own tag plus contributing children.
    let mut tags: Vec<Tag> = vec![ev.tag];
    for c in tree.solid_children(idx) {
        if let Some(o) = &ctx.outputs[c as usize] {
            tags.extend_from_slice(&o.tags);
        }
    }
    if let (Op::Blend { .. } | Op::Shell { .. }, Some(&b)) = (&node.op, node.children.first())
        && let Some(o) = &ctx.outputs[b as usize]
    {
        tags.extend_from_slice(&o.tags);
    }
    tags.sort_unstable();
    tags.dedup();

    if dim == Some(Dim::Two) && !is_profile_op(&node.op) {
        // 2D node on its own: no kernel work; bounds from the polygon approximation.
        let mut warn = Vec::new();
        let open = match &node.op {
            Op::Profile(p) => profile::open_sketch_bounds(p, ctx.config.default_tol),
            _ => None,
        };
        let bounds = match open {
            // an open sketch is a path (sweep), not a region
            Some(b) => b.map(|(lo, hi)| (lo.extend(0.0), hi.extend(0.0))),
            None => match profile::lift(tree, idx, DMat4::IDENTITY, &mut warn) {
                Ok(e) => {
                    profile::expr_bounds(&e, ctx.config.default_tol).map(|(lo, hi)| (lo.extend(0.0), hi.extend(0.0)))
                }
                Err(msg) => {
                    ev.error(msg);
                    None
                }
            },
        };
        for w in warn {
            ev.warn(w);
        }
        return Ok(NodeOutput {
            parts: Vec::new(),
            tags: tags.into(),
            dim,
            bounds,
            diags: ev.diags,
            hints: ev.hints,
            selected_edges: Vec::new(),
            opened_faces: Vec::new(),
            blend_ms: 0.0,
            suspended: false,
        });
    }

    let parts = ev.eval_op()?;
    let bounds = match ev.derived_bounds() {
        Some(b) => b,
        None => {
            let mut bounds = None;
            for p in &parts {
                if let Ok(Some(b)) = ev.run.call(|k| k.bounds(&p.shape))? {
                    bounds = merge_box(bounds, Some(b));
                }
            }
            bounds
        }
    };
    Ok(NodeOutput {
        parts,
        tags: tags.into(),
        dim: dim.or(Some(Dim::Three)),
        bounds,
        diags: ev.diags,
        hints: ev.hints,
        selected_edges: ev.selected,
        opened_faces: ev.opened,
        blend_ms: ev.blend_ms,
        suspended: ev.suspended,
    })
}

/// The linear part maps every axis onto an axis (signed permutation times a diagonal scale):
/// a box transformed by it is the box of the transformed body.
fn axis_aligned(m: &DMat4) -> bool {
    [m.x_axis, m.y_axis, m.z_axis].iter().all(|c| {
        let c = c.truncate().abs();
        let big = c.max_element();
        big > 0.0 && (c.x + c.y + c.z - big) <= 1e-12 * big
    })
}

impl<K: Kernel> NodeEval<'_, '_, '_, K>
where
    K::Shape: 'static,
{
    fn error(&mut self, msg: impl Into<String>) {
        self.diags.push(Diag { severity: Severity::Error, message: msg.into(), edges: Vec::new() });
    }

    /// Bounds without a kernel call where they follow exactly from the children: groups,
    /// transforms and plain unions (the tight box of a union is the union of the tight
    /// boxes; a failed union returns its inputs) whose children are placed by axis-aligned
    /// matrices. `None`: ask the kernel. The tight box costs more than the union itself on
    /// bodies with rounds (lochplatte: 5 ms per call).
    fn derived_bounds(&mut self) -> Option<Option<(DVec3, DVec3)>> {
        let tree = self.ctx.tree;
        let node = tree.node(self.idx);
        let plain = match &node.op {
            Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty => true,
            Op::Boolean { op: BoolOp::Union, fillet, chamfer, fillet_law } => {
                !fillet.is_some_and(|r| r > 0.0) && !chamfer.is_some_and(|c| c > 0.0) && fillet_law.is_none()
            }
            _ => false,
        };
        if !plain {
            return None;
        }
        let mut b = None;
        for c in tree.solid_children(self.idx) {
            if tree.dim(c) == Some(Dim::Two) {
                continue;
            }
            let Some(o) = self.output(c) else { continue };
            if o.parts.is_empty() {
                continue;
            }
            let m = tree.node(c).local;
            if !axis_aligned(&m) {
                return None;
            }
            b = merge_box(b, Some(transform_box(o.bounds?, &m)));
        }
        Some(b)
    }

    fn error_edges(&mut self, msg: impl Into<String>, edges: Vec<Vec<DVec3>>) {
        self.diags.push(Diag { severity: Severity::Error, message: msg.into(), edges });
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.diags.push(Diag { severity: Severity::Warning, message: msg.into(), edges: Vec::new() });
    }

    fn tree(&self) -> &Tree {
        self.ctx.tree
    }

    fn output(&self, c: NodeIdx) -> Option<&Rc<NodeOutput<K::Shape>>> {
        self.ctx.outputs[c as usize].as_ref()
    }

    /// Parts of a child in this node's system.
    fn contrib(&mut self, c: NodeIdx) -> R<Vec<Part<K::Shape>>> {
        let Some(o) = self.output(c).cloned() else { return Ok(Vec::new()) };
        let m = self.tree().node(c).local;
        if is_identity(&m) {
            return Ok(o.parts.clone());
        }
        let mut out = Vec::new();
        for p in &o.parts {
            match self.run.call(|k| k.transform(&p.shape, &m))? {
                Ok(s) => out.push(Part { shape: s, solid: p.solid }),
                Err(e) => self.error(format!("Transformation gescheitert: {e}")),
            }
        }
        Ok(out)
    }

    /// Contributing 3D children (2D children are reported and skipped).
    fn child_list(&mut self) -> Vec<NodeIdx> {
        let tree = self.ctx.tree;
        let mut v = Vec::new();
        let mut skipped_2d = false;
        for c in tree.solid_children(self.idx) {
            if tree.dim(c) == Some(Dim::Two) {
                skipped_2d = true;
                continue;
            }
            v.push(c);
        }
        if skipped_2d {
            self.warn("2D-Form in 3D-Kontext wird ignoriert");
        }
        v
    }

    fn all_children_parts(&mut self) -> R<Vec<Part<K::Shape>>> {
        let mut parts = Vec::new();
        for c in self.child_list() {
            parts.extend(self.contrib(c)?);
        }
        Ok(parts)
    }

    fn check_solid(&mut self, parts: &[Part<K::Shape>]) -> bool {
        if parts.len() > 1 && parts.iter().any(|p| !p.solid) {
            self.error("Offene Fläche kann nicht in einem Boolean verwendet werden (zuerst thicken)");
            return false;
        }
        true
    }

    /// Union of parts (no kernel call for 0 or 1 part). On failure the inputs are returned.
    fn union(&mut self, parts: Vec<Part<K::Shape>>, tag: Tag) -> R<Vec<Part<K::Shape>>> {
        if parts.len() <= 1 || !self.check_solid(&parts) {
            return Ok(parts);
        }
        let shapes: Vec<_> = parts.iter().map(|p| p.shape.clone()).collect();
        match self.run.call(|k| k.boolean(BoolOp::Union, &shapes, tag))? {
            Ok(s) => Ok(vec![Part { shape: s, solid: true }]),
            Err(e) => {
                self.error(format!("Vereinigung gescheitert: {e}"));
                Ok(parts)
            }
        }
    }

    /// 3D boolean of groups of parts (one group per operand). Failure → inputs (difference:
    /// the first operand).
    fn boolean(&mut self, op: BoolOp, groups: Vec<Vec<Part<K::Shape>>>, tag: Tag) -> R<Vec<Part<K::Shape>>> {
        match op {
            BoolOp::Union => {
                let all = groups.into_iter().flatten().collect();
                self.union(all, tag)
            }
            BoolOp::Difference => {
                let mut it = groups.into_iter();
                let base = it.next().unwrap_or_default();
                let subs: Vec<_> = it.flatten().collect();
                if base.is_empty() || subs.is_empty() {
                    return Ok(base);
                }
                let mut all = base.clone();
                all.extend(subs.iter().cloned());
                if !self.check_solid(&all) {
                    return Ok(base);
                }
                let b = self.union(base.clone(), tag)?;
                if b.len() != 1 {
                    return Ok(base);
                }
                let mut shapes = vec![b[0].shape.clone()];
                shapes.extend(subs.iter().map(|p| p.shape.clone()));
                match self.run.call(|k| k.boolean(BoolOp::Difference, &shapes, tag))? {
                    Ok(s) => Ok(vec![Part { shape: s, solid: true }]),
                    Err(e) => {
                        self.error(format!("Differenz gescheitert: {e}"));
                        Ok(base)
                    }
                }
            }
            BoolOp::Intersection => {
                if groups.iter().any(|g| g.is_empty()) {
                    return Ok(Vec::new());
                }
                if groups.len() == 1 {
                    return Ok(groups.into_iter().next().unwrap());
                }
                let all: Vec<_> = groups.iter().flatten().cloned().collect();
                if !self.check_solid(&all) {
                    return Ok(all);
                }
                let mut shapes = Vec::new();
                for g in groups {
                    let u = self.union(g, tag)?;
                    if u.len() != 1 {
                        return Ok(all);
                    }
                    shapes.push(u[0].shape.clone());
                }
                match self.run.call(|k| k.boolean(BoolOp::Intersection, &shapes, tag))? {
                    Ok(s) => Ok(vec![Part { shape: s, solid: true }]),
                    Err(e) => {
                        self.error(format!("Schnittmenge gescheitert: {e}"));
                        Ok(all)
                    }
                }
            }
        }
    }

    /// Fillet or chamfer `edges` of `part`; failure → part unchanged + error.
    fn blend_part(
        &mut self,
        part: &Part<K::Shape>,
        kind: BlendKind,
        r: f64,
        sel: &[&EdgeInfo],
        tag: Tag,
    ) -> R<Part<K::Shape>> {
        if self.ctx.skip.get(self.idx as usize).copied().unwrap_or(false) {
            if !self.suspended {
                self.suspended = true;
                self.hints.push(SUSPENDED_HINT.into());
            }
            return Ok(part.clone());
        }
        let ids: Vec<u32> = sel.iter().map(|e| e.id).collect();
        let (res, ms) = self.run.call_timed(|k| match kind {
            BlendKind::Fillet => k.fillet(&part.shape, &ids, r, tag),
            BlendKind::Chamfer => k.chamfer(&part.shape, &ids, r, tag),
        })?;
        self.blend_ms += ms;
        match res {
            Ok(s) => Ok(Part { shape: s, solid: part.solid }),
            Err(e) => {
                let what = match kind {
                    BlendKind::Fillet => "Verrundung",
                    BlendKind::Chamfer => "Fase",
                };
                let bad: Vec<Vec<DVec3>> = match &e {
                    KernelError::Failed { edges, .. } if !edges.is_empty() => {
                        sel.iter().filter(|x| edges.contains(&x.id)).map(|x| x.points.clone()).collect()
                    }
                    _ => sel.iter().map(|x| x.points.clone()).collect(),
                };
                self.error_edges(format!("{what} gescheitert ({r} mm): {e}"), bad);
                Ok(part.clone())
            }
        }
    }

    /// Variable-radius fillet of `edges` of `part` (in the system `to_law` maps to the law's,
    /// the system of the fillet statement): the law is evaluated at the kernel's stations
    /// along each edge. `range` collects the smallest and largest radius (hint `r 2–4`).
    /// Radii ≤ 0, values that are no number, and kernel failures → part unchanged + error.
    fn blend_part_law(
        &mut self,
        part: &Part<K::Shape>,
        law: &Law,
        to_law: &DMat4,
        sel: &[&EdgeInfo],
        tag: Tag,
        range: &mut Option<(f64, f64)>,
    ) -> R<Part<K::Shape>> {
        let ids: Vec<u32> = sel.iter().map(|e| e.id).collect();
        let counts: Vec<u32> = sel.iter().map(|e| law_station_count(&e.points)).collect();
        let stations = match self.run.call(|k| k.law_stations(&part.shape, &ids, &counts))? {
            Ok(s) => s,
            Err(e) => {
                let edges = sel.iter().map(|x| x.points.clone()).collect();
                self.error_edges(format!("Verrundung gescheitert (variabler Radius): {e}"), edges);
                return Ok(part.clone());
            }
        };
        let mut radii = Vec::with_capacity(stations.len());
        let mut bad: Option<(String, usize)> = None;
        for (i, st) in stations.iter().enumerate() {
            let mut rs = Vec::with_capacity(st.len());
            for q in st {
                let p = to_law.transform_point3(*q);
                match law.eval(p) {
                    Ok(r) if r.is_finite() && r > 0.0 => {
                        *range = Some(range.map_or((r, r), |(lo, hi)| (lo.min(r), hi.max(r))));
                        rs.push(r);
                    }
                    Ok(r) => {
                        bad.get_or_insert((format!("Radius {} ≤ 0 bei {}", fmt_num(r), cad_ir::law::fmt_point(p)), i));
                    }
                    Err(msg) => {
                        bad.get_or_insert((msg, i));
                    }
                }
            }
            radii.push(rs);
        }
        if let Some((msg, i)) = bad {
            self.error_edges(format!("Verrundung gescheitert (variabler Radius): {msg}"), vec![sel[i].points.clone()]);
            return Ok(part.clone());
        }
        if self.ctx.skip.get(self.idx as usize).copied().unwrap_or(false) {
            if !self.suspended {
                self.suspended = true;
                self.hints.push(SUSPENDED_HINT.into());
            }
            return Ok(part.clone());
        }
        let (lo, hi) =
            radii.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &r| (lo.min(r), hi.max(r)));
        // one radius everywhere: the constant fillet (same result as `fillet(r)`)
        let constant = hi - lo <= 1e-12 * hi;
        let (res, ms) = self.run.call_timed(|k| {
            if constant {
                k.fillet(&part.shape, &ids, lo, tag)
            } else {
                k.fillet_variable(&part.shape, &ids, &radii, tag)
            }
        })?;
        self.blend_ms += ms;
        match res {
            Ok(s) => Ok(Part { shape: s, solid: part.solid }),
            Err(e) => {
                let bad: Vec<Vec<DVec3>> = match &e {
                    KernelError::Failed { edges, .. } if !edges.is_empty() => {
                        sel.iter().filter(|x| edges.contains(&x.id)).map(|x| x.points.clone()).collect()
                    }
                    _ => sel.iter().map(|x| x.points.clone()).collect(),
                };
                self.error_edges(format!("Verrundung gescheitert ({} mm): {e}", radius_range(lo, hi)), bad);
                Ok(part.clone())
            }
        }
    }

    fn push_radius_hint(&mut self, range: Option<(f64, f64)>) {
        if let Some((lo, hi)) = range {
            self.hints.push(format!("r {}", radius_range(lo, hi)));
        }
    }

    fn eval_op(&mut self) -> R<Vec<Part<K::Shape>>> {
        let tree = self.ctx.tree;
        let node = tree.node(self.idx);
        let tag = self.tag;
        match &node.op {
            Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty => {
                let parts = self.all_children_parts()?;
                self.union(parts, tag)
            }
            Op::Prim { prim, place } => self.prim(prim, place, node.blend),
            Op::Profile(_) => Ok(Vec::new()), // handled as 2D before
            Op::Boolean { op, fillet, chamfer, fillet_law } => {
                let kids = self.child_list();
                let mut groups = Vec::new();
                for &c in &kids {
                    groups.push(self.contrib(c)?);
                }
                let n_in = groups.iter().filter(|g| !g.is_empty()).count();
                let inputs: Vec<Part<K::Shape>> = if fillet.is_some() || chamfer.is_some() || fillet_law.is_some() {
                    groups.iter().flatten().cloned().collect()
                } else {
                    Vec::new()
                };
                let diags_before = self.diags.len();
                let parts = self.boolean(*op, groups, tag)?;
                let failed = self.diags[diags_before..].iter().any(|d| d.severity == Severity::Error);
                let blend = match (fillet, chamfer) {
                    _ if fillet_law.is_some() => Some((BlendKind::Fillet, 0.0)),
                    (Some(r), _) if *r > 0.0 => Some((BlendKind::Fillet, *r)),
                    (_, Some(c)) if *c > 0.0 => Some((BlendKind::Chamfer, *c)),
                    _ => None,
                };
                match blend {
                    Some((kind, r)) if !failed && parts.len() == 1 && n_in >= 2 => {
                        self.new_edge_blend(&kids, &inputs, parts, kind, r, fillet_law.as_deref())
                    }
                    Some(_) if !failed => {
                        self.warn("0 Kanten: die Operation erzeugt keine neuen Kanten");
                        self.hints.push("0 Kanten".into());
                        Ok(parts)
                    }
                    _ => Ok(parts),
                }
            }
            Op::Blend { kind, r, select, law } => self.zone_block(Some((*kind, *r)), law.as_deref(), None, *select),
            Op::Shell { t, select } => self.zone_block(None, None, Some(*t), *select),
            Op::Zone(z) => self.zone(z),
            Op::LinearExtrude { h, twist, scale, center } => {
                if *h <= 0.0 {
                    self.error(format!("linear_extrude: Höhe {h} ≤ 0"));
                    return Ok(Vec::new());
                }
                let z0 = if *center { -h / 2.0 } else { 0.0 };
                // OpenSCAD: positive twist turns clockwise seen from above; the kernel's twist is
                // counter-clockwise.
                let mode = LiftMode::Extrude { h: *h, twist: -twist.to_radians(), scale: *scale, z0 };
                self.lifted(&mode)
            }
            Op::RotateExtrude { angle } => {
                let a = if *angle == 0.0 { 360.0 } else { *angle };
                self.lifted(&LiftMode::Revolve { angle: a.to_radians() })
            }
            Op::Sweep { path, .. } if path.is_empty() => self.sweep_sketch_path(),
            Op::Sweep { path, interp } => {
                if path.len() < 2 {
                    self.error("sweep: Pfad braucht mindestens 2 Punkte");
                    return Ok(Vec::new());
                }
                self.lifted(&LiftMode::Sweep { path: path.clone(), interp: *interp })
            }
            Op::Loft { ruled } => self.loft(*ruled),
            Op::Surface { rows } => match self.run.call(|k| k.nurbs_surface(rows, tag))? {
                Ok(s) => Ok(vec![Part { shape: s, solid: false }]),
                Err(e) => {
                    self.error(format!("surface gescheitert: {e}"));
                    Ok(Vec::new())
                }
            },
            Op::Thicken { t } => {
                let parts = self.all_children_parts()?;
                let mut out = Vec::new();
                for p in parts {
                    if p.solid {
                        self.error("thicken erwartet eine offene Fläche");
                        out.push(p);
                        continue;
                    }
                    match self.run.call(|k| k.thicken(&p.shape, *t, tag))? {
                        Ok(s) => out.push(Part { shape: s, solid: true }),
                        Err(e) => {
                            self.error(format!("thicken gescheitert: {e}"));
                            out.push(p);
                        }
                    }
                }
                Ok(out)
            }
            Op::Resize { size, auto } => {
                let kids = self.child_list();
                let mut b = None;
                for &c in &kids {
                    if let Some(cb) = self.output(c).and_then(|o| o.bounds) {
                        b = merge_box(b, Some(transform_box(cb, &tree.node(c).local)));
                    }
                }
                let parts = self.all_children_parts()?;
                let parts = self.union(parts, tag)?;
                let Some((lo, hi)) = b else { return Ok(parts) };
                let s = profile::resize_scale(hi - lo, *size, *auto);
                let m = DMat4::from_scale(s);
                if is_identity(&m) {
                    return Ok(parts);
                }
                let mut out = Vec::new();
                for p in parts {
                    match self.run.call(|k| k.transform(&p.shape, &m))? {
                        Ok(sh) => out.push(Part { shape: sh, solid: p.solid }),
                        Err(e) => {
                            self.error(format!("resize gescheitert: {e}"));
                            out.push(p);
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    fn prim(&mut self, prim: &Prim, place: &DMat4, blend: Option<(BlendKind, f64)>) -> R<Vec<Part<K::Shape>>> {
        let tag = self.tag;
        let res = self.run.call(|k| match prim {
            Prim::Box { min, max } => k.make_box(*min, *max, tag),
            Prim::Sphere { r } => k.make_sphere(*r, tag),
            Prim::Cylinder { h, r1, r2 } => k.make_cylinder(*h, *r1, *r2, tag),
            Prim::Torus { big_r, r } => k.make_torus(*big_r, *r, tag),
            Prim::Polyhedron { points, faces } => k.make_polyhedron(points, faces, tag),
        })?;
        let mut part = match res {
            Ok(s) => Part { shape: s, solid: true },
            Err(e) => {
                self.error(format!("{} gescheitert: {e}", self.tree().node(self.idx).op.label()));
                return Ok(Vec::new());
            }
        };
        if let Some((kind, r)) = blend
            && r > 0.0
        {
            let samples = self.ctx.config.edge_samples;
            match self.run.call(|k| k.edges(&part.shape, samples))? {
                Ok(edges) => {
                    // seams, poles and tangent edges are not edges to blend
                    let sel: Vec<&EdgeInfo> = edges.iter().filter(|e| !e.smooth).collect();
                    self.hints.push(plural(sel.len(), "Kante", "Kanten"));
                    if sel.is_empty() {
                        self.warn("0 Kanten: das Primitiv hat keine Kanten zum Verrunden");
                    } else {
                        part = self.blend_part(&part, kind, r, &sel, tag)?;
                        let pl: Vec<_> = sel.iter().map(|e| e.points.clone()).collect();
                        self.selected
                            .extend(pl.into_iter().map(|p| p.iter().map(|q| place.transform_point3(*q)).collect()));
                    }
                }
                Err(e) => self.error(format!("Kantenabfrage gescheitert: {e}")),
            }
        }
        if !is_identity(place) {
            match self.run.call(|k| k.transform(&part.shape, place))? {
                Ok(s) => part.shape = s,
                Err(e) => self.error(format!("Platzierung gescheitert: {e}")),
            }
        }
        Ok(vec![part])
    }

    /// `difference(fillet = r)` etc.: blend the edges whose two faces come from different
    /// children (no child carries both tags).
    fn new_edge_blend(
        &mut self,
        kids: &[NodeIdx],
        inputs: &[Part<K::Shape>],
        parts: Vec<Part<K::Shape>>,
        kind: BlendKind,
        r: f64,
        law: Option<&Law>,
    ) -> R<Vec<Part<K::Shape>>> {
        let sets: Vec<Rc<[Tag]>> = kids.iter().filter_map(|&c| self.output(c).map(|o| o.tags.clone())).collect();
        let part = &parts[0];
        let samples = self.ctx.config.edge_samples;
        let edges = match self.run.call(|k| k.edges(&part.shape, samples))? {
            Ok(e) => e,
            Err(e) => {
                self.error(format!("Kantenabfrage gescheitert: {e}"));
                return Ok(parts);
            }
        };
        let known = |t: Tag| sets.iter().any(|s| s.binary_search(&t).is_ok());
        let cand: Vec<&EdgeInfo> = edges
            .iter()
            .filter(|e| {
                if e.smooth || e.face_tags.len() != 2 {
                    return false;
                }
                let (a, b) = (e.face_tags[0], e.face_tags[1]);
                a != b
                    && known(a)
                    && known(b)
                    && !sets.iter().any(|s| s.binary_search(&a).is_ok() && s.binary_search(&b).is_ok())
            })
            .collect();
        // An edge of an operand stays an old edge even where coplanar faces of two operands
        // merged into one face with one operand's tag (union(chamfer) of a slab and a tower
        // sharing their front plane: the tower's front edges then separate faces of both).
        let mut old: Vec<Vec<DVec3>> = Vec::new();
        if !cand.is_empty() {
            for p in inputs {
                match self.run.call(|k| k.edges(&p.shape, samples))? {
                    Ok(es) => old.extend(es.into_iter().map(|e| e.points)),
                    Err(e) => {
                        self.error(format!("Kantenabfrage gescheitert: {e}"));
                        return Ok(parts);
                    }
                }
            }
        }
        let sel: Vec<&EdgeInfo> = cand.into_iter().filter(|e| !on_polylines(&e.points, &old)).collect();
        self.hints.push(plural(sel.len(), "Kante", "Kanten"));
        if sel.is_empty() {
            self.warn("0 Kanten: keine neuen Kanten gefunden");
            return Ok(parts);
        }
        self.selected.extend(sel.iter().map(|e| e.points.clone()));
        let p = match law {
            Some(law) => {
                let mut range = None;
                let p = self.blend_part_law(part, law, &DMat4::IDENTITY, &sel, self.tag, &mut range)?;
                self.push_radius_hint(range);
                p
            }
            None => self.blend_part(part, kind, r, &sel, self.tag)?,
        };
        Ok(vec![p])
    }

    /// Zone points of `pts` (body system) against all zones: per point, inside/on any zone.
    fn zone_hits(&mut self, zones: &[(NodeIdx, DMat4)], pts: &[DVec3]) -> R<Vec<bool>> {
        let mut hit = vec![false; pts.len()];
        for (z, to_zone) in zones {
            let Some(o) = self.output(*z).cloned() else { continue };
            let local: Vec<DVec3> = pts.iter().map(|p| to_zone.transform_point3(*p)).collect();
            for zp in &o.parts {
                match self.run.call(|k| k.classify(&zp.shape, &local))? {
                    Ok(cls) => {
                        for (h, c) in hit.iter_mut().zip(cls) {
                            *h |= matches!(c, Class::Inside | Class::On);
                        }
                    }
                    Err(e) => self.error(format!("Klassifizierung gegen Zone gescheitert: {e}")),
                }
            }
        }
        Ok(hit)
    }

    /// `fillet(r)/chamfer(r) { body; zones… }` (blend) or `shell(t) { … }` (shell).
    fn zone_block(
        &mut self,
        blend: Option<(BlendKind, f64)>,
        law: Option<&Law>,
        shell: Option<f64>,
        select: Select,
    ) -> R<Vec<Part<K::Shape>>> {
        let tree = self.ctx.tree;
        let node = tree.node(self.idx);
        let Some(&body) = node.children.first() else {
            self.error("Zonenblock ohne Körper");
            return Ok(Vec::new());
        };
        if tree.node(body).flags.disabled {
            self.error("Körper des Zonenblocks ist deaktiviert");
            return Ok(Vec::new());
        }
        let body_local = tree.node(body).local;
        let body_parts = self.output(body).map(|o| o.parts.clone()).unwrap_or_default();
        let zones: Vec<(NodeIdx, DMat4)> = node.children[1..]
            .iter()
            .filter(|&&z| !tree.node(z).flags.disabled)
            .map(|&z| (z, tree.node(z).local.inverse() * body_local))
            .collect();
        if !zones.is_empty() {
            self.hints.push(plural(zones.len(), "Zone", "Zonen"));
        }
        let tag = self.tag;
        let mut out = Vec::new();
        let mut count = 0usize;
        let mut range: Option<(f64, f64)> = None;
        for part in &body_parts {
            if let Some((kind, r)) = blend {
                let samples = self.ctx.config.edge_samples;
                let edges = match self.run.call(|k| k.edges(&part.shape, samples))? {
                    // seams, poles and tangent edges are never selected
                    Ok(e) => e.into_iter().filter(|e| !e.smooth).collect::<Vec<_>>(),
                    Err(e) => {
                        self.error(format!("Kantenabfrage gescheitert: {e}"));
                        out.push(part.clone());
                        continue;
                    }
                };
                let sel: Vec<&EdgeInfo> = if zones.is_empty() {
                    edges.iter().collect()
                } else {
                    let pts: Vec<DVec3> = edges.iter().flat_map(|e| e.points.iter().copied()).collect();
                    let hit = self.zone_hits(&zones, &pts)?;
                    let mut i = 0;
                    let mut sel = Vec::new();
                    for e in &edges {
                        let h = &hit[i..i + e.points.len()];
                        i += e.points.len();
                        let take = match select {
                            Select::Inside => h.iter().all(|&x| x),
                            Select::Touch => h.iter().any(|&x| x),
                        };
                        if take {
                            sel.push(e);
                        }
                    }
                    sel
                };
                count += sel.len();
                self.selected
                    .extend(sel.iter().map(|e| e.points.iter().map(|p| body_local.transform_point3(*p)).collect()));
                if sel.is_empty() {
                    out.push(part.clone());
                } else if let Some(law) = law {
                    let p = self.blend_part_law(part, law, &body_local, &sel, tag, &mut range)?;
                    out.push(p);
                } else {
                    let p = self.blend_part(part, kind, r, &sel, tag)?;
                    out.push(p);
                }
            } else if let Some(t) = shell {
                let samples = self.ctx.config.face_samples;
                let faces: Vec<FaceInfo> = if zones.is_empty() {
                    Vec::new()
                } else {
                    match self.run.call(|k| k.faces(&part.shape, samples))? {
                        Ok(f) => f,
                        Err(e) => {
                            self.error(format!("Flächenabfrage gescheitert: {e}"));
                            out.push(part.clone());
                            continue;
                        }
                    }
                };
                let mut open = Vec::new();
                if !faces.is_empty() {
                    let pts: Vec<DVec3> = faces.iter().flat_map(|f| f.points.iter().copied()).collect();
                    let hit = self.zone_hits(&zones, &pts)?;
                    let mut i = 0;
                    for f in &faces {
                        let h = &hit[i..i + f.points.len()];
                        i += f.points.len();
                        let take = match select {
                            Select::Inside => !h.is_empty() && h.iter().all(|&x| x),
                            Select::Touch => h.iter().any(|&x| x),
                        };
                        if take {
                            open.push(f.id);
                            self.opened.push(f.points.iter().map(|p| body_local.transform_point3(*p)).collect());
                        }
                    }
                }
                count += open.len();
                match self.run.call(|k| k.shell(&part.shape, t, &open, tag))? {
                    Ok(s) => out.push(Part { shape: s, solid: part.solid }),
                    Err(e) => {
                        self.error(format!("shell gescheitert ({t} mm): {e}"));
                        out.push(part.clone());
                    }
                }
            }
        }
        if blend.is_some() {
            self.hints.push(plural(count, "Kante", "Kanten"));
            if count == 0 && !body_parts.is_empty() {
                self.warn("0 Kanten");
            }
            if law.is_some() {
                self.push_radius_hint(range);
            }
        } else {
            self.hints.push(plural(count, "Fläche geöffnet", "Flächen geöffnet"));
            if count == 0 && !zones.is_empty() && !body_parts.is_empty() {
                self.warn("0 Flächen");
            }
        }
        // Into this node's system.
        if is_identity(&body_local) {
            return Ok(out);
        }
        let mut res = Vec::new();
        for p in out {
            match self.run.call(|k| k.transform(&p.shape, &body_local))? {
                Ok(s) => res.push(Part { shape: s, solid: p.solid }),
                Err(e) => self.error(format!("Transformation gescheitert: {e}")),
            }
        }
        Ok(res)
    }

    /// `zone(line, d)` / `zone(plane, d)`, limited to the body's bounds plus `d`.
    fn zone(&mut self, z: &ZoneShape) -> R<Vec<Part<K::Shape>>> {
        let tree = self.ctx.tree;
        let Some((_, body, path)) = zone_block(tree, self.idx) else {
            self.warn("zone() außerhalb eines fillet/chamfer/shell-Blocks wird ignoriert");
            return Ok(Vec::new());
        };
        let Some(bb) = self.output(body).and_then(|o| o.bounds) else {
            self.warn("Zone: Körper ist leer");
            return Ok(Vec::new());
        };
        let rel = path.inverse() * tree.node(body).local;
        let (lo, hi) = transform_box(bb, &rel);
        let tag = self.tag;
        match z {
            ZoneShape::Line { point, dir, d } => {
                if *d <= 0.0 || dir.length() < 1e-12 {
                    self.error("zone: Durchmesser und Richtung müssen > 0 sein");
                    return Ok(Vec::new());
                }
                let dir = dir.normalize();
                let (lo, hi) = (lo - DVec3::splat(*d), hi + DVec3::splat(*d));
                let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
                for i in 0..3 {
                    if dir[i].abs() < 1e-12 {
                        if point[i] < lo[i] || point[i] > hi[i] {
                            t1 = t0 - 1.0;
                        }
                    } else {
                        let a = (lo[i] - point[i]) / dir[i];
                        let b = (hi[i] - point[i]) / dir[i];
                        t0 = t0.max(a.min(b));
                        t1 = t1.min(a.max(b));
                    }
                }
                if t1 - t0 <= 1e-9 {
                    self.warn("Zone trifft den Körper nicht");
                    return Ok(Vec::new());
                }
                let m = DMat4::from_translation(*point + dir * t0)
                    * DMat4::from_quat(DQuat::from_rotation_arc(DVec3::Z, dir));
                let s = match self.run.call(|k| k.make_cylinder(t1 - t0, d / 2.0, d / 2.0, tag))? {
                    Ok(s) => s,
                    Err(e) => {
                        self.error(format!("Zone gescheitert: {e}"));
                        return Ok(Vec::new());
                    }
                };
                match self.run.call(|k| k.transform(&s, &m))? {
                    Ok(s) => Ok(vec![Part { shape: s, solid: true }]),
                    Err(e) => {
                        self.error(format!("Zone gescheitert: {e}"));
                        Ok(Vec::new())
                    }
                }
            }
            ZoneShape::Plane { point, normal, d } => {
                if *d <= 0.0 || normal.length() < 1e-12 {
                    self.error("zone: Dicke und Normale müssen > 0 sein");
                    return Ok(Vec::new());
                }
                let n = normal.normalize();
                let u = n.any_orthonormal_vector();
                let v = n.cross(u);
                let (lo, hi) = (lo - DVec3::splat(*d), hi + DVec3::splat(*d));
                let mut ulo = DVec3::splat(f64::INFINITY);
                let mut uhi = DVec3::splat(f64::NEG_INFINITY);
                for i in 0..8 {
                    let c = DVec3::new(
                        if i & 1 == 0 { lo.x } else { hi.x },
                        if i & 2 == 0 { lo.y } else { hi.y },
                        if i & 4 == 0 { lo.z } else { hi.z },
                    ) - *point;
                    let q = DVec3::new(c.dot(u), c.dot(v), c.dot(n));
                    ulo = ulo.min(q);
                    uhi = uhi.max(q);
                }
                let h = d / 2.0;
                if uhi.z < -h || ulo.z > h {
                    self.warn("Zone trifft den Körper nicht");
                    return Ok(Vec::new());
                }
                let frame = DMat4::from_cols(u.extend(0.0), v.extend(0.0), n.extend(0.0), point.extend(1.0));
                let (bmin, bmax) = (DVec3::new(ulo.x, ulo.y, -h), DVec3::new(uhi.x, uhi.y, h));
                let s = match self.run.call(|k| k.make_box(bmin, bmax, tag))? {
                    Ok(s) => s,
                    Err(e) => {
                        self.error(format!("Zone gescheitert: {e}"));
                        return Ok(Vec::new());
                    }
                };
                match self.run.call(|k| k.transform(&s, &frame))? {
                    Ok(s) => Ok(vec![Part { shape: s, solid: true }]),
                    Err(e) => {
                        self.error(format!("Zone gescheitert: {e}"));
                        Ok(Vec::new())
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ profile operations

    /// Lifted 2D children of this node (their union), or `None` after reporting errors.
    fn lift_children(&mut self) -> Option<Expr2> {
        let tree = self.ctx.tree;
        let mut args = Vec::new();
        let mut warn = Vec::new();
        for c in tree.solid_children(self.idx) {
            if tree.dim(c) == Some(Dim::Three) {
                self.error(format!("{} erwartet 2D-Profile als Kinder", tree.node(self.idx).op.label()));
                return None;
            }
            if profile::open_sketch_in(tree, c) {
                self.error("offene Skizze ist kein Profil (nur als `sweep`-Pfad)");
                return None;
            }
            match profile::lift(tree, c, tree.node(c).local, &mut warn) {
                Ok(e) => args.push(e),
                Err(msg) => {
                    self.error(msg);
                    return None;
                }
            }
        }
        for w in warn {
            self.warn(w);
        }
        if args.is_empty() {
            self.warn("kein Profil");
            return None;
        }
        Some(Expr2::Op(BoolOp::Union, args))
    }

    fn lifted(&mut self, mode: &LiftMode) -> R<Vec<Part<K::Shape>>> {
        let Some(expr) = self.lift_children() else { return Ok(Vec::new()) };
        self.lift_expr(&expr, mode)
    }

    fn lift_expr(&mut self, e: &Expr2, mode: &LiftMode) -> R<Vec<Part<K::Shape>>> {
        match e {
            Expr2::Leaf(l) => self.leaf_solid(l, mode),
            Expr2::Op(op, args) => {
                let mut groups = Vec::new();
                for a in args {
                    groups.push(self.lift_expr(a, mode)?);
                }
                self.boolean(*op, groups, self.tag)
            }
        }
    }

    /// Maps a leaf's loops into the operation's system (see `LiftMode`).
    fn leaf_loops(&mut self, l: &Leaf, a: &glam::DAffine2) -> Option<Vec<Loop>> {
        let loops = match profile::profile_loops(&l.profile) {
            Ok(v) => v,
            Err(msg) => {
                self.error(msg);
                return None;
            }
        };
        let mut approx = false;
        let out = loops
            .iter()
            .map(|lp| {
                let (m, ap) = profile::map_loop(lp, a);
                approx |= ap;
                m
            })
            .collect();
        if approx {
            self.warn("Kreis unter ungleichmäßiger Skalierung wird als Spline angenähert");
        }
        Some(out)
    }

    fn leaf_solid(&mut self, l: &Leaf, mode: &LiftMode) -> R<Vec<Part<K::Shape>>> {
        let tag = self.ctx.tags[l.node as usize];
        let planar = profile::is_planar(&l.m);
        let (frame, a) = match mode {
            LiftMode::Extrude { twist, scale, z0, .. } => {
                if planar {
                    (DMat4::from_translation(DVec3::new(0.0, 0.0, *z0)), profile::affine2(&l.m))
                } else {
                    if *twist != 0.0 || *scale != DVec2::ONE {
                        self.warn("Profil liegt nicht in der xy-Ebene: twist/scale wirken um seine eigene Normale");
                    } else {
                        self.warn("Profil liegt nicht in der xy-Ebene: Extrusion entlang seiner Normale");
                    }
                    let (f, a) = profile::split_frame(&l.m);
                    (DMat4::from_translation(DVec3::new(0.0, 0.0, *z0)) * f, a)
                }
            }
            LiftMode::Revolve { .. } => {
                if !planar {
                    self.warn("Profil liegt nicht in der xy-Ebene und wird projiziert");
                }
                // Profile xy → xz (x → x, y → z).
                let r = DMat4::from_cols(
                    DVec3::X.extend(0.0),
                    DVec3::Z.extend(0.0),
                    DVec3::NEG_Y.extend(0.0),
                    glam::DVec4::W,
                );
                (r, profile::affine2(&l.m))
            }
            LiftMode::Sweep { .. } | LiftMode::SweepPath { .. } => {
                if !planar {
                    self.warn("Profil liegt nicht in der xy-Ebene und wird projiziert");
                }
                (DMat4::IDENTITY, profile::affine2(&l.m))
            }
        };
        let Some(loops) = self.leaf_loops(l, &a) else { return Ok(Vec::new()) };
        let mut solids = Vec::new();
        for lp in &loops {
            let pl = PlacedLoop { lp: lp.clone(), frame };
            let r = self.run.call(|k| match mode {
                LiftMode::Extrude { h, twist, scale, .. } => k.extrude(&pl, *h, *twist, *scale, tag),
                LiftMode::Revolve { angle } => k.revolve(&pl, *angle, tag),
                LiftMode::Sweep { path, interp } => k.sweep(&pl.lp, path, *interp, tag),
                LiftMode::SweepPath { path } => k.sweep_path(&pl.lp, path, tag),
            })?;
            match r {
                Ok(s) => solids.push(Part { shape: s, solid: true }),
                Err(e) => {
                    let what = self.tree().node(self.idx).op.label();
                    self.error(format!("{what} gescheitert: {e}"));
                    if solids.is_empty() {
                        return Ok(Vec::new());
                    }
                }
            }
        }
        if solids.len() <= 1 {
            return Ok(solids);
        }
        let outer = vec![solids.remove(0)];
        let holes = solids.into_iter().map(|p| vec![p]).collect::<Vec<_>>();
        let mut groups = vec![outer];
        groups.extend(holes);
        self.boolean(BoolOp::Difference, groups, tag)
    }

    /// `sweep()` without `path`: one child is an open sketch (the path), the others are the
    /// profile.
    fn sweep_sketch_path(&mut self) -> R<Vec<Part<K::Shape>>> {
        let tree = self.ctx.tree;
        let kids: Vec<NodeIdx> = tree.solid_children(self.idx).collect();
        let paths: Vec<(NodeIdx, NodeIdx, DMat4)> = kids
            .iter()
            .filter_map(|&c| find_open_sketch(tree, c, tree.node(c).local).map(|(s, m)| (c, s, m)))
            .collect();
        let Some(&(path_child, sk, m)) = paths.first() else {
            self.error("sweep braucht `path` oder einen offenen Skizzenpfad `sketch(closed = false) { … }` als Kind");
            return Ok(Vec::new());
        };
        if paths.len() > 1 {
            self.warn("sweep: mehrere offene Skizzen, die erste ist der Pfad");
        }
        let Op::Profile(cad_ir::Profile::Sketch { contours, .. }) = &tree.node(sk).op else { return Ok(Vec::new()) };
        if contours.len() != 1 {
            self.error(format!("sweep: der Skizzenpfad braucht genau eine Kontur (hat {})", contours.len()));
            return Ok(Vec::new());
        }
        let segs = match profile::contour_segments(&contours[0]) {
            Ok(s) => s,
            Err(e) => {
                self.error(e);
                return Ok(Vec::new());
            }
        };
        let mut args = Vec::new();
        let mut warn = Vec::new();
        for &c in kids.iter().filter(|&&c| c != path_child) {
            if tree.dim(c) == Some(Dim::Three) {
                self.error("sweep erwartet 2D-Profile als Kinder");
                return Ok(Vec::new());
            }
            match profile::lift(tree, c, tree.node(c).local, &mut warn) {
                Ok(e) => args.push(e),
                Err(msg) => {
                    self.error(msg);
                    return Ok(Vec::new());
                }
            }
        }
        for w in warn {
            self.warn(w);
        }
        if args.is_empty() {
            self.warn("kein Profil");
            return Ok(Vec::new());
        }
        let mode = LiftMode::SweepPath { path: cad_kernel::PlacedPath { segs, frame: m } };
        self.lift_expr(&Expr2::Op(BoolOp::Union, args), &mode)
    }

    fn loft(&mut self, ruled: bool) -> R<Vec<Part<K::Shape>>> {
        let tree = self.ctx.tree;
        let mut sections = Vec::new();
        let mut warn = Vec::new();
        for c in tree.solid_children(self.idx) {
            if tree.dim(c) == Some(Dim::Three) {
                self.error("loft erwartet 2D-Profile als Kinder");
                return Ok(Vec::new());
            }
            if profile::open_sketch_in(tree, c) {
                self.error("offene Skizze ist kein Profil (nur als `sweep`-Pfad)");
                return Ok(Vec::new());
            }
            let e = match profile::lift(tree, c, tree.node(c).local, &mut warn) {
                Ok(e) => e,
                Err(msg) => {
                    self.error(msg);
                    return Ok(Vec::new());
                }
            };
            let leaves = e.leaves();
            let single = leaves.len() == 1 && single_union(&e);
            let loops = if single { profile::profile_loops(&leaves[0].profile).ok() } else { None };
            match (single, loops) {
                (true, Some(lps)) if lps.len() == 1 => {
                    let (frame, a) = profile::split_frame(&leaves[0].m);
                    let (lp, approx) = profile::map_loop(&lps[0], &a);
                    if approx {
                        warn.push("Kreis unter ungleichmäßiger Skalierung wird als Spline angenähert".into());
                    }
                    sections.push(PlacedLoop { lp: profile::loft_normalize(lp), frame });
                }
                _ => {
                    self.error(format!(
                        "loft: jedes Kind muss genau eine geschlossene Kontur sein (Kind {} ist es nicht)",
                        tree.node(c).op.label()
                    ));
                    return Ok(Vec::new());
                }
            }
        }
        for w in warn {
            self.warn(w);
        }
        if sections.len() < 2 {
            self.error("loft braucht mindestens 2 Profile");
            return Ok(Vec::new());
        }
        let tag = self.tag;
        match self.run.call(|k| k.loft(&sections, ruled, tag))? {
            Ok(s) => Ok(vec![Part { shape: s, solid: true }]),
            Err(e) => {
                self.error(format!("loft gescheitert: {e}"));
                Ok(Vec::new())
            }
        }
    }
}

/// `true` if the expression is a leaf wrapped only in single-argument unions.
fn single_union(e: &Expr2) -> bool {
    match e {
        Expr2::Leaf(_) => true,
        Expr2::Op(BoolOp::Union, a) if a.len() == 1 => single_union(&a[0]),
        _ => false,
    }
}

enum LiftMode {
    Extrude { h: f64, twist: f64, scale: DVec2, z0: f64 },
    Revolve { angle: f64 },
    Sweep { path: Vec<DVec3>, interp: cad_kernel::PathInterp },
    /// `sweep() { sketch(closed = false) { … } profil(); }`: path of exact segments.
    SweepPath { path: cad_kernel::PlacedPath },
}

/// The open sketch in the subtree of `idx` (through groups, module calls, transforms and
/// colours) with its matrix into the system `m` maps the subtree's own system to.
fn find_open_sketch(tree: &cad_ir::Tree, idx: NodeIdx, m: DMat4) -> Option<(NodeIdx, DMat4)> {
    let n = tree.node(idx);
    match &n.op {
        Op::Profile(cad_ir::Profile::Sketch { closed: false, .. }) => Some((idx, m)),
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } => {
            let kids: Vec<NodeIdx> = tree.solid_children(idx).collect();
            if kids.len() != 1 {
                return None;
            }
            find_open_sketch(tree, kids[0], m * tree.node(kids[0]).local)
        }
        _ => None,
    }
}
