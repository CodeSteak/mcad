//! Recursive-descent parser with error recovery.

use std::rc::Rc;

use crate::ast::*;
use crate::lexer::{Tok, Token};
use crate::{Diagnostic, Span};

pub struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
    pub diags: Vec<Diagnostic>,
    next_stmt: u32,
    next_expr: u32,
}

type PResult<T> = Result<T, ()>;

impl<'a> Parser<'a> {
    pub fn new(toks: &'a [Token]) -> Self {
        Parser { toks, pos: 0, diags: Vec::new(), next_stmt: 0, next_expr: 0 }
    }

    pub fn parse_file(mut self) -> (File, Vec<Diagnostic>) {
        let mut stmts = Vec::new();
        while !self.at(&Tok::Eof) {
            if self.at(&Tok::RBrace) {
                let sp = self.span();
                self.error(sp, "unexpected `}`");
                self.bump();
                continue;
            }
            stmts.push(self.stmt_recover());
        }
        let file = File { stmts, stmt_count: self.next_stmt, expr_count: self.next_expr };
        (file, self.diags)
    }

    // ---------------------------------------------------------------- helpers

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.pos].span
    }

    fn prev_end(&self) -> u32 {
        if self.pos == 0 { 0 } else { self.toks[self.pos - 1].span.end }
    }

    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn at_ident(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Ident(w) if w == word)
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(span, msg));
    }

    fn expect(&mut self, t: &Tok, what: &str) -> PResult<Span> {
        if self.at(t) {
            Ok(self.bump().span)
        } else {
            let sp = self.span();
            self.error(sp, format!("expected {what}, found {}", describe(self.peek())));
            Err(())
        }
    }

    fn ident(&mut self) -> PResult<Ident> {
        if let Tok::Ident(name) = self.peek().clone() {
            let span = self.bump().span;
            Ok(Ident { name, span })
        } else {
            let sp = self.span();
            self.error(sp, format!("expected identifier, found {}", describe(self.peek())));
            Err(())
        }
    }

    fn stmt_id(&mut self) -> StmtId {
        self.next_stmt += 1;
        self.next_stmt - 1
    }

    fn expr_id(&mut self) -> ExprId {
        self.next_expr += 1;
        self.next_expr - 1
    }

    fn mk_expr(&mut self, span: Span, kind: ExprKind) -> Expr {
        Expr { id: self.expr_id(), span, kind }
    }

    /// Skips to just after the next `;` at depth 0, or to (not past) a `}` at depth 0.
    fn recover(&mut self) {
        let mut depth = 0i32;
        loop {
            match self.peek() {
                Tok::Eof => return,
                Tok::Semi if depth == 0 => {
                    self.bump();
                    return;
                }
                Tok::LBrace | Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RBrace if depth == 0 => return,
                Tok::RBrace | Tok::RParen | Tok::RBracket => depth -= 1,
                _ => {}
            }
            self.bump();
            if depth < 0 {
                depth = 0;
            }
        }
    }

    // ---------------------------------------------------------------- statements

    fn stmt_recover(&mut self) -> Stmt {
        let start = self.span().start;
        let before = self.pos;
        match self.stmt() {
            Ok(s) => s,
            Err(()) => {
                self.recover();
                if self.pos == before {
                    self.bump();
                }
                let id = self.stmt_id();
                Stmt {
                    id,
                    span: Span { start, end: self.prev_end().max(start) },
                    modifiers: vec![],
                    kind: StmtKind::Error,
                }
            }
        }
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.span().start;
        match self.peek().clone() {
            Tok::Semi => {
                self.bump();
                let id = self.stmt_id();
                Ok(Stmt { id, span: Span::new(start as usize, self.prev_end() as usize), modifiers: vec![], kind: StmtKind::Empty })
            }
            Tok::LBrace => {
                let id = self.stmt_id();
                let body = self.block()?;
                Ok(Stmt { id, span: self.span_from(start), modifiers: vec![], kind: StmtKind::Block(body) })
            }
            Tok::Ident(w) => match w.as_str() {
                "module" => self.module_def(),
                "function" if matches!(self.peek_at(1), Tok::Ident(_)) => self.function_def(),
                "include" | "use" => {
                    self.bump();
                    let is_use = w == "use";
                    let path = match self.peek().clone() {
                        Tok::Path(p) => {
                            self.bump();
                            p
                        }
                        _ => {
                            let sp = self.span();
                            self.error(sp, "expected `<path>`");
                            return Err(());
                        }
                    };
                    self.eat(&Tok::Semi);
                    let id = self.stmt_id();
                    Ok(Stmt { id, span: self.span_from(start), modifiers: vec![], kind: StmtKind::Include { path, is_use } })
                }
                "anchor" if matches!(self.peek_at(1), Tok::Ident(_)) && self.peek_at(2) == &Tok::Assign => {
                    self.bump();
                    let name = self.ident()?;
                    self.expect(&Tok::Assign, "`=`")?;
                    let point = self.expr()?;
                    let mut dir = None;
                    if self.eat(&Tok::Comma) {
                        let kw = self.ident()?;
                        if kw.name != "dir" {
                            self.error(kw.span, "expected `dir = …`");
                            return Err(());
                        }
                        self.expect(&Tok::Assign, "`=`")?;
                        dir = Some(self.expr()?);
                    }
                    self.expect(&Tok::Semi, "`;`")?;
                    let id = self.stmt_id();
                    Ok(Stmt { id, span: self.span_from(start), modifiers: vec![], kind: StmtKind::Anchor { name, point, dir } })
                }
                _ if self.peek_at(1) == &Tok::Assign => {
                    let name = self.ident()?;
                    self.bump();
                    let value = self.expr()?;
                    self.expect(&Tok::Semi, "`;`")?;
                    let id = self.stmt_id();
                    Ok(Stmt { id, span: self.span_from(start), modifiers: vec![], kind: StmtKind::Assign { name, value } })
                }
                _ if self.peek_at(1) == &Tok::Colon => {
                    let name = self.ident()?;
                    self.bump();
                    let inst = self.instantiation()?;
                    let id = self.stmt_id();
                    Ok(Stmt {
                        id,
                        span: self.span_from(start),
                        modifiers: vec![],
                        kind: StmtKind::Named { name, inst: Box::new(inst) },
                    })
                }
                _ => self.instantiation(),
            },
            Tok::Not | Tok::Hash | Tok::Percent | Tok::Star => self.instantiation(),
            other => {
                let sp = self.span();
                self.error(sp, format!("expected statement, found {}", describe(&other)));
                Err(())
            }
        }
    }

    fn span_from(&self, start: u32) -> Span {
        Span { start, end: self.prev_end().max(start) }
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                Tok::RBrace => {
                    self.bump();
                    return Ok(out);
                }
                Tok::Eof => {
                    self.error(open, "unclosed `{`");
                    return Ok(out);
                }
                _ => out.push(self.stmt_recover()),
            }
        }
    }

    fn module_def(&mut self) -> PResult<Stmt> {
        let start = self.bump().span.start;
        let name = self.ident()?;
        self.expect(&Tok::LParen, "`(`")?;
        let params = self.params()?;
        let body_start = self.span().start;
        let id = self.stmt_id();
        let body = if self.at(&Tok::LBrace) { self.block()? } else { vec![self.stmt_recover()] };
        let body_span = self.span_from(body_start);
        let span = self.span_from(start);
        Ok(Stmt {
            id,
            span,
            modifiers: vec![],
            kind: StmtKind::Module(Rc::new(ModuleDef { name, params, body, span, body_span, stmt: id })),
        })
    }

    fn function_def(&mut self) -> PResult<Stmt> {
        let start = self.bump().span.start;
        let name = self.ident()?;
        self.expect(&Tok::LParen, "`(`")?;
        let params = self.params()?;
        self.expect(&Tok::Assign, "`=`")?;
        let body = self.expr()?;
        self.expect(&Tok::Semi, "`;`")?;
        let span = self.span_from(start);
        let id = self.stmt_id();
        Ok(Stmt {
            id,
            span,
            modifiers: vec![],
            kind: StmtKind::Function(Rc::new(FunctionDef { name, params, body, span })),
        })
    }

    /// Parameter list after `(`, consumes `)`.
    fn params(&mut self) -> PResult<Vec<Param>> {
        let mut out = Vec::new();
        loop {
            if self.eat(&Tok::RParen) {
                return Ok(out);
            }
            let start = self.span().start;
            let name = self.ident()?;
            let default = if self.eat(&Tok::Assign) { Some(self.expr()?) } else { None };
            out.push(Param { name, default, span: self.span_from(start) });
            if !self.eat(&Tok::Comma) {
                self.expect(&Tok::RParen, "`)` or `,`")?;
                return Ok(out);
            }
        }
    }

    /// Argument list after `(`, consumes `)`.
    fn args(&mut self) -> PResult<Vec<Arg>> {
        let mut out = Vec::new();
        loop {
            if self.eat(&Tok::RParen) {
                return Ok(out);
            }
            let start = self.span().start;
            let name = if matches!(self.peek(), Tok::Ident(_)) && self.peek_at(1) == &Tok::Assign {
                let n = self.ident()?;
                self.bump();
                Some(n)
            } else {
                None
            };
            let value = self.expr()?;
            out.push(Arg { name, value, span: self.span_from(start) });
            if !self.eat(&Tok::Comma) {
                self.expect(&Tok::RParen, "`)` or `,`")?;
                return Ok(out);
            }
        }
    }

    fn assignments(&mut self, close: &Tok) -> PResult<Vec<Assignment>> {
        let mut out = Vec::new();
        loop {
            if self.eat(close) {
                return Ok(out);
            }
            let start = self.span().start;
            let name = self.ident()?;
            self.expect(&Tok::Assign, "`=`")?;
            let value = self.expr()?;
            out.push(Assignment { name, value, span: self.span_from(start) });
            if !self.eat(&Tok::Comma) {
                self.expect(close, "closing delimiter or `,`")?;
                return Ok(out);
            }
        }
    }

    fn instantiation(&mut self) -> PResult<Stmt> {
        let start = self.span().start;
        let mut modifiers = Vec::new();
        loop {
            let m = match self.peek() {
                Tok::Not => Modifier::Root,
                Tok::Hash => Modifier::Highlight,
                Tok::Percent => Modifier::Background,
                Tok::Star => Modifier::Disable,
                _ => break,
            };
            modifiers.push(m);
            self.bump();
        }
        let word = match self.peek() {
            Tok::Ident(w) => w.clone(),
            Tok::LBrace if !modifiers.is_empty() => {
                let id = self.stmt_id();
                let body = self.block()?;
                return Ok(Stmt { id, span: self.span_from(start), modifiers, kind: StmtKind::Block(body) });
            }
            _ => {
                let sp = self.span();
                self.error(sp, format!("expected module instantiation, found {}", describe(self.peek())));
                return Err(());
            }
        };
        let kind = match word.as_str() {
            "for" | "intersection_for" => {
                self.bump();
                self.expect(&Tok::LParen, "`(`")?;
                let vars = self.assignments(&Tok::RParen)?;
                let body = self.child_stmt()?;
                StmtKind::For { vars, body: Box::new(body), intersection: word == "intersection_for" }
            }
            "let" => {
                self.bump();
                self.expect(&Tok::LParen, "`(`")?;
                let vars = self.assignments(&Tok::RParen)?;
                let body = self.child_stmt()?;
                StmtKind::Let { vars, body: Box::new(body) }
            }
            "if" => {
                self.bump();
                self.expect(&Tok::LParen, "`(`")?;
                let cond = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                let then = self.child_stmt()?;
                let els = if self.at_ident("else") {
                    self.bump();
                    Some(Box::new(self.child_stmt()?))
                } else {
                    None
                };
                StmtKind::If { cond, then: Box::new(then), els }
            }
            "grid" if matches!(self.peek_at(1), Tok::Ident(_)) && self.peek_at(2) == &Tok::LBrace => {
                StmtKind::Grid(Rc::new(self.grid_decl()?))
            }
            _ => {
                let name = self.ident()?;
                self.expect(&Tok::LParen, "`(`")?;
                let args = self.args()?;
                let (children, has_block) = match self.peek() {
                    Tok::Semi => {
                        self.bump();
                        (vec![], false)
                    }
                    Tok::LBrace => (self.block()?, true),
                    _ => (vec![self.child_stmt()?], false),
                };
                StmtKind::Call { name, args, children, has_block }
            }
        };
        let id = self.stmt_id();
        Ok(Stmt { id, span: self.span_from(start), modifiers, kind })
    }

    /// Child of a transform / control statement: `;`, block, or instantiation.
    fn child_stmt(&mut self) -> PResult<Stmt> {
        match self.peek() {
            Tok::Semi | Tok::LBrace => self.stmt(),
            Tok::Ident(w) if w == "module" || w == "function" => {
                let sp = self.span();
                self.error(sp, "definition not allowed here");
                Err(())
            }
            Tok::Ident(_) if self.peek_at(1) == &Tok::Colon => self.stmt(),
            Tok::Ident(_) if self.peek_at(1) == &Tok::Assign => {
                let sp = self.span();
                self.error(sp, "assignment not allowed as a child");
                Err(())
            }
            _ => self.instantiation(),
        }
    }

    fn grid_decl(&mut self) -> PResult<GridDecl> {
        let start = self.bump().span.start;
        let name = self.ident()?;
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let mut planes = Vec::new();
        loop {
            match self.peek().clone() {
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => {
                    self.error(open, "unclosed grid");
                    return Err(());
                }
                Tok::Ident(ax) => {
                    let axis = match ax.as_str() {
                        "x" => Axis::X,
                        "y" => Axis::Y,
                        "z" => Axis::Z,
                        _ => {
                            let sp = self.span();
                            self.error(sp, "expected axis `x`, `y` or `z`");
                            return Err(());
                        }
                    };
                    self.bump();
                    self.expect(&Tok::Colon, "`:`")?;
                    loop {
                        let pstart = self.span().start;
                        let pname = self.ident()?;
                        self.expect(&Tok::Assign, "`=`")?;
                        let value = self.expr()?;
                        planes.push(PlaneDef { axis, name: pname, value, span: self.span_from(pstart) });
                        if self.eat(&Tok::Comma) {
                            continue;
                        }
                        self.expect(&Tok::Semi, "`;` or `,`")?;
                        break;
                    }
                }
                other => {
                    let sp = self.span();
                    self.error(sp, format!("expected axis, found {}", describe(&other)));
                    return Err(());
                }
            }
        }
        Ok(GridDecl { name, planes, span: self.span_from(start) })
    }

    // ---------------------------------------------------------------- expressions

    pub fn expr(&mut self) -> PResult<Expr> {
        let start = self.span().start;
        if self.at_ident("function") && self.peek_at(1) == &Tok::LParen {
            self.bump();
            self.bump();
            let params = self.params()?;
            let body = self.expr()?;
            return Ok(self.mk_expr(self.span_from(start), ExprKind::FuncLit { params, body: Rc::new(body) }));
        }
        if self.at_ident("let") && self.peek_at(1) == &Tok::LParen {
            self.bump();
            self.bump();
            let vars = self.assignments(&Tok::RParen)?;
            let body = self.expr()?;
            return Ok(self.mk_expr(self.span_from(start), ExprKind::Let { vars, body: Box::new(body) }));
        }
        if (self.at_ident("assert") || self.at_ident("echo")) && self.peek_at(1) == &Tok::LParen {
            let is_assert = self.at_ident("assert");
            self.bump();
            self.bump();
            let args = self.args()?;
            let body = if self.starts_expr() { Some(Box::new(self.expr()?)) } else { None };
            let kind = if is_assert { ExprKind::Assert { args, body } } else { ExprKind::Echo { args, body } };
            return Ok(self.mk_expr(self.span_from(start), kind));
        }
        let cond = self.binary(0)?;
        if self.eat(&Tok::Question) {
            let a = self.expr()?;
            self.expect(&Tok::Colon, "`:`")?;
            let b = self.expr()?;
            return Ok(self.mk_expr(
                self.span_from(start),
                ExprKind::Ternary(Box::new(cond), Box::new(a), Box::new(b)),
            ));
        }
        Ok(cond)
    }

    fn starts_expr(&self) -> bool {
        !matches!(
            self.peek(),
            Tok::Semi | Tok::Comma | Tok::RParen | Tok::RBracket | Tok::RBrace | Tok::Colon | Tok::Eof
        )
    }

    fn binop(&self) -> Option<(BinOp, u8)> {
        Some(match self.peek() {
            Tok::OrOr => (BinOp::Or, 1),
            Tok::AndAnd => (BinOp::And, 2),
            Tok::Eq => (BinOp::Eq, 3),
            Tok::Ne => (BinOp::Ne, 3),
            Tok::Lt => (BinOp::Lt, 4),
            Tok::Le => (BinOp::Le, 4),
            Tok::Gt => (BinOp::Gt, 4),
            Tok::Ge => (BinOp::Ge, 4),
            Tok::Plus => (BinOp::Add, 5),
            Tok::Minus => (BinOp::Sub, 5),
            Tok::Star => (BinOp::Mul, 6),
            Tok::Slash => (BinOp::Div, 6),
            Tok::Percent => (BinOp::Mod, 6),
            Tok::Caret => (BinOp::Pow, 8),
            _ => return None,
        })
    }

    fn binary(&mut self, min: u8) -> PResult<Expr> {
        let start = self.span().start;
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = self.binop() {
            if prec < min {
                break;
            }
            self.bump();
            // `^` is right associative, everything else left.
            let next = if op == BinOp::Pow { prec } else { prec + 1 };
            let rhs = self.binary(next)?;
            lhs = self.mk_expr(self.span_from(start), ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let start = self.span().start;
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Plus => UnOp::Plus,
            Tok::Not => UnOp::Not,
            _ => return self.postfix(),
        };
        self.bump();
        // Unary binds tighter than `*` but looser than `^` (OpenSCAD: -2^2 = -4).
        let e = self.binary(7)?;
        Ok(self.mk_expr(self.span_from(start), ExprKind::Unary(op, Box::new(e))))
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let start = self.span().start;
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Tok::LParen => {
                    self.bump();
                    let args = self.args()?;
                    e = self.mk_expr(self.span_from(start), ExprKind::Call { callee: Box::new(e), args });
                }
                Tok::LBracket => {
                    self.bump();
                    let mut items = Vec::new();
                    loop {
                        if self.at(&Tok::Star)
                            && matches!(self.peek_at(1), Tok::Comma | Tok::RBracket)
                        {
                            items.push(IndexItem::Wild(self.bump().span));
                        } else {
                            items.push(IndexItem::Expr(self.expr()?));
                        }
                        if self.eat(&Tok::Comma) {
                            continue;
                        }
                        self.expect(&Tok::RBracket, "`]`")?;
                        break;
                    }
                    e = self.mk_expr(self.span_from(start), ExprKind::Index { base: Box::new(e), items });
                }
                Tok::Dot => {
                    self.bump();
                    let name = self.ident()?;
                    e = self.mk_expr(self.span_from(start), ExprKind::Member { base: Box::new(e), name });
                }
                _ => return Ok(e),
            }
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let t = self.peek().clone();
        let sp = self.span();
        match t {
            Tok::Number(v) => {
                self.bump();
                Ok(self.mk_expr(sp, ExprKind::Number(v)))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(self.mk_expr(sp, ExprKind::Str(s)))
            }
            Tok::Ident(w) => {
                self.bump();
                let kind = match w.as_str() {
                    "true" => ExprKind::Bool(true),
                    "false" => ExprKind::Bool(false),
                    "undef" => ExprKind::Undef,
                    _ => ExprKind::Ident(w),
                };
                Ok(self.mk_expr(sp, kind))
            }
            Tok::LParen => {
                self.bump();
                let e = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(e)
            }
            Tok::LBracket => self.vector(),
            other => {
                self.error(sp, format!("expected expression, found {}", describe(&other)));
                Err(())
            }
        }
    }

    fn vector(&mut self) -> PResult<Expr> {
        let start = self.bump().span.start;
        if self.eat(&Tok::RBracket) {
            return Ok(self.mk_expr(self.span_from(start), ExprKind::Vector(vec![])));
        }
        // Range or first element.
        if !self.starts_comprehension() {
            let first = self.expr()?;
            if self.eat(&Tok::Colon) {
                let second = self.expr()?;
                let (step, end) = if self.eat(&Tok::Colon) {
                    (Some(Box::new(second)), self.expr()?)
                } else {
                    (None, second)
                };
                self.expect(&Tok::RBracket, "`]`")?;
                return Ok(self.mk_expr(
                    self.span_from(start),
                    ExprKind::Range { start: Box::new(first), step, end: Box::new(end) },
                ));
            }
            let mut elems = vec![VecElem::Expr(first)];
            self.vector_rest(&mut elems)?;
            return Ok(self.mk_expr(self.span_from(start), ExprKind::Vector(elems)));
        }
        let mut elems = vec![self.vec_elem()?];
        self.vector_rest(&mut elems)?;
        Ok(self.mk_expr(self.span_from(start), ExprKind::Vector(elems)))
    }

    fn vector_rest(&mut self, elems: &mut Vec<VecElem>) -> PResult<()> {
        loop {
            if self.eat(&Tok::RBracket) {
                return Ok(());
            }
            self.expect(&Tok::Comma, "`,` or `]`")?;
            if self.eat(&Tok::RBracket) {
                return Ok(());
            }
            elems.push(self.vec_elem()?);
        }
    }

    fn starts_comprehension(&self) -> bool {
        match self.peek() {
            Tok::Ident(w) => {
                matches!(w.as_str(), "for" | "if" | "each")
                    || (w == "let" && self.peek_at(1) == &Tok::LParen && self.let_is_comprehension())
            }
            _ => false,
        }
    }

    /// `let (...) expr` inside a vector is a comprehension element only when followed by
    /// another comprehension keyword; a plain `let` expression is also valid there, and
    /// both evaluate the same for a single value, so treat it as an element.
    fn let_is_comprehension(&self) -> bool {
        true
    }

    fn vec_elem(&mut self) -> PResult<VecElem> {
        let start = self.span().start;
        match self.peek().clone() {
            Tok::Ident(w) if w == "for" => {
                self.bump();
                self.expect(&Tok::LParen, "`(`")?;
                // C-style: for (i = 0; i < n; i = i + 1)
                let init = self.assign_list_until_semi_or_paren()?;
                if self.eat(&Tok::Semi) {
                    let cond = self.expr()?;
                    self.expect(&Tok::Semi, "`;`")?;
                    let update = self.assignments(&Tok::RParen)?;
                    let body = self.vec_elem()?;
                    return Ok(VecElem::ForC {
                        init,
                        cond,
                        update,
                        body: Box::new(body),
                        span: self.span_from(start),
                    });
                }
                self.expect(&Tok::RParen, "`)`")?;
                let body = self.vec_elem()?;
                Ok(VecElem::For { vars: init, body: Box::new(body), span: self.span_from(start) })
            }
            Tok::Ident(w) if w == "if" => {
                self.bump();
                self.expect(&Tok::LParen, "`(`")?;
                let cond = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                let then = self.vec_elem()?;
                let els = if self.at_ident("else") {
                    self.bump();
                    Some(Box::new(self.vec_elem()?))
                } else {
                    None
                };
                Ok(VecElem::If { cond, then: Box::new(then), els, span: self.span_from(start) })
            }
            Tok::Ident(w) if w == "let" && self.peek_at(1) == &Tok::LParen => {
                self.bump();
                self.bump();
                let vars = self.assignments(&Tok::RParen)?;
                let body = self.vec_elem()?;
                Ok(VecElem::Let { vars, body: Box::new(body), span: self.span_from(start) })
            }
            Tok::Ident(w) if w == "each" => {
                self.bump();
                let body = self.vec_elem()?;
                Ok(VecElem::Each(Box::new(body), self.span_from(start)))
            }
            Tok::LParen if self.paren_wraps_comprehension() => {
                self.bump();
                let e = self.vec_elem()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(e)
            }
            _ => Ok(VecElem::Expr(self.expr()?)),
        }
    }

    fn paren_wraps_comprehension(&self) -> bool {
        matches!(self.peek_at(1), Tok::Ident(w) if matches!(w.as_str(), "for" | "if" | "each"))
    }

    fn assign_list_until_semi_or_paren(&mut self) -> PResult<Vec<Assignment>> {
        let mut out = Vec::new();
        loop {
            if matches!(self.peek(), Tok::RParen | Tok::Semi) {
                return Ok(out);
            }
            let start = self.span().start;
            let name = self.ident()?;
            self.expect(&Tok::Assign, "`=`")?;
            let value = self.expr()?;
            out.push(Assignment { name, value, span: self.span_from(start) });
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Number(v) => format!("number `{v}`"),
        Tok::Str(_) => "string".into(),
        Tok::Ident(w) => format!("`{w}`"),
        Tok::Path(_) => "path".into(),
        Tok::Eof => "end of file".into(),
        other => format!("`{}`", tok_text(other)),
    }
}

fn tok_text(t: &Tok) -> &'static str {
    match t {
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBracket => "[",
        Tok::RBracket => "]",
        Tok::LBrace => "{",
        Tok::RBrace => "}",
        Tok::Semi => ";",
        Tok::Comma => ",",
        Tok::Colon => ":",
        Tok::Dot => ".",
        Tok::Question => "?",
        Tok::Assign => "=",
        Tok::Eq => "==",
        Tok::Ne => "!=",
        Tok::Lt => "<",
        Tok::Le => "<=",
        Tok::Gt => ">",
        Tok::Ge => ">=",
        Tok::AndAnd => "&&",
        Tok::OrOr => "||",
        Tok::Not => "!",
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::Percent => "%",
        Tok::Caret => "^",
        Tok::Hash => "#",
        _ => "?",
    }
}
