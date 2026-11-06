//! Client side of the daemon (`daemon.rs`): connect or start it, send the command, write
//! stdout, stderr and the files of the answer. Every problem before the daemon accepted the
//! command falls back silently to computing in this process (`--verbose` says why, `--json`
//! has `daemon: {used, reason}`); a daemon that fails while running the command (watchdog,
//! crash, no heartbeat) is reported on stderr and the command is computed here again.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use cad_i18n::{tr, trs};
use cad_ipc::{Conn, Frame, Kind};
use serde_json::{Value as Json, json};

use crate::config::DaemonConfig;
use crate::daemon::{self, DAEMON_PROTO, ENDPOINT, build_id};
use crate::host::{self, FileOp};
use crate::{DaemonInfo, Output};

pub enum Attempt {
    /// The daemon computed the command; print this.
    Done(Output),
    /// Compute here.
    Local(DaemonInfo),
}

/// Time the client waits for a daemon it started.
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// Answer to `hello`.
const HELLO_TIMEOUT: Duration = Duration::from_secs(2);
/// Without any frame (the daemon sends `wait` every second) for this long: no daemon.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(10);

enum Fail {
    /// Before the daemon accepted the command: try once more (a daemon was just ending).
    Retry(String),
    /// Not usable: compute here, silently.
    Quiet(String),
    /// The daemon failed with the command: compute here and say so.
    Loud(String),
}

struct Hello {
    build_id: String,
    pid: u32,
}

fn hello(conn: &mut Conn) -> Result<Hello, String> {
    let _ = conn.set_timeout(Some(HELLO_TIMEOUT));
    let f = conn.request("hello", json!({"proto": DAEMON_PROTO})).map_err(|e| format!("hello: {e}"))?;
    let b = &f.header.body;
    if b["proto"].as_u64() != Some(DAEMON_PROTO as u64) {
        return Ok(Hello {
            build_id: tr!("Protokoll {}", "protocol {}", b["proto"]),
            pid: b["pid"].as_u64().unwrap_or(0) as u32,
        });
    }
    Ok(Hello {
        build_id: b["build_id"].as_str().unwrap_or_default().to_string(),
        pid: b["pid"].as_u64().unwrap_or(0) as u32,
    })
}

/// Connects to the daemon; `Ok(None)`: none running.
fn connect() -> Result<Option<(Conn, Hello)>, String> {
    match cad_ipc::connect(ENDPOINT) {
        Ok(mut c) => {
            let h = hello(&mut c)?;
            Ok(Some((c, h)))
        }
        Err(cad_ipc::Error::NotRunning(_)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Starts `mcad daemon serve` detached: new session, stdin `/dev/null`, output to the log.
fn spawn(dir: &Path) -> std::io::Result<Child> {
    let exe = std::env::current_exe()?;
    let log = dir.join("daemon.log");
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::rename(&log, dir.join("daemon.log.1"));
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let out = opts.open(&log)?;
    let mut cmd = Command::new(exe);
    cmd.args(["daemon", "serve"])
        .env("MCAD_IPC_DIR", dir)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe; nothing else runs between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    cmd.spawn()
}

/// A connection to a daemon of this build: running one, or a new one.
fn ensure(verbose: bool) -> Result<(Conn, u32), String> {
    let me = build_id();
    match connect()? {
        Some((c, h)) if h.build_id == me => return Ok((c, h.pid)),
        Some((c, h)) => {
            if verbose {
                eprintln!(
                    "{}",
                    tr!(
                        "mcad: Daemon pid {} hat Build {}, dieses mcad {me}: wird ersetzt",
                        "mcad: daemon pid {} has build {}, this mcad {me}: replacing it",
                        h.pid,
                        h.build_id
                    )
                );
            }
            daemon::stop_daemon(Some(c), Duration::from_secs(3))
                .map_err(|e| tr!("alter Daemon: {e}", "old daemon: {e}"))?;
        }
        None => {}
    }
    let dir = cad_ipc::runtime_dir().map_err(|e| e.to_string())?;
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let mut child = spawn(&dir).map_err(|e| tr!("Start: {e}", "start: {e}"))?;
    if verbose {
        eprintln!(
            "{}",
            tr!(
                "mcad: Daemon gestartet (pid {}, Log {})",
                "mcad: daemon started (pid {}, log {})",
                child.id(),
                dir.join("daemon.log").display()
            )
        );
    }
    let t0 = Instant::now();
    loop {
        match connect() {
            Ok(Some((c, h))) if h.build_id == me => return Ok((c, h.pid)),
            Ok(Some((_, h))) => {
                return Err(tr!(
                    "anderer Daemon (Build {}) kam zuvor",
                    "another daemon (build {}) came first",
                    h.build_id
                ));
            }
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        if let Ok(Some(st)) = child.try_wait()
            && !st.success()
        {
            return Err(tr!(
                "Start fehlgeschlagen ({st}), siehe {}",
                "start failed ({st}), see {}",
                dir.join("daemon.log").display()
            ));
        }
        if t0.elapsed() > START_TIMEOUT {
            return Err(tr!("keine Antwort nach {} s", "no answer after {} s", START_TIMEOUT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Answer of a `run` request.
struct Answer {
    out: Output,
    ops: Vec<FileOp>,
    info: Json,
}

fn request(conn: &mut Conn, pid: u32, argv: &[String], cwd: &Path, stdin: Option<&str>) -> Result<Answer, Fail> {
    let hang_ms: u64 = cad_ipc::env_var("DAEMON_DEBUG_HANG_MS").and_then(|v| v.parse().ok()).unwrap_or(0);
    // the language of this client: the daemon answers in it
    let body = json!({
        "proto": DAEMON_PROTO, "build_id": build_id(), "argv": argv, "cwd": cwd.to_string_lossy(),
        "stdin": stdin.is_some(), "hang_ms": hang_ms, "lang": cad_i18n::lang().code(),
    });
    let _ = conn.set_timeout(Some(SILENCE_TIMEOUT));
    let req = Frame::request(1, "run", body).with_data(stdin.map(|s| s.as_bytes().to_vec()).unwrap_or_default());
    if let Err(e) = conn.send(&req) {
        return Err(Fail::Retry(tr!("senden: {e}", "send: {e}")));
    }
    let mut accepted = false;
    loop {
        let f = match conn.recv() {
            Ok(Some(f)) => f,
            Ok(None) if accepted => {
                return Err(Fail::Loud(tr!(
                    "Daemon (pid {pid}) hat die Verbindung während des Befehls beendet",
                    "daemon (pid {pid}) closed the connection during the command"
                )));
            }
            Ok(None) => return Err(Fail::Retry(tr!("Verbindung beendet", "connection closed"))),
            Err(cad_ipc::Error::Timeout) => {
                daemon::kill_hard(pid);
                return Err(Fail::Loud(tr!(
                    "Daemon (pid {pid}) antwortet seit {} s nicht, beendet",
                    "daemon (pid {pid}) has not answered for {} s, killed",
                    SILENCE_TIMEOUT.as_secs()
                )));
            }
            Err(e) if accepted => return Err(Fail::Loud(tr!("Daemon (pid {pid}): {e}", "daemon (pid {pid}): {e}"))),
            Err(e) => return Err(Fail::Retry(e.to_string())),
        };
        match f.header.kind {
            Kind::Event if f.header.method == "accepted" => accepted = true,
            Kind::Event => {}
            Kind::Response => {
                if let Some(e) = &f.header.error {
                    if e.starts_with("shutdown") && !accepted {
                        return Err(Fail::Retry(e.clone()));
                    }
                    if e.starts_with("watchdog") {
                        return Err(Fail::Loud(e.trim_start_matches("watchdog: ").to_string()));
                    }
                    return Err(Fail::Quiet(tr!("Daemon: {e}", "daemon: {e}")));
                }
                return decode(&f)
                    .map_err(|e| Fail::Quiet(tr!("Antwort des Daemons: {e}", "answer of the daemon: {e}")));
            }
            Kind::Request => {}
        }
    }
}

fn decode(f: &Frame) -> Result<Answer, String> {
    let b = &f.header.body;
    let n = |k: &str| b[k].as_u64().map(|v| v as usize).ok_or_else(|| missing(k));
    let (so, se) = (n("stdout_len")?, n("stderr_len")?);
    let code = b["exit_code"].as_u64().ok_or_else(|| missing("exit_code"))? as u8;
    let mut pos = 0;
    let mut take = |len: usize| -> Result<Vec<u8>, String> {
        let end = pos + len;
        let s = f.data.get(pos..end).ok_or_else(|| tr!("Daten zu kurz", "data too short"))?.to_vec();
        pos = end;
        Ok(s)
    };
    let stdout = take(so)?;
    let stderr = take(se)?;
    let mut ops = Vec::new();
    for e in b["files"].as_array().ok_or_else(|| missing("files"))? {
        let path = PathBuf::from(e["path"].as_str().ok_or_else(|| missing("path"))?);
        let len = e["len"].as_u64().unwrap_or(0) as usize;
        ops.push(match e["op"].as_str() {
            Some("mkdir") => FileOp::Mkdir(path),
            Some("write") => FileOp::Write(path, take(len)?),
            Some("atomic") => FileOp::Atomic(path, take(len)?),
            other => return Err(tr!("unbekannte Dateioperation {other:?}", "unknown file operation {other:?}")),
        });
    }
    Ok(Answer { out: Output { stdout, stderr, code }, ops, info: b["info"].clone() })
}

fn missing(k: &str) -> String {
    tr!("{k} fehlt", "{k} missing")
}

/// Tries to run the command (`argv` without the program name) in the daemon; it answers in
/// `lang` (the language of this process, also the thread's).
pub fn run(argv: &[OsString], uses_stdin: bool, no_daemon: bool, verbose: bool, lang: cad_i18n::Lang) -> Attempt {
    let _lang = cad_i18n::scoped(lang);
    let local = |reason: String, loud: bool| {
        if loud {
            eprintln!(
                "{}",
                tr!(
                    "mcad: {reason}; der Befehl läuft ohne Daemon",
                    "mcad: {reason}; the command runs without the daemon"
                )
            );
        } else if verbose {
            eprintln!("{}", tr!("mcad: ohne Daemon: {reason}", "mcad: without daemon: {reason}"));
        }
        Attempt::Local(DaemonInfo { used: false, reason: Some(reason) })
    };
    if no_daemon {
        return local("--no-daemon".into(), false);
    }
    let cfg = DaemonConfig::load();
    if verbose {
        for w in &cfg.warnings {
            eprintln!("{}", tr!("mcad: Konfiguration: {w}", "mcad: configuration: {w}"));
        }
    }
    if !cfg.enabled {
        let by = cfg.disabled_by.clone().unwrap_or_else(|| trs!("Konfiguration", "configuration").into());
        return local(tr!("abgeschaltet ({by})", "switched off ({by})"), false);
    }
    if !cfg!(unix) {
        return local(tr!("Daemon nur unter Unix", "daemon only on Unix"), false);
    }
    let Some(args) = argv.iter().map(|a| a.to_str().map(String::from)).collect::<Option<Vec<String>>>() else {
        return local(tr!("Argumente sind kein UTF-8", "arguments are not UTF-8"), false);
    };
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => return local(tr!("Arbeitsverzeichnis: {e}", "working directory: {e}"), false),
    };
    let stdin = if uses_stdin {
        match host::read_stdin() {
            Ok(s) => {
                host::preload_stdin(s.clone());
                Some(s)
            }
            // the local run reports the error
            Err(e) => return local(format!("stdin: {e}"), false),
        }
    } else {
        None
    };
    let mut last = String::new();
    for _ in 0..2 {
        let (mut conn, pid) = match ensure(verbose) {
            Ok(c) => c,
            Err(e) => return local(tr!("Daemon nicht verfügbar: {e}", "daemon not available: {e}"), false),
        };
        match request(&mut conn, pid, &args, &cwd, stdin.as_deref()) {
            Ok(a) => {
                for op in &a.ops {
                    if let Err(e) = host::apply(op) {
                        // The local run gives the same error with the same message.
                        return local(tr!("Datei schreiben: {e}", "write file: {e}"), false);
                    }
                }
                if verbose {
                    let mib = format!("{:.1}", a.info["cache_bytes"].as_f64().unwrap_or(0.0) / (1 << 20) as f64);
                    let (pid, ms) = (&a.info["pid"], a.info["ms"].as_f64().unwrap_or(0.0));
                    let (hits, misses, entries) =
                        (&a.info["cache_hits"], &a.info["cache_misses"], &a.info["cache_entries"]);
                    if cad_i18n::is_de() {
                        eprintln!(
                            "mcad: Daemon pid {pid}: {ms:.0} ms, Cache {hits} Treffer / {misses} neu, {entries} Einträge, {} MiB",
                            mib.replace('.', ",")
                        );
                    } else {
                        eprintln!(
                            "mcad: daemon pid {pid}: {ms:.0} ms, cache {hits} hits / {misses} new, {entries} entries, {mib} MiB"
                        );
                    }
                }
                return Attempt::Done(a.out);
            }
            Err(Fail::Retry(e)) => last = e,
            Err(Fail::Quiet(e)) => return local(e, false),
            Err(Fail::Loud(e)) => return local(e, true),
        }
    }
    local(tr!("Daemon nicht verfügbar: {last}", "daemon not available: {last}"), false)
}
