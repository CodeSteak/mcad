//! Projects of several files: `include <datei>` and `use <datei>` with OpenSCAD semantics.
//!
//! * `include <f>` inserts the statements of `f` at its place (definitions, variables and
//!   geometry), as if the text stood there. The included statements are spliced into the
//!   including statement list when the project is loaded ([`Project::load`]); the `include`
//!   statement itself stays as a no-op.
//! * `use <f>` makes the modules and functions of `f` visible in the whole using file (not its
//!   variables, no geometry). They run in `f`'s own file scope: they see `f`'s top-level
//!   variables and the files `f` uses itself (not transitive). Own definitions win over used
//!   ones; of several used files the last `use` wins (as OpenSCAD 2021.01).
//! * Paths are relative to the including file (or absolute). Include cycles and missing files
//!   are errors at the `include`/`use` statement.
//!
//! Every loaded file gets a [`FileId`]; its spans and statement ids live in their own region
//! (`cad_syntax::FILE_SHIFT`), so diagnostics, hints, handles and text edits know their file.
//! A file included twice is loaded twice (two ids, same path); a used file once.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use cad_syntax::ast::*;
use cad_syntax::{Diagnostic, FileId, LineIndex, MAX_FILE_LEN, MAX_FILES, Span, file_base, file_of};

/// Provides the texts of project files.
pub trait FileResolver {
    /// Text of the file at `path` (normalised project path, `/`-separated, relative to the
    /// project root, or absolute). `Err` with a reason if it cannot be read.
    fn read(&self, path: &str) -> Result<String, String>;
}

/// No files besides the main file.
pub struct NoFiles;

impl FileResolver for NoFiles {
    fn read(&self, _path: &str) -> Result<String, String> {
        Err("keine Projektdateien".into())
    }
}

impl FileResolver for BTreeMap<String, String> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.get(path).cloned().ok_or_else(|| "nicht im Projekt".into())
    }
}

impl FileResolver for HashMap<String, String> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.get(path).cloned().ok_or_else(|| "nicht im Projekt".into())
    }
}

/// Files of a directory on disk: project paths are relative to `root` (absolute paths as they
/// are). Used by the CLI and tests.
pub struct FsResolver {
    pub root: std::path::PathBuf,
}

impl FileResolver for FsResolver {
    fn read(&self, path: &str) -> Result<String, String> {
        let p = std::path::Path::new(path);
        let full = if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) };
        std::fs::read_to_string(&full).map_err(|e| e.to_string())
    }
}

/// Normalises a path: `/` separators, no `.` segments, `..` folded where possible (leading
/// `..` of a relative path stay).
pub fn normalize_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    let abs = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !abs {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if abs { format!("/{joined}") } else { joined }
}

/// Path of `rel` (as written in `include <rel>`) seen from the file `from`.
pub fn resolve_path(from: &str, rel: &str) -> String {
    let rel = rel.trim();
    if rel.starts_with('/') || rel.starts_with('\\') {
        return normalize_path(rel);
    }
    let from = from.replace('\\', "/");
    match from.rfind('/') {
        Some(i) => normalize_path(&format!("{}/{rel}", &from[..i])),
        None => normalize_path(rel),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    Main,
    /// Inserted by `include`.
    Include,
    /// Loaded by `use` (a library).
    Use,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    /// Normalised project path.
    pub path: String,
    pub text: String,
    pub lines: LineIndex,
    pub role: FileRole,
    /// `include`/`use` statement that loaded the file (`None` for the main file).
    pub loaded_at: Option<Span>,
}

/// A file loaded by `use`: its statements (includes spliced) and the libraries it uses.
#[derive(Debug, Clone)]
pub struct Library {
    pub file: FileId,
    pub path: String,
    pub ast: File,
    /// Indices into [`Project::libs`], in `use` order.
    pub uses: Vec<usize>,
}

/// A loaded project: all files, the main file's statements with includes spliced in, and the
/// used libraries.
#[derive(Debug, Clone)]
pub struct Project {
    /// Indexed by [`FileId`]; file 0 is the main file.
    pub files: Vec<SourceFile>,
    /// Main file, includes spliced.
    pub main: File,
    /// Libraries the main file uses (indices into `libs`, in `use` order).
    pub main_uses: Vec<usize>,
    pub libs: Vec<Library>,
    /// Syntax errors of all files.
    pub syntax: Vec<Diagnostic>,
    /// Missing files, include cycles, too many files.
    pub link: Vec<Diagnostic>,
}

struct Loader<'r> {
    resolver: &'r dyn FileResolver,
    main_path: String,
    main_text: String,
    files: Vec<SourceFile>,
    libs: Vec<Library>,
    lib_by_path: HashMap<String, usize>,
    syntax: Vec<Diagnostic>,
    link: Vec<Diagnostic>,
}

impl Project {
    /// Loads the main file `main_path` (text given) and everything it includes or uses
    /// through `resolver`.
    pub fn load(main_path: &str, main_text: &str, resolver: &dyn FileResolver) -> Project {
        let main_path = normalize_path(main_path);
        let (ast, syntax) = cad_syntax::parse(main_text);
        Self::load_parsed(&main_path, main_text, ast, syntax, resolver)
    }

    /// Like [`Project::load`] with the main file already parsed (as file 0).
    pub fn load_parsed(
        main_path: &str,
        main_text: &str,
        ast: File,
        syntax: Vec<Diagnostic>,
        resolver: &dyn FileResolver,
    ) -> Project {
        let main_path = normalize_path(main_path);
        let mut l = Loader {
            resolver,
            main_path: main_path.clone(),
            main_text: main_text.to_string(),
            files: vec![SourceFile {
                id: 0,
                path: main_path.clone(),
                text: main_text.to_string(),
                lines: LineIndex::new(main_text),
                role: FileRole::Main,
                loaded_at: None,
            }],
            libs: Vec::new(),
            lib_by_path: HashMap::new(),
            syntax,
            link: Vec::new(),
        };
        let mut stack = vec![main_path.clone()];
        let stmts = l.splice(ast.stmts, &main_path, &mut stack);
        let main = File { stmts, stmt_count: ast.stmt_count, expr_count: ast.expr_count };
        let main_uses = l.uses_of(&main.stmts, &main_path);
        Project {
            files: l.files,
            main,
            main_uses,
            libs: l.libs,
            syntax: l.syntax,
            link: l.link,
        }
    }

    /// A single file without other project files (`include`/`use` report missing files).
    pub fn single(text: &str) -> Project {
        Self::load("main.scad", text, &NoFiles)
    }

    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id as usize)
    }

    /// File of a span.
    pub fn file_of(&self, span: Span) -> Option<&SourceFile> {
        self.file(file_of(span.start))
    }

    /// Source text of a span (empty if out of range).
    pub fn text(&self, span: Span) -> &str {
        let Some(f) = self.file_of(span) else { return "" };
        let l = span.local();
        f.text.get(l.start as usize..l.end as usize).unwrap_or("")
    }

    /// 1-based line of a span's start in its file.
    pub fn line(&self, span: Span) -> u32 {
        match self.file_of(span) {
            Some(f) => f.lines.line_of(span.local().start) + 1,
            None => 0,
        }
    }

    /// Path of the file a span belongs to.
    pub fn path_of(&self, span: Span) -> &str {
        self.file_of(span).map_or("", |f| f.path.as_str())
    }

    /// Text of the main file.
    pub fn main_text(&self) -> &str {
        &self.files[0].text
    }

    /// All diagnostics of loading (syntax first, then link errors).
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.syntax.iter().chain(self.link.iter())
    }
}

/// Text lookup by span for writing edits back ([`crate::drag_edits`]): a plain source text
/// (single file) or a [`Project`].
pub trait SourceText {
    /// Text of `span`, `None` if the span is not inside a file or not on char boundaries.
    fn span_text(&self, span: Span) -> Option<&str>;
}

impl SourceText for str {
    fn span_text(&self, s: Span) -> Option<&str> {
        if s.start > s.end {
            return None;
        }
        self.get(s.start as usize..s.end as usize)
    }
}

impl SourceText for String {
    fn span_text(&self, s: Span) -> Option<&str> {
        self.as_str().span_text(s)
    }
}

impl SourceText for Project {
    fn span_text(&self, s: Span) -> Option<&str> {
        if s.start > s.end || file_of(s.start) != file_of(s.end) {
            return None;
        }
        let f = self.file_of(s)?;
        let l = s.local();
        f.text.get(l.start as usize..l.end as usize)
    }
}

impl Loader<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path == self.main_path {
            return Ok(self.main_text.clone());
        }
        self.resolver.read(path)
    }

    /// Parses `text` as a new file; `None` (with a diagnostic at `at`) if no id is left or the
    /// file is too large.
    fn add_file(&mut self, path: &str, text: String, role: FileRole, at: Span) -> Option<(FileId, File)> {
        let id = self.files.len() as FileId;
        if id > MAX_FILES {
            self.link.push(Diagnostic::error(at, format!("zu viele Projektdateien (höchstens {MAX_FILES})")));
            return None;
        }
        if text.len() > MAX_FILE_LEN {
            self.link.push(Diagnostic::error(at, format!("`{path}` ist zu groß (höchstens {MAX_FILE_LEN} Bytes)")));
            return None;
        }
        let (ast, mut diags) = cad_syntax::parse_file(&text, id);
        self.syntax.append(&mut diags);
        debug_assert_eq!(file_base(id) >> cad_syntax::FILE_SHIFT, id);
        self.files.push(SourceFile {
            id,
            path: path.to_string(),
            lines: LineIndex::new(&text),
            text,
            role,
            loaded_at: Some(at),
        });
        Some((id, ast))
    }

    /// Replaces every `include` in `stmts` (recursively) by the include statement followed by
    /// the included file's statements. `stack`: include chain (cycle detection).
    fn splice(&mut self, stmts: Vec<Stmt>, from: &str, stack: &mut Vec<String>) -> Vec<Stmt> {
        if !stmts.iter().any(has_include) {
            return stmts;
        }
        let mut out = Vec::with_capacity(stmts.len());
        for s in stmts {
            match s.kind {
                StmtKind::Include { ref path, path_span, is_use: false } => {
                    let target = resolve_path(from, path);
                    let at = s.span;
                    out.push(s.clone());
                    if let Some(i) = stack.iter().position(|p| *p == target) {
                        let mut chain: Vec<&str> = stack[i..].iter().map(|s| s.as_str()).collect();
                        chain.push(&target);
                        self.link.push(Diagnostic::error(
                            path_span,
                            format!("zyklisches `include`: {}", chain.join(" → ")),
                        ));
                        continue;
                    }
                    match self.read(&target) {
                        Err(why) => self.link.push(Diagnostic::error(
                            path_span,
                            format!("Datei `{target}` nicht gefunden ({why})"),
                        )),
                        Ok(text) => {
                            if let Some((_, ast)) = self.add_file(&target, text, FileRole::Include, at) {
                                stack.push(target.clone());
                                let inner = self.splice(ast.stmts, &target, stack);
                                stack.pop();
                                out.extend(inner);
                            }
                        }
                    }
                }
                _ => out.push(self.splice_stmt(s, from, stack)),
            }
        }
        out
    }

    fn splice_stmt(&mut self, mut s: Stmt, from: &str, stack: &mut Vec<String>) -> Stmt {
        if !has_include(&s) {
            return s;
        }
        s.kind = match s.kind {
            StmtKind::Block(b) => StmtKind::Block(self.splice(b, from, stack)),
            StmtKind::Module(m) => {
                let mut m = Rc::unwrap_or_clone(m);
                m.body = self.splice(std::mem::take(&mut m.body), from, stack);
                StmtKind::Module(Rc::new(m))
            }
            StmtKind::Call { name, args, children, has_block } => {
                StmtKind::Call { name, args, children: self.splice(children, from, stack), has_block }
            }
            StmtKind::Named { name, index, inst } => {
                StmtKind::Named { name, index, inst: Box::new(self.splice_stmt(*inst, from, stack)) }
            }
            StmtKind::For { vars, body, intersection } => {
                StmtKind::For { vars, body: Box::new(self.splice_child(*body, from, stack)), intersection }
            }
            StmtKind::Let { vars, body } => StmtKind::Let { vars, body: Box::new(self.splice_child(*body, from, stack)) },
            StmtKind::If { cond, then, els } => StmtKind::If {
                cond,
                then: Box::new(self.splice_child(*then, from, stack)),
                els: els.map(|e| Box::new(self.splice_child(*e, from, stack))),
            },
            k => k,
        };
        s
    }

    /// A single child statement that is itself an include becomes a block.
    fn splice_child(&mut self, s: Stmt, from: &str, stack: &mut Vec<String>) -> Stmt {
        if matches!(s.kind, StmtKind::Include { is_use: false, .. }) {
            let (id, span) = (s.id, s.span);
            let stmts = self.splice(vec![s], from, stack);
            return Stmt { id, span, modifiers: vec![], kind: StmtKind::Block(stmts) };
        }
        self.splice_stmt(s, from, stack)
    }

    /// Libraries used by a (spliced) statement list, in source order; loads them.
    fn uses_of(&mut self, stmts: &[Stmt], from: &str) -> Vec<usize> {
        let mut found: Vec<(String, Span)> = Vec::new();
        for s in stmts {
            s.walk(&mut |s| {
                if let StmtKind::Include { path, path_span, is_use: true } = &s.kind {
                    found.push((path.clone(), *path_span));
                }
            });
        }
        let mut out = Vec::new();
        for (path, span) in found {
            // A spliced include brings its `use` statements; their paths are relative to the
            // file they are written in.
            let base = self.files.get(file_of(span.start) as usize).map_or(from.to_string(), |f| f.path.clone());
            let target = resolve_path(&base, &path);
            if let Some(i) = self.load_lib(&target, span) {
                out.retain(|&x| x != i);
                out.push(i);
            }
        }
        out
    }

    fn load_lib(&mut self, path: &str, at: Span) -> Option<usize> {
        if let Some(&i) = self.lib_by_path.get(path) {
            return Some(i);
        }
        let text = match self.read(path) {
            Ok(t) => t,
            Err(why) => {
                self.link.push(Diagnostic::error(at, format!("Datei `{path}` nicht gefunden ({why})")));
                return None;
            }
        };
        let (id, ast) = self.add_file(path, text, FileRole::Use, at)?;
        let idx = self.libs.len();
        self.libs.push(Library { file: id, path: path.to_string(), ast: File { stmts: Vec::new(), stmt_count: 0, expr_count: 0 }, uses: Vec::new() });
        self.lib_by_path.insert(path.to_string(), idx);
        let mut stack = vec![path.to_string()];
        let stmts = self.splice(ast.stmts, path, &mut stack);
        let uses = self.uses_of(&stmts, path);
        self.libs[idx].ast = File { stmts, stmt_count: ast.stmt_count, expr_count: ast.expr_count };
        self.libs[idx].uses = uses;
        Some(idx)
    }
}

fn has_include(s: &Stmt) -> bool {
    let mut found = false;
    s.walk(&mut |s| found |= matches!(s.kind, StmtKind::Include { is_use: false, .. }));
    found
}
