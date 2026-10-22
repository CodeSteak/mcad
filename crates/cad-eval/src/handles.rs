//! Construction of handles (Spec: Griffe und Rückschreiben).

use cad_ir::NodeIdx;
use cad_syntax::Span;
use cad_syntax::ast::Stmt;
use glam::{DMat4, DVec3};

use crate::eval::{ArgV, Ctx, Ev, NameRef};
use crate::output::{Comp, Handle, HandleKind};
use crate::value::{Origin, Val, Value};
use crate::{MASK53, mix};

/// At most this many handles per statement, kind and index (loop iterations, several
/// instances); all of them edit the same literals.
pub(crate) const MAX_REPEATS: u32 = 32;

pub(crate) struct HSpec<'x, 'a> {
    pub stmt: &'a Stmt,
    pub ctx: &'x Ctx<'a>,
    pub node: Option<NodeIdx>,
    pub kind: HandleKind,
    pub index: u32,
    pub frame: DMat4,
    pub pos: DVec3,
    pub axis: Option<DVec3>,
    pub base: Option<DVec3>,
    pub value: Vec<f64>,
    pub comps: Vec<Option<Comp>>,
    pub has_ref: bool,
    pub expr_span: Span,
    /// Definition span of the value (`Val::def`), if it differs from `expr_span`.
    pub def_span: Option<Span>,
    /// Reached through a variable even if no definition span is known.
    pub via_var: bool,
}

fn comp_of(v: &Val, scale: f64) -> (Option<Comp>, bool) {
    match v.origin {
        Origin::Literal(span) if v.as_num().is_some() => (Some(Comp { span, scale }), false),
        o => (None, o.is_ref()),
    }
}

/// Literal per component of a vector value (or of a scalar used for all `n`
/// components, as in `cube(20)`), and whether some component is a reference.
pub(crate) fn comps_of(v: &Val, n: usize, scale: f64) -> (Vec<Option<Comp>>, bool) {
    let mut has_ref = false;
    let comps = match &v.v {
        Value::List(l) => (0..n)
            .map(|i| match l.get(i) {
                Some(c) => {
                    let (c, r) = comp_of(c, scale);
                    has_ref |= r;
                    c
                }
                None => None,
            })
            .collect(),
        _ => {
            let (c, r) = comp_of(v, scale);
            has_ref = r;
            vec![c; n]
        }
    };
    (comps, has_ref)
}

/// Single-component literal of a scalar value.
pub(crate) fn comp_scalar(v: &Val, scale: f64) -> (Vec<Option<Comp>>, bool) {
    let (c, r) = comp_of(v, scale);
    (vec![c], r)
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

impl<'a> Ev<'a> {
    #[inline(never)]
    pub(crate) fn add_handle(&mut self, h: HSpec<'_, 'a>) {
        if !h.ctx.in_context() {
            return;
        }
        if h.comps.iter().all(|c| c.is_none()) && !h.has_ref {
            return;
        }
        let count = self.handle_count.entry((h.stmt.id, h.kind.tag(), h.index)).or_insert(0);
        if *count >= MAX_REPEATS {
            return;
        }
        *count += 1;
        let key = h.node.map_or(h.ctx.key, |n| self.nodes[n as usize].inst_key);
        let id = mix(mix(mix(key, h.stmt.id as u64), h.kind.tag()), h.index as u64) & MASK53;
        if !self.handle_ids.insert(id) {
            return;
        }
        let def_span = h.def_span.filter(|d| *d != h.expr_span);
        let via_var =
            h.via_var || def_span.is_some() || h.comps.iter().flatten().any(|c| !contains(h.expr_span, c.span));
        let via_default = h.comps.iter().flatten().any(|c| self.default_spans.iter().any(|d| contains(*d, c.span)));
        let inst = h.ctx.frame.inst;
        let names = self.names_of(inst);
        let grids = names.iter().filter_map(|(_, r)| if let NameRef::Grid(g) = r { Some(*g) } else { None }).collect();
        let instances =
            names.iter().filter_map(|(_, r)| if let NameRef::Inst(i) = r { Some(*i) } else { None }).collect();
        self.handles.push(Handle {
            id,
            stmt: h.stmt.id,
            span: h.stmt.span,
            kind: h.kind,
            index: h.index,
            frame: h.frame,
            pos: h.pos,
            axis: h.axis,
            base: h.base,
            value: h.value,
            comps: h.comps,
            has_ref: h.has_ref,
            expr_span: h.expr_span,
            def_span,
            via_var,
            via_default,
            grids,
            instances,
            node: h.node,
            module_inst: inst,
        });
    }

    /// Point handle for a vector argument (`translate(v)`, box corners, …).
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    pub(crate) fn point_handle(
        &mut self,
        s: &'a Stmt,
        ctx: &Ctx<'a>,
        node: NodeIdx,
        index: u32,
        arg: &ArgV<'a>,
        pos: DVec3,
        dims: usize,
        scale: f64,
    ) {
        let (comps, has_ref) = comps_of(&arg.val, dims, scale);
        let frame = self.nodes[node as usize].world;
        self.add_handle(HSpec {
            stmt: s,
            ctx,
            node: Some(node),
            kind: HandleKind::Point,
            index,
            frame,
            pos,
            axis: None,
            base: None,
            value: (0..dims).map(|i| pos[i]).collect(),
            comps,
            has_ref,
            expr_span: arg.expr.span,
            def_span: arg.val.def,
            via_var: false,
        });
    }

    /// Point handle for element `i` of a list argument (polygon points, paths).
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    pub(crate) fn elem_handle(
        &mut self,
        s: &'a Stmt,
        ctx: &Ctx<'a>,
        node: NodeIdx,
        index: u32,
        arg: &ArgV<'a>,
        elem: &Val<'a>,
        pos: DVec3,
        dims: usize,
        frame: DMat4,
    ) {
        let (comps, has_ref) = comps_of(elem, dims, 1.0);
        let espan = elem.def.unwrap_or(arg.expr.span);
        self.add_handle(HSpec {
            stmt: s,
            ctx,
            node: Some(node),
            kind: HandleKind::Point,
            index,
            frame,
            pos,
            axis: None,
            base: None,
            value: (0..dims).map(|i| pos[i]).collect(),
            comps,
            has_ref,
            expr_span: espan,
            def_span: None,
            // Outside the argument: the element was defined elsewhere (variable).
            via_var: !contains(arg.expr.span, espan),
        });
    }

    /// 1D handle (radius, height, angle, scale) for a scalar argument.
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    pub(crate) fn scalar_handle(
        &mut self,
        s: &'a Stmt,
        ctx: &Ctx<'a>,
        node: NodeIdx,
        kind: HandleKind,
        index: u32,
        val: &Val<'a>,
        expr_span: Span,
        value: f64,
        scale: f64,
        frame: DMat4,
        base: DVec3,
        axis: DVec3,
    ) {
        let (comps, has_ref) = comp_scalar(val, scale);
        let pos = if kind.is_angle() { base } else { base + axis * value };
        self.add_handle(HSpec {
            stmt: s,
            ctx,
            node: Some(node),
            kind,
            index,
            frame,
            pos,
            axis: Some(axis),
            base: Some(base),
            value: vec![value],
            comps,
            has_ref,
            expr_span,
            def_span: val.def,
            via_var: false,
        });
    }
}
