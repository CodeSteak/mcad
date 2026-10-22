//! Lexer, parser and syntax tree of the CAD language (OpenSCAD plus grids, anchors,
//! named instances and zones).

pub mod ast;
pub mod lexer;
mod lines;
pub mod parser;

pub use lines::LineIndex;

use serde::{Deserialize, Serialize};

/// Byte range into the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start: start as u32, end: end as u32 }
    }

    pub fn contains(&self, pos: u32) -> bool {
        self.start <= pos && pos <= self.end
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn merge(self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub span: Span,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, msg: impl Into<String>) -> Self {
        Diagnostic { span, severity: Severity::Error, message: msg.into() }
    }

    pub fn warning(span: Span, msg: impl Into<String>) -> Self {
        Diagnostic { span, severity: Severity::Warning, message: msg.into() }
    }

    pub fn info(span: Span, msg: impl Into<String>) -> Self {
        Diagnostic { span, severity: Severity::Info, message: msg.into() }
    }
}

/// Parses a whole file. Always returns a tree; syntax errors are in the diagnostics.
pub fn parse(src: &str) -> (ast::File, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let toks = lexer::lex(src, &mut diags);
    let (file, mut pdiags) = parser::Parser::new(&toks).parse_file();
    diags.append(&mut pdiags);
    (file, diags)
}

#[cfg(test)]
mod tests;
