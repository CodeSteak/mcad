//! Conversion of evaluation and engine results into protocol types (byte spans → UTF-16).

use cad_engine::{Evaluation, Status};
use cad_eval::{EvalOutput, PValue};
use cad_ir::{NO_STMT, Op, Tree};
use cad_kernel::BoolOp;
use cad_syntax::{LineIndex, Span};
use glam::{DMat4, DVec3};

use crate::protocol as p;

/// Source text with its line index: converts byte spans to [`p::SrcRange`].
pub struct Source {
    pub text: String,
    pub lines: LineIndex,
}

impl Source {
    pub fn new(text: String) -> Self {
        let lines = LineIndex::new(&text);
        Source { text, lines }
    }

    fn clamp(&self, b: u32) -> u32 {
        let mut b = b.min(self.text.len() as u32);
        while !self.text.is_char_boundary(b as usize) {
            b -= 1;
        }
        b
    }

    /// 1-based (line, column in UTF-16 units) of a byte offset.
    pub fn pos(&self, byte: u32) -> (u32, u32) {
        let byte = self.clamp(byte);
        let line = self.lines.line_of(byte);
        let start = self.lines.line_start(line);
        let col = self.lines.byte_to_utf16(byte) - self.lines.byte_to_utf16(start);
        (line + 1, col + 1)
    }

    pub fn range(&self, s: Span) -> p::SrcRange {
        let (a, b) = (self.clamp(s.start), self.clamp(s.end.max(s.start)));
        let (line, col) = self.pos(a);
        let (end_line, end_col) = self.pos(b);
        p::SrcRange { from: self.lines.byte_to_utf16(a), to: self.lines.byte_to_utf16(b), line, col, end_line, end_col }
    }

    /// Byte offset of a 1-based line start.
    pub fn line_start(&self, line: u32) -> u32 {
        self.lines.line_start(line.saturating_sub(1))
    }

    pub fn edit(&self, e: &cad_eval::TextEdit) -> p::Edit {
        let r = self.range(e.span);
        p::Edit { from: r.from, to: r.to, text: e.text.clone() }
    }
}

pub fn v3(v: DVec3) -> p::Vec3 {
    p::Vec3(v.to_array())
}

pub fn m4(m: &DMat4) -> p::Mat4 {
    p::Mat4(m.to_cols_array())
}

pub fn value(v: &PValue) -> p::Value {
    match v {
        PValue::Undef => p::Value::Undef,
        PValue::Bool(b) => p::Value::Bool(*b),
        PValue::Num(n) => p::Value::Num(*n),
        PValue::Str(s) => p::Value::Str(s.clone()),
        PValue::List(l) => p::Value::List(l.iter().map(value).collect()),
        PValue::Range { start, step, end } => p::Value::Range { start: *start, step: *step, end: *end },
        PValue::Function => p::Value::Opaque { opaque: "function".into() },
        PValue::Other(s) => p::Value::Opaque { opaque: s.clone() },
    }
}

pub fn pvalue(v: &p::Value) -> PValue {
    match v {
        p::Value::Undef | p::Value::Opaque { .. } => PValue::Undef,
        p::Value::Bool(b) => PValue::Bool(*b),
        p::Value::Num(n) => PValue::Num(*n),
        p::Value::Str(s) => PValue::Str(s.clone()),
        p::Value::List(l) => PValue::List(l.iter().map(pvalue).collect()),
        p::Value::Range { start, step, end } => PValue::Range { start: *start, step: *step, end: *end },
    }
}

/// Source text of a value (for "Als Standard übernehmen").
pub fn value_text(v: &p::Value) -> String {
    match v {
        p::Value::Undef | p::Value::Opaque { .. } => "undef".into(),
        p::Value::Bool(b) => b.to_string(),
        p::Value::Num(n) => cad_eval::fmt_literal(*n, 0.0),
        p::Value::Str(s) => format!("{s:?}"),
        p::Value::List(l) => format!("[{}]", l.iter().map(value_text).collect::<Vec<_>>().join(", ")),
        p::Value::Range { start, step, end } => format!(
            "[{} : {} : {}]",
            cad_eval::fmt_literal(*start, 0.0),
            cad_eval::fmt_literal(*step, 0.0),
            cad_eval::fmt_literal(*end, 0.0)
        ),
    }
}

pub fn stmt(s: u32) -> Option<u32> {
    (s != NO_STMT).then_some(s)
}

pub fn severity(s: cad_syntax::Severity) -> p::Severity {
    match s {
        cad_syntax::Severity::Error => p::Severity::Error,
        cad_syntax::Severity::Warning => p::Severity::Warning,
        cad_syntax::Severity::Info => p::Severity::Info,
    }
}

pub fn engine_severity(s: cad_engine::Severity) -> p::Severity {
    match s {
        cad_engine::Severity::Error => p::Severity::Error,
        cad_engine::Severity::Warning => p::Severity::Warning,
    }
}

pub fn lang_diagnostics(src: &Source, diags: &[cad_syntax::Diagnostic], source: p::DiagSource) -> Vec<p::Diagnostic> {
    diags
        .iter()
        .map(|d| p::Diagnostic {
            severity: severity(d.severity),
            message: d.message.clone(),
            range: src.range(d.span),
            source,
            node: None,
        })
        .collect()
}

pub fn kernel_diagnostics<S>(src: &Source, ev: &Evaluation<S>) -> Vec<p::Diagnostic> {
    ev.diagnostics()
        .map(|d| p::Diagnostic {
            severity: engine_severity(d.severity),
            message: d.message.clone(),
            range: src.range(d.span),
            source: p::DiagSource::Kernel,
            node: Some(d.node),
        })
        .collect()
}

fn polylines(v: &[Vec<DVec3>]) -> Vec<Vec<p::Vec3>> {
    v.iter().map(|l| l.iter().map(|&q| v3(q)).collect()).collect()
}

fn dim(d: cad_ir::Dim) -> p::Dim {
    match d {
        cad_ir::Dim::Two => p::Dim::Two,
        cad_ir::Dim::Three => p::Dim::Three,
    }
}

/// Non-first solid child of a `difference`.
pub fn is_subtracted(tree: &Tree, n: u32) -> bool {
    let Some(par) = tree.node(n).parent else { return false };
    if !matches!(tree.node(par).op, Op::Boolean { op: BoolOp::Difference, .. }) {
        return false;
    }
    let f = tree.node(n).flags;
    if f.disabled || f.background || f.zone {
        return false;
    }
    tree.solid_children(par).next() != Some(n)
}

fn select(s: cad_ir::Select) -> p::SelectMode {
    match s {
        cad_ir::Select::Inside => p::SelectMode::Inside,
        cad_ir::Select::Touch => p::SelectMode::Touch,
    }
}

/// Parameters the view needs to draw zone blocks, profile operations and surfaces.
pub fn op_info(op: &Op) -> Option<p::OpInfo> {
    Some(match op {
        Op::Blend { kind, r, select: s } => {
            p::OpInfo::Blend { chamfer: *kind == cad_ir::BlendKind::Chamfer, r: *r, select: select(*s) }
        }
        Op::Shell { t, select: s } => p::OpInfo::Shell { t: *t, select: select(*s) },
        Op::Zone(z) => match *z {
            cad_ir::ZoneShape::Line { point, dir, d } => p::OpInfo::Zone { line: true, point: v3(point), dir: v3(dir), d },
            cad_ir::ZoneShape::Plane { point, normal, d } => {
                p::OpInfo::Zone { line: false, point: v3(point), dir: v3(normal), d }
            }
        },
        Op::LinearExtrude { h, twist, scale, center } => {
            p::OpInfo::LinearExtrude { h: *h, center: *center, twist: *twist, scale: scale.to_array() }
        }
        Op::RotateExtrude { angle } => p::OpInfo::RotateExtrude { angle: *angle },
        Op::Loft { ruled } => p::OpInfo::Loft { ruled: *ruled },
        Op::Sweep { path, interp } => p::OpInfo::Sweep {
            path: path.iter().map(|&q| v3(q)).collect(),
            spline: *interp == cad_kernel::PathInterp::Spline,
        },
        Op::Surface { rows } => {
            p::OpInfo::Surface { rows: rows.iter().map(|r| r.iter().map(|&q| v3(q)).collect()).collect() }
        }
        Op::Thicken { t } => p::OpInfo::Thicken { t: *t },
        _ => return None,
    })
}

/// Mesh key of a node: hex of its content key.
pub fn mesh_key(k: &cad_engine::CacheKey) -> String {
    k.to_hex()
}

/// Per-node data available from the engine (complete scene) or from the started job
/// (preliminary scene: keys only).
pub enum KernelInfo<'a, S> {
    Done(&'a Evaluation<S>),
    Pending { keys: &'a [cad_engine::CacheKey], tags: &'a [u32] },
}

pub struct SceneInput<'a, S> {
    pub version: u64,
    pub src: &'a Source,
    pub out: &'a EvalOutput,
    pub kernel: KernelInfo<'a, S>,
    pub stats: p::Stats,
}

pub fn scene<S>(i: SceneInput<'_, S>) -> p::Scene {
    let SceneInput { version, src, out, kernel, stats } = i;
    let tree = &out.tree;
    let r = |s: Span| src.range(s);
    let ro = |s: Option<Span>| s.map(|s| src.range(s));

    let root_inv = tree.root.map(|n| tree.node(n).world.inverse()).unwrap_or(DMat4::IDENTITY);
    let nodes = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let idx = i as u32;
            let (op_instance, label) = match &n.op {
                Op::ModuleCall { name, instance } => (instance.clone(), name.clone()),
                o => (None, o.label()),
            };
            let base = p::SceneNode {
                idx,
                parent: n.parent,
                children: n.children.clone(),
                stmt: stmt(n.stmt),
                range: r(n.span),
                label,
                instance: op_instance,
                flags: p::NodeFlags {
                    highlight: n.flags.highlight,
                    background: n.flags.background,
                    disabled: n.flags.disabled,
                    zone: n.flags.zone,
                },
                subtracted: is_subtracted(tree, idx),
                color: n.color.map(p::Rgba),
                world: m4(&n.world),
                display_world: m4(&(root_inv * n.world)),
                module_inst: n.module_inst,
                inst_key: n.inst_key,
                status: p::NodeStatus::Pending,
                messages: Vec::new(),
                hints: Vec::new(),
                selected_edges: Vec::new(),
                opened_faces: 0,
                blend_suspended: false,
                bounds: None,
                dim: tree.dim(idx).map(dim),
                mesh_key: None,
                tag: 0,
                cached: false,
                tol: n.tol,
                op_info: op_info(&n.op),
            };
            match &kernel {
                KernelInfo::Pending { keys, tags } => p::SceneNode {
                    mesh_key: keys.get(i).map(mesh_key),
                    tag: tags.get(i).copied().unwrap_or(0),
                    status: if n.flags.disabled { p::NodeStatus::Skipped } else { p::NodeStatus::Pending },
                    ..base
                },
                KernelInfo::Done(ev) => {
                    let rep = ev.report(idx);
                    let status = match rep.status {
                        Status::Ok => p::NodeStatus::Ok,
                        Status::Warning => p::NodeStatus::Warning,
                        Status::Error => p::NodeStatus::Error,
                        Status::Skipped => p::NodeStatus::Skipped,
                    };
                    p::SceneNode {
                        status,
                        messages: rep
                            .diagnostics
                            .iter()
                            .map(|d| p::NodeMessage {
                                severity: engine_severity(d.severity),
                                message: d.message.clone(),
                                edges: polylines(&d.edges),
                            })
                            .collect(),
                        hints: rep.hints.clone(),
                        selected_edges: polylines(&rep.selected_edges),
                        opened_faces: rep.opened_faces as u32,
                        blend_suspended: rep.blend_suspended,
                        bounds: rep.bounds.map(|(a, b)| p::Bounds { min: v3(a), max: v3(b) }),
                        dim: rep.dim.map(dim).or(base.dim),
                        mesh_key: (status != p::NodeStatus::Skipped).then(|| mesh_key(&rep.key)),
                        tag: rep.tag,
                        cached: rep.cached,
                        display_world: m4(&rep.display_world),
                        ..base
                    }
                }
            }
        })
        .collect();

    let tag_nodes = match &kernel {
        KernelInfo::Done(ev) => ev
            .tag_nodes
            .iter()
            .map(|(&tag, refs)| p::TagNodes { tag, nodes: refs.iter().map(|n| n.node).collect() })
            .collect(),
        KernelInfo::Pending { tags, .. } => {
            let mut m: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
            for (i, &t) in tags.iter().enumerate() {
                if !tree.nodes[i].flags.disabled {
                    m.entry(t).or_default().push(i as u32);
                }
            }
            m.into_iter().map(|(tag, nodes)| p::TagNodes { tag, nodes }).collect()
        }
    };

    let grids = out
        .grids
        .iter()
        .map(|g| p::Grid {
            id: g.id,
            name: g.name.clone(),
            stmt: g.stmt,
            range: r(g.span),
            name_range: r(g.name_span),
            module_inst: g.module_inst,
            in_context: g.in_context,
            frame: m4(&g.frame),
            frame_module: m4(&g.frame_module),
            planes: g
                .planes
                .iter()
                .map(|pl| p::Plane {
                    axis: pl.axis,
                    name: pl.name.clone(),
                    base: pl.base.clone(),
                    index: pl.index,
                    value: pl.value,
                    value_range: ro(pl.value_span),
                    literal: ro(pl.literal),
                    is_ref: pl.origin.is_ref(),
                    implicit: pl.implicit,
                })
                .collect(),
            visible: r(g.visible),
        })
        .collect();

    let mut anchors = Vec::new();
    let instances = out
        .instances
        .iter()
        .map(|inst| {
            // Anchors are in instance coordinates; `world` maps them into the context.
            for (k, a) in inst.anchors.iter().enumerate() {
                anchors.push(p::Anchor {
                    inst: inst.id,
                    index: k as u32,
                    name: a.name.clone(),
                    instance: inst.label.clone(),
                    text: inst.label.as_ref().map(|l| format!("{l}.{}", a.name)),
                    stmt: a.stmt,
                    range: r(a.span),
                    point: v3(inst.world.transform_point3(a.point)),
                    dir: a.dir.map(|d| v3(inst.world.transform_vector3(d).normalize_or_zero())),
                });
            }
            p::Instance {
                id: inst.id,
                module: inst.module.clone(),
                label: inst.label.clone(),
                call_stmt: inst.call_stmt,
                call_range: ro(inst.call_span),
                def_stmt: inst.def_stmt,
                def_range: ro(inst.def_span),
                node: inst.node,
                parent: inst.parent,
                matrix: m4(&inst.matrix),
                world: m4(&inst.world),
                anchors: inst
                    .anchors
                    .iter()
                    .map(|a| p::LocalAnchor {
                        name: a.name.clone(),
                        stmt: a.stmt,
                        range: r(a.span),
                        point: v3(a.point),
                        dir: a.dir.map(v3),
                    })
                    .collect(),
                visible: ro(inst.visible),
                depth: inst.depth,
            }
        })
        .collect();

    let handle = |h: &cad_eval::Handle| p::Handle {
            id: h.id,
            stmt: h.stmt,
            range: r(h.span),
            kind: match h.kind {
                cad_eval::HandleKind::Point => p::HandleKind::Point,
                cad_eval::HandleKind::Radius => p::HandleKind::Radius,
                cad_eval::HandleKind::Height => p::HandleKind::Height,
                cad_eval::HandleKind::Angle { axis } => p::HandleKind::Angle { axis },
                cad_eval::HandleKind::Plane { axis } => p::HandleKind::Plane { axis },
                cad_eval::HandleKind::Scale { axis } => p::HandleKind::Scale { axis },
            },
            index: h.index,
            frame: m4(&h.frame),
            pos: v3(h.pos),
            world_pos: v3(h.frame.transform_point3(h.pos)),
            axis: h.axis.map(v3),
            base: h.base.map(v3),
            value: h.value.clone(),
            comps: h.comps.iter().map(|c| c.map(|c| p::Comp { range: r(c.span), scale: c.scale })).collect(),
            has_ref: h.has_ref,
            expr_range: r(h.expr_span),
            def_range: ro(h.def_span),
            via_var: h.via_var,
            via_default: h.via_default,
            locked: h.locked,
            grids: h.grids.clone(),
            instances: h.instances.clone(),
            node: h.node,
            module_inst: h.module_inst,
    };
    let handles = out.handles.iter().map(handle).collect();
    let locked_handles = out.locked_handles.iter().map(handle).collect();

    let hints = out
        .hints
        .iter()
        .map(|h| p::Hint {
            stmt: h.stmt,
            range: r(h.span),
            text: h.text.clone(),
            value: value(&h.value),
            literal: ro(h.literal),
            in_context: h.in_context,
        })
        .collect();

    let modules = out
        .modules
        .iter()
        .map(|m| p::Module {
            name: m.name.clone(),
            stmt: m.stmt,
            range: r(m.span),
            name_range: r(m.name_span),
            body_range: r(m.body_span),
            path: m.path.clone(),
            params: m
                .params
                .iter()
                .map(|pa| p::Param {
                    name: pa.name.clone(),
                    range: r(pa.span),
                    default_text: pa.default_text.clone(),
                    default_range: ro(pa.default_span),
                    default_value: pa.default_value.as_ref().map(value),
                    ty: match pa.ty {
                        cad_eval::ParamType::Number => p::ParamType::Number,
                        cad_eval::ParamType::Bool => p::ParamType::Bool,
                        cad_eval::ParamType::Vector => p::ParamType::Vector,
                        cad_eval::ParamType::String => p::ParamType::String,
                        cad_eval::ParamType::List => p::ParamType::List,
                        cad_eval::ParamType::Unknown => p::ParamType::Unknown,
                    },
                })
                .collect(),
        })
        .collect();

    let sel = |s: &cad_eval::value::Sel| match *s {
        cad_eval::value::Sel::Plane(plane) => p::Sel::Plane { plane },
        cad_eval::value::Sel::Wild => p::Sel::Wild,
        cad_eval::value::Sel::Coord(value) => p::Sel::Coord { value },
    };
    let refs = out
        .refs
        .iter()
        .map(|rf| p::Ref {
            range: r(rf.span),
            stmt: rf.stmt,
            target: match &rf.target {
                cad_eval::RefTarget::Grid { grid, sel: s } => {
                    p::RefTarget::Grid { grid: *grid, sel: [sel(&s[0]), sel(&s[1]), sel(&s[2])] }
                }
                cad_eval::RefTarget::Plane { grid, plane } => p::RefTarget::Plane { grid: *grid, plane: *plane },
                cad_eval::RefTarget::Anchor { inst, anchor } => p::RefTarget::Anchor { inst: *inst, anchor: *anchor },
            },
        })
        .collect();

    let echoes = out
        .echoes
        .iter()
        .map(|e| p::Echo { stmt: e.stmt, range: r(e.span), text: e.text.clone(), in_context: e.in_context })
        .collect();

    let c = &out.context;
    let context = p::ContextInfo {
        module: c.module.clone(),
        def_stmt: c.def_stmt,
        def_range: ro(c.def_span),
        path: c.path.clone(),
        params: c
            .params
            .iter()
            .map(|pv| p::ParamValue { name: pv.name.clone(), value: value(&pv.value), overridden: pv.overridden })
            .collect(),
        root_modifier: c.root_modifier,
    };

    let root_is_subtree = tree.root.is_some_and(|r| tree.node(r).parent.is_some());
    p::Scene {
        version,
        complete: matches!(kernel, KernelInfo::Done(_)),
        context,
        root: tree.root,
        root_is_subtree,
        nodes,
        grids,
        anchors,
        instances,
        handles,
        locked_handles,
        hints,
        modules,
        refs,
        tag_nodes,
        echoes,
        aborted: out.aborted,
        stats,
    }
}
