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

use cad_i18n::{tr, trs};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use cad_syntax::ast::*;
use cad_syntax::{Diagnostic, FileId, LineIndex, MAX_FILE_LEN, MAX_FILES, Span, file_base, file_of};

/// Provides the texts of project files.
pub trait FileResolver {
    /// Text of the file at `path` (normalised project path, `/`-separated, relative to the
    /// project root, or absolute). `Err` with a reason if it cannot be read.
    fn read(&self, path: &str) -> Result<String, String>;

    /// Contents of a data file (`import("logo.svg")`, `text(font = "schrift.ttf")`). By default
    /// the text: projects held as texts (browser, `SetFiles`) store binary files as a
    /// `data:…;base64,…` URL, which is decoded.
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let t = self.read(path)?;
        match decode_data_url(&t) {
            Some(b) => b,
            None => Ok(t.into_bytes()),
        }
    }
}

/// Largest data file read by `import()`/`text()` (bytes).
pub const MAX_ASSET_LEN: usize = 32 << 20;

/// Extensions of data files the loader reads when their names appear as text in the project.
pub const ASSET_EXTENSIONS: &[&str] = &[".svg", ".stl", ".ttf", ".otf", ".ttc", ".otc"];

/// `data:[mime];base64,…` → bytes (`None` if `t` is no base64 data URL).
pub fn decode_data_url(t: &str) -> Option<Result<Vec<u8>, String>> {
    let rest = t.strip_prefix("data:")?;
    let (head, data) = rest.split_once(',')?;
    if !head.ends_with(";base64") {
        return None;
    }
    Some(base64_decode(data.trim()).ok_or_else(|| trs!("ungültige base64-Daten", "invalid base64 data").to_string()))
}

/// A `data:` URL of `bytes` (how binary files are kept in text projects).
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 32);
    out.push_str("data:");
    out.push_str(mime);
    out.push_str(";base64,");
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = acc << 6 | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Contents of a data file (or why it could not be read).
#[derive(Clone)]
pub struct Asset(pub Result<std::sync::Arc<[u8]>, String>);

impl std::fmt::Debug for Asset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Ok(b) => write!(f, "Asset({} Bytes)", b.len()),
            Err(e) => write!(f, "Asset(Err({e}))"),
        }
    }
}

/// No files besides the main file.
pub struct NoFiles;

impl FileResolver for NoFiles {
    fn read(&self, _path: &str) -> Result<String, String> {
        Err(trs!("keine Projektdateien", "no project files").into())
    }
}

impl FileResolver for BTreeMap<String, String> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.get(path).cloned().ok_or_else(|| trs!("nicht im Projekt", "not in the project").into())
    }
}

impl FileResolver for HashMap<String, String> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.get(path).cloned().ok_or_else(|| trs!("nicht im Projekt", "not in the project").into())
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

    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let p = std::path::Path::new(path);
        let full = if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) };
        std::fs::read(&full).map_err(|e| e.to_string())
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

/// Extension of new model files (`mcad`; files of OpenSCAD and from before the rename keep
/// `.scad`, which opens everywhere as well).
pub const MODEL_EXTENSION: &str = "mcad";

/// Extensions of model files, new first: `mcad`, `scad`.
pub const MODEL_EXTENSIONS: &[&str] = &["mcad", "scad"];

/// Default name of a main file without a name (stdin, in-memory sources).
pub const DEFAULT_MAIN: &str = "main.mcad";

/// Whether `path` is a model file (`.mcad` or `.scad`, any case).
pub fn is_model_file(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| MODEL_EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// `path` without its model file extension (`teil.mcad` → `teil`); other paths unchanged.
pub fn strip_model_extension(path: &str) -> &str {
    match path.rsplit_once('.') {
        Some((stem, _)) if is_model_file(path) => stem,
        _ => path,
    }
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
    /// Data files (`import("logo.svg")`, fonts) by normalised project path: every text in the
    /// project's expressions that names a file with an [`ASSET_EXTENSIONS`] extension, resolved
    /// relative to each loaded file's folder (only existing ones are kept, except relative to
    /// the file the name is written in, which keeps the error). Part of the project's content:
    /// a changed SVG changes the evaluation.
    pub assets: BTreeMap<String, Asset>,
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
        let assets = load_assets(&l.files, &main, &l.libs, resolver);
        Project { files: l.files, main, main_uses, libs: l.libs, syntax: l.syntax, link: l.link, assets }
    }

    /// A data file by the path written in `import(…)`/`font = …` at `span`: relative to that
    /// statement's file. `None` if it was not loaded (a name computed at run time).
    pub fn asset(&self, span: Span, rel: &str) -> Option<(String, &Asset)> {
        let path = resolve_path(self.path_of(span), rel);
        self.assets.get(&path).map(|a| (path, a))
    }

    /// A single file without other project files (`include`/`use` report missing files).
    pub fn single(text: &str) -> Project {
        Self::load(DEFAULT_MAIN, text, &NoFiles)
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
            self.link.push(Diagnostic::error(
                at,
                tr!("zu viele Projektdateien (höchstens {MAX_FILES})", "too many project files (at most {MAX_FILES})"),
            ));
            return None;
        }
        if text.len() > MAX_FILE_LEN {
            self.link.push(Diagnostic::error(
                at,
                tr!(
                    "`{path}` ist zu groß (höchstens {MAX_FILE_LEN} Bytes)",
                    "`{path}` is too large (at most {MAX_FILE_LEN} bytes)"
                ),
            ));
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
                            tr!("zyklisches `include`: {}", "cyclic `include`: {}", chain.join(" → ")),
                        ));
                        continue;
                    }
                    match self.read(&target) {
                        Err(why) => self.link.push(Diagnostic::error(
                            path_span,
                            tr!("Datei `{target}` nicht gefunden ({why})", "file `{target}` not found ({why})"),
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
            StmtKind::Let { vars, body } => {
                StmtKind::Let { vars, body: Box::new(self.splice_child(*body, from, stack)) }
            }
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
                self.link.push(Diagnostic::error(
                    at,
                    tr!("Datei `{path}` nicht gefunden ({why})", "file `{path}` not found ({why})"),
                ));
                return None;
            }
        };
        let (id, ast) = self.add_file(path, text, FileRole::Use, at)?;
        let idx = self.libs.len();
        self.libs.push(Library {
            file: id,
            path: path.to_string(),
            ast: File { stmts: Vec::new(), stmt_count: 0, expr_count: 0 },
            uses: Vec::new(),
        });
        self.lib_by_path.insert(path.to_string(), idx);
        let mut stack = vec![path.to_string()];
        let stmts = self.splice(ast.stmts, path, &mut stack);
        let uses = self.uses_of(&stmts, path);
        self.libs[idx].ast = File { stmts, stmt_count: ast.stmt_count, expr_count: ast.expr_count };
        self.libs[idx].uses = uses;
        Some(idx)
    }
}

/// Visits the expressions of a statement (not of its child statements).
fn stmt_exprs<'a>(s: &'a Stmt, f: &mut dyn FnMut(&'a Expr)) {
    match &s.kind {
        StmtKind::Assign { value, .. } => value.walk(f),
        StmtKind::Module(m) => m.params.iter().filter_map(|p| p.default.as_ref()).for_each(|e| e.walk(f)),
        StmtKind::Function(d) => {
            d.params.iter().filter_map(|p| p.default.as_ref()).for_each(|e| e.walk(f));
            d.body.walk(f);
        }
        StmtKind::Anchor { point, dir, .. } => {
            point.walk(f);
            if let Some(d) = dir {
                d.walk(f);
            }
        }
        StmtKind::Named { index: Some(i), .. } => i.walk(f),
        StmtKind::Call { args, .. } => args.iter().for_each(|a| a.value.walk(f)),
        StmtKind::For { vars, .. } | StmtKind::Let { vars, .. } => vars.iter().for_each(|v| v.value.walk(f)),
        StmtKind::If { cond, .. } => cond.walk(f),
        _ => {}
    }
}

/// Loads the data files named by string literals of all loaded files (see [`Project::assets`]).
fn load_assets(
    files: &[SourceFile],
    main: &File,
    libs: &[Library],
    resolver: &dyn FileResolver,
) -> BTreeMap<String, Asset> {
    let mut names: Vec<(String, FileId)> = Vec::new();
    let mut visit = |e: &Expr| {
        if let ExprKind::Str(t) = &e.kind {
            let name = t.trim().split(':').next().unwrap_or("").to_string();
            let l = name.to_ascii_lowercase();
            if ASSET_EXTENSIONS.iter().any(|x| l.ends_with(x)) {
                let f = file_of(e.span.start);
                if !names.iter().any(|(n, g)| *n == name && *g == f) {
                    names.push((name, f));
                }
            }
        }
    };
    for s in main.stmts.iter().chain(libs.iter().flat_map(|l| l.ast.stmts.iter())) {
        s.walk(&mut |s| stmt_exprs(s, &mut visit));
    }
    let mut out = BTreeMap::new();
    for (name, own) in names {
        let own_path = files.get(own as usize).map_or("", |f| f.path.as_str());
        for from in std::iter::once(own_path).chain(files.iter().map(|f| f.path.as_str())) {
            let path = resolve_path(from, &name);
            if out.contains_key(&path) {
                continue;
            }
            let res = resolver.read_bytes(&path).and_then(|b| {
                if b.len() > MAX_ASSET_LEN {
                    Err(tr!(
                        "zu groß ({} Bytes, höchstens {MAX_ASSET_LEN})",
                        "too large ({} bytes, at most {MAX_ASSET_LEN})",
                        b.len()
                    ))
                } else {
                    Ok(std::sync::Arc::<[u8]>::from(b))
                }
            });
            if res.is_ok() || from == own_path {
                out.insert(path, Asset(res));
            }
        }
    }
    out
}

fn has_include(s: &Stmt) -> bool {
    let mut found = false;
    s.walk(&mut |s| found |= matches!(s.kind, StmtKind::Include { is_use: false, .. }));
    found
}
