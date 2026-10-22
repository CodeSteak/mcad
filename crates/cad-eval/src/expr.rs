//! Expression evaluation: variables, operators, calls (with tail calls), list
//! comprehensions, indexing, members.

use std::rc::Rc;

use cad_syntax::Span;
use cad_syntax::ast::*;

use crate::env::{Binding, Dyn, Env, NO_INST, dyn_lookup, dyn_push};
use crate::eval::{Args, Ev, NameRef, R};
use crate::ops::{binop, unop};
use crate::output::Echo;
use crate::value::{Closure, Origin, Range, Val, Value, to_display};

enum Tail<'a> {
    Val(Val<'a>),
    Call { params: &'a [Param], body: &'a Expr, env: Env<'a>, name: Option<&'a str>, args: Args<'a> },
}

impl<'a> Ev<'a> {
    pub(crate) fn expr(&mut self, e: &'a Expr, env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        self.step(e.span)?;
        let mut v = match &e.kind {
            ExprKind::Number(n) => Val::with_origin(Value::Num(*n), Origin::Literal(e.span)),
            ExprKind::Bool(b) => Val::with_origin(Value::Bool(*b), Origin::Literal(e.span)),
            ExprKind::Str(s) => Val::str(s),
            ExprKind::Undef => Val::undef(),
            ExprKind::Ident(name) => return self.ident(name, e.span, env, dy),
            ExprKind::Unary(op, x) => {
                if let (UnOp::Neg | UnOp::Plus, ExprKind::Number(n)) = (op, &x.kind) {
                    let n = if *op == UnOp::Neg { -*n } else { *n };
                    Val::with_origin(Value::Num(n), Origin::Literal(e.span))
                } else {
                    let a = self.expr(x, env, dy)?;
                    self.op_result(unop(*op, &a), e.span)
                }
            }
            ExprKind::Binary(op, a, b) => match op {
                BinOp::And => {
                    let l = self.expr(a, env, dy)?;
                    Val::bool(l.truthy() && self.expr(b, env, dy)?.truthy())
                }
                BinOp::Or => {
                    let l = self.expr(a, env, dy)?;
                    Val::bool(l.truthy() || self.expr(b, env, dy)?.truthy())
                }
                _ => {
                    let l = self.expr(a, env, dy)?;
                    let r = self.expr(b, env, dy)?;
                    self.op_result(binop(*op, &l, &r), e.span)
                }
            },
            ExprKind::Ternary(c, a, b) => {
                let c = self.expr(c, env, dy)?;
                return if c.truthy() { self.expr(a, env, dy) } else { self.expr(b, env, dy) };
            }
            ExprKind::Call { callee, args } => self.call_expr(e, callee, args, env, dy)?,
            ExprKind::Index { base, items } => return self.index_expr(e, base, items, env, dy),
            ExprKind::Member { base, name } => return self.member_expr(e, base, name, env, dy),
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
            ExprKind::Error => Val::undef(),
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

    pub(crate) fn echo(&mut self, span: Span, a: &Args<'a>, env: &Env<'a>) {
        let parts: Vec<String> = a
            .list
            .iter()
            .map(|x| match x.name {
                Some(n) => format!("{} = {}", n.name, to_display(&x.val)),
                None => to_display(&x.val),
            })
            .chain(a.dyn_vars.iter().map(|(n, v)| format!("{n} = {}", to_display(v))))
            .collect();
        let in_context = self.is_in_context(env);
        self.echoes.push(Echo { stmt: self.cur_stmt, span, text: parts.join(", "), in_context });
    }

    pub(crate) fn assert_check(&mut self, span: Span, args: &'a [Arg], env: &Env<'a>, dy: &Dyn<'a>) -> R<()> {
        let a = self.eval_args(args, env, dy)?;
        self.assert_args(span, &a)
    }

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
                    self.warn(span, format!("unknown variable `{name}`"));
                    Val::undef()
                }
            });
        }
        match self.lookup(name, env)? {
            Some(v) => Ok(v),
            None => {
                self.warn(span, format!("unknown variable `{name}`"));
                Ok(Val { def: Some(span), ..Val::undef() })
            }
        }
    }

    /// Looks up a variable (forcing lazy bindings), grid or named instance.
    pub(crate) fn lookup(&mut self, name: &str, env: &Env<'a>) -> R<Option<Val<'a>>> {
        enum Hit<'a> {
            Done(Val<'a>),
            Lazy(&'a Expr),
            Cycle,
            Miss,
        }
        let mut cur = env.clone();
        loop {
            let (hit, inst, root, parent) = {
                let sc = cur.0.borrow();
                let hit = match sc.vars.iter().rev().find(|(n, _)| *n == name) {
                    Some((_, Binding::Done(v))) => Hit::Done(v.clone()),
                    Some((_, Binding::Lazy(e))) => Hit::Lazy(e),
                    Some((_, Binding::InProgress)) => Hit::Cycle,
                    None => Hit::Miss,
                };
                (hit, sc.inst, sc.module_root, sc.parent.clone())
            };
            match hit {
                Hit::Done(v) => return Ok(Some(v)),
                Hit::Lazy(e) => return self.force(&cur, name, e).map(Some),
                Hit::Cycle => {
                    let sp = self.cur_span;
                    self.warn(sp, format!("recursive definition of `{name}`"));
                    return Ok(Some(Val::undef()));
                }
                Hit::Miss => {}
            }
            if root
                && inst != NO_INST
                && let Some((_, r)) = self.names_of(inst).iter().rev().find(|(n, _)| *n == name)
            {
                return Ok(Some(Val::new(match *r {
                    NameRef::Grid(g) => Value::Grid(g),
                    NameRef::Inst(i) => Value::Instance(i),
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

    fn force(&mut self, env: &Env<'a>, name: &str, e: &'a Expr) -> R<Val<'a>> {
        let set = |env: &Env<'a>, b: Binding<'a>| {
            let mut s = env.0.borrow_mut();
            if let Some(slot) = s.vars.iter_mut().rev().find(|(n, _)| *n == name) {
                slot.1 = b;
            }
        };
        set(env, Binding::InProgress);
        let dy = env.dyn_();
        let r = self.expr(e, env, &dy);
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

    fn call_expr(&mut self, e: &'a Expr, callee: &'a Expr, args: &'a [Arg], env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        if let ExprKind::Ident(name) = &callee.kind {
            if let Some((f, fenv)) = env.find_func(name) {
                let a = self.eval_args(args, env, dy)?;
                return self.call_fn(&f.params, &f.body, fenv, Some(&f.name.name), a, e.span, dy);
            }
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
            self.warn(callee.span, format!("unknown function `{name}`"));
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
                let t = f.type_name();
                self.warn(callee.span, format!("cannot call a value of type {t}"));
                Ok(Val::undef())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
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
            let max = self.opts.max_function_depth;
            let n = name.unwrap_or("function literal");
            return self.fatal(span, format!("recursion too deep: function `{n}` called more than {max} levels deep"));
        }
        self.check_stack(span)?;
        self.fn_depth += 1;
        let r = self.call_loop(params, body, fenv, name, args, span, dy.clone());
        self.fn_depth -= 1;
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn call_loop(
        &mut self,
        mut params: &'a [Param],
        mut body: &'a Expr,
        mut fenv: Env<'a>,
        mut name: Option<&'a str>,
        mut args: Args<'a>,
        span: Span,
        mut dy: Dyn<'a>,
    ) -> R<Val<'a>> {
        loop {
            let fdy = dyn_push(&dy, std::mem::take(&mut args.dyn_vars));
            let env = Env::new(Some(&fenv), fdy.clone(), fenv.inst(), false);
            self.bind_params(params, std::mem::take(&mut args.list), &env, name.unwrap_or("function"), span);
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
                        return Ok(Tail::Call { params: &f.params, body: &f.body, env: fenv, name: Some(&f.name.name), args: a });
                    }
                    if !name.starts_with('$')
                        && let Some(v) = self.lookup(name, env)?
                        && let Value::Func(c) = &v.v
                    {
                        self.step(e.span)?;
                        let c = c.clone();
                        let a = self.eval_args(args, env, dy)?;
                        return Ok(Tail::Call { params: c.params, body: c.body, env: c.env.clone(), name: Some(name), args: a });
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
            return self.fatal(span, format!("list has more than {max} elements"));
        }
        out.push(v);
        Ok(())
    }

    fn vec_elems(&mut self, elems: &'a [VecElem], env: &Env<'a>, dy: &Dyn<'a>, out: &mut Vec<Val<'a>>, span: Span) -> R<()> {
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
                    let next = env.child(d.clone());
                    for a in update {
                        let v = self.expr(&a.value, &e, &d)?;
                        next.set(&a.name.name, v);
                    }
                    // Variables not updated keep their values.
                    for a in init {
                        if !next.has_local(&a.name.name)
                            && let Some(v) = self.lookup(&a.name.name, &e)?
                        {
                            next.set(&a.name.name, v);
                        }
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
            self.warn(e.span, "range bounds must be numbers");
            return Ok(Val::undef());
        };
        let stepv = match &st {
            Some(v) => match v.as_num() {
                Some(n) => n,
                None => {
                    self.warn(e.span, "range step must be a number");
                    return Ok(Val::undef());
                }
            },
            None => {
                if a > b {
                    self.warn(
                        e.span,
                        "ranges of the form [begin:end] with begin greater than end are deprecated",
                    );
                    std::mem::swap(&mut a, &mut b);
                }
                1.0
            }
        };
        Ok(Val::new(Value::Range(Range { start: a, step: stepv, end: b })))
    }

    // ------------------------------------------------------------ index / member

    fn index_expr(&mut self, e: &'a Expr, base: &'a Expr, items: &'a [IndexItem], env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        let b = self.expr(base, env, dy)?;
        if let Value::Grid(g) = b.v {
            return self.grid_access(e, g, items, env, dy);
        }
        let undef = Val { def: Some(e.span), ..Val::undef() };
        let item = match items {
            [IndexItem::Expr(x)] => x,
            [IndexItem::Wild(sp)] => {
                self.error(*sp, "`*` is only allowed in grid access");
                return Ok(undef);
            }
            _ => {
                self.warn(e.span, "only one index allowed");
                return Ok(undef);
            }
        };
        let iv = self.expr(item, env, dy)?;
        let Some(i) = iv.as_num() else {
            return Ok(undef);
        };
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

    fn member_expr(&mut self, e: &'a Expr, base: &'a Expr, name: &'a Ident, env: &Env<'a>, dy: &Dyn<'a>) -> R<Val<'a>> {
        let b = self.expr(base, env, dy)?;
        let in_ctx = self.is_in_context(env);
        match &b.v {
            Value::Grid(g) => Ok(self.plane_member(*g, name, e.span, in_ctx)),
            Value::Instance(i) => Ok(self.anchor_member(*i, name, e.span, in_ctx)),
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
