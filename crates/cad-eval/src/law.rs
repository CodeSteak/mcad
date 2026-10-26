//! Compiles a function value into a radius law ([`cad_ir::Law`]) for `fillet(r = function (p) …)`.
//!
//! The engine evaluates the law at points along the selected edges, long after evaluation
//! (the edges only exist once the kernel has built the body). The function is therefore
//! translated into a self-contained expression over its parameter `p`:
//!
//! * a sub-expression that does not mention `p` (or a local derived from it) is evaluated
//!   here, in the function's environment, and becomes a constant: captured variables,
//!   arithmetic on them, list comprehensions, ranges, any built-in;
//! * the rest is translated node by node: literals, `+ - * / % ^`, comparisons, `&& || !`,
//!   `?:`, `let`, `[i]`, `.x/.y/.z`, vectors, and calls of the math built-ins
//!   ([`cad_ir::LawFn`]);
//! * calls of user functions and function values are inlined (their parameters become
//!   local slots), up to a nesting depth of [`MAX_INLINE`] (no recursion on `p`).
//!
//! Anything else (strings, `echo`, `assert`, function literals depending on `p`, list
//! comprehensions over `p`) is an error with its own message.

use cad_i18n::{tr, trs};
use std::collections::HashSet;

use cad_ir::{Law, LawExpr, LawFn, LawOp};
use cad_syntax::Span;
use cad_syntax::ast::*;

use crate::env::{Dyn, Env};
use crate::eval::{Ev, R};
use crate::value::{Closure, Val, Value};

/// Maximum nesting of inlined function calls.
pub const MAX_INLINE: u32 = 16;

#[derive(Clone, Copy)]
enum Bind {
    Point,
    Slot(u32),
}

#[derive(Clone)]
struct Cx<'a> {
    /// Innermost last.
    scope: Vec<(&'a str, Bind)>,
    env: Env<'a>,
    depth: u32,
}

impl Cx<'_> {
    fn find(&self, name: &str) -> Option<Bind> {
        self.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }
}

/// Error of the compilation: message and the span it refers to.
pub(crate) struct LawErr {
    pub span: Span,
    pub msg: String,
}

type C<T> = Result<T, LawErr>;

fn err<T>(span: Span, msg: impl Into<String>) -> R<C<T>> {
    Ok(Err(LawErr { span, msg: msg.into() }))
}

/// Does `e` mention one of `names` (lexically, ignoring shadowing: conservative)?
fn mentions(e: &Expr, names: &HashSet<&str>) -> bool {
    let m = |x: &Expr| mentions(x, names);
    match &e.kind {
        ExprKind::Ident(n) => names.contains(n.as_str()),
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Undef | ExprKind::Error => false,
        ExprKind::Unary(_, x) => m(x),
        ExprKind::Binary(_, a, b) => m(a) || m(b),
        ExprKind::Ternary(c, a, b) => m(c) || m(a) || m(b),
        ExprKind::Call { callee, args } => m(callee) || args.iter().any(|a| m(&a.value)),
        ExprKind::Index { base, items } => {
            m(base)
                || items.iter().any(|i| match i {
                    IndexItem::Expr(x) => m(x),
                    IndexItem::Wild(_) => false,
                })
        }
        ExprKind::Member { base, .. } => m(base),
        ExprKind::Vector(v) => v.iter().any(|x| elem_mentions(x, names)),
        ExprKind::Range { start, step, end } => m(start) || step.as_deref().is_some_and(m) || m(end),
        ExprKind::FuncLit { params, body } => params.iter().any(|p| p.default.as_ref().is_some_and(m)) || m(body),
        ExprKind::Let { vars, body } => vars.iter().any(|a| m(&a.value)) || m(body),
        ExprKind::Assert { args, body } | ExprKind::Echo { args, body } => {
            args.iter().any(|a| m(&a.value)) || body.as_deref().is_some_and(m)
        }
    }
}

fn elem_mentions(e: &VecElem, names: &HashSet<&str>) -> bool {
    let asg = |v: &[Assignment]| v.iter().any(|a| mentions(&a.value, names));
    match e {
        VecElem::Expr(x) => mentions(x, names),
        VecElem::For { vars, body, .. } => asg(vars) || elem_mentions(body, names),
        VecElem::ForC { init, cond, update, body, .. } => {
            asg(init) || mentions(cond, names) || asg(update) || elem_mentions(body, names)
        }
        VecElem::If { cond, then, els, .. } => {
            mentions(cond, names)
                || elem_mentions(then, names)
                || els.as_deref().is_some_and(|x| elem_mentions(x, names))
        }
        VecElem::Let { vars, body, .. } => asg(vars) || elem_mentions(body, names),
        VecElem::Each(x, _) => elem_mentions(x, names),
    }
}

/// A value as a constant of the law (numbers, booleans, undef and lists of them).
fn constant(v: &Val) -> Result<LawExpr, String> {
    Ok(match &v.v {
        Value::Undef => LawExpr::Undef,
        Value::Bool(b) => LawExpr::Bool(*b),
        Value::Num(x) => LawExpr::Num(*x),
        Value::PlaneRef { value, .. } => LawExpr::Num(*value),
        Value::List(l) => LawExpr::List(l.iter().map(constant).collect::<Result<_, _>>()?),
        Value::Range(r) => {
            let n = r.count();
            if n > 10_000.0 {
                return Err(trs!("Bereich mit mehr als 10000 Werten", "range with more than 10000 values").into());
            }
            LawExpr::List((0..n as usize).map(|i| LawExpr::Num(r.at(i))).collect())
        }
        Value::Str(_) => {
            return Err(trs!(
                "Texte sind in einer Radiusfunktion nicht möglich",
                "texts are not possible in a radius function"
            )
            .into());
        }
        Value::Func(_) => {
            return Err(trs!(
                "ein Funktionswert ist hier nicht möglich (nur als Aufruf)",
                "a function value is not possible here (only as a call)"
            )
            .into());
        }
        _ => {
            return Err(tr!(
                "{} ist in einer Radiusfunktion nicht möglich",
                "{} is not possible in a radius function",
                v.type_de()
            ));
        }
    })
}

fn bx(e: LawExpr) -> Box<LawExpr> {
    Box::new(e)
}

impl<'a> Ev<'a> {
    /// Compiles the function value `c` (one parameter: the position) into a law.
    pub(crate) fn compile_law(&mut self, c: &Closure<'a>, span: Span, dy: &Dyn<'a>) -> R<C<Law>> {
        let Some(first) = c.params.first() else {
            return err(
                span,
                trs!(
                    "Radiusfunktion braucht einen Parameter (die Position p)",
                    "radius function needs one parameter (the position p)"
                ),
            );
        };
        let mut slots = 0u32;
        let mut cx = Cx { scope: vec![(first.name.name.as_str(), Bind::Point)], env: c.env.clone(), depth: 0 };
        // further parameters: their defaults, as locals
        let mut lets = Vec::new();
        for p in &c.params[1..] {
            let v = match &p.default {
                Some(d) => match self.law_expr(d, &cx, &mut slots, dy)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Err(e)),
                },
                None => LawExpr::Undef,
            };
            let s = slots;
            slots += 1;
            lets.push((s, v));
            cx.scope.push((p.name.name.as_str(), Bind::Slot(s)));
        }
        let mut body = match self.law_expr(c.body, &cx, &mut slots, dy)? {
            Ok(b) => b,
            Err(e) => return Ok(Err(e)),
        };
        for (s, v) in lets.into_iter().rev() {
            body = LawExpr::Let(s, bx(v), bx(body));
        }
        let text = self.proj.text(span).to_string();
        Ok(Ok(Law { expr: body, slots, text }))
    }

    fn law_expr(&mut self, e: &'a Expr, cx: &Cx<'a>, slots: &mut u32, dy: &Dyn<'a>) -> R<C<LawExpr>> {
        // constant sub-expression: evaluate it now
        let names: HashSet<&str> = cx.scope.iter().map(|(n, _)| *n).collect();
        let is_call_of_fn = matches!(&e.kind, ExprKind::Call { .. });
        if !mentions(e, &names) && !is_call_of_fn {
            let v = self.expr(e, &cx.env, dy)?;
            return Ok(constant(&v).map_err(|msg| LawErr { span: e.span, msg }));
        }
        macro_rules! sub {
            ($x:expr) => {
                match self.law_expr($x, cx, slots, dy)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Err(e)),
                }
            };
        }
        Ok(Ok(match &e.kind {
            ExprKind::Ident(n) => match cx.find(n) {
                Some(Bind::Point) => LawExpr::Point,
                Some(Bind::Slot(s)) => LawExpr::Slot(s),
                None => {
                    let v = self.expr(e, &cx.env, dy)?;
                    match constant(&v) {
                        Ok(c) => c,
                        Err(msg) => return err(e.span, msg),
                    }
                }
            },
            ExprKind::Unary(op, x) => {
                let x = sub!(x);
                match op {
                    UnOp::Neg => LawExpr::Neg(bx(x)),
                    UnOp::Plus => x,
                    UnOp::Not => LawExpr::Not(bx(x)),
                }
            }
            ExprKind::Binary(op, a, b) => {
                let (a, b) = (sub!(a), sub!(b));
                let op = match op {
                    BinOp::And => return Ok(Ok(LawExpr::And(bx(a), bx(b)))),
                    BinOp::Or => return Ok(Ok(LawExpr::Or(bx(a), bx(b)))),
                    BinOp::Add => LawOp::Add,
                    BinOp::Sub => LawOp::Sub,
                    BinOp::Mul => LawOp::Mul,
                    BinOp::Div => LawOp::Div,
                    BinOp::Mod => LawOp::Mod,
                    BinOp::Pow => LawOp::Pow,
                    BinOp::Lt => LawOp::Lt,
                    BinOp::Le => LawOp::Le,
                    BinOp::Gt => LawOp::Gt,
                    BinOp::Ge => LawOp::Ge,
                    BinOp::Eq => LawOp::Eq,
                    BinOp::Ne => LawOp::Ne,
                };
                LawExpr::Bin(op, bx(a), bx(b))
            }
            ExprKind::Ternary(c, a, b) => LawExpr::Cond(bx(sub!(c)), bx(sub!(a)), bx(sub!(b))),
            ExprKind::Member { base, name } => {
                let i = match name.name.as_str() {
                    "x" => 0.0,
                    "y" => 1.0,
                    "z" => 2.0,
                    other => {
                        return err(
                            name.span,
                            tr!(
                                "`.{other}` ist in einer Radiusfunktion nicht möglich (nur .x, .y, .z)",
                                "`.{other}` is not possible in a radius function (only .x, .y, .z)"
                            ),
                        );
                    }
                };
                LawExpr::Index(bx(sub!(base)), bx(LawExpr::Num(i)))
            }
            ExprKind::Index { base, items } => match items.as_slice() {
                [IndexItem::Expr(i)] => LawExpr::Index(bx(sub!(base)), bx(sub!(i))),
                _ => {
                    return err(
                        e.span,
                        trs!("Index mit genau einem Ausdruck erwartet", "expected index with exactly one expression"),
                    );
                }
            },
            ExprKind::Vector(elems) => {
                let mut out = Vec::new();
                for el in elems {
                    match el {
                        VecElem::Expr(x) => out.push(sub!(x)),
                        _ => {
                            return err(
                                e.span,
                                trs!(
                                    "Listenausdrücke, die von der Position abhängen, sind in einer Radiusfunktion nicht möglich",
                                    "list expressions that depend on the position are not possible in a radius function"
                                ),
                            );
                        }
                    }
                }
                LawExpr::List(out)
            }
            ExprKind::Let { vars, body } => {
                let mut inner = cx.clone();
                let mut binds = Vec::new();
                for a in vars {
                    let v = match self.law_expr(&a.value, &inner, slots, dy)? {
                        Ok(v) => v,
                        Err(e) => return Ok(Err(e)),
                    };
                    let s = *slots;
                    *slots += 1;
                    binds.push((s, v));
                    inner.scope.push((a.name.name.as_str(), Bind::Slot(s)));
                }
                let mut b = match self.law_expr(body, &inner, slots, dy)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Err(e)),
                };
                for (s, v) in binds.into_iter().rev() {
                    b = LawExpr::Let(s, bx(v), bx(b));
                }
                b
            }
            ExprKind::Call { callee, args } => return self.law_call(e, callee, args, cx, slots, dy),
            ExprKind::Assert { body: Some(b), .. } | ExprKind::Echo { body: Some(b), .. } => sub!(b),
            ExprKind::FuncLit { .. } => {
                return err(
                    e.span,
                    trs!(
                        "Funktionsliteral in einer Radiusfunktion nicht möglich",
                        "function literal not possible in a radius function"
                    ),
                );
            }
            ExprKind::Range { .. } => {
                return err(
                    e.span,
                    trs!(
                        "Bereiche, die von der Position abhängen, sind nicht möglich",
                        "ranges that depend on the position are not possible"
                    ),
                );
            }
            _ => {
                return err(
                    e.span,
                    trs!(
                        "Ausdruck in einer Radiusfunktion nicht möglich",
                        "expression not possible in a radius function"
                    ),
                );
            }
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn law_call(
        &mut self,
        e: &'a Expr,
        callee: &'a Expr,
        args: &'a [Arg],
        cx: &Cx<'a>,
        slots: &mut u32,
        dy: &Dyn<'a>,
    ) -> R<C<LawExpr>> {
        let names: HashSet<&str> = cx.scope.iter().map(|(n, _)| *n).collect();
        // the function to call: user function, function value, or built-in
        let target: Option<(&'a [Param], &'a Expr, Env<'a>)> = match &callee.kind {
            ExprKind::Ident(name) if cx.find(name).is_some() => {
                return err(
                    callee.span,
                    trs!(
                        "Aufruf eines Parameters ist in einer Radiusfunktion nicht möglich",
                        "calling a parameter is not possible in a radius function"
                    ),
                );
            }
            ExprKind::Ident(name) => {
                if let Some((f, fenv)) = cx.env.find_func(name) {
                    Some((&f.params, &f.body, fenv))
                } else if !name.starts_with('$')
                    && let Some(v) = self.lookup(name, &cx.env)?
                    && let Value::Func(c) = &v.v
                {
                    Some((c.params, c.body, c.env.clone()))
                } else {
                    None
                }
            }
            _ if !mentions(callee, &names) => match &self.expr(callee, &cx.env, dy)?.v {
                Value::Func(c) => Some((c.params, c.body, c.env.clone())),
                _ => return err(callee.span, trs!("kein Funktionswert", "not a function value")),
            },
            _ => {
                return err(
                    callee.span,
                    trs!("Aufruf in einer Radiusfunktion nicht möglich", "call not possible in a radius function"),
                );
            }
        };
        let Some((params, body, fenv)) = target else {
            // built-in: constant if no argument depends on p
            let ExprKind::Ident(name) = &callee.kind else { unreachable!() };
            if !mentions(e, &names) {
                let v = self.expr(e, &cx.env, dy)?;
                return Ok(constant(&v).map_err(|msg| LawErr { span: e.span, msg }));
            }
            let Some(f) = LawFn::from_name(name) else {
                return err(
                    callee.span,
                    tr!(
                        "`{name}()` ist in einer Radiusfunktion nicht möglich",
                        "`{name}()` is not possible in a radius function"
                    ),
                );
            };
            let mut out = Vec::new();
            for a in args {
                if a.name.is_some() {
                    return err(
                        a.span,
                        trs!(
                            "benannte Argumente sind bei eingebauten Funktionen hier nicht möglich",
                            "named arguments of built-in functions are not possible here"
                        ),
                    );
                }
                match self.law_expr(&a.value, cx, slots, dy)? {
                    Ok(v) => out.push(v),
                    Err(e) => return Ok(Err(e)),
                }
            }
            return Ok(Ok(LawExpr::Call(f, out)));
        };
        if !mentions(e, &names) {
            let v = self.expr(e, &cx.env, dy)?;
            return Ok(constant(&v).map_err(|msg| LawErr { span: e.span, msg }));
        }
        if cx.depth >= MAX_INLINE {
            return err(
                e.span,
                tr!(
                    "Radiusfunktion: Aufrufe tiefer als {MAX_INLINE} verschachtelt (Rekursion?)",
                    "radius function: calls nested deeper than {MAX_INLINE} (recursion?)"
                ),
            );
        }
        // arguments, compiled in the caller's context, by position or name
        let mut vals: Vec<Option<LawExpr>> = vec![None; params.len()];
        let mut pos = 0usize;
        for a in args {
            let i = match &a.name {
                Some(n) => match params.iter().position(|p| p.name.name == n.name) {
                    Some(i) => i,
                    None => continue,
                },
                None => {
                    let i = pos;
                    pos += 1;
                    if i >= params.len() {
                        continue;
                    }
                    i
                }
            };
            match self.law_expr(&a.value, cx, slots, dy)? {
                Ok(v) => vals[i] = Some(v),
                Err(e) => return Ok(Err(e)),
            }
        }
        let mut inner = Cx { scope: Vec::new(), env: fenv, depth: cx.depth + 1 };
        let mut binds = Vec::new();
        for (p, v) in params.iter().zip(vals) {
            let v = match (v, &p.default) {
                (Some(v), _) => v,
                (None, Some(d)) => match self.law_expr(d, &inner, slots, dy)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Err(e)),
                },
                (None, None) => LawExpr::Undef,
            };
            let s = *slots;
            *slots += 1;
            binds.push((s, v));
            inner.scope.push((p.name.name.as_str(), Bind::Slot(s)));
        }
        let mut b = match self.law_expr(body, &inner, slots, dy)? {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        for (s, v) in binds.into_iter().rev() {
            b = LawExpr::Let(s, bx(v), bx(b));
        }
        Ok(Ok(b))
    }
}
