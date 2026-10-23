//! `cad daemon`: a background process that keeps the engine cache between CLI calls.
//!
//! **Start.** The first command that computes (`check`, `eval`, `inspect`, `render`,
//! `export`, `outline`, `measure`, `set`) connects to the endpoint `daemon` of [`cad_ipc`]
//! (socket `<runtime dir>/daemon.sock`, directory 0700, socket 0600, peer uid checked) or
//! starts `cad daemon serve` detached (new session, stdin `/dev/null`, stdout/stderr to
//! `<runtime dir>/daemon.log`), see `client.rs`. Exactly one daemon: it holds an exclusive
//! `flock` on `daemon.lock` for its lifetime; a second one waits briefly for the lock and ends
//! as soon as the first answers.
//!
//! **Requests** (frames of `cad_ipc`): `hello` (protocol, build id, pid), `run` (argv, working
//! directory, stdin in the data part; events `accepted` and `wait` every second while it is
//! queued or running; answer: stdout, stderr, exit code and the files to write, all in the
//! data part), `status`, `stop`. Nothing else is processed; `run` only accepts the commands
//! above (parsed with the CLI's own parser).
//!
//! **Execution.** One worker thread (256 MiB stack, like the CLI) owns one
//! `Engine<SmlibKernel>` for all projects (content keys do not depend on the project) and runs
//! the requests one after another with the CLI's command code (`host.rs`: sessions on the
//! shared engine, stdin from the client, file writes recorded for the client); before each
//! request it changes to the client's working directory, so paths and messages are the same
//! as in the CLI.
//!
//! **Limits.** Cache: `cache_mb` (512) MiB estimated for node results + meshes and
//! `cache_entries` entries (LRU of the engine); `max_projects` (8): the keys touched per
//! project (main file) are recorded, a project beyond the limit loses its entries that no
//! other kept project uses; resident memory (`/proc/self/statm`) above the start value +
//! `rss_factor` × `cache_mb`: the cache is cleared (and freed memory returned with
//! `malloc_trim`). Lifetime: ends after `idle` (15 min) without a request and removes socket
//! and lock. Watchdog: a request running longer than `watchdog` (60 s; a hanging kernel
//! call) ends the daemon; the waiting clients compute the command themselves.
//!
//! **Version.** A client with another build id (version + size and mtime of the executable)
//! stops the daemon and starts its own.

use std::collections::HashSet;
use std::io::Write as _;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cad_engine::{CacheKey, CacheStats, Engine, EngineConfig};
use cad_ipc::{Conn, Frame, Kind, LockFile, Server};
use clap::{Args, Parser, Subcommand};
use kernel_smlib::SmlibKernel;
use serde_json::{Value as Json, json};

use crate::config::DaemonConfig;
use crate::host::{self, Ctx, FileOp};
use crate::{Cli, DaemonInfo, Output};

/// Version of the requests between client and daemon (besides the frame version).
pub const DAEMON_PROTO: u32 = 1;
/// Endpoint name.
pub const ENDPOINT: &str = "daemon";
/// Engine replaced after this many interned tags (see `worker_loop`).
const MAX_TAGS: usize = 1 << 20;
/// Keys remembered per project at most.
const MAX_PROJECT_KEYS: usize = 1 << 17;
/// Interval of the `wait` events while a request is queued or running.
pub const HEARTBEAT: Duration = Duration::from_secs(1);

#[derive(Subcommand, Clone)]
pub enum DaemonCmd {
    /// Daemon im Vordergrund (Fehlersuche; sonst startet ihn der erste rechnende Befehl im
    /// Hintergrund).
    Serve(ServeArgs),
    /// Läuft ein Daemon? PID, Version, Laufzeit, Leerlauf, Cache, Trefferquote, Anfragen.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Daemon beenden (wartet auf den laufenden Befehl).
    Stop {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args, Clone, Default)]
pub struct ServeArgs {
    /// Cache-Grenze in MiB (geschätzt: Knotenergebnisse + Netze; Standard 512).
    #[arg(long)]
    pub cache_mb: Option<f64>,
    /// Ende nach so vielen Minuten ohne Anfrage (Standard 15).
    #[arg(long)]
    pub idle_minutes: Option<f64>,
    /// Wie --idle-minutes, in Sekunden.
    #[arg(long)]
    pub idle_secs: Option<f64>,
    /// Höchstzahl der Projekte, deren Ergebnisse der Cache hält (Standard 8).
    #[arg(long)]
    pub max_projects: Option<usize>,
    /// Watchdog: ein Befehl, der länger läuft, beendet den Daemon (Standard 60 s).
    #[arg(long)]
    pub watchdog_secs: Option<f64>,
}

/// Build id: client and daemon must match (`CAD_DAEMON_BUILD_ID` overrides it, tests).
pub fn build_id() -> String {
    if let Ok(v) = std::env::var("CAD_DAEMON_BUILD_ID")
        && !v.is_empty()
    {
        return v;
    }
    let meta = std::env::current_exe().and_then(std::fs::metadata);
    let (len, mtime) = match meta {
        Ok(m) => (
            m.len(),
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ),
        Err(_) => (0, 0),
    };
    format!("{}+{len:x}.{mtime:x}", env!("CARGO_PKG_VERSION"))
}

/// Commands the daemon runs (all that compute; `watch` keeps its own cache).
pub fn eligible(cmd: &crate::Cmd) -> bool {
    use crate::Cmd as C;
    matches!(
        cmd,
        C::Check { .. }
            | C::Eval { .. }
            | C::Inspect { .. }
            | C::Render { .. }
            | C::Export { .. }
            | C::Outline { .. }
            | C::Measure { .. }
            | C::Set { .. }
    )
}

pub fn command(cmd: &DaemonCmd) -> ExitCode {
    match cmd {
        DaemonCmd::Serve(a) => serve(a),
        DaemonCmd::Status { json } => status(*json),
        DaemonCmd::Stop { json } => stop(*json),
    }
}

// ------------------------------------------------------------------------------------------
// process helpers
// ------------------------------------------------------------------------------------------

/// Resident set size of this process in bytes (`/proc/self/statm`, Linux; else 0).
pub fn rss_bytes() -> u64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/statm") else { return 0 };
    let pages: u64 = s.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
    pages * page_size()
}

fn page_size() -> u64 {
    #[cfg(unix)]
    {
        // SAFETY: sysconf has no preconditions.
        let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if p > 0 {
            return p as u64;
        }
    }
    4096
}

fn trim_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only releases free heap memory.
    unsafe {
        libc::malloc_trim(0);
    }
}

pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks for existence and permission.
        let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

pub fn kill_hard(pid: u32) {
    #[cfg(unix)]
    // SAFETY: plain kill(2) on a pid we got from the daemon itself.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Waits until `pid` is gone (up to `max`).
pub fn wait_gone(pid: u32, max: Duration) -> bool {
    let t0 = Instant::now();
    while pid_alive(pid) {
        if t0.elapsed() > max {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

fn log(msg: &str) {
    let mut e = std::io::stderr().lock();
    let _ = writeln!(e, "[{}] cad daemon {}: {msg}", crate::utc_timestamp(), std::process::id());
}

fn mib(b: u64) -> String {
    format!("{:.1}", b as f64 / (1 << 20) as f64).replace('.', ",")
}

fn dur_text(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{} h {} min", s / 3600, s % 3600 / 60)
    } else if s >= 60 && s.is_multiple_of(60) {
        format!("{} min", s / 60)
    } else if s >= 60 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{} s", s)
    }
}

// ------------------------------------------------------------------------------------------
// server
// ------------------------------------------------------------------------------------------

struct Job {
    argv: Vec<String>,
    cwd: PathBuf,
    stdin: Option<String>,
    hang_ms: u64,
    reply: Sender<Result<Reply, String>>,
}

struct Reply {
    out: Output,
    ops: Vec<FileOp>,
    info: Json,
}

#[derive(Default)]
struct State {
    /// Requests accepted but not answered yet.
    pending: usize,
    busy_since: Option<Instant>,
    current: Option<String>,
    shutting_down: bool,
    /// The watchdog fired: message for the waiting clients.
    dying: Option<String>,
    last_activity: Option<Instant>,
    requests: u64,
    failed: u64,
    cache: CacheStats,
    projects: usize,
    project_evictions: u64,
    rss_clears: u64,
    /// Cache hits/misses of node results and meshes summed over all requests.
    hits: u64,
    misses: u64,
}

struct Shared {
    cfg: DaemonConfig,
    build_id: String,
    started: Instant,
    started_at: String,
    rss_start: u64,
    dir: PathBuf,
    socket: PathBuf,
    socket_ino: Option<u64>,
    state: Mutex<State>,
    lock: Mutex<Option<LockFile>>,
}

impl Shared {
    fn rss_limit(&self) -> u64 {
        self.rss_start + (self.cfg.rss_factor * self.cfg.cache_bytes() as f64) as u64
    }

    /// Removes socket (if still ours) and lock, then exits.
    fn exit(&self, code: i32, why: &str) -> ! {
        log(&format!("Ende ({why})"));
        if self.socket_ino.is_some() && socket_ino(&self.socket) == self.socket_ino {
            let _ = std::fs::remove_file(&self.socket);
        }
        drop(self.lock.lock().unwrap_or_else(|e| e.into_inner()).take());
        std::process::exit(code)
    }

    fn status_json(&self) -> Json {
        let st = self.state.lock().unwrap();
        let idle = st.last_activity.unwrap_or(self.started).elapsed();
        let rss = rss_bytes();
        let lookups = st.hits + st.misses;
        json!({
            "running": true,
            "pid": std::process::id(),
            "version": env!("CARGO_PKG_VERSION"),
            "build_id": self.build_id,
            "proto": DAEMON_PROTO,
            "started_at": self.started_at,
            "uptime_s": self.started.elapsed().as_secs_f64(),
            "idle_s": idle.as_secs_f64(),
            "idle_limit_s": self.cfg.idle.as_secs_f64(),
            "watchdog_s": self.cfg.watchdog.as_secs_f64(),
            "busy": st.busy_since.is_some(),
            "current": st.current,
            "pending": st.pending,
            "requests": st.requests,
            "failed": st.failed,
            "projects": st.projects,
            "max_projects": self.cfg.max_projects,
            "project_evictions": st.project_evictions,
            "cache": {
                "entries": st.cache.nodes + st.cache.meshes,
                "nodes": st.cache.nodes,
                "meshes": st.cache.meshes,
                "bytes": st.cache.node_bytes + st.cache.mesh_bytes,
                "node_bytes": st.cache.node_bytes,
                "mesh_bytes": st.cache.mesh_bytes,
                "limit_bytes": st.cache.limit_bytes,
                "max_entries": st.cache.max_entries,
                "evictions": st.cache.evictions,
                "hits": st.hits,
                "misses": st.misses,
                "hit_rate": if lookups > 0 { Json::from(st.hits as f64 / lookups as f64) } else { Json::Null },
            },
            "rss_bytes": rss,
            "rss_start_bytes": self.rss_start,
            "rss_limit_bytes": self.rss_limit(),
            "rss_clears": st.rss_clears,
            "socket": self.socket.display().to_string(),
            "socket_substitute": cad_ipc::is_substitute(&self.dir, ENDPOINT),
            "log": self.dir.join("daemon.log").display().to_string(),
            "config_file": self.cfg.file.as_ref().map(|p| p.display().to_string()),
        })
    }
}

fn socket_ino(p: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::symlink_metadata(p).ok().map(|m| m.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        None
    }
}

fn serve(a: &ServeArgs) -> ExitCode {
    let mut cfg = DaemonConfig::load();
    if let Some(v) = a.cache_mb
        && !cfg.set_cache_mb(v)
    {
        eprintln!("cad daemon serve: --cache-mb {v}: mindestens 1");
        return ExitCode::from(2);
    }
    if let Some(v) = a.idle_minutes {
        cfg.set_idle(v * 60.0);
    }
    if let Some(v) = a.idle_secs {
        cfg.set_idle(v);
    }
    if let Some(v) = a.max_projects {
        cfg.max_projects = v.max(1);
    }
    if let Some(v) = a.watchdog_secs {
        cfg.set_watchdog(v);
    }
    for w in &cfg.warnings {
        log(&format!("Konfiguration: {w}"));
    }
    let dir = match cad_ipc::runtime_dir() {
        Ok(d) => std::fs::canonicalize(&d).unwrap_or(d),
        Err(e) => {
            eprintln!("cad daemon serve: {e}");
            return ExitCode::from(3);
        }
    };
    // a socket path longer than the Unix limit goes to a short substitute (cad_ipc docs)
    let socket = match cad_ipc::socket_path_in(&dir, ENDPOINT) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cad daemon serve: {e}");
            return ExitCode::from(3);
        }
    };
    if cad_ipc::is_substitute(&dir, ENDPOINT) {
        log(&format!(
            "Socketpfad in {} wäre länger als {} Byte: Ersatz {}",
            dir.display(),
            cad_ipc::MAX_SOCKET_PATH,
            socket.display()
        ));
    }
    let lock_path = dir.join(format!("{ENDPOINT}.lock"));
    // One daemon: the lock. Another start meanwhile waits briefly (a replaced daemon may
    // still finish a request) and ends once a daemon answers.
    let t0 = Instant::now();
    let mut lock = loop {
        match LockFile::try_lock_path(&lock_path) {
            Ok(Some(l)) => break l,
            Ok(None) => {
                if cad_ipc::connect_path(&socket).is_ok() {
                    eprintln!("cad daemon serve: läuft bereits ({})", socket.display());
                    return ExitCode::SUCCESS;
                }
                if t0.elapsed() > Duration::from_secs(10) {
                    eprintln!("cad daemon serve: {} ist gesperrt", lock_path.display());
                    return ExitCode::from(3);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                eprintln!("cad daemon serve: {e}");
                return ExitCode::from(3);
            }
        }
    };
    let build = build_id();
    let _ = lock.write_info(&format!("{}\n{build}\n", std::process::id()));
    let server = match Server::bind_path(&socket) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cad daemon serve: {e}");
            return ExitCode::from(3);
        }
    };
    let shared = Arc::new(Shared {
        build_id: build,
        started: Instant::now(),
        started_at: crate::utc_timestamp(),
        rss_start: rss_bytes(),
        dir: dir.clone(),
        socket_ino: socket_ino(&socket),
        socket,
        state: Mutex::new(State::default()),
        lock: Mutex::new(Some(lock)),
        cfg,
    });
    log(&format!(
        "Start: Version {}, Build {}, Cache {} MiB / {} Einträge, {} Projekte, Leerlauf {} s, Watchdog {} s, RSS {} MiB",
        env!("CARGO_PKG_VERSION"),
        shared.build_id,
        shared.cfg.cache_mb,
        shared.cfg.cache_entries,
        shared.cfg.max_projects,
        shared.cfg.idle.as_secs_f64(),
        shared.cfg.watchdog.as_secs_f64(),
        mib(shared.rss_start)
    ));

    let (tx, rx) = channel::<Job>();
    let w = shared.clone();
    let worker = std::thread::Builder::new()
        .stack_size(crate::EVAL_STACK)
        .name("cad-daemon-engine".into())
        .spawn(move || worker_loop(&w, rx));
    if let Err(e) = worker {
        eprintln!("cad daemon serve: Thread: {e}");
        return ExitCode::from(3);
    }
    let m = shared.clone();
    let _ = std::thread::Builder::new().name("cad-daemon-monitor".into()).spawn(move || monitor(&m));

    loop {
        let conn = match server.accept() {
            Ok(c) => c,
            Err(cad_ipc::Error::Unsafe(e)) => {
                log(&format!("abgewiesen: {e}"));
                continue;
            }
            Err(e) => {
                log(&format!("accept: {e}"));
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        let s = shared.clone();
        let tx = tx.clone();
        let _ = std::thread::Builder::new().name("cad-daemon-conn".into()).spawn(move || handle(&s, conn, &tx));
    }
}

/// Idle end, watchdog, socket check.
fn monitor(s: &Arc<Shared>) {
    let tick = Duration::from_millis(50).min(s.cfg.idle / 4).max(Duration::from_millis(5));
    let mut last_sock = Instant::now();
    loop {
        std::thread::sleep(tick);
        let mut st = s.state.lock().unwrap();
        if let Some(t) = st.busy_since
            && t.elapsed() > s.cfg.watchdog
        {
            let msg = format!(
                "Watchdog: `cad {}` läuft seit {} s (hängender Kernel-Aufruf?), Daemon beendet",
                st.current.clone().unwrap_or_default(),
                s.cfg.watchdog.as_secs()
            );
            log(&msg);
            st.dying = Some(msg);
            st.shutting_down = true;
            drop(st);
            // Let the waiting connections answer, then end (the kernel call cannot be stopped).
            std::thread::sleep(HEARTBEAT + Duration::from_millis(300));
            s.exit(70, "Watchdog");
        }
        let idle = st.last_activity.unwrap_or(s.started).elapsed();
        if st.pending == 0 && !st.shutting_down && idle > s.cfg.idle {
            st.shutting_down = true;
            drop(st);
            s.exit(0, &format!("{} ohne Anfrage", dur_text(s.cfg.idle)));
        }
        drop(st);
        if last_sock.elapsed() > Duration::from_millis(500) {
            last_sock = Instant::now();
            if socket_ino(&s.socket) != s.socket_ino {
                s.exit(0, "Socket entfernt oder ersetzt");
            }
        }
    }
}

fn handle(s: &Arc<Shared>, mut conn: Conn, tx: &Sender<Job>) {
    let _ = conn.set_timeout(None);
    loop {
        let f = match conn.recv() {
            Ok(Some(f)) => f,
            _ => return,
        };
        if f.header.kind != Kind::Request {
            continue;
        }
        let h = f.header.clone();
        let r = match h.method.as_str() {
            "hello" => conn.respond(
                &h,
                json!({"proto": DAEMON_PROTO, "build_id": s.build_id, "version": env!("CARGO_PKG_VERSION"),
                       "pid": std::process::id()}),
                Vec::new(),
            ),
            "status" => conn.respond(&h, s.status_json(), Vec::new()),
            "stop" => {
                let mut st = s.state.lock().unwrap();
                st.shutting_down = true;
                let idle = st.pending == 0;
                drop(st);
                let _ = conn.respond(&h, json!({"pid": std::process::id()}), Vec::new());
                if idle {
                    s.exit(0, "cad daemon stop");
                }
                log("Stopp angefordert, warte auf laufende Befehle");
                Ok(())
            }
            "run" => run_request(s, &mut conn, &f, tx),
            m => conn.respond_err(&h, &format!("unbekannte Anfrage `{m}`")),
        };
        if r.is_err() {
            return;
        }
    }
}

fn run_request(s: &Arc<Shared>, conn: &mut Conn, f: &Frame, tx: &Sender<Job>) -> cad_ipc::Result<()> {
    let h = &f.header;
    let b = &h.body;
    if b["proto"].as_u64() != Some(DAEMON_PROTO as u64) || b["build_id"].as_str() != Some(s.build_id.as_str()) {
        return conn.respond_err(h, &format!("version: Daemon hat Build {}", s.build_id));
    }
    let argv: Vec<String> = b["argv"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let Some(cwd) = b["cwd"].as_str().map(PathBuf::from) else {
        return conn.respond_err(h, "run: cwd fehlt");
    };
    let stdin = if b["stdin"].as_bool() == Some(true) {
        match String::from_utf8(f.data.clone()) {
            Ok(t) => Some(t),
            Err(_) => return conn.respond_err(h, "run: stdin ist kein UTF-8"),
        }
    } else {
        None
    };
    let (rtx, rrx) = channel();
    {
        let mut st = s.state.lock().unwrap();
        if st.shutting_down {
            drop(st);
            return conn.respond_err(h, "shutdown: Daemon endet");
        }
        st.pending += 1;
        let job = Job { argv, cwd, stdin, hang_ms: b["hang_ms"].as_u64().unwrap_or(0), reply: rtx };
        if tx.send(job).is_err() {
            st.pending -= 1;
            drop(st);
            return conn.respond_err(h, "shutdown: Daemon-Thread beendet");
        }
    }
    let _ = conn.send_event("accepted", json!({}));
    loop {
        match rrx.recv_timeout(HEARTBEAT) {
            Ok(Ok(r)) => {
                let mut files = Vec::new();
                let mut data = Vec::with_capacity(r.out.stdout.len() + r.out.stderr.len());
                data.extend_from_slice(&r.out.stdout);
                data.extend_from_slice(&r.out.stderr);
                for op in &r.ops {
                    match op {
                        FileOp::Mkdir(p) => files.push(json!({"op": "mkdir", "path": p.to_string_lossy(), "len": 0})),
                        FileOp::Write(p, bytes) | FileOp::Atomic(p, bytes) => {
                            let kind = if matches!(op, FileOp::Write(..)) { "write" } else { "atomic" };
                            files.push(json!({"op": kind, "path": p.to_string_lossy(), "len": bytes.len()}));
                            data.extend_from_slice(bytes);
                        }
                    }
                }
                let body = json!({
                    "exit_code": r.out.code, "stdout_len": r.out.stdout.len(), "stderr_len": r.out.stderr.len(),
                    "files": files, "info": r.info,
                });
                return conn.respond(h, body, data);
            }
            Ok(Err(e)) => return conn.respond_err(h, &e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Some(msg) = s.state.lock().unwrap().dying.clone() {
                    return conn.respond_err(h, &format!("watchdog: {msg}"));
                }
                if conn.send_event("wait", json!({})).is_err() {
                    // client gone; the job still runs (its answer is dropped)
                    return Err(cad_ipc::Error::Protocol("Client weg".into()));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let msg = s.state.lock().unwrap().dying.clone().unwrap_or_else(|| "Daemon-Thread beendet".into());
                return conn.respond_err(h, &format!("watchdog: {msg}"));
            }
        }
    }
}

/// Bytes in use on the heap (glibc: `mallinfo2`, small and mmapped blocks).
pub fn heap_in_use() -> Option<usize> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: plain glibc statistics call.
        let m = unsafe { libc::mallinfo2() };
        Some(m.uordblks + m.hblkhd)
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    None
}

fn new_engine(cfg: &DaemonConfig) -> Engine<SmlibKernel> {
    let config =
        EngineConfig { cache_bytes: cfg.cache_bytes(), cache_entries: cfg.cache_entries, ..Default::default() };
    let mut e = Engine::with_config(SmlibKernel::new(), config);
    // Cost of a node result: what its evaluation kept on the heap (at least the estimate).
    if heap_in_use().is_some() {
        e.set_heap_probe(Some(Box::new(|| heap_in_use().unwrap_or(0))));
    }
    e
}

/// Main file of a command, as a project id (absolute path; stdin per working directory).
fn project_id(cmd: &crate::Cmd, cwd: &Path) -> Option<String> {
    use crate::Cmd as C;
    let file = match cmd {
        C::Check { c, .. }
        | C::Eval { c }
        | C::Inspect { c, .. }
        | C::Render { c, .. }
        | C::Export { c, .. }
        | C::Outline { c, .. }
        | C::Measure { c, .. } => &c.file,
        C::Set { a } => &a.file,
        _ => return None,
    };
    if file.as_os_str() == "-" {
        return Some(format!("<stdin>@{}", cwd.display()));
    }
    let p = cwd.join(file);
    Some(std::fs::canonicalize(&p).unwrap_or(p).display().to_string())
}

fn worker_loop(s: &Arc<Shared>, rx: Receiver<Job>) {
    let mut engine = Some(new_engine(&s.cfg));
    // Projects in order of use (last = newest) with the keys their commands touched.
    let mut projects: Vec<(String, HashSet<CacheKey>)> = Vec::new();
    while let Ok(job) = rx.recv() {
        {
            let mut st = s.state.lock().unwrap();
            st.busy_since = Some(Instant::now());
            st.current = Some(job.argv.join(" "));
        }
        if job.hang_ms > 0 {
            // test aid: behaves like a hanging kernel call
            std::thread::sleep(Duration::from_millis(job.hang_ms));
        }
        let t0 = Instant::now();
        let eng = engine.take().unwrap_or_else(|| new_engine(&s.cfg));
        let before = eng.cache_stats();
        let res = execute(&job, eng);
        let (reply, ctx, project, panicked) = match res {
            Ok(r) => r,
            Err(e) => {
                let mut st = s.state.lock().unwrap();
                st.failed += 1;
                finish_job(s, st);
                let _ = job.reply.send(Err(e));
                engine = Some(new_engine(&s.cfg));
                continue;
            }
        };
        let mut eng = match ctx.engine {
            // The tag table of an engine only grows (one entry per node content ever seen):
            // start afresh after very long use.
            Some(e) if !panicked && e.tag_count() < MAX_TAGS => e,
            Some(_) if !panicked => {
                log("Tag-Tabelle voll: neue Engine (Cache geleert)");
                new_engine(&s.cfg)
            }
            _ => new_engine(&s.cfg),
        };
        // projects
        let mut project_evictions = 0;
        if let Some(id) = project {
            let mut keys = match projects.iter().position(|(p, _)| *p == id) {
                Some(i) => projects.remove(i).1,
                None => HashSet::new(),
            };
            if keys.len() > MAX_PROJECT_KEYS {
                // only used to evict the project later; old states stay to the LRU
                keys.clear();
            }
            keys.extend(ctx.touched);
            projects.push((id, keys));
            while projects.len() > s.cfg.max_projects {
                let (old, keys) = projects.remove(0);
                let keep: HashSet<&CacheKey> = projects.iter().flat_map(|(_, k)| k.iter()).collect();
                let drop: HashSet<CacheKey> = keys.into_iter().filter(|k| !keep.contains(k)).collect();
                let n = eng.evict_keys(&drop);
                project_evictions += 1;
                log(&format!("Projekt {old} verdrängt ({n} Einträge)"));
            }
        }
        // memory
        let rss = rss_bytes();
        let mut rss_clear = false;
        if rss > s.rss_limit() {
            eng.clear_cache();
            trim_heap();
            rss_clear = true;
            log(&format!(
                "RSS {} MiB über {} MiB: Cache geleert, jetzt {} MiB",
                mib(rss),
                mib(s.rss_limit()),
                mib(rss_bytes())
            ));
        }
        let after = eng.cache_stats();
        let hits = after.hits - before.hits.min(after.hits);
        let misses = after.misses - before.misses.min(after.misses);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        log(&format!(
            "`cad {}` → {} in {:.0} ms, Cache {} Treffer / {} neu, {} Einträge, {} MiB",
            job.argv.join(" "),
            reply.out.code,
            ms,
            hits,
            misses,
            after.nodes + after.meshes,
            mib((after.node_bytes + after.mesh_bytes) as u64)
        ));
        let info = json!({"pid": std::process::id(), "ms": ms, "cache_hits": hits, "cache_misses": misses,
                          "cache_entries": after.nodes + after.meshes, "cache_bytes": after.node_bytes + after.mesh_bytes});
        engine = Some(eng);
        {
            let mut st = s.state.lock().unwrap();
            st.requests += 1;
            st.hits += hits;
            st.misses += misses;
            st.cache = after;
            st.projects = projects.len();
            st.project_evictions += project_evictions;
            st.rss_clears += rss_clear as u64;
            if panicked {
                st.failed += 1;
            }
            finish_job(s, st);
        }
        let _ = job.reply.send(Ok(Reply { out: reply.out, ops: ctx.ops, info }));
        let st = s.state.lock().unwrap();
        if st.shutting_down && st.pending == 0 && st.dying.is_none() {
            drop(st);
            // give the connection thread a moment to send the answer
            std::thread::sleep(Duration::from_millis(100));
            s.exit(0, "nach Stopp");
        }
    }
}

fn finish_job(_s: &Shared, mut st: std::sync::MutexGuard<'_, State>) {
    st.pending = st.pending.saturating_sub(1);
    st.busy_since = None;
    st.current = None;
    st.last_activity = Some(Instant::now());
}

type Executed = (Reply, Ctx, Option<String>, bool);

/// Runs one request with the CLI's command code (in the client's working directory).
fn execute(job: &Job, engine: Engine<SmlibKernel>) -> Result<Executed, String> {
    let cli = Cli::try_parse_from(std::iter::once("cad".to_string()).chain(job.argv.iter().cloned()))
        .map_err(|e| format!("Argumente: {e}"))?;
    let Some(cmd) = cli.cmd else { return Err("kein Befehl".into()) };
    if !eligible(&cmd) {
        return Err(format!("`cad {}` läuft nicht im Daemon", crate::command_name(&cmd)));
    }
    std::env::set_current_dir(&job.cwd).map_err(|e| format!("{}: {e}", job.cwd.display()))?;
    let project = project_id(&cmd, &job.cwd);
    let ctx = Ctx { stdin: job.stdin.clone(), engine: Some(engine), ..Default::default() };
    let name = crate::command_name(&cmd);
    let json_mode = crate::json_mode(&cmd);
    let res = std::panic::catch_unwind(AssertUnwindSafe(|| host::with_ctx(ctx, || crate::run(&cmd))));
    let _ = std::env::set_current_dir("/");
    let info = DaemonInfo { used: true, reason: None };
    let (result, ctx, panicked) = match res {
        Ok((r, ctx)) => (r, ctx, false),
        Err(e) => {
            let ctx = host::take_ctx().unwrap_or_default();
            (Err(crate::Fail::Kernel(format!("interner Fehler: {}", crate::panic_text(&*e)))), ctx, true)
        }
    };
    let out = crate::finish(name, json_mode, result, Some(&info));
    Ok((Reply { out, ops: Vec::new(), info: Json::Null }, ctx, project, panicked))
}

// ------------------------------------------------------------------------------------------
// status / stop
// ------------------------------------------------------------------------------------------

fn not_running_json(cmd: &str) -> Json {
    json!({"schema_version": cad_session::PROTOCOL_VERSION, "command": cmd, "ok": true, "exit_code": 0, "running": false,
           "socket": cad_ipc::endpoint_path(ENDPOINT).ok().map(|p| p.display().to_string()),
           "socket_substitute": cad_ipc::runtime_dir().is_ok_and(|d| cad_ipc::is_substitute(&d, ENDPOINT))})
}

fn status(as_json: bool) -> ExitCode {
    let mut conn = match cad_ipc::connect(ENDPOINT) {
        Ok(c) => c,
        Err(cad_ipc::Error::NotRunning(p)) => {
            if as_json {
                println!("{}", serde_json::to_string_pretty(&not_running_json("daemon status")).unwrap());
            } else {
                println!("kein Daemon ({})", p.display());
            }
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("cad daemon status: {e}");
            return ExitCode::from(3);
        }
    };
    let _ = conn.set_timeout(Some(Duration::from_secs(5)));
    let st = match conn.request("status", json!({})) {
        Ok(f) => f.header.body,
        Err(e) => {
            eprintln!("cad daemon status: {e}");
            return ExitCode::from(3);
        }
    };
    if as_json {
        let mut j = st.clone();
        j["schema_version"] = json!(cad_session::PROTOCOL_VERSION);
        j["command"] = json!("daemon status");
        j["ok"] = json!(true);
        j["exit_code"] = json!(0);
        j["client_build_id"] = json!(build_id());
        println!("{}", serde_json::to_string_pretty(&j).unwrap());
        return ExitCode::SUCCESS;
    }
    let f = |k: &str| st[k].as_f64().unwrap_or(0.0);
    let c = &st["cache"];
    let cf = |k: &str| c[k].as_f64().unwrap_or(0.0);
    let mut t = String::new();
    t.push_str(&format!(
        "Daemon läuft: pid {}, Version {} (Build {}{})\n",
        st["pid"],
        st["version"].as_str().unwrap_or("?"),
        st["build_id"].as_str().unwrap_or("?"),
        if st["build_id"].as_str() == Some(build_id().as_str()) { "" } else { ", anders als dieses cad" }
    ));
    t.push_str(&format!(
        "Laufzeit {}, Leerlauf {} (Ende nach {} ohne Anfrage), Watchdog {} s\n",
        dur_text(Duration::from_secs_f64(f("uptime_s"))),
        dur_text(Duration::from_secs_f64(f("idle_s"))),
        dur_text(Duration::from_secs_f64(f("idle_limit_s"))),
        f("watchdog_s")
    ));
    t.push_str(&format!(
        "Anfragen: {}{}, Projekte {} von {} ({} verdrängt){}\n",
        st["requests"],
        if f("failed") > 0.0 { format!(" ({} fehlgeschlagen)", st["failed"]) } else { String::new() },
        st["projects"],
        st["max_projects"],
        st["project_evictions"],
        st["current"].as_str().map(|c| format!(", läuft: cad {c}")).unwrap_or_default()
    ));
    let rate = c["hit_rate"].as_f64().map(|r| format!("{:.0} %", r * 100.0)).unwrap_or_else(|| "–".into());
    t.push_str(&format!(
        "Cache: {} Knoten + {} Netze/abgeleitete, {} MiB von {} MiB, Trefferquote {rate} ({} Treffer, {} neu berechnet), {} verdrängt\n",
        c["nodes"],
        c["meshes"],
        mib(cf("bytes") as u64),
        mib(cf("limit_bytes") as u64),
        c["hits"],
        c["misses"],
        c["evictions"]
    ));
    t.push_str(&format!(
        "Speicher (RSS): {} MiB, Start {} MiB, Cache wird geleert über {} MiB ({}× geleert)\n",
        mib(f("rss_bytes") as u64),
        mib(f("rss_start_bytes") as u64),
        mib(f("rss_limit_bytes") as u64),
        st["rss_clears"]
    ));
    t.push_str(&format!(
        "Socket {}{}, Log {}\n",
        st["socket"].as_str().unwrap_or(""),
        if st["socket_substitute"].as_bool() == Some(true) {
            format!(" (Ersatzpfad: Pfad im IPC-Verzeichnis länger als {} Byte)", cad_ipc::MAX_SOCKET_PATH)
        } else {
            String::new()
        },
        st["log"].as_str().unwrap_or("")
    ));
    print!("{t}");
    ExitCode::SUCCESS
}

/// Stops a running daemon; `Ok(Some(pid))` if one was stopped.
pub fn stop_daemon(conn: Option<Conn>, wait: Duration) -> Result<Option<u32>, String> {
    let mut conn = match conn {
        Some(c) => c,
        None => match cad_ipc::connect(ENDPOINT) {
            Ok(c) => c,
            Err(cad_ipc::Error::NotRunning(_)) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        },
    };
    let _ = conn.set_timeout(Some(Duration::from_secs(3)));
    let pid = match conn.request("stop", json!({})) {
        Ok(f) => f.header.body["pid"].as_u64().map(|p| p as u32),
        Err(_) => lock_pid(),
    };
    let Some(pid) = pid else { return Err("Daemon antwortet nicht, PID unbekannt".into()) };
    if !wait_gone(pid, wait) {
        kill_hard(pid);
        wait_gone(pid, Duration::from_secs(2));
    }
    Ok(Some(pid))
}

/// PID from the lock file.
pub fn lock_pid() -> Option<u32> {
    let p = cad_ipc::runtime_dir().ok()?.join(format!("{ENDPOINT}.lock"));
    LockFile::read_info(&p)?.lines().next()?.trim().parse().ok()
}

fn stop(as_json: bool) -> ExitCode {
    let r = stop_daemon(None, Duration::from_secs(30));
    let (code, text, j) = match r {
        Ok(None) => (0, "kein Daemon\n".to_string(), json!({"stopped": false, "pid": null})),
        Ok(Some(pid)) => (0, format!("Daemon beendet (pid {pid})\n"), json!({"stopped": true, "pid": pid})),
        Err(e) => (3, format!("cad daemon stop: {e}\n"), json!({"stopped": false, "error": e})),
    };
    if as_json {
        let mut j = j;
        j["schema_version"] = json!(cad_session::PROTOCOL_VERSION);
        j["command"] = json!("daemon stop");
        j["ok"] = json!(code == 0);
        j["exit_code"] = json!(code);
        println!("{}", serde_json::to_string_pretty(&j).unwrap());
    } else if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    ExitCode::from(code)
}
