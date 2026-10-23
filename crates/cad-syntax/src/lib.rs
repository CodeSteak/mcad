//! Lexer, parser and syntax tree of the CAD language (OpenSCAD plus grids, anchors,
//! named instances and zones).

pub mod ast;
pub mod lexer;
mod lines;
pub mod parser;

pub use lines::LineIndex;

use serde::{Deserialize, Serialize};

/// Index of a source file of a project (0 = main file).
pub type FileId = u32;

/// Spans and statement ids of file `f` start at `f << FILE_SHIFT` / `f << ID_SHIFT`: every
/// project file has its own region of one global position space (like a source map), so a
/// span alone tells which file it belongs to and ids of different files never collide. A
/// single file is file 0, its positions are plain byte offsets.
pub const FILE_SHIFT: u32 = 24;
/// Largest file (bytes) and number of files a project may have.
pub const MAX_FILE_LEN: usize = (1 << FILE_SHIFT) - 1;
pub const MAX_FILES: u32 = 255;
/// Statement and expression ids of file `f` start at `f << ID_SHIFT`.
pub const ID_SHIFT: u32 = 22;

/// File of a global position.
pub fn file_of(pos: u32) -> FileId {
    pos >> FILE_SHIFT
}

/// First global position of a file.
pub fn file_base(f: FileId) -> u32 {
    f << FILE_SHIFT
}

/// Byte range into the source (global position, see [`FILE_SHIFT`]).
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

    /// File the span belongs to.
    pub fn file(&self) -> FileId {
        file_of(self.start)
    }

    /// Byte offsets within its file.
    pub fn local(&self) -> Span {
        let b = file_base(self.file());
        Span { start: self.start - b, end: self.end.saturating_sub(b) }
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
    parse_file(src, 0)
}

/// Parses file `file` of a project: spans are global positions of that file's region and ids
/// start at `file << ID_SHIFT` (see [`FILE_SHIFT`]). The caller checks [`MAX_FILE_LEN`].
pub fn parse_file(src: &str, file: FileId) -> (ast::File, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut toks = lexer::lex(src, &mut diags);
    let base = file_base(file);
    if base != 0 {
        for t in &mut toks {
            t.span.start += base;
            t.span.end += base;
        }
        for d in &mut diags {
            d.span.start += base;
            d.span.end += base;
        }
    }
    let (file, mut pdiags) = parser::Parser::with_ids(&toks, file << ID_SHIFT).parse_file();
    diags.append(&mut pdiags);
    (file, diags)
}

#[cfg(test)]
mod tests;
