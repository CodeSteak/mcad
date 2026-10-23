//! Lexical environments with lazily evaluated assignments, and the dynamic chain of
//! `$` variables.
//!
//! Cycles: an environment can only be reached back from a value through a function
//! literal (closure → captured environment). The evaluator registers every captured
//! environment and clears it at the end of an evaluation, so nothing leaks.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use cad_syntax::ast::{Expr, FunctionDef, ModuleDef};

use crate::value::{InstId, Val};

/// Instance id for scopes that belong to no module instance (the file scope while a
/// module is the context, enclosing modules of a nested context module).
pub const NO_INST: InstId = u32::MAX;

#[derive(Debug, Clone)]
pub enum Binding<'a> {
    /// Not yet evaluated; evaluated in the scope that holds the binding.
    Lazy(&'a Expr),
    /// Parameter default, not yet evaluated. As in OpenSCAD it is evaluated in the
    /// definition environment (it does not see the other parameters) with the `$`
    /// variables of the caller (without the call's own `$` arguments).
    Default(&'a Expr, Env<'a>, Dyn<'a>),
    /// Being evaluated (cycle detection).
    InProgress,
    Done(Val<'a>),
}

#[derive(Debug)]
pub struct Scope<'a> {
    pub vars: Vec<(&'a str, Binding<'a>)>,
    pub funcs: Vec<(&'a str, &'a FunctionDef)>,
    pub modules: Vec<(&'a str, &'a ModuleDef)>,
    /// File scopes of libraries made visible by `use` (file roots only): their own modules
    /// and functions are found after this scope's, the last `use` first.
    pub uses: Vec<Env<'a>>,
    pub parent: Option<Env<'a>>,
    /// `$` variables in effect for code of this scope (used to force lazy bindings).
    pub dyn_: Dyn<'a>,
    /// Module instance whose code this scope belongs to.
    pub inst: InstId,
    /// Top scope of a module instance (or of the file): grids and named instances of
    /// `inst` are visible below it, before the definition environment.
    pub module_root: bool,
}

/// Shared, mutable scope chain.
#[derive(Debug, Clone)]
pub struct Env<'a>(pub Rc<RefCell<Scope<'a>>>);

impl<'a> Env<'a> {
    #[inline(never)]
    pub fn new(parent: Option<&Env<'a>>, dyn_: Dyn<'a>, inst: InstId, module_root: bool) -> Self {
        Env(Rc::new(RefCell::new(Scope {
            vars: Vec::new(),
            funcs: Vec::new(),
            modules: Vec::new(),
            uses: Vec::new(),
            parent: parent.cloned(),
            dyn_,
            inst,
            module_root,
        })))
    }

    /// Plain nested scope (block, loop body, let, function call) of the same instance.
    pub fn child(&self, dyn_: Dyn<'a>) -> Self {
        let inst = self.inst();
        Env::new(Some(self), dyn_, inst, false)
    }

    pub fn inst(&self) -> InstId {
        self.0.borrow().inst
    }

    pub fn set(&self, name: &'a str, v: Val<'a>) {
        let mut s = self.0.borrow_mut();
        if let Some(e) = s.vars.iter_mut().rev().find(|(n, _)| *n == name) {
            e.1 = Binding::Done(v);
        } else {
            s.vars.push((name, Binding::Done(v)));
        }
    }

    /// Adds a lazy binding; returns `true` if the name was already bound in this scope
    /// (the new definition replaces the old one).
    pub fn set_lazy(&self, name: &'a str, expr: &'a Expr) -> bool {
        let mut s = self.0.borrow_mut();
        if let Some(e) = s.vars.iter_mut().find(|(n, _)| *n == name) {
            e.1 = Binding::Lazy(expr);
            true
        } else {
            s.vars.push((name, Binding::Lazy(expr)));
            false
        }
    }

    /// Binds a parameter default (see [`Binding::Default`]).
    pub fn set_default(&self, name: &'a str, expr: &'a Expr, def_env: Env<'a>, dyn_: Dyn<'a>) {
        let b = Binding::Default(expr, def_env, dyn_);
        let mut s = self.0.borrow_mut();
        if let Some(e) = s.vars.iter_mut().rev().find(|(n, _)| *n == name) {
            e.1 = b;
        } else {
            s.vars.push((name, b));
        }
    }

    pub fn has_local(&self, name: &str) -> bool {
        self.0.borrow().vars.iter().any(|(n, _)| *n == name)
    }

    pub fn set_uses(&self, uses: Vec<Env<'a>>) {
        self.0.borrow_mut().uses = uses;
    }

    pub fn set_dyn(&self, d: Dyn<'a>) {
        self.0.borrow_mut().dyn_ = d;
    }

    pub fn dyn_(&self) -> Dyn<'a> {
        self.0.borrow().dyn_.clone()
    }

    pub fn find_func(&self, name: &str) -> Option<(&'a FunctionDef, Env<'a>)> {
        let mut cur = Some(self.clone());
        while let Some(e) = cur {
            let next = {
                let s = e.0.borrow();
                if let Some((_, f)) = s.funcs.iter().rev().find(|(n, _)| *n == name) {
                    return Some((*f, e.clone()));
                }
                for u in s.uses.iter().rev() {
                    if let Some((_, f)) = u.0.borrow().funcs.iter().rev().find(|(n, _)| *n == name) {
                        return Some((*f, u.clone()));
                    }
                }
                s.parent.clone()
            };
            cur = next;
        }
        None
    }

    pub fn find_module(&self, name: &str) -> Option<(&'a ModuleDef, Env<'a>)> {
        let mut cur = Some(self.clone());
        while let Some(e) = cur {
            let next = {
                let s = e.0.borrow();
                if let Some((_, m)) = s.modules.iter().rev().find(|(n, _)| *n == name) {
                    return Some((*m, e.clone()));
                }
                for u in s.uses.iter().rev() {
                    if let Some((_, m)) = u.0.borrow().modules.iter().rev().find(|(n, _)| *n == name) {
                        return Some((*m, u.clone()));
                    }
                }
                s.parent.clone()
            };
            cur = next;
        }
        None
    }

    pub fn weak(&self) -> Weak<RefCell<Scope<'a>>> {
        Rc::downgrade(&self.0)
    }

    /// Drops all values of a scope (breaks closure cycles at the end of an evaluation).
    pub fn clear_weak(w: &Weak<RefCell<Scope<'a>>>) {
        if let Some(rc) = w.upgrade()
            && let Ok(mut s) = rc.try_borrow_mut()
        {
            s.vars.clear();
            s.dyn_ = None;
            s.parent = None;
            s.uses.clear();
        }
    }
}

/// One frame of `$` variables (a scope with `$` assignments or a call with `$`
/// arguments). Values are evaluated eagerly.
#[derive(Debug)]
pub struct DynFrame<'a> {
    pub vars: Vec<(&'a str, Val<'a>)>,
    pub parent: Dyn<'a>,
}

pub type Dyn<'a> = Option<Rc<DynFrame<'a>>>;

pub fn dyn_lookup<'a>(d: &Dyn<'a>, name: &str) -> Option<Val<'a>> {
    let mut cur = d.as_ref();
    while let Some(f) = cur {
        if let Some((_, v)) = f.vars.iter().rev().find(|(n, _)| *n == name) {
            return Some(v.clone());
        }
        cur = f.parent.as_ref();
    }
    None
}

/// The innermost frame that sets `name`.
pub fn dyn_frame<'x, 'a>(d: &'x Dyn<'a>, name: &str) -> Option<&'x DynFrame<'a>> {
    let mut cur = d.as_ref();
    while let Some(f) = cur {
        if f.vars.iter().any(|(n, _)| *n == name) {
            return Some(f);
        }
        cur = f.parent.as_ref();
    }
    None
}

pub fn dyn_push<'a>(d: &Dyn<'a>, vars: Vec<(&'a str, Val<'a>)>) -> Dyn<'a> {
    if vars.is_empty() {
        return d.clone();
    }
    Some(Rc::new(DynFrame { vars, parent: d.clone() }))
}
