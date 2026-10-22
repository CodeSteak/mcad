//! Lexical environments with lazily evaluated assignments.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use cad_syntax::Span;
use cad_syntax::ast::{Expr, FunctionDef, ModuleDef};

use crate::value::Val;

#[derive(Debug, Clone)]
pub enum Binding {
    /// Not yet evaluated: expression + the environment it is evaluated in.
    Lazy { expr: Rc<Expr>, name_span: Span },
    /// Being evaluated (cycle detection).
    InProgress,
    Done(Val),
}

#[derive(Debug, Default)]
pub struct Scope {
    pub vars: HashMap<String, Binding>,
    pub funcs: HashMap<String, Rc<FunctionDef>>,
    pub modules: HashMap<String, Rc<ModuleDef>>,
    pub parent: Option<Env>,
}

/// Shared, mutable scope chain.
#[derive(Debug, Clone, Default)]
pub struct Env(pub Rc<RefCell<Scope>>);

impl Env {
    pub fn root() -> Self {
        Env::default()
    }

    pub fn child(&self) -> Self {
        Env(Rc::new(RefCell::new(Scope { parent: Some(self.clone()), ..Default::default() })))
    }

    pub fn set(&self, name: &str, v: Val) {
        self.0.borrow_mut().vars.insert(name.to_string(), Binding::Done(v));
    }

    pub fn set_lazy(&self, name: &str, expr: Rc<Expr>, name_span: Span) {
        self.0.borrow_mut().vars.insert(name.to_string(), Binding::Lazy { expr, name_span });
    }

    pub fn has_local(&self, name: &str) -> bool {
        self.0.borrow().vars.contains_key(name)
    }

    /// Finds the scope that binds `name`.
    pub fn find_var(&self, name: &str) -> Option<Env> {
        let mut cur = Some(self.clone());
        while let Some(e) = cur {
            if e.0.borrow().vars.contains_key(name) {
                return Some(e);
            }
            cur = e.0.borrow().parent.clone();
        }
        None
    }

    pub fn find_func(&self, name: &str) -> Option<(Rc<FunctionDef>, Env)> {
        let mut cur = Some(self.clone());
        while let Some(e) = cur {
            if let Some(f) = e.0.borrow().funcs.get(name) {
                return Some((f.clone(), e.clone()));
            }
            cur = e.0.borrow().parent.clone();
        }
        None
    }

    pub fn find_module(&self, name: &str) -> Option<(Rc<ModuleDef>, Env)> {
        let mut cur = Some(self.clone());
        while let Some(e) = cur {
            if let Some(m) = e.0.borrow().modules.get(name) {
                return Some((m.clone(), e.clone()));
            }
            cur = e.0.borrow().parent.clone();
        }
        None
    }

    pub fn ptr_eq(&self, other: &Env) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
