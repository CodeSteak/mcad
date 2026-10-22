//! Syntax tree. Every node has a byte span into the source; statements and
//! expressions have ids unique within one parse.

use std::rc::Rc;

use crate::Span;

pub type StmtId = u32;
pub type ExprId = u32;

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    /// `#`: highlight.
    Highlight,
    /// `%`: background (removed from the result, shown grey).
    Background,
    /// `!`: root.
    Root,
    /// `*`: disable.
    Disable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub stmts: Vec<Stmt>,
    /// Number of statement ids handed out.
    pub stmt_count: u32,
    pub expr_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub id: StmtId,
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Empty,
    Block(Vec<Stmt>),
    Assign { name: Ident, value: Expr },
    Module(Rc<ModuleDef>),
    Function(Rc<FunctionDef>),
    Include { path: String, is_use: bool },
    /// `anchor name = point, dir = v;`
    Anchor { name: Ident, point: Expr, dir: Option<Expr> },
    Grid(Rc<GridDecl>),
    /// `name: call;`
    Named { name: Ident, inst: Box<Stmt> },
    /// Module instantiation `name(args) children`.
    Call { name: Ident, args: Vec<Arg>, children: Vec<Stmt>, has_block: bool },
    For { vars: Vec<Assignment>, body: Box<Stmt>, intersection: bool },
    Let { vars: Vec<Assignment>, body: Box<Stmt> },
    If { cond: Expr, then: Box<Stmt>, els: Option<Box<Stmt>> },
    /// Unparseable statement (diagnostic already reported).
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<Ident>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub name: Ident,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    /// Span of the whole definition.
    pub span: Span,
    /// Span of the body block (or single statement).
    pub body_span: Span,
    /// Id of the `module` statement.
    pub stmt: StmtId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: Ident,
    pub params: Vec<Param>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GridDecl {
    pub name: Ident,
    pub planes: Vec<PlaneDef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaneDef {
    pub axis: Axis,
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub id: ExprId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Number(f64),
    Str(String),
    Bool(bool),
    Undef,
    Ident(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call { callee: Box<Expr>, args: Vec<Arg> },
    /// `a[i]` or grid access `g[x, *, _]`.
    Index { base: Box<Expr>, items: Vec<IndexItem> },
    Member { base: Box<Expr>, name: Ident },
    Vector(Vec<VecElem>),
    Range { start: Box<Expr>, step: Option<Box<Expr>>, end: Box<Expr> },
    FuncLit { params: Vec<Param>, body: Rc<Expr> },
    Let { vars: Vec<Assignment>, body: Box<Expr> },
    Assert { args: Vec<Arg>, body: Option<Box<Expr>> },
    Echo { args: Vec<Arg>, body: Option<Box<Expr>> },
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexItem {
    Expr(Expr),
    /// `*` in grid access.
    Wild(Span),
}

#[derive(Debug, Clone, PartialEq)]
pub enum VecElem {
    Expr(Expr),
    For { vars: Vec<Assignment>, body: Box<VecElem>, span: Span },
    ForC {
        init: Vec<Assignment>,
        cond: Expr,
        update: Vec<Assignment>,
        body: Box<VecElem>,
        span: Span,
    },
    If { cond: Expr, then: Box<VecElem>, els: Option<Box<VecElem>>, span: Span },
    Let { vars: Vec<Assignment>, body: Box<VecElem>, span: Span },
    Each(Box<VecElem>, Span),
}

impl Stmt {
    /// Visits this statement and all nested statements (not module bodies' callers).
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Stmt)) {
        f(self);
        match &self.kind {
            StmtKind::Block(s) => s.iter().for_each(|s| s.walk(f)),
            StmtKind::Module(m) => m.body.iter().for_each(|s| s.walk(f)),
            StmtKind::Named { inst, .. } => inst.walk(f),
            StmtKind::Call { children, .. } => children.iter().for_each(|s| s.walk(f)),
            StmtKind::For { body, .. } | StmtKind::Let { body, .. } => body.walk(f),
            StmtKind::If { then, els, .. } => {
                then.walk(f);
                if let Some(e) = els {
                    e.walk(f);
                }
            }
            _ => {}
        }
    }
}

impl Expr {
    /// Visits this expression and all sub-expressions.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        f(self);
        let args = |a: &'a [Arg], f: &mut dyn FnMut(&'a Expr)| a.iter().for_each(|a| a.value.walk(f));
        match &self.kind {
            ExprKind::Unary(_, e) => e.walk(f),
            ExprKind::Binary(_, a, b) => {
                a.walk(f);
                b.walk(f);
            }
            ExprKind::Ternary(a, b, c) => {
                a.walk(f);
                b.walk(f);
                c.walk(f);
            }
            ExprKind::Call { callee, args: a } => {
                callee.walk(f);
                args(a, f);
            }
            ExprKind::Index { base, items } => {
                base.walk(f);
                for it in items {
                    if let IndexItem::Expr(e) = it {
                        e.walk(f);
                    }
                }
            }
            ExprKind::Member { base, .. } => base.walk(f),
            ExprKind::Vector(elems) => elems.iter().for_each(|e| e.walk(f)),
            ExprKind::Range { start, step, end } => {
                start.walk(f);
                if let Some(s) = step {
                    s.walk(f);
                }
                end.walk(f);
            }
            ExprKind::FuncLit { body, .. } => body.walk(f),
            ExprKind::Let { vars, body } => {
                vars.iter().for_each(|v| v.value.walk(f));
                body.walk(f);
            }
            ExprKind::Assert { args: a, body } | ExprKind::Echo { args: a, body } => {
                args(a, f);
                if let Some(b) = body {
                    b.walk(f);
                }
            }
            _ => {}
        }
    }
}

impl VecElem {
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        match self {
            VecElem::Expr(e) => e.walk(f),
            VecElem::For { vars, body, .. } | VecElem::Let { vars, body, .. } => {
                vars.iter().for_each(|v| v.value.walk(f));
                body.walk(f);
            }
            VecElem::ForC { init, cond, update, body, .. } => {
                init.iter().for_each(|v| v.value.walk(f));
                cond.walk(f);
                update.iter().for_each(|v| v.value.walk(f));
                body.walk(f);
            }
            VecElem::If { cond, then, els, .. } => {
                cond.walk(f);
                then.walk(f);
                if let Some(e) = els {
                    e.walk(f);
                }
            }
            VecElem::Each(e, _) => e.walk(f),
        }
    }
}
