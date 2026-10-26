//! Modules as namespaces (Plan 17.2): `parts.screw(d = 3);`, `a.b.c();` and, in
//! expressions, functions and constants defined in a module body: `geo.area(2)`, `geo.M3`.
//!
//! A path `a.b` is resolved from the module `a` visible at the call: its body is the
//! namespace, entered with `a`'s parameters at their defaults (like the lexical scope a nested
//! module sees anyway). The namespace belongs to no module instance, so grids and named
//! instances of `a` are not visible there, only its parameters, variables, functions and
//! modules.

use cad_i18n::tr;
use std::rc::Rc;

use cad_syntax::Span;
use cad_syntax::ast::{Expr, ExprKind, Ident, ModuleDef};

use crate::env::{Binding, Dyn, Env, NO_INST};
use crate::eval::{Ctx, Ev, R};
use crate::value::{Closure, Val, Value};

/// `a.b.c` as identifiers, if the expression is such a path.
pub(crate) fn ident_path(e: &Expr) -> Option<Vec<&str>> {
    match &e.kind {
        ExprKind::Ident(n) => Some(vec![n.as_str()]),
        ExprKind::Member { base, name } => {
            let mut p = ident_path(base)?;
            p.push(name.name.as_str());
            Some(p)
        }
        _ => None,
    }
}

impl<'a> Ev<'a> {
    /// Body scope of module `def` (defined in `denv`) as a namespace.
    fn ns_env(&mut self, def: &'a ModuleDef, denv: &Env<'a>, dy: &Dyn<'a>) -> R<Env<'a>> {
        let e = Env::new(Some(denv), dy.clone(), NO_INST, true);
        for p in &def.params {
            match &p.default {
                Some(d) => e.set_default(&p.name.name, d, denv.clone(), dy.clone()),
                None => e.set(&p.name.name, Val::undef()),
            }
        }
        self.hoist(&def.body, &e, dy)?;
        self.captured.push(e.weak());
        Ok(e)
    }

    /// Namespace of the module path `parts` (all modules), searched from `env`. Reports a
    /// missing part at `span` and returns `None`.
    fn ns_path(&mut self, parts: &[&str], span: Span, env: &Env<'a>, dy: &Dyn<'a>) -> R<Option<Env<'a>>> {
        let Some((mut def, mut denv)) = env.find_module(parts[0]) else {
            self.error(span, tr!("unbekanntes Modul `{}`", "unknown module `{}`", parts[0]));
            return Ok(None);
        };
        for (i, part) in parts.iter().enumerate().skip(1) {
            let ns = self.ns_env(def, &denv, dy)?;
            let found = ns.0.borrow().modules.iter().rev().find(|(n, _)| n == part).map(|(_, m)| *m);
            match found {
                Some(m) => {
                    def = m;
                    denv = ns;
                }
                None => {
                    let outer = parts[..i].join(".");
                    self.error(
                        span,
                        tr!(
                            "Modul `{outer}` hat kein Untermodul `{part}`",
                            "module `{outer}` has no submodule `{part}`"
                        ),
                    );
                    return Ok(None);
                }
            }
        }
        Ok(Some(self.ns_env(def, &denv, dy)?))
    }

    /// Module for a call `name(…)`: a plain name as before, a qualified name `a.b.c` through
    /// the namespaces `a` and `a.b`. Errors are reported here.
    pub(crate) fn find_module_ns(
        &mut self,
        name: &str,
        span: Span,
        ctx: &Ctx<'a>,
    ) -> R<Option<(&'a ModuleDef, Env<'a>)>> {
        if !name.contains('.') {
            return Ok(ctx.env.find_module(name));
        }
        let parts: Vec<&str> = name.split('.').collect();
        let (last, outer) = parts.split_last().expect("non-empty path");
        let Some(ns) = self.ns_path(outer, span, &ctx.env, &ctx.dyn_)? else { return Ok(None) };
        let found = ns.0.borrow().modules.iter().rev().find(|(n, _)| n == last).map(|(_, m)| *m);
        match found {
            Some(m) => Ok(Some((m, ns))),
            None => {
                self.error(
                    span,
                    tr!(
                        "Modul `{}` hat kein Untermodul `{last}`",
                        "module `{}` has no submodule `{last}`",
                        outer.join(".")
                    ),
                );
                Ok(None)
            }
        }
    }

    /// `ns.name` where `ns` is a module path and not a value: a function (as a function value)
    /// or a constant of the module body. `prefer_fn`: the member is called. `None` if `base`
    /// is not a module path.
    pub(crate) fn ns_member(
        &mut self,
        e: &'a Expr,
        base: &'a Expr,
        name: &'a Ident,
        env: &Env<'a>,
        dy: &Dyn<'a>,
        prefer_fn: bool,
    ) -> R<Option<Val<'a>>> {
        let Some(path) = ident_path(base) else { return Ok(None) };
        if path[0].starts_with('$') || self.lookup(path[0], env)?.is_some() || env.find_module(path[0]).is_none() {
            return Ok(None);
        }
        let Some(ns) = self.ns_path(&path, base.span, env, dy)? else { return Ok(Some(Val::undef())) };
        let func = ns.0.borrow().funcs.iter().rev().find(|(n, _)| *n == name.name).map(|(_, f)| *f);
        let has_var = ns.0.borrow().vars.iter().any(|(n, b)| *n == name.name && !matches!(b, Binding::InProgress));
        let has_mod = ns.0.borrow().modules.iter().any(|(n, _)| *n == name.name);
        let as_fn = |f: &'a cad_syntax::ast::FunctionDef| {
            Val::new(Value::Func(Rc::new(Closure {
                params: &f.params,
                body: &f.body,
                env: ns.clone(),
                name: Some(&f.name.name),
            })))
        };
        let ns_name = path.join(".");
        Ok(Some(match (func, has_var) {
            (Some(f), false) => as_fn(f),
            (Some(f), true) if prefer_fn => as_fn(f),
            (_, true) => {
                let v = self.lookup(&name.name, &ns)?.unwrap_or_else(Val::undef);
                Val { def: Some(e.span), ..v }
            }
            (None, false) if has_mod => {
                self.error(
                    e.span,
                    tr!(
                        "`{ns_name}.{}` ist ein Modul, kein Wert",
                        "`{ns_name}.{}` is a module, not a value",
                        name.name
                    ),
                );
                Val::undef()
            }
            (None, false) => {
                self.warn(
                    e.span,
                    tr!(
                        "Modul `{ns_name}` hat keine Funktion oder Variable `{}`",
                        "module `{ns_name}` has no function or variable `{}`",
                        name.name
                    ),
                );
                Val::undef()
            }
        }))
    }
}
