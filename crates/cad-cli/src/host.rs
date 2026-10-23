//! Where a command runs: in the CLI process itself, or in the daemon (`daemon.rs`) for a
//! client. The commands call these functions instead of touching stdin, the file system and
//! the kernel directly, so the same code runs in both places:
//!
//! * [`new_session`]: in the daemon a session on the daemon's engine (shared cache), in the
//!   CLI a session with a fresh engine. [`Pooled`] gives the engine back when the command's
//!   model is dropped.
//! * [`read_stdin`]: stdin of the CLI process, or the text the client sent.
//! * [`write_file`], [`write_atomic`], [`create_dir_all`]: done at once in the CLI; in the
//!   daemon recorded as [`FileOp`]s that the client carries out (the client writes all files).

use std::cell::RefCell;
use std::collections::HashSet;
use std::io::Read;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cad_engine::{CacheKey, Engine};
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

/// A file system change recorded in the daemon.
#[derive(Debug, Clone)]
pub enum FileOp {
    Mkdir(PathBuf),
    Write(PathBuf, Vec<u8>),
    /// Temporary file + rename, keeping the permissions of an existing file (`cad set`).
    Atomic(PathBuf, Vec<u8>),
}

/// State of a command running in the daemon.
#[derive(Default)]
pub struct Ctx {
    pub stdin: Option<String>,
    pub ops: Vec<FileOp>,
    /// The daemon's engine while no session holds it.
    pub engine: Option<Engine<SmlibKernel>>,
    /// Cache keys touched by the sessions of this command.
    pub touched: HashSet<CacheKey>,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// Stdin read by the CLI before it tried the daemon (used by a local run after a fallback).
static PRELOADED_STDIN: Mutex<Option<String>> = Mutex::new(None);

/// Runs `f` with `ctx` installed on this thread; returns the result and the context.
pub fn with_ctx<R>(ctx: Ctx, f: impl FnOnce() -> R) -> (R, Ctx) {
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
    let r = f();
    let ctx = CTX.with(|c| c.borrow_mut().take()).unwrap_or_default();
    (r, ctx)
}

/// Takes the context off this thread (after a panic inside [`with_ctx`]).
pub fn take_ctx() -> Option<Ctx> {
    CTX.with(|c| c.borrow_mut().take())
}

fn in_daemon() -> bool {
    CTX.with(|c| c.borrow().is_some())
}

pub fn preload_stdin(text: String) {
    *PRELOADED_STDIN.lock().unwrap() = Some(text);
}

/// The whole stdin as text.
pub fn read_stdin() -> std::io::Result<String> {
    if let Some(s) = CTX.with(|c| c.borrow().as_ref().map(|c| c.stdin.clone())) {
        return s.ok_or_else(|| std::io::Error::other("keine Eingabe"));
    }
    if let Some(s) = PRELOADED_STDIN.lock().unwrap().clone() {
        return Ok(s);
    }
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn record(op: FileOp) -> Option<FileOp> {
    CTX.with(|c| match c.borrow_mut().as_mut() {
        Some(ctx) => {
            ctx.ops.push(op);
            None
        }
        None => Some(op),
    })
}

pub fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    match record(FileOp::Write(path.to_path_buf(), bytes.to_vec())) {
        None => Ok(()),
        Some(op) => apply(&op),
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    match record(FileOp::Atomic(path.to_path_buf(), bytes.to_vec())) {
        None => Ok(()),
        Some(op) => apply(&op),
    }
}

pub fn create_dir_all(path: &Path) -> std::io::Result<()> {
    match record(FileOp::Mkdir(path.to_path_buf())) {
        None => Ok(()),
        Some(op) => apply(&op),
    }
}

/// Carries out a file operation (CLI, or the client after the daemon answered).
pub fn apply(op: &FileOp) -> std::io::Result<()> {
    match op {
        FileOp::Mkdir(p) => std::fs::create_dir_all(p),
        FileOp::Write(p, b) => std::fs::write(p, b),
        FileOp::Atomic(path, b) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "datei".into());
            let tmp = path.with_file_name(format!(".{name}.cad-set.tmp"));
            std::fs::write(&tmp, b)?;
            if let Ok(meta) = std::fs::metadata(path) {
                let _ = std::fs::set_permissions(&tmp, meta.permissions());
            }
            std::fs::rename(&tmp, path)
        }
    }
}

/// A session; in the daemon its engine goes back to the daemon when this is dropped.
pub struct Pooled(Option<Session<SmlibKernel>>);

impl Deref for Pooled {
    type Target = Session<SmlibKernel>;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("session")
    }
}

impl DerefMut for Pooled {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("session")
    }
}

impl Drop for Pooled {
    fn drop(&mut self) {
        let Some(s) = self.0.take() else { return };
        if !in_daemon() {
            return;
        }
        let touched = s.touched_keys().clone();
        let engine = s.into_engine();
        CTX.with(|c| {
            if let Some(ctx) = c.borrow_mut().as_mut() {
                ctx.touched.extend(touched);
                // A second model of the same command made its own engine: keep the first.
                if ctx.engine.is_none() {
                    ctx.engine = Some(engine);
                }
            }
        });
    }
}

/// A session for a command: on the daemon's engine if there is one, else on a new engine.
pub fn new_session(opts: SessionOptions) -> Pooled {
    let engine = CTX.with(|c| c.borrow_mut().as_mut().and_then(|ctx| ctx.engine.take()));
    let s = match engine {
        Some(e) => Session::with_engine(e, opts, std_clock()),
        None => Session::new(SmlibKernel::new(), opts, std_clock()),
    };
    Pooled(Some(s))
}
