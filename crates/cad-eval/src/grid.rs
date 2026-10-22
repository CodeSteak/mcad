//! Grids (Spec: Raster): declaration, access `g[a, b, c]`, `g.name`, and anchors of
//! named instances `k.name`.

use cad_syntax::Span;
use cad_syntax::ast::*;
use glam::DVec3;

use crate::env::{Dyn, Env};
use crate::eval::{Ctx, Ev, NameRef, R};
use crate::handles::{HSpec, comp_scalar};
use crate::output::{GridInfo, HandleKind, PlaneInfo, RefTarget};
use crate::value::{GridId, InstId, Origin, Sel, Val, Value};

const AXES: [&str; 3] = ["x", "y", "z"];

enum Item<'a> {
    Plane(u32),
    Coord(Val<'a>),
    Bad,
}

impl<'a> Ev<'a> {
    pub(crate) fn grid_stmt(&mut self, s: &'a Stmt, g: &'a GridDecl, ctx: &Ctx<'a>) -> R<()> {
        if ctx.in_loop {
            self.error(g.name.span, "grids are not allowed inside for loops");
            return Ok(());
        }
        if ctx.in_children {
            self.error(g.name.span, "grids are not allowed in the children of a module call");
            return Ok(());
        }
        if crate::math::uniform_scale(&ctx.xf.mod_local).is_none() {
            self.error(
                g.name.span,
                format!(
                    "grid `{}` is placed under a non-uniform scale (only uniform scale is allowed before a grid)",
                    g.name.name
                ),
            );
        }
        let in_ctx = ctx.in_context();
        // (plane info, value expression for handles)
        let mut planes: Vec<(PlaneInfo, Option<Val<'a>>, Span)> = Vec::new();
        let mut taken: Vec<(&str, Span)> = Vec::new();
        for pd in &g.planes {
            if let Some((_, first)) = taken.iter().find(|(n, _)| *n == pd.name.name) {
                let line = self.line(*first);
                self.error(
                    pd.name.span,
                    format!("plane name `{}` is already used in grid `{}` (line {line})", pd.name.name, g.name.name),
                );
                continue;
            }
            if pd.name.name == "_" {
                self.error(pd.name.span, "`_` is reserved for the implicit plane of a missing axis");
                continue;
            }
            taken.push((&pd.name.name, pd.name.span));
            let v = self.expr(&pd.value, &ctx.env, &ctx.dyn_)?;
            let axis = pd.axis.index() as u8;
            if let Some(n) = v.as_num() {
                planes.push((plane(axis, &pd.name.name, None, n, &v, pd.value.span), Some(v), pd.value.span));
            } else if let Some(l) = v.as_list() {
                for (i, e) in l.iter().enumerate() {
                    match e.as_num() {
                        Some(n) => {
                            let span = e.def.unwrap_or(pd.value.span);
                            planes.push((
                                plane(axis, &pd.name.name, Some(i as u32), n, e, span),
                                Some(e.clone()),
                                span,
                            ));
                        }
                        None => {
                            self.error(pd.value.span, format!("plane `{}[{i}]` is not a number", pd.name.name));
                        }
                    }
                }
            } else {
                let t = v.type_name();
                self.error(
                    pd.value.span,
                    format!("plane `{}` must be a number or a list of numbers, found {t}", pd.name.name),
                );
            }
        }
        for axis in 0..3u8 {
            if !planes.iter().any(|(p, _, _)| p.axis == axis) {
                planes.push((
                    PlaneInfo {
                        axis,
                        name: "_".into(),
                        base: "_".into(),
                        index: None,
                        value: 0.0,
                        origin: Origin::Computed,
                        value_span: None,
                        literal: None,
                        implicit: true,
                    },
                    None,
                    Span::default(),
                ));
            }
        }
        planes.sort_by(|a, b| a.0.axis.cmp(&b.0.axis).then(a.0.value.total_cmp(&b.0.value)));
        let id = self.grids.len() as GridId;
        let info = GridInfo {
            id,
            name: g.name.name.clone(),
            stmt: s.id,
            span: s.span,
            name_span: g.name.span,
            module_inst: ctx.frame.inst,
            in_context: in_ctx,
            frame: ctx.xf.world,
            frame_module: ctx.xf.mod_local,
            planes: planes.iter().map(|(p, _, _)| p.clone()).collect(),
            visible: Span { start: s.span.end, end: ctx.frame.body_end },
        };
        self.grids.push(info);
        self.declare_name(ctx.frame.inst, &g.name, NameRef::Grid(id), ctx);
        for (i, (p, v, span)) in planes.iter().enumerate() {
            let Some(v) = v else { continue };
            let mut axis = DVec3::ZERO;
            axis[p.axis as usize] = 1.0;
            let (comps, has_ref) = comp_scalar(v, 1.0);
            self.add_handle(HSpec {
                stmt: s,
                ctx,
                node: None,
                kind: HandleKind::Plane { axis: p.axis },
                index: i as u32,
                frame: ctx.xf.world,
                pos: axis * p.value,
                axis: Some(axis),
                base: Some(DVec3::ZERO),
                value: vec![p.value],
                comps,
                has_ref,
                expr_span: *span,
                def_span: v.def,
                via_var: false,
            });
        }
        Ok(())
    }

    pub(crate) fn grid_access(
        &mut self,
        e: &'a Expr,
        g: GridId,
        items: &'a [IndexItem],
        env: &Env<'a>,
        dy: &Dyn<'a>,
    ) -> R<Val<'a>> {
        let undef = Val { def: Some(e.span), ..Val::undef() };
        let gname = self.grids[g as usize].name.clone();
        let z_implicit = self.grids[g as usize].planes.iter().any(|p| p.axis == 2 && p.implicit);
        if !(items.len() == 3 || (items.len() == 2 && z_implicit)) {
            self.error(
                e.span,
                format!("grid access `{gname}[…]` needs one plane per axis (x, y, z), found {}", items.len()),
            );
            return Ok(undef);
        }
        let mut sel = [Sel::Wild; 3];
        // Origins of coordinate items (expressions instead of plane names).
        let mut coord_origin = [None; 3];
        for (i, it) in items.iter().enumerate() {
            sel[i] = match it {
                IndexItem::Wild(_) => Sel::Wild,
                IndexItem::Expr(x) => match self.plane_item(g, i as u8, x, env, dy)? {
                    Item::Plane(p) => Sel::Plane(p),
                    Item::Coord(v) => {
                        coord_origin[i] = Some(v.origin);
                        Sel::Coord(v.as_num().unwrap_or(0.0))
                    }
                    Item::Bad => return Ok(undef),
                },
            };
        }
        if items.len() == 2 {
            let p = self.grids[g as usize].planes.iter().position(|p| p.axis == 2 && p.implicit).unwrap_or(0);
            sel[2] = Sel::Plane(p as u32);
        }
        let wild = sel.iter().filter(|s| **s == Sel::Wild).count();
        if wild == 3 {
            self.error(e.span, "`[*, *, *]` selects the whole grid; use at most two `*`");
            return Ok(undef);
        }
        if self.is_in_context(env) {
            self.add_ref(e.span, RefTarget::Grid { grid: g, sel });
        }
        if wild > 0 {
            return Ok(Val { def: Some(e.span), ..Val::new(Value::GridSel { grid: g, sel: std::rc::Rc::new(sel) }) });
        }
        let (p, _) = self.sel_point(g, &sel);
        let identity = self.grids[g as usize].frame_module == glam::DMat4::IDENTITY;
        let o = Origin::GridRef(e.span);
        let comps = (0..3)
            .map(|i| {
                let oi = match coord_origin[i] {
                    Some(co) if identity => co,
                    _ => o,
                };
                Val::with_origin(Value::Num(clean(p[i])), oi)
            })
            .collect();
        Ok(Val { def: Some(e.span), ..Val::with_origin(Value::List(std::rc::Rc::new(comps)), o) })
    }

    /// Point of a grid selection in module coordinates (wildcards at 0) and the
    /// wildcard axes.
    pub(crate) fn sel_point(&self, g: GridId, sel: &[Sel; 3]) -> (DVec3, Vec<usize>) {
        let gi = &self.grids[g as usize];
        let mut c = DVec3::ZERO;
        let mut wild = Vec::new();
        for (i, s) in sel.iter().enumerate() {
            match s {
                Sel::Plane(p) => c[i] = gi.planes[*p as usize].value,
                Sel::Coord(v) => c[i] = *v,
                Sel::Wild => wild.push(i),
            }
        }
        (gi.frame_module.transform_point3(c), wild)
    }

    /// Resolves one item of a grid access: a plane on `axis`, or (if the item is not
    /// a plane name of the grid) a number expression used as coordinate.
    fn plane_item(&mut self, g: GridId, axis: u8, x: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<Item<'a>> {
        let gname = self.grids[g as usize].name.clone();
        let (name, idx_expr): (&str, Option<&'a Expr>) = match &x.kind {
            ExprKind::Ident(n) => (n, None),
            ExprKind::Index { base, items } if matches!(base.kind, ExprKind::Ident(_)) && items.len() == 1 => {
                let ExprKind::Ident(n) = &base.kind else { unreachable!() };
                match &items[0] {
                    IndexItem::Expr(ie) => (n, Some(ie)),
                    IndexItem::Wild(_) => return self.coord_item(x, env, dy),
                }
            }
            _ => return self.coord_item(x, env, dy),
        };
        let planes = &self.grids[g as usize].planes;
        if name == "_" {
            return Ok(match planes.iter().position(|p| p.axis == axis && p.implicit) {
                Some(p) => Item::Plane(p as u32),
                None => {
                    self.error(
                        x.span,
                        format!(
                            "axis {} of grid `{gname}` has planes; `_` only stands for a missing axis",
                            AXES[axis as usize]
                        ),
                    );
                    Item::Bad
                }
            });
        }
        let named: Vec<usize> =
            planes.iter().enumerate().filter(|(_, p)| !p.implicit && p.base == name).map(|(i, _)| i).collect();
        if named.is_empty() {
            // Not a plane of this grid: a coordinate expression (`t` in the spec example).
            if matches!(x.kind, ExprKind::Ident(_)) && self.lookup(name, env)?.is_none() {
                self.error(x.span, format!("grid `{gname}` has no plane `{name}`"));
                return Ok(Item::Bad);
            }
            return self.coord_item(x, env, dy);
        }
        let idx = match idx_expr {
            None => None,
            Some(ie) => {
                let iv = self.expr(ie, env, dy)?;
                match iv.as_num() {
                    Some(i) if i >= 0.0 && i.fract() == 0.0 => Some(i as u32),
                    _ => {
                        self.error(ie.span, "plane index must be a non-negative integer");
                        return Ok(Item::Bad);
                    }
                }
            }
        };
        let planes = &self.grids[g as usize].planes;
        if let Some(&p) = named.iter().find(|&&i| planes[i].index == idx) {
            let pa = planes[p].axis;
            if pa != axis {
                let pname = planes[p].name.clone();
                self.error(
                    x.span,
                    format!(
                        "plane `{pname}` lies on axis {}, but position {} of `{gname}[…]` needs an {} plane",
                        AXES[pa as usize],
                        axis + 1,
                        AXES[axis as usize]
                    ),
                );
                return Ok(Item::Bad);
            }
            return Ok(Item::Plane(p as u32));
        }
        let count = named.iter().filter(|&&i| planes[i].index.is_some()).count();
        let msg = match (idx, count) {
            (None, c) if c > 0 => format!("`{name}` is a list of planes in grid `{gname}`; use `{name}[i]`"),
            (Some(i), c) if c > 0 => format!("plane index {i} out of range: `{name}` has {c} planes"),
            _ => format!("plane `{name}` of grid `{gname}` is not a list"),
        };
        self.error(x.span, msg);
        Ok(Item::Bad)
    }

    fn coord_item(&mut self, x: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<Item<'a>> {
        let v = self.expr(x, env, dy)?;
        if v.as_num().is_some() {
            Ok(Item::Coord(v))
        } else {
            let t = v.type_name();
            self.error(x.span, format!("grid access item must be a plane name or a number, found {t}"));
            Ok(Item::Bad)
        }
    }

    /// `g.name`: plane reference (or list of references for indexed planes).
    pub(crate) fn plane_member(&mut self, g: GridId, name: &Ident, span: Span, in_ctx: bool) -> Val<'a> {
        let gi = &self.grids[g as usize];
        let found: Vec<(u32, Option<u32>, f64)> = gi
            .planes
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.implicit && p.base == name.name)
            .map(|(i, p)| (i as u32, p.index, p.value))
            .collect();
        let gname = gi.name.clone();
        let o = Origin::GridRef(span);
        let undef = Val { def: Some(span), ..Val::undef() };
        match found.as_slice() {
            [] => {
                self.error(name.span, format!("grid `{gname}` has no plane `{}`", name.name));
                undef
            }
            [(i, None, v)] => {
                if in_ctx {
                    self.add_ref(span, RefTarget::Plane { grid: g, plane: *i });
                }
                Val { def: Some(span), ..Val::with_origin(Value::PlaneRef { grid: g, plane: *i, value: *v }, o) }
            }
            list => {
                let mut l = list.to_vec();
                l.sort_by_key(|(_, idx, _)| *idx);
                let items = l
                    .iter()
                    .map(|(i, _, v)| Val::with_origin(Value::PlaneRef { grid: g, plane: *i, value: *v }, o))
                    .collect();
                Val { def: Some(span), ..Val::list(items) }
            }
        }
    }

    /// `k.name`: anchor of a named instance in the current module's coordinates.
    pub(crate) fn anchor_member(&mut self, inst: InstId, name: &Ident, span: Span, in_ctx: bool) -> Val<'a> {
        let info = &self.insts[inst as usize];
        let Some(ai) = info.anchors.iter().position(|a| a.name == name.name) else {
            let (label, module) = (info.label.clone().unwrap_or_default(), info.module.clone());
            self.error(name.span, format!("instance `{label}` (module `{module}`) has no anchor `{}`", name.name));
            return Val { def: Some(span), ..Val::undef() };
        };
        let p = info.matrix.transform_point3(info.anchors[ai].point);
        if in_ctx {
            self.add_ref(span, RefTarget::Anchor { inst, anchor: ai as u32 });
        }
        let o = Origin::AnchorRef { span, inst, anchor: ai as u32 };
        let comps = (0..3).map(|i| Val::with_origin(Value::Num(clean(p[i])), o)).collect();
        Val { def: Some(span), ..Val::with_origin(Value::List(std::rc::Rc::new(comps)), o) }
    }

    /// `dir(k.name)`.
    pub(crate) fn anchor_dir(&mut self, v: &Val<'a>, span: Span) -> Val<'a> {
        let Origin::AnchorRef { inst, anchor, .. } = v.origin else {
            self.error(span, "dir() expects an anchor of a named instance (`k.name`)");
            return Val::undef();
        };
        let info = &self.insts[inst as usize];
        let a = &info.anchors[anchor as usize];
        match a.dir {
            Some(d) => {
                let d = crate::math::transform_dir(&info.matrix, d);
                Val::list((0..3).map(|i| Val::num(clean(d[i]))).collect())
            }
            None => {
                let msg = format!("anchor `{}` has no direction", a.name);
                self.error(span, msg);
                Val::undef()
            }
        }
    }
}

fn plane(axis: u8, base: &str, index: Option<u32>, value: f64, v: &Val, span: Span) -> PlaneInfo {
    PlaneInfo {
        axis,
        name: match index {
            Some(i) => format!("{base}[{i}]"),
            None => base.to_string(),
        },
        base: base.to_string(),
        index,
        value,
        origin: v.origin,
        value_span: Some(span),
        literal: match v.origin {
            Origin::Literal(s) => Some(s),
            _ => None,
        },
        implicit: false,
    }
}

/// Removes floating noise from transformed coordinates (`1e-16` → `0`).
pub(crate) fn clean(x: f64) -> f64 {
    let r = x.round();
    if (x - r).abs() < 1e-9 { r + 0.0 } else { x }
}
