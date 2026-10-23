//! Statement evaluation: scopes, module instantiation, control flow, grids, anchors,
//! named instances, and building the geometry tree.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::{Rc, Weak};

use cad_ir::{Flags, Node, NodeIdx, Op, Tree};
use cad_kernel::BoolOp;
use cad_syntax::ast::*;
use cad_syntax::{Diagnostic, Span};
use glam::DMat4;

use crate::env::{Dyn, Env, NO_INST, Scope, dyn_push};
use crate::output::*;
use crate::project::Project;
use crate::value::{InstId, Origin, PValue, Val, Value, fmt_num, to_hint};
use crate::{MASK53, mix};

/// Evaluation aborted (diagnostic already reported).
#[derive(Debug)]
pub(crate) struct Fatal;
pub(crate) type R<T> = Result<T, Fatal>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameRef {
    Grid(u32),
    Inst(InstId),
    /// `k` of `k[i]: m();` (index into `Ev::families`).
    Family(u32),
}

#[derive(Debug, Clone)]
pub(crate) struct ArgV<'a> {
    pub name: Option<&'a Ident>,
    pub val: Val<'a>,
    pub expr: &'a Expr,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Args<'a> {
    pub list: Vec<ArgV<'a>>,
    /// `$name = value` arguments.
    pub dyn_vars: Vec<(&'a str, Val<'a>)>,
    /// Per `$` argument: number of `list` arguments before it (source order for echo).
    pub dyn_at: Vec<usize>,
}

impl<'a> Args<'a> {
    pub fn positional(&self) -> impl Iterator<Item = &ArgV<'a>> {
        self.list.iter().filter(|a| a.name.is_none())
    }

    /// Named argument `name`, else the `i`-th positional argument.
    pub fn get(&self, i: usize, name: &str) -> Option<&ArgV<'a>> {
        self.list.iter().rev().find(|a| a.name.is_some_and(|n| n.name == name)).or_else(|| self.positional().nth(i))
    }

    pub fn dyn_num(&self, name: &str) -> Option<f64> {
        self.dyn_vars.iter().rev().find(|(n, _)| *n == name).and_then(|(_, v)| v.as_num())
    }
}

pub(crate) struct ChildrenRef<'a> {
    pub stmts: &'a [Stmt],
    pub env: Env<'a>,
    pub frame: Rc<Frame<'a>>,
    pub mod_local: DMat4,
    pub in_loop: bool,
}

/// One module instance being evaluated.
pub(crate) struct Frame<'a> {
    pub inst: InstId,
    pub children: Option<ChildrenRef<'a>>,
    /// End of the module body (end of visibility of grids and named instances).
    pub body_end: u32,
}

#[derive(Clone)]
pub(crate) struct Ctx<'a> {
    pub env: Env<'a>,
    pub dyn_: Dyn<'a>,
    pub frame: Rc<Frame<'a>>,
    /// Coordinate systems (shared: contexts are cloned often and sit on the stack).
    pub xf: Rc<Xf>,
    pub parent: Option<NodeIdx>,
    /// Key of the parent (instance path hash).
    pub key: u64,
    pub in_loop: bool,
    /// Inside children of a user module call.
    pub in_children: bool,
    pub color: Option<[f32; 4]>,
}

pub(crate) struct Xf {
    /// Current system → context root.
    pub world: DMat4,
    /// Transformation the parent node applies (local of nodes created here).
    pub local: DMat4,
    /// Current system → current module system.
    pub mod_local: DMat4,
}

impl Xf {
    pub fn identity() -> Rc<Xf> {
        Rc::new(Xf { world: DMat4::IDENTITY, local: DMat4::IDENTITY, mod_local: DMat4::IDENTITY })
    }

    /// System after applying `m` (children of a transform).
    #[inline(never)]
    pub fn child(&self, m: &DMat4) -> Rc<Xf> {
        Rc::new(Xf { world: self.world * *m, local: *m, mod_local: self.mod_local * *m })
    }

    /// Same place, new local system with module system `mod_local`.
    #[inline(never)]
    pub fn rebase(&self, mod_local: &DMat4) -> Rc<Xf> {
        Rc::new(Xf { world: self.world, local: DMat4::IDENTITY, mod_local: *mod_local })
    }
}

impl Ctx<'_> {
    pub fn in_context(&self) -> bool {
        self.frame.inst == 0
    }
}

pub(crate) struct HintAcc {
    pub span: Span,
    pub texts: Vec<String>,
    pub value: PValue,
    pub literal: Option<Span>,
    pub in_context: bool,
}

pub(crate) struct Ev<'a> {
    pub proj: &'a Project,
    pub file: &'a File,
    pub opts: &'a EvalOptions,
    /// File scopes of the used libraries (index = `Project::libs`).
    pub lib_envs: Vec<Env<'a>>,
    /// Families of indexed named instances (`k[i]: m();`): index → instance.
    pub families: Vec<BTreeMap<i64, InstId>>,
    pub parts: Vec<PartInfo>,
    pub steps: u64,
    pub aborted: bool,
    pub diags: Vec<Diagnostic>,
    seen: HashSet<(u32, u32, String)>,
    pub nodes: Vec<Node>,
    pub grids: Vec<GridInfo>,
    pub insts: Vec<InstanceInfo>,
    /// Grid and instance names per module instance.
    pub names: Vec<Vec<(&'a str, NameRef)>>,
    pub hints: BTreeMap<StmtId, HintAcc>,
    pub handles: Vec<Handle>,
    pub handle_ids: HashSet<u64>,
    /// Handles per (statement, kind, index): loops repeat a statement many times.
    pub handle_count: std::collections::HashMap<(StmtId, u64, u32), u32>,
    pub echoes: Vec<Echo>,
    pub refs: Vec<RefInfo>,
    refs_seen: HashSet<(u32, u32)>,
    pub captured: Vec<Weak<RefCell<Scope<'a>>>>,
    pub fn_depth: u32,
    pub mod_depth: u32,
    stack_base: usize,
    pub root_mod: Option<NodeIdx>,
    pub rng: u64,
    /// Spans of all parameter default expressions (for `via_default`).
    pub default_spans: Vec<Span>,
    pub cur_stmt: Option<StmtId>,
    pub cur_span: Span,
    pub file_context: bool,
    /// First explicit `filament = n` per colour (RGBA bytes), for conflict warnings.
    pub filaments: std::collections::HashMap<[u8; 4], (u32, Span)>,
}

#[inline(never)]
fn stack_addr() -> usize {
    let x = 0u8;
    std::hint::black_box(&x) as *const u8 as usize
}

pub(crate) fn is_geometric(s: &Stmt) -> bool {
    !matches!(
        s.kind,
        StmtKind::Assign { .. }
            | StmtKind::Module(_)
            | StmtKind::Function(_)
            | StmtKind::Empty
            | StmtKind::Include { .. }
    )
}

fn needs_scope(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| matches!(s.kind, StmtKind::Assign { .. } | StmtKind::Module(_) | StmtKind::Function(_)))
}

/// Evaluates a loaded project (its main file).
pub fn eval<'a>(proj: &'a Project, opts: &'a EvalOptions) -> EvalOutput {
    let mut ev = Ev::new(proj, opts);
    let mut modules = crate::sig::module_sigs(proj, &proj.main);
    for lib in &proj.libs {
        modules.extend(crate::sig::module_sigs(proj, &lib.ast));
    }
    let mut context = ContextInfo::default();
    let _ = match &opts.context {
        Context::File => ev.run_file(),
        Context::Module { name, overrides } => ev.run_module(name, overrides, &mut context),
    };
    context.root_modifier = ev.root_mod;
    ev.finish(modules, context)
}

impl<'a> Ev<'a> {
    fn new(proj: &'a Project, opts: &'a EvalOptions) -> Self {
        let file = &proj.main;
        let mut default_spans = Vec::new();
        for s in file.stmts.iter().chain(proj.libs.iter().flat_map(|l| l.ast.stmts.iter())) {
            s.walk(&mut |s| match &s.kind {
                StmtKind::Module(m) => {
                    default_spans.extend(m.params.iter().filter_map(|p| p.default.as_ref().map(|d| d.span)))
                }
                StmtKind::Function(f) => {
                    default_spans.extend(f.params.iter().filter_map(|p| p.default.as_ref().map(|d| d.span)))
                }
                _ => {}
            });
        }
        Ev {
            proj,
            file,
            opts,
            lib_envs: Vec::new(),
            families: Vec::new(),
            parts: Vec::new(),
            steps: 0,
            aborted: false,
            diags: Vec::new(),
            seen: HashSet::new(),
            nodes: Vec::new(),
            grids: Vec::new(),
            insts: Vec::new(),
            names: Vec::new(),
            hints: BTreeMap::new(),
            handles: Vec::new(),
            handle_ids: HashSet::new(),
            handle_count: std::collections::HashMap::new(),
            echoes: Vec::new(),
            refs: Vec::new(),
            refs_seen: HashSet::new(),
            captured: Vec::new(),
            fn_depth: 0,
            mod_depth: 0,
            stack_base: stack_addr(),
            root_mod: None,
            rng: opts.seed,
            default_spans,
            cur_stmt: None,
            cur_span: Span::default(),
            file_context: matches!(opts.context, Context::File),
            filaments: std::collections::HashMap::new(),
        }
    }

    // ------------------------------------------------------------ diagnostics

    #[inline(never)]
    pub(crate) fn diag(&mut self, d: Diagnostic) {
        if self.seen.insert((d.span.start, d.span.end, d.message.clone())) {
            self.diags.push(d);
        }
    }

    #[inline(never)]
    pub(crate) fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.diag(Diagnostic::warning(span, msg));
    }

    #[inline(never)]
    pub(crate) fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.diag(Diagnostic::error(span, msg));
    }

    #[inline(never)]
    pub(crate) fn info(&mut self, span: Span, msg: impl Into<String>) {
        self.diag(Diagnostic::info(span, msg));
    }

    #[inline(never)]
    pub(crate) fn fatal<T>(&mut self, span: Span, msg: impl Into<String>) -> R<T> {
        self.error(span, msg);
        self.aborted = true;
        Err(Fatal)
    }

    #[inline]
    pub(crate) fn step(&mut self, span: Span) -> R<()> {
        self.steps += 1;
        if self.steps > self.opts.max_steps {
            return self.steps_exceeded(span);
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn steps_exceeded(&mut self, span: Span) -> R<()> {
        let max = self.opts.max_steps;
        self.fatal(span, format!("Auswertung abgebrochen: mehr als {max} Schritte (Endlosschleife?)"))
    }

    #[inline]
    pub(crate) fn check_stack(&mut self, span: Span) -> R<()> {
        if self.stack_base.abs_diff(stack_addr()) > self.opts.max_stack_bytes {
            return self.stack_exceeded(span);
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn stack_exceeded(&mut self, span: Span) -> R<()> {
        self.fatal(span, "Rekursion zu tief: Auswertungsstapel erschöpft")
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn too_deep<T>(&mut self, span: Span, what: &str, name: &str, max: u32) -> R<T> {
        let msg = match what {
            "module" => format!("Rekursion zu tief: Modul `{name}` mehr als {max} Ebenen verschachtelt"),
            _ => format!("Rekursion zu tief: Funktion `{name}` mehr als {max} Ebenen tief aufgerufen"),
        };
        self.fatal(span, msg)
    }

    pub(crate) fn text(&self, span: Span) -> &'a str {
        self.proj.text(span)
    }

    pub(crate) fn line(&self, span: Span) -> u32 {
        self.proj.line(span)
    }

    /// "Zeile n" or "Zeile n in `datei`" for messages about a position seen from `here`.
    pub(crate) fn where_(&self, span: Span, here: Span) -> String {
        let l = self.line(span);
        if span.file() == here.file() {
            format!("Zeile {l}")
        } else {
            format!("Zeile {l} in `{}`", self.proj.path_of(span))
        }
    }

    /// File scopes of the used libraries; `env` (a file root) sees the main file's libraries.
    fn setup_libs(&mut self, env: &Env<'a>) -> R<()> {
        let proj = self.proj;
        self.lib_envs = proj.libs.iter().map(|_| Env::new(None, None, NO_INST, true)).collect();
        for (i, lib) in proj.libs.iter().enumerate() {
            let uses = lib.uses.iter().map(|&u| self.lib_envs[u].clone()).collect();
            self.lib_envs[i].set_uses(uses);
        }
        env.set_uses(proj.main_uses.iter().map(|&u| self.lib_envs[u].clone()).collect());
        for (i, lib) in proj.libs.iter().enumerate() {
            let e = self.lib_envs[i].clone();
            self.hoist(&lib.ast.stmts, &e, &None)?;
        }
        Ok(())
    }

    pub(crate) fn add_ref(&mut self, span: Span, target: RefTarget) {
        if self.refs_seen.insert((span.start, span.end)) {
            self.refs.push(RefInfo { span, stmt: self.cur_stmt, target });
        }
    }

    pub(crate) fn names_of(&self, inst: InstId) -> &[(&'a str, NameRef)] {
        self.names.get(inst as usize).map(|v| v.as_slice()).unwrap_or(&[])
    }

    // ------------------------------------------------------------ nodes

    #[inline(never)]
    pub(crate) fn node(&mut self, s: &Stmt, op: Op, ctx: &Ctx<'a>) -> NodeIdx {
        let idx = self.nodes.len() as NodeIdx;
        let inst_key = mix(mix(ctx.key, s.id as u64), 0x6e6f6465) & MASK53;
        let mut flags = Flags::default();
        for m in &s.modifiers {
            match m {
                Modifier::Highlight => flags.highlight = true,
                Modifier::Background => flags.background = true,
                Modifier::Disable => flags.disabled = true,
                Modifier::Root => {}
            }
        }
        self.nodes.push(Node {
            stmt: s.id,
            span: s.span,
            op,
            children: Vec::new(),
            parent: ctx.parent,
            local: ctx.xf.local,
            world: ctx.xf.world,
            flags,
            module_inst: ctx.frame.inst,
            blend: None,
            tol: None,
            color: ctx.color,
            inst_key,
        });
        if let Some(p) = ctx.parent {
            self.nodes[p as usize].children.push(idx);
        }
        if s.modifiers.contains(&Modifier::Root) {
            if self.root_mod.is_none() {
                self.root_mod = Some(idx);
            } else {
                self.warn(s.span, "mehrere `!`-Modifikatoren: nur der erste gilt");
            }
        }
        idx
    }

    /// Context for the children of `node`, transformed by `m`.
    #[inline(never)]
    pub(crate) fn sub(&self, ctx: &Ctx<'a>, node: NodeIdx, m: &DMat4) -> Ctx<'a> {
        let mut c = ctx.clone();
        c.xf = if *m == DMat4::IDENTITY && ctx.xf.local == DMat4::IDENTITY { ctx.xf.clone() } else { ctx.xf.child(m) };
        c.parent = Some(node);
        c.key = self.nodes[node as usize].inst_key;
        c
    }

    // ------------------------------------------------------------ scopes

    /// Hoists definitions and lazy assignments of a statement list into `env` and
    /// evaluates its `$` assignments. Returns the `$` chain for the list.
    #[inline(never)]
    pub(crate) fn hoist(&mut self, stmts: &'a [Stmt], env: &Env<'a>, dy: &Dyn<'a>) -> R<Dyn<'a>> {
        let mut dollar = Vec::new();
        let mut first: Vec<(&'a str, Span)> = Vec::new();
        for s in stmts {
            match &s.kind {
                StmtKind::Module(m) => env.0.borrow_mut().modules.push((m.name.name.as_str(), m.as_ref())),
                StmtKind::Function(f) => env.0.borrow_mut().funcs.push((f.name.name.as_str(), f.as_ref())),
                StmtKind::Assign { name, value } => {
                    if name.name.starts_with('$') {
                        dollar.push((s, name, value));
                        continue;
                    }
                    if env.set_lazy(&name.name, value) {
                        if let Some((_, sp)) = first.iter().find(|(n, _)| *n == name.name) {
                            let (w1, w2) = (self.where_(*sp, s.span), self.where_(s.span, *sp));
                            self.warn(name.span, format!("`{}` aus {w1} wird in {w2} überschrieben", name.name));
                        }
                    } else {
                        first.push((&name.name, s.span));
                    }
                }
                _ => {}
            }
        }
        env.set_dyn(dy.clone());
        if dollar.is_empty() {
            return Ok(dy.clone());
        }
        let mut vars: Vec<(&'a str, Val<'a>)> = Vec::new();
        let mut cur = dy.clone();
        let in_ctx = env.inst() == 0;
        for (s, name, value) in dollar {
            let prev = self.cur_stmt.replace(s.id);
            let v = self.expr(value, env, &cur);
            self.cur_stmt = prev;
            let v = v?;
            self.hint(s, &v, in_ctx);
            vars.retain(|(n, _)| *n != name.name);
            vars.push((&name.name, v));
            cur = dyn_push(dy, vars.clone());
            env.set_dyn(cur.clone());
        }
        Ok(cur)
    }

    /// Evaluates a statement list as one scope (block, children of a call).
    #[inline(never)]
    pub(crate) fn scope_list(&mut self, stmts: &'a [Stmt], ctx: &Ctx<'a>) -> R<()> {
        if needs_scope(stmts) {
            let env = ctx.env.child(ctx.dyn_.clone());
            let dy = self.hoist(stmts, &env, &ctx.dyn_)?;
            let c = Ctx { env, dyn_: dy, ..ctx.clone() };
            for s in stmts {
                self.stmt(s, &c)?;
            }
        } else {
            for s in stmts {
                self.stmt(s, ctx)?;
            }
        }
        Ok(())
    }

    /// Children of a builtin operation: evaluated as one scope under `node`.
    #[inline(never)]
    pub(crate) fn eval_children(
        &mut self,
        node: NodeIdx,
        m: &DMat4,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let mut c = self.sub(ctx, node, m);
        c.dyn_ = dy.clone();
        self.scope_list(children, &c)
    }

    // ------------------------------------------------------------ entry points

    #[inline(never)]
    fn new_instance(&mut self, info: InstanceInfo) -> InstId {
        let id = self.insts.len() as InstId;
        self.insts.push(InstanceInfo { id, ..info });
        self.names.push(Vec::new());
        id
    }

    fn root_ctx(&self, env: Env<'a>, dy: Dyn<'a>, body_end: u32, key: u64) -> Ctx<'a> {
        Ctx {
            env,
            dyn_: dy,
            frame: Rc::new(Frame { inst: 0, children: None, body_end }),
            xf: Xf::identity(),
            parent: Some(0),
            key,
            in_loop: false,
            in_children: false,
            color: None,
        }
    }

    fn run_file(&mut self) -> R<()> {
        let end = self.proj.main_text().len() as u32;
        let key = mix(0x66696c65, 0) & MASK53;
        self.nodes.push(Node {
            stmt: cad_ir::NO_STMT,
            span: Span { start: 0, end },
            op: Op::Group,
            children: Vec::new(),
            parent: None,
            local: DMat4::IDENTITY,
            world: DMat4::IDENTITY,
            flags: Flags::default(),
            module_inst: 0,
            blend: None,
            tol: None,
            color: None,
            inst_key: key,
        });
        self.new_instance(InstanceInfo {
            id: 0,
            module: String::new(),
            label: None,
            call_stmt: None,
            call_span: None,
            def_stmt: None,
            def_span: None,
            node: Some(0),
            parent: None,
            matrix: DMat4::IDENTITY,
            world: DMat4::IDENTITY,
            anchors: Vec::new(),
            visible: None,
            depth: 0,
        });
        let env = Env::new(None, None, 0, true);
        self.setup_libs(&env)?;
        let dy = self.hoist(&self.file.stmts, &env, &None)?;
        let ctx = self.root_ctx(env, dy, end, key);
        for s in &self.file.stmts {
            self.stmt(s, &ctx)?;
        }
        Ok(())
    }

    fn run_module(&mut self, name: &str, overrides: &'a [(String, PValue)], info: &mut ContextInfo) -> R<()> {
        // The main file's own modules first, then those of the used libraries (last `use`
        // first, as their lookup).
        let main_root = Env::new(None, None, NO_INST, true);
        self.setup_libs(&main_root)?;
        let proj = self.proj;
        let found = find_module_path(&self.file.stmts, name).map(|p| (p, None)).or_else(|| {
            proj.libs
                .iter()
                .enumerate()
                .rev()
                .find_map(|(i, l)| find_module_path(&l.ast.stmts, name).map(|p| (p, Some(i))))
        });
        let Some((path, lib)) = found else {
            return self.fatal(Span::default(), format!("Modul `{name}` nicht gefunden"));
        };
        let def = *path.last().unwrap();
        info.module = Some(def.name.name.clone());
        info.def_stmt = Some(def.stmt);
        info.def_span = Some(def.span);
        info.path = path.iter().map(|m| m.name.name.clone()).collect();

        let (root, mut dy) = match lib {
            None => {
                let dy = self.hoist(&self.file.stmts, &main_root, &None)?;
                (main_root, dy)
            }
            Some(i) => {
                let e = self.lib_envs[i].clone();
                let dy = e.dyn_();
                (e, dy)
            }
        };
        let mut env = root;
        for anc in &path[..path.len() - 1] {
            let e = Env::new(Some(&env), dy.clone(), NO_INST, true);
            for p in &anc.params {
                match &p.default {
                    Some(d) => e.set_default(&p.name.name, d, env.clone(), dy.clone()),
                    None => e.set(&p.name.name, Val::undef()),
                }
            }
            dy = self.hoist(&anc.body, &e, &dy)?;
            env = e;
        }

        let key = mix(0x6d6f64, def.stmt as u64) & MASK53;
        self.nodes.push(Node {
            stmt: def.stmt,
            span: def.span,
            op: Op::ModuleCall { name: def.name.name.clone(), instance: None },
            children: Vec::new(),
            parent: None,
            local: DMat4::IDENTITY,
            world: DMat4::IDENTITY,
            flags: Flags::default(),
            module_inst: 0,
            blend: None,
            tol: None,
            color: None,
            inst_key: key,
        });
        self.new_instance(InstanceInfo {
            id: 0,
            module: def.name.name.clone(),
            label: None,
            call_stmt: None,
            call_span: None,
            def_stmt: Some(def.stmt),
            def_span: Some(def.span),
            node: Some(0),
            parent: None,
            matrix: DMat4::IDENTITY,
            world: DMat4::IDENTITY,
            anchors: Vec::new(),
            visible: None,
            depth: 0,
        });
        let e = Env::new(Some(&env), None, 0, true);
        for (oname, _) in overrides {
            if !def.params.iter().any(|p| p.name.name == *oname) {
                self.warn(def.name.span, format!("Modul `{}` hat keinen Parameter `{oname}`", def.name.name));
            }
        }
        for p in &def.params {
            if let Some((_, v)) = overrides.iter().rev().find(|(n, _)| *n == p.name.name) {
                e.set(&p.name.name, v.to_val());
            } else if let Some(d) = &p.default {
                e.set_default(&p.name.name, d, env.clone(), dy.clone());
            } else {
                e.set(&p.name.name, Val::undef());
            }
        }
        let dy = dyn_push(&dy, vec![("$children", Val::num(0.0))]);
        e.set_dyn(dy.clone());
        let body_dy = self.hoist(&def.body, &e, &dy)?;
        for p in &def.params {
            let v = self.lookup(&p.name.name, &e)?.unwrap_or_else(Val::undef);
            info.params.push(ParamValue {
                name: p.name.name.clone(),
                value: PValue::from_val(&v),
                overridden: overrides.iter().any(|(n, _)| *n == p.name.name),
            });
        }
        let ctx = self.root_ctx(e, body_dy, def.body_span.end, key);
        for s in &def.body {
            self.stmt(s, &ctx)?;
        }
        Ok(())
    }

    fn finish(mut self, modules: Vec<ModuleSig>, context: ContextInfo) -> EvalOutput {
        for w in self.captured.drain(..) {
            Env::clear_weak(&w);
        }
        let hints = std::mem::take(&mut self.hints)
            .into_iter()
            .map(|(stmt, h)| {
                let mut text = h.texts.join(" | ");
                if h.texts.len() >= 4 {
                    text.push_str(" | …");
                }
                Hint { stmt, span: h.span, text, value: h.value, literal: h.literal, in_context: h.in_context }
            })
            .collect();
        let root = if self.nodes.is_empty() { None } else { Some(self.root_mod.unwrap_or(0)) };
        EvalOutput {
            tree: Tree { nodes: self.nodes, root },
            diagnostics: self.diags,
            hints,
            parts: self.parts,
            grids: self.grids,
            instances: self.insts,
            handles: self.handles,
            locked_handles: Vec::new(),
            echoes: self.echoes,
            modules,
            context,
            refs: self.refs,
            aborted: self.aborted,
            steps: self.steps,
        }
    }

    // ------------------------------------------------------------ statements

    #[inline(never)]
    pub(crate) fn stmt(&mut self, s: &'a Stmt, ctx: &Ctx<'a>) -> R<()> {
        if s.modifiers.contains(&Modifier::Disable) {
            return Ok(());
        }
        self.step(s.span)?;
        self.check_stack(s.span)?;
        let prev = self.cur_stmt.replace(s.id);
        let prev_span = std::mem::replace(&mut self.cur_span, s.span);
        let r = self.stmt_inner(s, ctx);
        self.cur_stmt = prev;
        self.cur_span = prev_span;
        r
    }

    #[inline(never)]
    fn stmt_inner(&mut self, s: &'a Stmt, ctx: &Ctx<'a>) -> R<()> {
        match &s.kind {
            StmtKind::Empty | StmtKind::Module(_) | StmtKind::Function(_) | StmtKind::Error => Ok(()),
            // Resolved when the project is loaded (`project.rs`).
            StmtKind::Include { .. } => Ok(()),
            StmtKind::Assign { name, value } => self.assign_stmt(s, name, value, ctx),
            StmtKind::Block(stmts) => self.block_stmt(s, stmts, ctx),
            StmtKind::Anchor { name, point, dir } => self.anchor_stmt(s, name, point, dir.as_ref(), ctx),
            StmtKind::Grid(g) => self.grid_stmt(s, g, ctx),
            StmtKind::Named { name, index, inst } => self.named_stmt(s, name, index.as_ref(), inst, ctx),

            StmtKind::Call { name, args, children, .. } => {
                self.call_stmt(s, name, args, children, ctx, None).map(|_| ())
            }
            StmtKind::For { vars, body, intersection } => self.for_stmt(s, vars, body, *intersection, ctx),
            StmtKind::Let { vars, body } => self.let_stmt(s, vars, body, ctx),
            StmtKind::If { cond, then, els } => self.if_stmt(s, cond, then, els.as_deref(), ctx),
        }
    }

    #[inline(never)]
    fn block_stmt(&mut self, s: &'a Stmt, stmts: &'a [Stmt], ctx: &Ctx<'a>) -> R<()> {
        let n = self.node(s, Op::Group, ctx);
        let c = self.sub(ctx, n, &DMat4::IDENTITY);
        self.scope_list(stmts, &c)
    }

    #[inline(never)]
    fn let_stmt(&mut self, s: &'a Stmt, vars: &'a [Assignment], body: &'a Stmt, ctx: &Ctx<'a>) -> R<()> {
        let n = self.node(s, Op::Group, ctx);
        let (env, dy) = self.let_env(vars, &ctx.env, &ctx.dyn_)?;
        let mut c = self.sub(ctx, n, &DMat4::IDENTITY);
        c.env = env;
        c.dyn_ = dy;
        self.stmt(body, &c)
    }

    #[inline(never)]
    fn if_stmt(&mut self, s: &'a Stmt, cond: &'a Expr, then: &'a Stmt, els: Option<&'a Stmt>, ctx: &Ctx<'a>) -> R<()> {
        let n = self.node(s, Op::Group, ctx);
        let t = self.expr(cond, &ctx.env, &ctx.dyn_)?.truthy();
        let c = self.sub(ctx, n, &DMat4::IDENTITY);
        match (t, els) {
            (true, _) => self.stmt(then, &c),
            (false, Some(e)) => self.stmt(e, &c),
            (false, None) => Ok(()),
        }
    }

    #[inline(never)]
    pub(crate) fn hint(&mut self, s: &Stmt, v: &Val<'a>, in_ctx: bool) {
        let text = to_hint(v);
        let literal = match (&v.v, v.origin) {
            (Value::Num(_) | Value::Bool(_), Origin::Literal(sp)) => Some(sp),
            _ => None,
        };
        let e = self.hints.entry(s.id).or_insert_with(|| HintAcc {
            span: s.span,
            texts: Vec::new(),
            value: PValue::from_val(v),
            literal,
            in_context: in_ctx,
        });
        if e.in_context && !in_ctx {
            return;
        }
        if in_ctx && !e.in_context {
            *e = HintAcc { span: s.span, texts: Vec::new(), value: PValue::from_val(v), literal, in_context: true };
        }
        if e.texts.len() < 4 && !e.texts.contains(&text) {
            e.texts.push(text);
        }
    }

    #[inline(never)]
    fn assign_stmt(&mut self, s: &'a Stmt, name: &'a Ident, value: &'a Expr, ctx: &Ctx<'a>) -> R<()> {
        if name.name.starts_with('$') {
            return Ok(());
        }
        let v = match self.lookup(&name.name, &ctx.env)? {
            Some(v) => v,
            None => self.expr(value, &ctx.env, &ctx.dyn_)?,
        };
        self.hint(s, &v, ctx.in_context());
        if let Some(l) = v.as_list()
            && (l.len() == 2 || l.len() == 3)
            && l.iter().all(|c| c.as_num().is_some())
        {
            let n = l.len();
            let (comps, has_ref) = crate::handles::comps_of(&v, n, 1.0);
            let pos = v.as_vec3().unwrap_or_default();
            let vals = (0..n).map(|i| pos[i]).collect();
            self.add_handle(crate::handles::HSpec {
                stmt: s,
                ctx,
                node: None,
                kind: HandleKind::Point,
                index: 0,
                frame: ctx.xf.world,
                pos,
                axis: None,
                base: None,
                value: vals,
                comps,
                has_ref,
                expr_span: value.span,
                def_span: v.def,
                via_var: false,
            });
        }
        Ok(())
    }

    #[inline(never)]
    pub(crate) fn let_env(&mut self, vars: &'a [Assignment], env: &Env<'a>, dy: &Dyn<'a>) -> R<(Env<'a>, Dyn<'a>)> {
        let e = env.child(dy.clone());
        let mut d = dy.clone();
        for (i, a) in vars.iter().enumerate() {
            // OpenSCAD keeps the first of several assignments to one name.
            if vars[..i].iter().any(|b| b.name.name == a.name.name) {
                self.warn(a.name.span, format!("doppelte Zuweisung an `{}` wird ignoriert", a.name.name));
                continue;
            }
            let v = self.expr(&a.value, &e, &d)?;
            if a.name.name.starts_with('$') {
                d = dyn_push(&d, vec![(&a.name.name, v)]);
                e.set_dyn(d.clone());
            } else {
                e.set(&a.name.name, v);
            }
        }
        Ok((e, d))
    }

    #[inline(never)]
    fn for_stmt(&mut self, s: &'a Stmt, vars: &'a [Assignment], body: &'a Stmt, inter: bool, ctx: &Ctx<'a>) -> R<()> {
        let op = if inter {
            Op::Boolean { op: BoolOp::Intersection, fillet: None, chamfer: None, fillet_law: None }
        } else {
            Op::Group
        };
        let n = self.node(s, op, ctx);
        let mut base = self.sub(ctx, n, &DMat4::IDENTITY);
        base.in_loop = true;
        let mut counter = 0u64;
        self.for_rec(vars, 0, body, &base, ctx.env.clone(), &mut counter)
    }

    fn for_rec(
        &mut self,
        vars: &'a [Assignment],
        i: usize,
        body: &'a Stmt,
        base: &Ctx<'a>,
        env: Env<'a>,
        counter: &mut u64,
    ) -> R<()> {
        if i == vars.len() {
            let mut c = base.clone();
            c.env = env;
            c.key = mix(base.key, *counter) & MASK53;
            *counter += 1;
            return self.stmt(body, &c);
        }
        let v = self.expr(&vars[i].value, &env, &base.dyn_)?;
        let n = self.iter_len(&v, vars[i].value.span)?;
        for k in 0..n {
            self.step(vars[i].span)?;
            let item = self.iter_item(&v, k);
            let e = env.child(base.dyn_.clone());
            e.set(&vars[i].name.name, item);
            self.for_rec(vars, i + 1, body, base, e, counter)?;
        }
        Ok(())
    }

    pub(crate) fn iter_len(&mut self, v: &Val<'a>, span: Span) -> R<usize> {
        let n = match &v.v {
            Value::Range(r) => {
                let c = r.count();
                if c > self.opts.max_list_len as f64 {
                    return self.fatal(span, format!("Bereich hat zu viele Elemente ({c})"));
                }
                c as usize
            }
            Value::List(l) => l.len(),
            Value::Str(s) => s.chars().count(),
            Value::Undef => 0,
            _ => 1,
        };
        Ok(n)
    }

    pub(crate) fn iter_item(&self, v: &Val<'a>, k: usize) -> Val<'a> {
        match &v.v {
            Value::Range(r) => Val::num(r.at(k)),
            Value::List(l) => l[k].clone(),
            Value::Str(s) => Val::str(&s.chars().nth(k).map(String::from).unwrap_or_default()),
            _ => v.clone(),
        }
    }

    // ------------------------------------------------------------ calls

    #[inline(never)]
    pub(crate) fn eval_args(&mut self, args: &'a [Arg], env: &Env<'a>, dy: &Dyn<'a>) -> R<Args<'a>> {
        let mut out = Args::default();
        for a in args {
            let v = self.expr(&a.value, env, dy)?;
            match &a.name {
                Some(n) if n.name.starts_with('$') => {
                    out.dyn_at.push(out.list.len());
                    out.dyn_vars.push((&n.name, v));
                }
                _ => out.list.push(ArgV { name: a.name.as_ref(), val: v, expr: &a.value }),
            }
        }
        Ok(out)
    }

    /// Binds call arguments to parameters in `env` (named first, then positional;
    /// missing ones get their default lazily, or `undef`). Defaults are evaluated as
    /// in OpenSCAD: in the definition environment `def_env` with the caller's `$`
    /// variables `caller_dy`, so they do not see the other parameters.
    #[inline(never)]
    pub(crate) fn bind_params(
        &mut self,
        params: &'a [Param],
        args: Vec<ArgV<'a>>,
        env: &Env<'a>,
        def_env: &Env<'a>,
        caller_dy: &Dyn<'a>,
        what: &str,
    ) {
        let mut pos = Vec::new();
        let mut named: Vec<ArgV<'a>> = Vec::new();
        for a in args {
            match a.name {
                Some(n) => {
                    if !params.iter().any(|p| p.name.name == n.name) {
                        self.warn(n.span, format!("`{what}` hat keinen Parameter `{}`", n.name));
                        continue;
                    }
                    if named.iter().any(|b| b.name.is_some_and(|m| m.name == n.name)) {
                        self.warn(n.span, format!("Argument `{}` mehrfach angegeben", n.name));
                    }
                    named.push(a);
                }
                None => pos.push(a),
            }
        }
        if pos.len() > params.len() {
            let sp = pos[params.len()].expr.span;
            self.warn(sp, format!("zu viele Argumente für `{what}` ({} statt {})", pos.len(), params.len()));
        }
        let mut pos = pos.into_iter();
        for p in params {
            let nv = named.iter().rposition(|a| a.name.is_some_and(|n| n.name == p.name.name));
            let pv = pos.next();
            if let Some(i) = nv {
                env.set(&p.name.name, named[i].val.clone());
            } else if let Some(a) = pv {
                env.set(&p.name.name, a.val);
            } else if let Some(d) = &p.default {
                env.set_default(&p.name.name, d, def_env.clone(), caller_dy.clone());
            } else {
                env.set(&p.name.name, Val::undef());
            }
        }
    }

    #[inline(never)]
    pub(crate) fn call_stmt(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        args: &'a [Arg],
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        label: Option<String>,
    ) -> R<Option<InstId>> {
        let a = self.eval_args(args, &ctx.env, &ctx.dyn_)?;
        if let Some((def, denv)) = ctx.env.find_module(&name.name) {
            return self.user_module(s, def, denv, a, children, ctx, label).map(Some);
        }
        if label.is_some() {
            self.error(name.span, format!("nur Aufrufe eigener Module sind benennbar, `{}` ist eingebaut", name.name));
        }
        self.builtin_module(s, name, a, children, ctx)?;
        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn user_module(
        &mut self,
        s: &'a Stmt,
        def: &'a ModuleDef,
        denv: Env<'a>,
        args: Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        label: Option<String>,
    ) -> R<InstId> {
        if self.mod_depth >= self.opts.max_module_depth {
            return self.too_deep(s.span, "module", &def.name.name, self.opts.max_module_depth);
        }
        let (id, node, env, dy, frame) = self.enter_module(s, def, denv, args, children, ctx, label);
        self.mod_depth += 1;
        let r = self.module_body(def, &env, dy, frame, node, ctx);
        self.mod_depth -= 1;
        r?;
        Ok(id)
    }

    /// Creates the call node, the instance and the module environment.
    #[inline(never)]
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn enter_module(
        &mut self,
        s: &'a Stmt,
        def: &'a ModuleDef,
        denv: Env<'a>,
        args: Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        label: Option<String>,
    ) -> (InstId, NodeIdx, Env<'a>, Dyn<'a>, Rc<Frame<'a>>) {
        let node = self.node(s, Op::ModuleCall { name: def.name.name.clone(), instance: label }, ctx);
        let depth = self.insts[ctx.frame.inst as usize].depth + 1;
        let id = self.new_instance(InstanceInfo {
            id: 0,
            module: def.name.name.clone(),
            label: None,
            call_stmt: Some(s.id),
            call_span: Some(s.span),
            def_stmt: Some(def.stmt),
            def_span: Some(def.span),
            node: Some(node),
            parent: Some(ctx.frame.inst),
            matrix: ctx.xf.mod_local,
            world: ctx.xf.world,
            anchors: Vec::new(),
            visible: None,
            depth,
        });
        let env = Env::new(Some(&denv), None, id, true);
        self.bind_params(&def.params, args.list, &env, &denv, &ctx.dyn_, &def.name.name);
        let nchildren = children.iter().filter(|c| is_geometric(c)).count();
        let mut dvars = args.dyn_vars;
        dvars.push(("$children", Val::num(nchildren as f64)));
        let dy = dyn_push(&ctx.dyn_, dvars);
        env.set_dyn(dy.clone());
        let frame = Rc::new(Frame {
            inst: id,
            children: Some(ChildrenRef {
                stmts: children,
                env: ctx.env.clone(),
                frame: ctx.frame.clone(),
                mod_local: ctx.xf.mod_local,
                in_loop: ctx.in_loop,
            }),
            body_end: def.body_span.end,
        });
        (id, node, env, dy, frame)
    }

    #[inline(never)]
    fn module_body(
        &mut self,
        def: &'a ModuleDef,
        env: &Env<'a>,
        dy: Dyn<'a>,
        frame: Rc<Frame<'a>>,
        node: NodeIdx,
        ctx: &Ctx<'a>,
    ) -> R<()> {
        let body_dy = self.hoist(&def.body, env, &dy)?;
        let c = Ctx {
            env: env.clone(),
            dyn_: body_dy,
            frame,
            xf: ctx.xf.rebase(&DMat4::IDENTITY),
            parent: Some(node),
            key: self.nodes[node as usize].inst_key,
            in_loop: false,
            in_children: false,
            color: ctx.color,
        };
        for st in &def.body {
            self.stmt(st, &c)?;
        }
        Ok(())
    }

    /// `children(…)` inside a module body.
    #[inline(never)]
    pub(crate) fn children_stmt(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) -> R<()> {
        let frame = ctx.frame.clone();
        let Some(ch) = frame.children.as_ref() else {
            return Ok(());
        };
        let geo: Vec<&'a Stmt> = ch.stmts.iter().filter(|c| is_geometric(c)).collect();
        let sel: Vec<usize> = match a.get(0, "index") {
            None => (0..geo.len()).collect(),
            Some(arg) => {
                let mut out = Vec::new();
                let vals: Vec<f64> = match &arg.val.v {
                    Value::Num(n) => vec![*n],
                    Value::List(l) => l.iter().filter_map(|v| v.as_num()).collect(),
                    Value::Range(r) => {
                        let n = r.count().min(self.opts.max_list_len as f64) as usize;
                        (0..n).map(|i| r.at(i)).collect()
                    }
                    _ => {
                        self.warn(arg.expr.span, "children(): Index muss Zahl, Liste oder Bereich sein");
                        vec![]
                    }
                };
                for v in vals {
                    if v >= 0.0 && (v as usize) < geo.len() {
                        out.push(v as usize);
                    } else {
                        self.warn(
                            arg.expr.span,
                            format!("children(): Index {} außerhalb ({} Kinder)", fmt_num(v), geo.len()),
                        );
                    }
                }
                out
            }
        };
        let node = self.node(s, Op::Group, ctx);
        let env = ch.env.child(ctx.dyn_.clone());
        let dy = self.hoist(ch.stmts, &env, &ctx.dyn_)?;
        let base_key = self.nodes[node as usize].inst_key;
        let xf = ctx.xf.rebase(&(ch.mod_local * ctx.xf.mod_local));
        for (k, i) in sel.into_iter().enumerate() {
            let c = Ctx {
                env: env.clone(),
                dyn_: dy.clone(),
                frame: ch.frame.clone(),
                xf: xf.clone(),
                parent: Some(node),
                key: mix(base_key, k as u64) & MASK53,
                in_loop: ch.in_loop,
                in_children: true,
                color: ctx.color,
            };
            self.stmt(geo[i], &c)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ grids, anchors, names

    pub(crate) fn declare_name(&mut self, inst: InstId, name: &'a Ident, r: NameRef, ctx: &Ctx<'a>) -> bool {
        if self.names_of(inst).iter().any(|(n, _)| *n == name.name) {
            self.error(name.span, format!("`{}` ist in diesem Modul schon deklariert", name.name));
            return false;
        }
        if ctx.env.has_local(&name.name) || self.module_scope_has_var(&ctx.env, &name.name) {
            self.error(name.span, format!("`{}` ist in diesem Modul schon eine Variable", name.name));
            return false;
        }
        self.names[inst as usize].push((&name.name, r));
        true
    }

    fn module_scope_has_var(&self, env: &Env<'a>, name: &str) -> bool {
        let mut cur = Some(env.clone());
        while let Some(e) = cur {
            let s = e.0.borrow();
            if s.vars.iter().any(|(n, _)| *n == name) {
                return true;
            }
            if s.module_root {
                return false;
            }
            cur = s.parent.clone();
        }
        false
    }

    #[inline(never)]
    fn anchor_stmt(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        point: &'a Expr,
        dir: Option<&'a Expr>,
        ctx: &Ctx<'a>,
    ) -> R<()> {
        if ctx.in_loop {
            self.error(s.span, "Anker sind in for-Schleifen nicht erlaubt");
            return Ok(());
        }
        if ctx.in_children {
            self.error(s.span, "Anker sind in den Kindern eines Modulaufrufs nicht erlaubt");
            return Ok(());
        }
        if self.file_context && ctx.frame.inst == 0 {
            self.warn(s.span, "Anker außerhalb eines Moduls ohne Wirkung");
        }
        let pv = self.expr(point, &ctx.env, &ctx.dyn_)?;
        let Some(p) = pv.as_vec3() else {
            let what = pv.type_de();
            self.error(point.span, format!("Anker braucht einen Punkt, ist {what}"));
            return Ok(());
        };
        let d = match dir {
            None => None,
            Some(de) => {
                let dv = self.expr(de, &ctx.env, &ctx.dyn_)?;
                match dv.as_vec3() {
                    Some(d) if d.length() > 0.0 => Some(crate::math::transform_dir(&ctx.xf.mod_local, d)),
                    _ => {
                        self.error(de.span, "Ankerrichtung muss ein Vektor ≠ 0 sein");
                        None
                    }
                }
            }
        };
        let inst = ctx.frame.inst as usize;
        if self.insts[inst].anchors.iter().any(|a| a.name == name.name) {
            self.error(name.span, format!("Anker `{}` ist schon deklariert", name.name));
            return Ok(());
        }
        self.insts[inst].anchors.push(AnchorInfo {
            name: name.name.clone(),
            stmt: s.id,
            span: s.span,
            point: ctx.xf.mod_local.transform_point3(p),
            dir: d,
        });
        if pv.as_list().is_some() {
            let n = pv.as_list().map_or(0, |l| l.len());
            let (comps, has_ref) = crate::handles::comps_of(&pv, n, 1.0);
            self.add_handle(crate::handles::HSpec {
                stmt: s,
                ctx,
                node: None,
                kind: HandleKind::Point,
                index: 0,
                frame: ctx.xf.world,
                pos: p,
                axis: None,
                base: None,
                value: (0..n).map(|i| p[i]).collect(),
                comps,
                has_ref,
                expr_span: point.span,
                def_span: pv.def,
                via_var: false,
            });
        }
        Ok(())
    }

    #[inline(never)]
    fn named_stmt(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        index: Option<&'a Expr>,
        inst: &'a Stmt,
        ctx: &Ctx<'a>,
    ) -> R<()> {
        if inst.modifiers.contains(&Modifier::Disable) {
            return Ok(());
        }
        let StmtKind::Call { name: mname, args, children, .. } = &inst.kind else {
            self.error(name.span, "nur Modulaufrufe sind benennbar");
            return self.stmt(inst, ctx);
        };
        if ctx.in_loop && index.is_none() {
            let n = &name.name;
            self.error(
                name.span,
                format!("benannte Instanzen sind in for-Schleifen nicht erlaubt (`{n}[i]: …` verwenden)"),
            );
            return self.stmt(inst, ctx);
        }
        if ctx.env.find_module(&mname.name).is_none() {
            if is_builtin_module(&mname.name) {
                self.error(
                    name.span,
                    format!("nur Aufrufe eigener Module sind benennbar, `{}` ist eingebaut", mname.name),
                );
            }
            return self.stmt(inst, ctx);
        }
        // `k[i]`: integer index, evaluated before the call.
        let idx = match index {
            None => None,
            Some(e) => {
                let v = self.expr(e, &ctx.env, &ctx.dyn_)?;
                match v.as_num() {
                    Some(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 => Some(n as i64),
                    _ => {
                        let shown = to_hint(&v);
                        self.error(e.span, format!("Index einer benannten Instanz muss ganzzahlig sein, ist {shown}"));
                        return self.stmt(inst, ctx);
                    }
                }
            }
        };
        let label = match idx {
            None => name.name.clone(),
            Some(i) => format!("{}[{i}]", name.name),
        };
        self.step(inst.span)?;
        let prev = self.cur_stmt.replace(inst.id);
        let r = self.call_stmt(inst, mname, args, children, ctx, Some(label.clone()));
        self.cur_stmt = prev;
        if let Some(id) = r? {
            let frame_inst = ctx.frame.inst;
            let ok = match idx {
                None => self.declare_name(frame_inst, name, NameRef::Inst(id), ctx),
                Some(i) => self.declare_member(frame_inst, name, i, id, ctx),
            };
            if ok {
                let visible = self.visible_span(s.span, ctx.frame.body_end);
                let node = self.insts[id as usize].node;
                let i = &mut self.insts[id as usize];
                i.label = Some(label.clone());
                i.visible = Some(visible);
                // Named instances at the top level of the file are the parts (Plan 12.9).
                if self.file_context
                    && frame_inst == 0
                    && let Some(node) = node
                {
                    self.parts.push(PartInfo { name: label, inst: id, node, stmt: s.id, span: s.span });
                }
            }
        }
        Ok(())
    }

    /// Visibility of a name declared by `decl` up to `body_end` of its module body. A name
    /// declared in an included file is visible to the end of that file's text.
    pub(crate) fn visible_span(&self, decl: Span, body_end: u32) -> Span {
        let end = if cad_syntax::file_of(body_end) == decl.file() && body_end >= decl.end {
            body_end
        } else {
            let len = self.proj.file_of(decl).map_or(0, |f| f.text.len() as u32);
            cad_syntax::file_base(decl.file()) + len
        };
        Span { start: decl.end, end }
    }

    /// Declares `name[index]` (a member of an instance family).
    fn declare_member(&mut self, inst: InstId, name: &'a Ident, index: i64, id: InstId, ctx: &Ctx<'a>) -> bool {
        let existing = self.names_of(inst).iter().rev().find(|(n, _)| *n == name.name).map(|(_, r)| *r);
        match existing {
            Some(NameRef::Family(f)) => {
                let fam = &mut self.families[f as usize];
                if fam.contains_key(&index) {
                    self.error(name.span, format!("`{}[{index}]` ist in diesem Modul schon deklariert", name.name));
                    return false;
                }
                fam.insert(index, id);
                true
            }
            Some(_) => {
                self.error(name.span, format!("`{}` ist in diesem Modul schon deklariert", name.name));
                false
            }
            None => {
                let f = self.families.len() as u32;
                if !self.declare_name(inst, name, NameRef::Family(f), ctx) {
                    return false;
                }
                self.families.push(BTreeMap::from([(index, id)]));
                true
            }
        }
    }

    pub(crate) fn rand(&mut self) -> f64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    pub(crate) fn is_in_context(&self, env: &Env<'a>) -> bool {
        env.inst() == 0
    }
}

pub(crate) fn find_module_path<'a>(stmts: &'a [Stmt], name: &str) -> Option<Vec<&'a ModuleDef>> {
    let want: Vec<&str> = name.split("::").collect();
    let mut path = Vec::new();
    fn rec<'a>(stmts: &'a [Stmt], want: &[&str], path: &mut Vec<&'a ModuleDef>) -> bool {
        for s in stmts {
            let mut found = false;
            s.walk_shallow(&mut |m: &'a ModuleDef| {
                if found {
                    return;
                }
                path.push(m);
                let names: Vec<&str> = path.iter().map(|m| m.name.name.as_str()).collect();
                if names.ends_with(want) || rec(&m.body, want, path) {
                    found = true;
                    return;
                }
                path.pop();
            });
            if found {
                return true;
            }
        }
        false
    }
    if rec(stmts, &want, &mut path) { Some(path) } else { None }
}

/// Visits module definitions directly in a statement (not inside other modules).
trait WalkShallow {
    fn walk_shallow<'a>(&'a self, f: &mut dyn FnMut(&'a ModuleDef));
}

impl WalkShallow for Stmt {
    fn walk_shallow<'a>(&'a self, f: &mut dyn FnMut(&'a ModuleDef)) {
        match &self.kind {
            StmtKind::Module(m) => f(m),
            StmtKind::Block(s) => s.iter().for_each(|s| s.walk_shallow(f)),
            StmtKind::Call { children, .. } => children.iter().for_each(|s| s.walk_shallow(f)),
            StmtKind::For { body, .. } | StmtKind::Let { body, .. } => body.walk_shallow(f),
            StmtKind::If { then, els, .. } => {
                then.walk_shallow(f);
                if let Some(e) = els {
                    e.walk_shallow(f);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn is_builtin_module(name: &str) -> bool {
    crate::builtin_mod::BUILTIN_MODULES.contains(&name)
}
