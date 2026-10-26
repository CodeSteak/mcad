//! The desktop engine: a separate engine process (native SMLib) and its supervisor in the app.
//!
//! **Engine process** ([`host_main`], started as `mcad engine-host`): reads [`cad_ipc`] frames
//! from stdin (event `post` with an array of `ToEngine` messages, `debug_hang {ms}`), runs a
//! `cad_session::Session` in a thread with a 256 MiB stack and the same loop as the browser
//! worker (`web/src/engine/worker.ts`): post the queued messages (a `SetSource` directly
//! followed by another is dropped), then while busy `step(16 ms)` and look for new messages
//! again, so a new `SetSource` aborts the running job. Every round's answers go to stdout as
//! one binary batch ([`crate::wire`]); `ready` is sent once the session exists.
//!
//! **Supervisor** ([`Supervisor`], in the app): forwards UI messages, receives the batches
//! and hands them to a sink (the webview channel). Watchdog like `WorkerTransport` (Plan 7.3):
//! while the engine is busy every round answers within milliseconds; if nothing arrives for
//! [`SupervisorOptions::watchdog`] (15 s) a kernel call hangs. The engine is then replaced:
//! the process is killed and started again (cache lost), the UI gets an `Error` naming the
//! node from the last `Progress.current`, whose cache key is blocked — the new engine first
//! gets `BlockKeys` with every key that hung so far, then the project files, the last
//! `SetContext` and, with a newly blocked node, the last `SetSource`.
//!
//! Why a process and not only a thread: SMLib and its C facade keep global state (handle
//! table, messages); a hanging call cannot be stopped, and a second engine thread next to it
//! would share that state. Killing the process is the native counterpart of
//! `Worker.terminate()`. Inside the process the engine is a thread with a large stack.
//! [`ThreadLauncher`] runs the same engine in a thread of the current process (tests).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cad_i18n::{tr, trs};
use cad_ipc::{Frame, Kind, read_frame, write_frame};
use cad_kernel::Kernel;
use cad_session::protocol::{FromEngine, ToEngine};
use cad_session::{Session, SessionOptions, std_clock};
use serde_json::{Value as Json, json};

use crate::wire;

/// Time budget of one step in ms (like the worker).
pub const STEP_BUDGET_MS: f64 = 16.0;
/// Stack of the engine thread.
pub const ENGINE_STACK: usize = 256 << 20;

/// Input of the engine loop.
pub enum Input {
    Post(ToEngine),
    /// Test aid: blocks the engine like a hanging kernel call.
    Hang(u64),
    /// A message that could not be parsed (the parser's error).
    Bad(String),
}

/// The engine loop (see the module docs). `emit(msgs, busy, seen)` returns `false` to stop;
/// `seen` counts the inputs taken so far (the supervisor knows what is still unanswered).
pub fn engine_loop<K: Kernel>(
    mut session: Session<K>,
    rx: Receiver<Vec<Input>>,
    mut emit: impl FnMut(Vec<FromEngine>, bool, u64) -> bool,
) where
    K::Shape: 'static,
{
    let mut seen = 0u64;
    loop {
        let mut queue = Vec::new();
        if !session.busy() {
            match rx.recv() {
                Ok(m) => queue.extend(m),
                Err(_) => return,
            }
        }
        while let Ok(m) = rx.try_recv() {
            queue.extend(m);
        }
        seen += queue.len() as u64;
        let mut out = Vec::new();
        let src = |m: &Input| matches!(m, Input::Post(ToEngine::SetSource { .. }));
        let skip: Vec<bool> = (0..queue.len()).map(|i| src(&queue[i]) && queue.get(i + 1).is_some_and(src)).collect();
        for (i, m) in queue.into_iter().enumerate() {
            if skip[i] {
                continue;
            }
            match m {
                Input::Post(m) => out.extend(session.post(m)),
                Input::Hang(ms) => {
                    // What came before is answered (e.g. `Progress.current` of a new job).
                    if !out.is_empty() && !emit(std::mem::take(&mut out), true, seen) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(ms))
                }
                Input::Bad(e) => out.push(FromEngine::Error {
                    message: tr!("ungültige Nachricht: {e}", "invalid message: {e}"),
                    version: None,
                    node: None,
                }),
            }
        }
        if session.busy() {
            out.extend(session.step(Some(STEP_BUDGET_MS)));
        }
        let busy = session.busy();
        if !emit(out, busy, seen) {
            return;
        }
    }
}

/// Session options of the desktop engine (deep recursion allowed: 256 MiB stack).
pub fn session_options() -> SessionOptions {
    let mut opts = SessionOptions::default();
    opts.eval.max_stack_bytes = 192 << 20;
    opts
}

/// Main of the engine process: frames on `input`, batches on `output`. Returns the exit code.
pub fn host_main<K, F>(make_kernel: F, input: impl Read + Send + 'static, output: impl Write + Send + 'static) -> i32
where
    K: Kernel,
    K::Shape: 'static,
    F: FnOnce() -> K + Send + 'static,
{
    // One channel message per frame: a batch (e.g. `SetSource` + `__debugHang` in one tick of
    // the webview) reaches the engine loop as a whole, never split between two loop passes.
    let (tx, rx) = channel::<Vec<Input>>();
    std::thread::Builder::new()
        .name("cad-engine-in".into())
        .spawn(move || {
            let mut input = input;
            while let Ok(Some(f)) = read_frame(&mut input) {
                let batch: Vec<Input> = match f.header.method.as_str() {
                    "post" => match &f.header.body {
                        Json::Array(a) => a
                            .iter()
                            .map(|m| {
                                if m["type"] == "__debugHang" {
                                    // Test aid (like the worker): the engine blocks like a hanging kernel call.
                                    return Input::Hang(m["ms"].as_u64().unwrap_or(0));
                                }
                                match serde_json::from_value::<ToEngine>(m.clone()) {
                                    Ok(m) => Input::Post(m),
                                    Err(e) => Input::Bad(e.to_string()),
                                }
                            })
                            .collect(),
                        _ => vec![Input::Bad("post: [ToEngine]".into())],
                    },
                    "debug_hang" => vec![Input::Hang(f.header.body["ms"].as_u64().unwrap_or(0))],
                    _ => continue,
                };
                if tx.send(batch).is_err() {
                    return;
                }
            }
        })
        .expect("thread");
    // the caller's language until the UI sends `SetLang`
    let lang = cad_i18n::lang();
    let engine = std::thread::Builder::new().name("cad-engine".into()).stack_size(ENGINE_STACK).spawn(move || {
        cad_i18n::set_lang(lang);
        let mut output = output;
        let session = Session::new(make_kernel(), session_options(), std_clock());
        if write_frame(&mut output, &Frame::event("ready", json!({ "protocol": cad_session::PROTOCOL_VERSION })))
            .is_err()
        {
            return;
        }
        engine_loop(session, rx, |msgs, busy, seen| {
            let mut f = wire::encode_batch(&msgs, busy);
            f.header.body["seen"] = json!(seen);
            write_frame(&mut output, &f).is_ok()
        });
    });
    match engine {
        Ok(h) => match h.join() {
            Ok(()) => 0,
            Err(_) => 3,
        },
        Err(_) => 3,
    }
}

// ------------------------------------------------------------------------------------------
// supervisor
// ------------------------------------------------------------------------------------------

/// A started engine.
pub struct Launched {
    pub input: Box<dyn Write + Send>,
    pub output: Box<dyn Read + Send>,
    /// Stops the engine (kills the process).
    pub kill: Box<dyn FnMut() + Send>,
}

pub trait Launcher: Send + Sync + 'static {
    fn launch(&self) -> std::io::Result<Launched>;
}

/// Starts `program args…` with piped stdin/stdout (stderr inherited).
pub struct ProcessLauncher {
    pub program: PathBuf,
    pub args: Vec<std::ffi::OsString>,
}

impl Launcher for ProcessLauncher {
    fn launch(&self) -> std::io::Result<Launched> {
        use std::process::{Command, Stdio};
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().expect("stdin");
        let output = child.stdout.take().expect("stdout");
        let child = Arc::new(Mutex::new(child));
        Ok(Launched {
            input: Box::new(input),
            output: Box::new(output),
            kill: Box::new(move || {
                let mut c = child.lock().unwrap();
                let _ = c.kill();
                let _ = c.wait();
            }),
        })
    }
}

/// Runs the engine in a thread of this process (tests; a hung thread cannot be stopped).
pub struct ThreadLauncher<F> {
    pub make_kernel: F,
}

impl<K, F> Launcher for ThreadLauncher<F>
where
    K: Kernel + 'static,
    K::Shape: 'static,
    F: Fn() -> K + Send + Sync + Clone + 'static,
{
    fn launch(&self) -> std::io::Result<Launched> {
        let (in_r, in_w) = std::io::pipe()?;
        let (out_r, out_w) = std::io::pipe()?;
        let mk = self.make_kernel.clone();
        std::thread::spawn(move || host_main(mk, in_r, out_w));
        Ok(Launched { input: Box::new(in_w), output: Box::new(out_r), kill: Box::new(|| {}) })
    }
}

/// "90 s", "20 min".
fn dur_text(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 120 && s.is_multiple_of(60) { format!("{} min", s / 60) } else { format!("{s} s") }
}

/// Default watchdog: a single kernel call may take this long (large models compute for minutes).
pub const WATCHDOG: Duration = Duration::from_secs(20 * 60);
/// Default time allowed for the engine to start.
pub const STARTUP: Duration = Duration::from_secs(3 * 60);

#[derive(Debug, Clone)]
pub struct SupervisorOptions {
    /// A busy engine that does not answer for this long hangs (default [`WATCHDOG`], 20 min).
    pub watchdog: Duration,
    /// Time allowed for the engine to start (default [`STARTUP`], 3 min).
    pub startup: Duration,
}

impl Default for SupervisorOptions {
    fn default() -> Self {
        SupervisorOptions { watchdog: WATCHDOG, startup: STARTUP }
    }
}

/// What the app shows about the engine (test hook, `mcad ui status`).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct HostStatus {
    pub ready: bool,
    pub busy: bool,
    pub restarts: u32,
    pub blocked_keys: Vec<String>,
}

/// Receives the frames for the UI: engine batches (`out`) and `host` status events.
pub type Sink = Arc<dyn Fn(Frame) + Send + Sync>;

#[derive(Clone)]
struct ProgressInfo {
    version: Option<u64>,
    node: Option<u32>,
    key: Option<String>,
    line: Option<u64>,
}

struct State {
    generation: u64,
    input: Option<Box<dyn Write + Send>>,
    kill: Option<Box<dyn FnMut() + Send>>,
    ready: bool,
    pending: Vec<Frame>,
    busy: bool,
    deadline: Option<Instant>,
    last_context: Option<ToEngine>,
    files: Option<(BTreeMap<String, String>, Option<String>)>,
    last_source: Option<ToEngine>,
    last_version: Option<u64>,
    /// The last `SetLang` (replayed first) and the language of the supervisor's own messages.
    last_lang: Option<ToEngine>,
    lang: cad_i18n::Lang,
    blocked: Vec<String>,
    last_progress: Option<ProgressInfo>,
    restarts: u32,
    stopped: bool,
    /// Inputs written to / taken by the current engine (unanswered ones keep it busy).
    sent: u64,
    seen: u64,
}

pub struct Supervisor {
    st: Mutex<State>,
    launcher: Arc<dyn Launcher>,
    sink: Sink,
    opts: SupervisorOptions,
}

impl Supervisor {
    /// Starts the engine, the reader and the watchdog.
    pub fn start(launcher: Arc<dyn Launcher>, sink: Sink, opts: SupervisorOptions) -> Arc<Supervisor> {
        let s = Arc::new(Supervisor {
            st: Mutex::new(State {
                generation: 0,
                input: None,
                kill: None,
                ready: false,
                pending: Vec::new(),
                busy: false,
                deadline: None,
                last_context: None,
                files: None,
                last_source: None,
                last_version: None,
                last_lang: None,
                lang: cad_i18n::lang(),
                blocked: Vec::new(),
                last_progress: None,
                restarts: 0,
                stopped: false,
                sent: 0,
                seen: 0,
            }),
            launcher,
            sink,
            opts,
        });
        {
            let mut st = s.st.lock().unwrap();
            s.launch(&mut st);
        }
        let w = Arc::downgrade(&s);
        std::thread::Builder::new()
            .name("cad-watchdog".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let Some(s) = w.upgrade() else { return };
                    let mut st = s.st.lock().unwrap();
                    if st.stopped {
                        return;
                    }
                    if st.busy && st.deadline.is_some_and(|d| Instant::now() >= d) {
                        let t = if st.ready { s.opts.watchdog } else { s.opts.startup.max(s.opts.watchdog) };
                        let _lang = cad_i18n::scoped(st.lang);
                        let why = tr!(
                            "Engine reagiert seit {} nicht (Kernel hängt)",
                            "Engine not responding for {} (kernel hangs)",
                            dur_text(t)
                        );
                        s.restart(&mut st, &why);
                    }
                }
            })
            .expect("thread");
        s
    }

    fn launch(self: &Arc<Self>, st: &mut State) {
        st.generation += 1;
        st.ready = false;
        st.sent = 0;
        st.seen = 0;
        let generation = st.generation;
        st.busy = true;
        st.deadline = Some(Instant::now() + self.opts.startup.max(self.opts.watchdog));
        match self.launcher.launch() {
            Ok(l) => {
                st.input = Some(l.input);
                st.kill = Some(l.kill);
                let me = Arc::downgrade(self);
                let mut output = l.output;
                std::thread::Builder::new()
                    .name("cad-engine-out".into())
                    .spawn(move || {
                        loop {
                            let f = read_frame(&mut output);
                            let Some(s) = me.upgrade() else { return };
                            match f {
                                Ok(Some(f)) => {
                                    if !s.receive(generation, f) {
                                        return;
                                    }
                                }
                                Ok(None) | Err(_) => {
                                    let mut st = s.st.lock().unwrap();
                                    if st.generation == generation && !st.stopped {
                                        let _lang = cad_i18n::scoped(st.lang);
                                        s.restart(&mut st, trs!("Engine-Prozess beendet", "engine process ended"));
                                    }
                                    return;
                                }
                            }
                        }
                    })
                    .expect("thread");
            }
            Err(e) => {
                st.input = None;
                st.kill = None;
                st.busy = false;
                st.deadline = None;
                let _lang = cad_i18n::scoped(st.lang);
                let message = tr!("Engine startet nicht: {e}", "engine does not start: {e}");
                (self.sink)(wire::encode_batch(&[FromEngine::Error { message, version: None, node: None }], false));
            }
        }
    }

    /// Handles a frame of engine generation `generation`; `false`: stop reading (old engine).
    fn receive(self: &Arc<Self>, generation: u64, f: Frame) -> bool {
        let mut st = self.st.lock().unwrap();
        if st.generation != generation || st.stopped {
            return false;
        }
        match f.header.method.as_str() {
            "ready" => {
                st.ready = true;
                st.busy = false;
                st.deadline = None;
                let pending = std::mem::take(&mut st.pending);
                for p in pending {
                    self.write(&mut st, p);
                }
                drop(st);
                self.send_status();
                true
            }
            wire::OUT => {
                if let Some(seen) = f.header.body["seen"].as_u64() {
                    st.seen = seen;
                }
                let busy = f.header.body["busy"].as_bool().unwrap_or(false) || st.seen < st.sent;
                st.busy = busy;
                st.deadline = busy.then(|| Instant::now() + self.opts.watchdog);
                let msgs = f.header.body["msgs"].as_array().cloned().unwrap_or_default();
                for m in &msgs {
                    match m["type"].as_str() {
                        Some("Progress") => {
                            let cur = &m["current"];
                            st.last_progress = Some(ProgressInfo {
                                version: m["version"].as_u64(),
                                node: cur["node"].as_u64().map(|n| n as u32),
                                key: cur["key"].as_str().map(String::from),
                                line: cur["range"]["line"].as_u64(),
                            });
                        }
                        Some("Scene") if m["complete"].as_bool() == Some(true) => st.last_progress = None,
                        _ => {}
                    }
                }
                drop(st);
                // Empty rounds are only heartbeats for the watchdog.
                if !msgs.is_empty() {
                    (self.sink)(f);
                }
                true
            }
            _ => true,
        }
    }

    fn write(&self, st: &mut State, f: Frame) {
        if !st.ready {
            st.pending.push(f);
            return;
        }
        let ok = st.input.as_mut().is_some_and(|w| write_frame(w, &f).is_ok());
        if ok {
            st.sent += match &f.header.body {
                Json::Array(a) if f.header.method == "post" => a.len() as u64,
                _ => 1,
            };
            // Every message may start work; the watchdog covers the answer.
            st.busy = true;
            st.deadline = Some(Instant::now() + self.opts.watchdog);
        }
    }

    fn record(st: &mut State, m: &ToEngine) {
        match m {
            ToEngine::SetContext { .. } => st.last_context = Some(m.clone()),
            ToEngine::SetFiles { files, main } => st.files = Some((files.clone(), main.clone())),
            ToEngine::SetFile { path, text } => {
                let f = st.files.get_or_insert_with(|| (BTreeMap::new(), None));
                match text {
                    Some(t) => {
                        f.0.insert(path.clone(), t.clone());
                    }
                    None => {
                        f.0.remove(path);
                    }
                }
            }
            ToEngine::SetSource { version, .. } => {
                st.last_version = Some(*version);
                st.last_source = Some(m.clone());
            }
            ToEngine::SetLang { lang } => {
                st.lang = cad_i18n::Lang::parse(lang).unwrap_or_default();
                st.last_lang = Some(m.clone());
            }
            _ => {}
        }
    }

    /// Sends UI messages (in order) to the engine.
    pub fn send(&self, msgs: Vec<ToEngine>) {
        self.send_json(msgs.iter().map(|m| serde_json::to_value(m).unwrap_or(Json::Null)).collect());
    }

    /// Sends UI messages as JSON (from the webview). Entries `{type: "__debugHang", ms}` block
    /// the engine (watchdog test, like the worker); other entries must be `ToEngine`.
    pub fn send_json(&self, msgs: Vec<Json>) {
        if msgs.is_empty() {
            return;
        }
        let mut st = self.st.lock().unwrap();
        if st.stopped {
            return;
        }
        for m in &msgs {
            if let Ok(t) = serde_json::from_value::<ToEngine>(m.clone()) {
                Self::record(&mut st, &t);
            }
        }
        self.write(&mut st, Frame::event("post", Json::Array(msgs)));
    }

    /// Blocks the engine for `ms` (watchdog test).
    pub fn debug_hang(&self, ms: u64) {
        let mut st = self.st.lock().unwrap();
        self.write(&mut st, Frame::event("debug_hang", json!({ "ms": ms })));
    }

    pub fn status(&self) -> HostStatus {
        let st = self.st.lock().unwrap();
        HostStatus { ready: st.ready, busy: st.busy, restarts: st.restarts, blocked_keys: st.blocked.clone() }
    }

    fn send_status(&self) {
        let s = self.status();
        (self.sink)(Frame::event("host", serde_json::to_value(&s).unwrap_or(Json::Null)));
    }

    /// Stops the engine for good.
    pub fn shutdown(&self) {
        let mut st = self.st.lock().unwrap();
        st.stopped = true;
        st.input = None;
        if let Some(mut k) = st.kill.take() {
            k();
        }
    }

    fn restart(self: &Arc<Self>, st: &mut State, reason: &str) {
        let _lang = cad_i18n::scoped(st.lang);
        if let Some(mut k) = st.kill.take() {
            k();
        }
        st.input = None;
        st.restarts += 1;
        let cur = st.last_progress.take();
        let where_ = cur.as_ref().and_then(|c| c.line).map(|l| tr!(" (Zeile {l})", " (line {l})")).unwrap_or_default();
        let key = cur.as_ref().and_then(|c| c.key.clone());
        let fresh = key.as_ref().is_some_and(|k| !st.blocked.contains(k));
        if let Some(k) = &key
            && fresh
        {
            st.blocked.push(k.clone());
        }
        let msg = tr!(
            "{reason}{where_}; Engine neu gestartet, Cache verworfen{}",
            "{reason}{where_}; engine restarted, cache discarded{}",
            if fresh { trs!(", Knoten gesperrt", ", node blocked") } else { "" }
        );
        let version = cur.as_ref().and_then(|c| c.version).or(st.last_version);
        let node = cur.as_ref().and_then(|c| c.node);
        (self.sink)(wire::encode_batch(&[FromEngine::Error { message: msg, version, node }], false));
        let mut replay = Vec::new();
        if let Some(l) = &st.last_lang {
            replay.push(l.clone());
        }
        if !st.blocked.is_empty() {
            replay.push(ToEngine::BlockKeys { keys: st.blocked.clone() });
        }
        if let Some((files, main)) = &st.files {
            replay.push(ToEngine::SetFiles { files: files.clone(), main: main.clone() });
        }
        if let Some(c) = &st.last_context {
            replay.push(c.clone());
        }
        // With a newly blocked node the same source evaluates again (that node reports an error).
        if fresh && let Some(s) = &st.last_source {
            replay.push(s.clone());
        }
        st.pending = if replay.is_empty() {
            Vec::new()
        } else {
            vec![Frame::event("post", serde_json::to_value(&replay).unwrap_or(Json::Null))]
        };
        self.launch(st);
        let s = HostStatus { ready: false, busy: true, restarts: st.restarts, blocked_keys: st.blocked.clone() };
        (self.sink)(Frame::event("host", serde_json::to_value(&s).unwrap_or(Json::Null)));
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Ok(mut st) = self.st.lock()
            && let Some(mut k) = st.kill.take()
        {
            k();
        }
    }
}

/// Output for the engine process: a duplicate of stdout, while stdout itself is redirected
/// to stderr so stray prints of the kernel cannot corrupt the frames.
pub fn protocol_stdout() -> Box<dyn Write + Send> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        // SAFETY: dup/dup2 on the process's own standard descriptors; the new descriptor is
        // owned by the returned File.
        unsafe {
            let fd = libc::dup(1);
            if fd >= 0 {
                libc::dup2(2, 1);
                return Box::new(std::io::BufWriter::with_capacity(1 << 20, std::fs::File::from_raw_fd(fd)));
            }
        }
    }
    #[allow(unreachable_code)]
    Box::new(std::io::BufWriter::with_capacity(1 << 20, std::io::stdout()))
}

/// Header kind check used by readers of engine frames.
pub fn is_batch(f: &Frame) -> bool {
    f.header.kind == Kind::Event && f.header.method == wire::OUT
}
