//! Expression evaluation: variables, operators, calls (with tail calls), list
//! comprehensions, indexing, members.

use std::rc::Rc;

use cad_syntax::Span;
use cad_syntax::ast::*;

use crate::env::{Binding, Dyn, Env, NO_INST, dyn_lookup, dyn_push};
use crate::eval::{Args, Ev, NameRef, R};
use crate::ops::{binop, unop};
use crate::output::Echo;
use crate::value::{Closure, Origin, Range, Val, Value, fmt_num, to_display, to_hint};

enum Tail<'a> {
    Val(Val<'a>),
    Call { params: &'a [Param], body: &'a Expr, env: Env<'a>, name: Option<&'a str>, args: Args<'a> },
}

impl<'a> Ev<'a> {
    /// Evaluates an expression. Kept small (rare cases are out of line) because it is
    /// on every recursion path: its stack frame limits the recursion depth.
    pub(crate) fn expr(&mut self, e: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        self.step(e.span)?;
        let mut v = match &e.kind {
            ExprKind::Number(n) => Val::with_origin(Value::Num(*n), Origin::Literal(e.span)),
            ExprKind::Ident(name) => return self.ident(name, e.span, env, dy),
            ExprKind::Ternary(c, a, b) => {
                let t = self.cond(c, env, dy)?;
                return self.expr(if t { a } else { b }, env, dy);
            }
            ExprKind::Call { callee, args } => self.call_expr(e, callee, args, env, dy)?,
            ExprKind::Binary(op, a, b) => self.binary_expr(*op, a, b, e.span, env, dy)?,
            ExprKind::Index { base, items } => return self.index_expr(e, base, items, env, dy),
            ExprKind::Member { base, name } => return self.member_expr(e, base, name, env, dy),
            _ => return self.expr_other(e, env, dy),
        };
        v.def = Some(e.span);
        Ok(v)
    }

    #[inline(never)]
    fn cond(&mut self, c: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<bool> {
        Ok(self.expr(c, env, dy)?.truthy())
    }

    #[inline(never)]
    fn binary_expr(
        &mut self,
        op: BinOp,
        a: &'a Expr,
        b: &'a Expr,
        span: Span,
        env: &Env<'a>,
        dy: &Dyn<'a>,
    ) -> R<Val<'a>> {
        Ok(match op {
            BinOp::And => Val::bool(self.cond(a, env, dy)? && self.cond(b, env, dy)?),
            BinOp::Or => Val::bool(self.cond(a, env, dy)? || self.cond(b, env, dy)?),
            _ => {
                let l = self.expr(a, env, dy)?;
                let r = self.expr(b, env, dy)?;
                let res = binop(op, &l, &r);
                if let Err(msg) = &res
                    && (l.is_special() || r.is_special())
                {
                    self.error(
                        span,
                        format!("Typfehler: {msg}; Raster, Rasterlinien/-ebenen und Instanzen sind keine Werte"),
                    );
                    return Ok(Val::undef());
                }
                self.op_result(res, span)
            }
        })
    }

    #[inline(never)]
    fn expr_other(&mut self, e: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        let mut v = match &e.kind {
            ExprKind::Bool(b) => Val::with_origin(Value::Bool(*b), Origin::Literal(e.span)),
            ExprKind::Str(s) => Val::str(s),
            ExprKind::Unary(op, x) => {
                if let (UnOp::Neg | UnOp::Plus, ExprKind::Number(n)) = (op, &x.kind) {
                    let n = if *op == UnOp::Neg { -*n } else { *n };
                    Val::with_origin(Value::Num(n), Origin::Literal(e.span))
                } else {
                    let a = self.expr(x, env, dy)?;
                    let res = unop(*op, &a);
                    self.op_result(res, e.span)
                }
            }
            ExprKind::Vector(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                self.vec_elems(elems, env, dy, &mut out, e.span)?;
                Val::list(out)
            }
            ExprKind::Range { start, step, end } => self.range_expr(e, start, step.as_deref(), end, env, dy)?,
            ExprKind::FuncLit { params, body } => {
                self.captured.push(env.weak());
                Val::new(Value::Func(Rc::new(Closure { params, body, env: env.clone(), name: None })))
            }
            ExprKind::Let { vars, body } => {
                let (e2, d2) = self.let_env(vars, env, dy)?;
                return self.expr(body, &e2, &d2);
            }
            ExprKind::Assert { args, body } => {
                self.assert_check(e.span, args, env, dy)?;
                match body {
                    Some(b) => return self.expr(b, env, dy),
                    None => Val::undef(),
                }
            }
            ExprKind::Echo { args, body } => {
                let a = self.eval_args(args, env, dy)?;
                self.echo(e.span, &a, env);
                match body {
                    Some(b) => return self.expr(b, env, dy),
                    None => Val::undef(),
                }
            }
            _ => Val::undef(),
        };
        v.def = Some(e.span);
        Ok(v)
    }

    fn op_result(&mut self, r: Result<Val<'a>, String>, span: Span) -> Val<'a> {
        match r {
            Ok(v) => v,
            Err(msg) => {
                self.warn(span, msg);
                Val::undef()
            }
        }
    }

    #[inline(never)]
    pub(crate) fn echo(&mut self, span: Span, a: &Args<'a>, env: &Env<'a>) {
        let mut parts: Vec<String> = Vec::with_capacity(a.list.len() + a.dyn_vars.len());
        let mut dyn_vars = a.dyn_vars.iter().zip(&a.dyn_at).peekable();
        for (i, x) in a.list.iter().enumerate() {
            while let Some(((n, v), _)) = dyn_vars.next_if(|(_, at)| **at <= i) {
                parts.push(format!("{n} = {}", to_display(v)));
            }
            parts.push(match x.name {
                Some(n) => format!("{} = {}", n.name, to_display(&x.val)),
                None => to_display(&x.val),
            });
        }
        parts.extend(dyn_vars.map(|((n, v), _)| format!("{n} = {}", to_display(v))));
        let in_context = self.is_in_context(env);
        self.echoes.push(Echo { stmt: self.cur_stmt, span, text: parts.join(", "), in_context });
    }

    #[inline(never)]
    pub(crate) fn assert_check(&mut self, span: Span, args: &'a [Arg], env: &Env<'a>, dy: &Dyn<'a>) -> R<()> {
        let a = self.eval_args(args, env, dy)?;
        self.assert_args(span, &a)
    }

    #[inline(never)]
    fn ident(&mut self, name: &'a str, span: Span, env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        if name.starts_with('$') {
            if let Some(v) = dyn_lookup(dy, name) {
                return Ok(v);
            }
            return Ok(match name {
                "$fn" | "$t" | "$children" | "$fillet" | "$chamfer" => {
                    Val::num(if name == "$t" { self.opts.time } else { 0.0 })
                }
                "$fa" => Val::num(12.0),
                "$fs" => Val::num(2.0),
                "$preview" => Val::bool(self.opts.preview),
                "$tol" | "$vpr" | "$vpt" | "$vpd" | "$vpf" | "$parent_modules" => Val::undef(),
                _ => {
                    self.warn(span, format!("unbekannte Variable `{name}`"));
                    Val::undef()
                }
            });
        }
        match self.lookup(name, env)? {
            Some(v) => Ok(v),
            None => {
                self.warn(span, format!("unbekannte Variable `{name}`"));
                Ok(Val { def: Some(span), ..Val::undef() })
            }
        }
    }

    /// Looks up a variable (forcing lazy bindings), grid or named instance.
    #[inline(never)]
    pub(crate) fn lookup(&mut self, name: &str, env: &Env<'a>) -> R<Option<Val<'a>>> {
        enum Hit<'a> {
            Done(Val<'a>),
            Lazy(&'a Expr),
            Default(&'a Expr, Env<'a>, Dyn<'a>),
            Cycle,
            Miss,
        }
        let mut cur = env.clone();
        // Grids and named instances are only visible in the module instance the code
        // belongs to (the first module root on the chain), not through definition
        // environments of enclosing modules or the file.
        let mut names_checked = false;
        loop {
            let (hit, inst, root, parent) = {
                let sc = cur.0.borrow();
                let hit = match sc.vars.iter().rev().find(|(n, _)| *n == name) {
                    Some((_, Binding::Done(v))) => Hit::Done(v.clone()),
                    Some((_, Binding::Lazy(e))) => Hit::Lazy(e),
                    Some((_, Binding::Default(e, de, dd))) => Hit::Default(e, de.clone(), dd.clone()),
                    Some((_, Binding::InProgress)) => Hit::Cycle,
                    None => Hit::Miss,
                };
                (hit, sc.inst, sc.module_root, sc.parent.clone())
            };
            match hit {
                Hit::Done(v) => return Ok(Some(v)),
                Hit::Lazy(e) => {
                    let dy = cur.dyn_();
                    return self.force(&cur, name, e, &cur, &dy).map(Some);
                }
                Hit::Default(e, de, dd) => return self.force(&cur, name, e, &de, &dd).map(Some),
                Hit::Cycle => {
                    let sp = self.cur_span;
                    self.warn(sp, format!("rekursive Definition von `{name}`"));
                    return Ok(Some(Val::undef()));
                }
                Hit::Miss => {}
            }
            let check_names = root && !names_checked;
            names_checked |= root;
            if check_names
                && inst != NO_INST
                && let Some((_, r)) = self.names_of(inst).iter().rev().find(|(n, _)| *n == name)
            {
                return Ok(Some(Val::new(match *r {
                    NameRef::Grid(g) => Value::Grid(g),
                    NameRef::Inst(i) => Value::Instance(i),
                    NameRef::Family(f) => Value::Family(f),
                })));
            }
            match parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        if name == "PI" {
            return Ok(Some(Val::num(std::f64::consts::PI)));
        }
        Ok(None)
    }

    #[inline(never)]
    /// Evaluates the lazy binding `name` of `env` in `eval_env` / `dy` and stores it.
    fn force(&mut self, env: &Env<'a>, name: &str, e: &'a Expr, eval_env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        let set = |env: &Env<'a>, b: Binding<'a>| {
            let mut s = env.0.borrow_mut();
            if let Some(slot) = s.vars.iter_mut().rev().find(|(n, _)| *n == name) {
                slot.1 = b;
            }
        };
        set(env, Binding::InProgress);
        let r = self.expr(e, eval_env, dy);
        match r {
            Ok(v) => {
                set(env, Binding::Done(v.clone()));
                Ok(v)
            }
            Err(f) => {
                set(env, Binding::Done(Val::undef()));
                Err(f)
            }
        }
    }

    // ------------------------------------------------------------ calls

    #[inline(never)]
    fn call_expr(&mut self, e: &'a Expr, callee: &'a Expr, args: &'a [Arg], env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        // Fast path (user functions) first: this frame is on every recursion.
        if let ExprKind::Ident(name) = &callee.kind
            && let Some((f, fenv)) = env.find_func(name)
        {
            let a = self.eval_args(args, env, dy)?;
            return self.call_fn(&f.params, &f.body, fenv, Some(&f.name.name), a, e.span, dy);
        }
        self.call_expr_other(e, callee, args, env, dy)
    }

    #[inline(never)]
    fn call_expr_other(
        &mut self,
        e: &'a Expr,
        callee: &'a Expr,
        args: &'a [Arg],
        env: &Env<'a>,
        dy: &Dyn<'a>,
    ) -> R<Val<'a>> {
        if let ExprKind::Ident(name) = &callee.kind {
            if !name.starts_with('$')
                && let Some(v) = self.lookup(name, env)?
                && let Value::Func(c) = &v.v
            {
                let c = c.clone();
                let a = self.eval_args(args, env, dy)?;
                return self.call_fn(c.params, c.body, c.env.clone(), Some(name), a, e.span, dy);
            }
            let a = self.eval_args(args, env, dy)?;
            if let Some(v) = self.builtin_fn(name, &a, e, env)? {
                return Ok(v);
            }
            self.warn(callee.span, format!("unbekannte Funktion `{name}`"));
            return Ok(Val::undef());
        }
        let f = self.expr(callee, env, dy)?;
        let a = self.eval_args(args, env, dy)?;
        match &f.v {
            Value::Func(c) => {
                let c = c.clone();
                self.call_fn(c.params, c.body, c.env.clone(), c.name, a, e.span, dy)
            }
            _ => {
                let t = f.type_de();
                self.warn(callee.span, format!("{t} ist nicht aufrufbar"));
                Ok(Val::undef())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn call_fn(
        &mut self,
        params: &'a [Param],
        body: &'a Expr,
        fenv: Env<'a>,
        name: Option<&'a str>,
        args: Args<'a>,
        span: Span,
        dy: &Dyn<'a>,
    ) -> R<Val<'a>> {
        if self.fn_depth >= self.opts.max_function_depth {
            return self.too_deep(span, "function", name.unwrap_or("Funktionsliteral"), self.opts.max_function_depth);
        }
        self.check_stack(span)?;
        self.fn_depth += 1;
        let r = self.call_loop(params, body, fenv, name, args, dy.clone());
        self.fn_depth -= 1;
        r
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn call_loop(
        &mut self,
        mut params: &'a [Param],
        mut body: &'a Expr,
        mut fenv: Env<'a>,
        mut name: Option<&'a str>,
        mut args: Args<'a>,
        mut dy: Dyn<'a>,
    ) -> R<Val<'a>> {
        loop {
            let fdy = dyn_push(&dy, std::mem::take(&mut args.dyn_vars));
            let env = Env::new(Some(&fenv), fdy.clone(), fenv.inst(), false);
            self.bind_params(params, std::mem::take(&mut args.list), &env, &fenv, &dy, name.unwrap_or("function"));
            match self.tail(body, &env, &fdy)? {
                Tail::Val(mut v) => {
                    v.def = None;
                    return Ok(v);
                }
                Tail::Call { params: p, body: b, env: e, name: n, args: a } => {
                    params = p;
                    body = b;
                    fenv = e;
                    name = n;
                    args = a;
                    dy = fdy;
                }
            }
        }
    }

    /// Evaluates an expression in tail position; a call to a user function there is
    /// returned instead of evaluated (no native recursion for tail calls).
    fn tail(&mut self, e: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<Tail<'a>> {
        match &e.kind {
            ExprKind::Ternary(c, a, b) => {
                self.step(e.span)?;
                let c = self.expr(c, env, dy)?;
                self.tail(if c.truthy() { a } else { b }, env, dy)
            }
            ExprKind::Let { vars, body } => {
                self.step(e.span)?;
                let (e2, d2) = self.let_env(vars, env, dy)?;
                self.tail(body, &e2, &d2)
            }
            ExprKind::Assert { args, body: Some(b) } => {
                self.step(e.span)?;
                self.assert_check(e.span, args, env, dy)?;
                self.tail(b, env, dy)
            }
            ExprKind::Echo { args, body: Some(b) } => {
                self.step(e.span)?;
                let a = self.eval_args(args, env, dy)?;
                self.echo(e.span, &a, env);
                self.tail(b, env, dy)
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(name) = &callee.kind {
                    if let Some((f, fenv)) = env.find_func(name) {
                        self.step(e.span)?;
                        let a = self.eval_args(args, env, dy)?;
                        return Ok(Tail::Call {
                            params: &f.params,
                            body: &f.body,
                            env: fenv,
                            name: Some(&f.name.name),
                            args: a,
                        });
                    }
                    if !name.starts_with('$')
                        && let Some(v) = self.lookup(name, env)?
                        && let Value::Func(c) = &v.v
                    {
                        self.step(e.span)?;
                        let c = c.clone();
                        let a = self.eval_args(args, env, dy)?;
                        return Ok(Tail::Call {
                            params: c.params,
                            body: c.body,
                            env: c.env.clone(),
                            name: Some(name),
                            args: a,
                        });
                    }
                }
                Ok(Tail::Val(self.expr(e, env, dy)?))
            }
            _ => Ok(Tail::Val(self.expr(e, env, dy)?)),
        }
    }

    // ------------------------------------------------------------ lists

    fn push_checked(&mut self, out: &mut Vec<Val<'a>>, v: Val<'a>, span: Span) -> R<()> {
        if out.len() >= self.opts.max_list_len {
            let max = self.opts.max_list_len;
            return self.fatal(span, format!("Liste hat mehr als {max} Elemente"));
        }
        out.push(v);
        Ok(())
    }

    #[inline(never)]
    fn vec_elems(
        &mut self,
        elems: &'a [VecElem],
        env: &Env<'a>,
        dy: &Dyn<'a>,
        out: &mut Vec<Val<'a>>,
        span: Span,
    ) -> R<()> {
        for el in elems {
            self.vec_elem(el, env, dy, out, span)?;
        }
        Ok(())
    }

    fn vec_elem(&mut self, el: &'a VecElem, env: &Env<'a>, dy: &Dyn<'a>, out: &mut Vec<Val<'a>>, span: Span) -> R<()> {
        match el {
            VecElem::Expr(e) => {
                let v = self.expr(e, env, dy)?;
                self.push_checked(out, v, span)
            }
            VecElem::For { vars, body, span: sp } => self.comp_for(vars, 0, body, env.clone(), dy, out, *sp),
            VecElem::ForC { init, cond, update, body, span: sp } => {
                let (mut e, d) = self.let_env(init, env, dy)?;
                loop {
                    self.step(*sp)?;
                    if !self.expr(cond, &e, &d)?.truthy() {
                        break;
                    }
                    self.vec_elem(body, &e, &d, out, *sp)?;
                    // Loop variables carry over; updates are applied in order (each
                    // sees the previous ones, as in OpenSCAD).
                    let next = env.child(d.clone());
                    for a in init {
                        if let Some(v) = self.lookup(&a.name.name, &e)? {
                            next.set(&a.name.name, v);
                        }
                    }
                    for a in update {
                        let v = self.expr(&a.value, &next, &d)?;
                        next.set(&a.name.name, v);
                    }
                    e = next;
                }
                Ok(())
            }
            VecElem::If { cond, then, els, .. } => {
                if self.expr(cond, env, dy)?.truthy() {
                    self.vec_elem(then, env, dy, out, span)
                } else if let Some(e) = els {
                    self.vec_elem(e, env, dy, out, span)
                } else {
                    Ok(())
                }
            }
            VecElem::Let { vars, body, .. } => {
                let (e, d) = self.let_env(vars, env, dy)?;
                self.vec_elem(body, &e, &d, out, span)
            }
            VecElem::Each(inner, sp) => {
                let mut tmp = Vec::new();
                self.vec_elem(inner, env, dy, &mut tmp, *sp)?;
                for v in tmp {
                    match &v.v {
                        Value::List(l) => {
                            for x in l.iter() {
                                self.push_checked(out, x.clone(), *sp)?;
                            }
                        }
                        Value::Range(_) | Value::Str(_) => {
                            let n = self.iter_len(&v, *sp)?;
                            for k in 0..n {
                                let x = self.iter_item(&v, k);
                                self.push_checked(out, x, *sp)?;
                            }
                        }
                        // `each undef` adds nothing.
                        Value::Undef => {}
                        _ => self.push_checked(out, v, *sp)?,
                    }
                }
                Ok(())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn comp_for(
        &mut self,
        vars: &'a [Assignment],
        i: usize,
        body: &'a VecElem,
        env: Env<'a>,
        dy: &Dyn<'a>,
        out: &mut Vec<Val<'a>>,
        span: Span,
    ) -> R<()> {
        if i == vars.len() {
            return self.vec_elem(body, &env, dy, out, span);
        }
        let v = self.expr(&vars[i].value, &env, dy)?;
        let n = self.iter_len(&v, vars[i].value.span)?;
        for k in 0..n {
            self.step(span)?;
            let item = self.iter_item(&v, k);
            let e = env.child(dy.clone());
            e.set(&vars[i].name.name, item);
            self.comp_for(vars, i + 1, body, e, dy, out, span)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn range_expr(
        &mut self,
        e: &'a Expr,
        start: &'a Expr,
        step: Option<&'a Expr>,
        end: &'a Expr,
        env: &Env<'a>,
        dy: &Dyn<'a>,
    ) -> R<Val<'a>> {
        let s = self.expr(start, env, dy)?;
        let st = match step {
            Some(x) => Some(self.expr(x, env, dy)?),
            None => None,
        };
        let en = self.expr(end, env, dy)?;
        let (Some(mut a), Some(mut b)) = (s.as_num(), en.as_num()) else {
            self.warn(e.span, "Bereichsgrenzen müssen Zahlen sein");
            return Ok(Val::undef());
        };
        let stepv = match &st {
            Some(v) => match v.as_num() {
                Some(n) => n,
                None => {
                    self.warn(e.span, "Bereichsschritt muss eine Zahl sein");
                    return Ok(Val::undef());
                }
            },
            None => {
                if a > b {
                    self.warn(e.span, "[Anfang:Ende] mit Anfang > Ende ist veraltet, wird vertauscht");
                    std::mem::swap(&mut a, &mut b);
                }
                1.0
            }
        };
        Ok(Val::new(Value::Range(Range { start: a, step: stepv, end: b })))
    }

    // ------------------------------------------------------------ index / member

    #[inline(never)]
    fn index_expr(
        &mut self,
        e: &'a Expr,
        base: &'a Expr,
        items: &'a [IndexItem],
        env: &Env<'a>,
        dy: &Dyn<'a>,
    ) -> R<Val<'a>> {
        let b = self.expr(base, env, dy)?;
        if let Value::Grid(g) = b.v {
            return self.grid_access(e, g, items, env, dy);
        }
        let undef = Val { def: Some(e.span), ..Val::undef() };
        let item = match items {
            [IndexItem::Expr(x)] => x,
            [IndexItem::Wild(sp)] => {
                self.error(*sp, "`*` nur in Rasterzugriffen erlaubt");
                return Ok(undef);
            }
            _ => {
                self.warn(e.span, "nur ein Index erlaubt");
                return Ok(undef);
            }
        };
        let iv = self.expr(item, env, dy)?;
        if let Value::Family(f) = b.v {
            // `k[i]`: member of a family of named instances.
            let member = match iv.as_num() {
                Some(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 => {
                    self.families[f as usize].get(&(n as i64)).copied().ok_or(n)
                }
                Some(n) => Err(n),
                None => Err(f64::NAN),
            };
            return Ok(match member {
                Ok(id) => Val::new(Value::Instance(id)),
                Err(n) => {
                    let base_txt = self.text(base.span);
                    let shown = if n.is_nan() { to_hint(&iv) } else { fmt_num(n) };
                    self.warn(e.span, format!("`{base_txt}[{shown}]` ist keine benannte Instanz"));
                    undef
                }
            });
        }
        let Some(i) = iv.as_num() else {
            return Ok(undef);
        };
        // OpenSCAD truncates the index toward zero first (`v[-0.5]` is `v[0]`).
        let i = i.trunc();
        if !i.is_finite() || i < 0.0 {
            return Ok(undef);
        }
        let i = i as usize;
        Ok(match &b.v {
            Value::List(l) => match l.get(i) {
                Some(x) => {
                    let mut x = x.clone();
                    if x.def.is_none() {
                        x.def = Some(e.span);
                    }
                    x
                }
                None => undef,
            },
            Value::Str(s) => match s.chars().nth(i) {
                Some(c) => Val { def: Some(e.span), ..Val::str(&c.to_string()) },
                None => undef,
            },
            Value::Range(r) => match i {
                0 => Val::num(r.start),
                1 => Val::num(r.step),
                2 => Val::num(r.end),
                _ => undef,
            },
            _ => undef,
        })
    }

    #[inline(never)]
    fn member_expr(&mut self, e: &'a Expr, base: &'a Expr, name: &'a Ident, env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        let b = self.expr(base, env, dy)?;
        let in_ctx = self.is_in_context(env);
        match &b.v {
            Value::Grid(g) => Ok(self.plane_member(*g, name, e.span, in_ctx)),
            Value::Instance(i) => Ok(self.anchor_member(*i, name, e.span, in_ctx)),
            Value::Family(_) => {
                let b = self.text(base.span);
                self.warn(e.span, format!("`{b}` ist eine Liste benannter Instanzen: `{b}[i].{}` verwenden", name.name));
                Ok(Val::undef())
            }
            Value::List(l) => {
                let idx = match name.name.as_str() {
                    "x" => 0,
                    "y" => 1,
                    "z" => 2,
                    _ => return Ok(Val::undef()),
                };
                Ok(l.get(idx).cloned().unwrap_or_else(Val::undef))
            }
            _ => Ok(Val::undef()),
        }
    }
}
