//! The CLI daemon (`mcad daemon`, `crates/cad-cli/src/daemon.rs`): autostart, cache, identical
//! output with and without daemon, one daemon for parallel starts, idle end, version change,
//! switching it off, fallback, cache limits, file changes, status/stop, watchdog.
//!
//! Every test has its own socket directory (`MCAD_IPC_DIR`) and ends its daemon (guard).
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use serde_json::Value;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models").canonicalize().unwrap()
}

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn ex(rel: &str) -> String {
    examples().join(rel).display().to_string()
}

/// A socket directory of its own; the daemon is stopped on drop.
struct Env {
    tmp: tempfile::TempDir,
    vars: Vec<(String, String)>,
}

impl Env {
    fn new() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("ipc")).unwrap();
        Env { tmp, vars: Vec::new() }
    }

    fn with(mut self, k: &str, v: &str) -> Env {
        self.vars.push((k.into(), v.into()));
        self
    }

    fn ipc(&self) -> PathBuf {
        self.tmp.path().join("ipc")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("mcad").unwrap();
        c.env("MCAD_LANG", "de");
        c.env("MCAD_IPC_DIR", self.ipc())
            .env("MCAD_CONFIG", self.tmp.path().join("no-config.toml"))
            .env_remove("MCAD_NO_DAEMON")
            .env_remove("MCAD_DAEMON_BUILD_ID")
            .env_remove("MCAD_DAEMON_DEBUG_HANG_MS")
            .env("MCAD_DAEMON_IDLE_SECS", "120");
        // old names (before the rename to mcad) are read as a fallback: none from outside
        for k in ["CAD_IPC_DIR", "CAD_CONFIG", "CAD_NO_DAEMON", "CAD_DAEMON_BUILD_ID", "CAD_DAEMON_DEBUG_HANG_MS"] {
            c.env_remove(k);
        }
        for (k, v) in &self.vars {
            c.env(k, v);
        }
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd().args(args).output().unwrap()
    }

    fn local(&self, args: &[&str]) -> Output {
        self.cmd().arg("--no-daemon").args(args).output().unwrap()
    }

    fn status(&self) -> Value {
        let out = self.run(&["daemon", "status", "--json"]);
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn stop(&self) {
        let _ = self.cmd().args(["daemon", "stop"]).output();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        self.stop();
    }
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}\nstdout: {}\nstderr: {}", text(&out.stdout), text(&out.stderr)))
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// JSON without the fields that differ between runs (times) or report what the cache did.
fn normalized(mut v: Value) -> Value {
    fn walk(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.retain(|k, _| {
                    !(k == "daemon"
                        || k == "timings"
                        || k.ends_with("_ms")
                        || k == "cache_hits"
                        || k == "kernel_calls"
                        || k == "cached")
                });
                m.values_mut().for_each(walk);
            }
            Value::Array(a) => a.iter_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut v);
    v
}

/// Text output without the timing line of `eval`.
fn normalized_text(s: &str) -> String {
    s.lines().filter(|l| !l.starts_with("Statistik:") && !l.starts_with("Statistics:")).collect::<Vec<_>>().join("\n")
}

/// STEP without the time stamp in the header.
fn normalized_step(b: &[u8]) -> String {
    text(b).lines().filter(|l| !l.starts_with("FILE_NAME(")).collect::<Vec<_>>().join("\n")
}

fn assert_same(a: &Output, b: &Output, what: &str) {
    assert_eq!(a.status.code(), b.status.code(), "{what}: exit\n{}\n{}", text(&a.stderr), text(&b.stderr));
    assert_eq!(text(&a.stderr), text(&b.stderr), "{what}: stderr");
}

fn used(out: &Output) -> bool {
    json(out)["daemon"]["used"] == true
}

#[test]
fn autostart_then_cache_hits_and_faster() {
    let env = Env::new();
    let f = ex("spec/variable_radius.mcad");
    assert_eq!(env.status()["running"], false);
    let t0 = Instant::now();
    let a = env.run(&["eval", &f, "--json"]);
    let first = t0.elapsed();
    assert_eq!(a.status.code(), Some(0), "{}", text(&a.stderr));
    let ja = json(&a);
    assert_eq!(ja["daemon"]["used"], true, "{}", ja["daemon"]);
    let st = env.status();
    assert_eq!(st["running"], true);
    assert_eq!(st["requests"], 1);
    let pid = st["pid"].clone();
    let hits0 = st["cache"]["hits"].as_u64().unwrap();

    let t1 = Instant::now();
    let b = env.run(&["eval", &f, "--json"]);
    let second = t1.elapsed();
    let jb = json(&b);
    assert_eq!(jb["daemon"]["used"], true);
    // the second evaluation comes from the cache
    let nodes = jb["scene"]["stats"]["nodes"].as_u64().unwrap();
    assert!(nodes > 0);
    assert_eq!(jb["scene"]["stats"]["cache_hits"].as_u64().unwrap(), nodes, "{}", jb["scene"]["stats"]);
    assert_eq!(jb["scene"]["stats"]["kernel_calls"], 0);
    assert_eq!(normalized(ja), normalized(jb));
    let st = env.status();
    assert_eq!(st["pid"], pid, "the same daemon");
    assert_eq!(st["requests"], 2);
    assert!(st["cache"]["hits"].as_u64().unwrap() > hits0);
    assert!(st["cache"]["hit_rate"].as_f64().unwrap() > 0.0);
    assert!(second < first, "second {second:?} vs first {first:?}");
    eprintln!("first {first:?}, second {second:?}");
}

#[test]
fn output_identical_with_and_without_daemon() {
    let env = Env::new();
    let out = env.path("out");
    std::fs::create_dir(&out).unwrap();
    let o = |n: &str| out.join(n).display().to_string();
    // warm the daemon with other models first: tags and cache state differ from a fresh process
    for f in ["spec/fillet_zones.mcad", "spec/bottle.mcad", "spec/hole_plate.mcad"] {
        assert!(used(&env.run(&["check", &ex(f), "--json"])));
    }
    let housing = ex("spec/housing.mcad");
    let project = ex("project-housing/housing.mcad");
    let vr = ex("spec/variable_radius.mcad");
    let cases: Vec<Vec<String>> = vec![
        vec!["check".into(), housing.clone()],
        vec!["check".into(), data("syntax_error.mcad")],
        vec!["check".into(), data("fillet_too_large.mcad")],
        vec!["eval".into(), housing.clone()],
        vec!["eval".into(), project.clone(), "--module".into(), "lid".into()],
        vec!["inspect".into(), ex("spec/fillet_zones.mcad"), "--line".into(), "12".into()],
        vec!["outline".into(), project.clone()],
        // -D without a module: top-level assignments
        vec!["measure".into(), data("variables.mcad"), "-D".into(), "w=40".into(), "--volume".into(), "all".into()],
        vec!["check".into(), data("variables.mcad"), "-D".into(), "nope=1".into()],
        // -m with arguments as in a call
        vec![
            "measure".into(),
            data("holder.mcad"),
            "-m".into(),
            "holder(width, h = double(2))".into(),
            "--volume".into(),
            "all".into(),
        ],
        vec!["check".into(), data("holder.mcad"), "-m".into(), "holder(1, 2, 3, 4)".into()],
        vec![
            "measure".into(),
            housing.clone(),
            "--between".into(),
            "k.corner".into(),
            "k.top".into(),
            "--volume".into(),
            "part:k".into(),
        ],
        vec![
            "measure".into(),
            project.clone(),
            "--interference".into(),
            "--clearance".into(),
            "housing".into(),
            "lid".into(),
        ],
        vec!["set".into(), housing.clone(), "--param".into(), "housing.w=50".into(), "--dry-run".into()],
        vec!["export".into(), housing.clone(), "--part".into(), "missing".into(), "-o".into(), o("x.stl")],
    ];
    // in both languages (alternating: the daemon's cache serves both)
    for args in &cases {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        for (lang, mode) in [("de", false), ("en", false), ("de", true), ("en", true)] {
            let mut aa = a.clone();
            aa.extend(["--lang", lang]);
            if mode {
                aa.push("--json");
            }
            let local = env.local(&aa);
            let d1 = env.run(&aa);
            let d2 = env.run(&aa);
            for d in [&d1, &d2] {
                assert_same(&local, d, &format!("{aa:?}"));
                if mode {
                    assert!(used(d), "{aa:?}: {}", json(d)["daemon"]);
                    assert_eq!(normalized(json(&local)), normalized(json(d)), "{aa:?}");
                } else {
                    assert_eq!(normalized_text(&text(&local.stdout)), normalized_text(&text(&d.stdout)), "{aa:?}");
                }
            }
        }
    }
    // files: PNG, STL, 3MF, STEP, parts
    let files: Vec<(Vec<String>, Vec<&str>)> = vec![
        (
            vec![
                "render".into(),
                vr.clone(),
                "--views".into(),
                "all".into(),
                "--show".into(),
                "grids,anchors,labels".into(),
            ],
            vec!["r.png"],
        ),
        (
            vec!["render".into(), housing.clone(), "--section".into(), "z=10".into(), "--show".into(), "dims".into()],
            vec!["s.png"],
        ),
        (
            vec!["render".into(), ex("spec/fillet_zones.mcad"), "--line".into(), "12".into(), "--xray".into()],
            vec!["l.png"],
        ),
        (vec!["export".into(), vr.clone()], vec!["v.stl"]),
        (vec!["export".into(), housing.clone()], vec!["g.3mf"]),
        (vec!["export".into(), project.clone()], vec!["p.step"]),
        (vec!["export".into(), ex("spec/hole_plate.mcad")], vec!["l.step"]),
    ];
    for (args, outs) in &files {
        let target = o(outs[0]);
        let mut a: Vec<&str> = args.iter().map(String::as_str).collect();
        a.extend(["-o", target.as_str()]);
        let local = env.local(&a);
        let lb = std::fs::read(&target).unwrap();
        std::fs::remove_file(&target).unwrap();
        let d = env.run(&a);
        assert_same(&local, &d, &format!("{a:?}"));
        assert_eq!(text(&local.stdout), text(&d.stdout), "{a:?}");
        let db = std::fs::read(&target).unwrap();
        if target.ends_with(".step") {
            assert_eq!(normalized_step(&lb), normalized_step(&db), "{a:?}");
        } else {
            assert!(lb == db, "{a:?}: {} vs {} bytes", lb.len(), db.len());
        }
        let mut aj = a.clone();
        aj.push("--json");
        let lj = env.local(&aj);
        let dj = env.run(&aj);
        assert!(used(&dj));
        assert_eq!(normalized(json(&lj)), normalized(json(&dj)), "{aj:?}");
    }
    // all parts into a directory (created by the client)
    let dir_l = o("parts_l");
    let dir_d = o("parts_d");
    let l = env.local(&["export", &project, "--all-parts", "-o", &dir_l, "--format", "stl"]);
    let d = env.run(&["export", &project, "--all-parts", "-o", &dir_d, "--format", "stl"]);
    assert_same(&l, &d, "all-parts");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir_l).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert!(!names.is_empty());
    for n in &names {
        assert!(
            std::fs::read(Path::new(&dir_l).join(n)).unwrap() == std::fs::read(Path::new(&dir_d).join(n)).unwrap(),
            "{n}"
        );
    }
    assert_eq!(text(&l.stdout).replace(&dir_l, "D"), text(&d.stdout).replace(&dir_d, "D"));
}

#[test]
fn relative_paths_and_stdin() {
    let env = Env::new();
    let run_in = |args: &[&str], local: bool| {
        let mut c = env.cmd();
        c.current_dir(examples());
        if local {
            c.arg("--no-daemon");
        }
        c.args(args).output().unwrap()
    };
    let a = run_in(&["check", "project-housing/housing.mcad"], true);
    let b = run_in(&["check", "project-housing/housing.mcad"], false);
    assert_same(&a, &b, "relative");
    assert_eq!(text(&a.stdout), text(&b.stdout));
    assert!(text(&b.stdout).starts_with("project-housing/housing.mcad"), "{}", text(&b.stdout));
    assert_eq!(env.status()["requests"], 1);

    let src = "cube(5);\ntranslate([10, 0, 0]) sphere(2);\n";
    let l = env.cmd().args(["--no-daemon", "check", "-", "--json"]).write_stdin(src).output().unwrap();
    let d = env.cmd().args(["check", "-", "--json"]).write_stdin(src).output().unwrap();
    assert!(used(&d), "{}", json(&d)["daemon"]);
    assert_eq!(normalized(json(&l)), normalized(json(&d)));
    let d = env.cmd().args(["eval", "-"]).write_stdin(src).output().unwrap();
    assert!(text(&d.stdout).contains("sphere"), "{}", text(&d.stdout));
}

/// A process exists and is not a zombie (in containers PID 1 may not reap).
fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => !s.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z')),
        Err(_) => false,
    }
}

/// Processes `mcad daemon serve` with this socket directory.
fn daemon_pids(ipc: &Path) -> Vec<u32> {
    let want = format!("MCAD_IPC_DIR={}", ipc.display());
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let cmdline = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
        if !text(&cmdline).contains("daemon\0serve") {
            continue;
        }
        let environ = std::fs::read(e.path().join("environ")).unwrap_or_default();
        if text(&environ).split('\0').any(|v| v == want) {
            out.push(pid);
        }
    }
    out
}

#[test]
fn parallel_starts_give_one_daemon() {
    let env = Env::new();
    let f = ex("spec/hole_plate.mcad");
    let mut children = Vec::new();
    for _ in 0..6 {
        let mut c = std::process::Command::new(assert_cmd::cargo::cargo_bin("mcad"));
        c.env("MCAD_LANG", "de")
            .env("MCAD_IPC_DIR", env.ipc())
            .env("MCAD_CONFIG", env.path("keine.toml"))
            .env_remove("MCAD_NO_DAEMON")
            .env_remove("MCAD_DAEMON_BUILD_ID")
            .env_remove("CAD_NO_DAEMON")
            .env_remove("CAD_DAEMON_BUILD_ID")
            .env("MCAD_DAEMON_IDLE_SECS", "120")
            .args(["check", &f, "--json"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        children.push(c.spawn().unwrap());
    }
    let outs: Vec<Output> = children.into_iter().map(|c| c.wait_with_output().unwrap()).collect();
    let mut used_n = 0;
    for o in &outs {
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        used_n += used(o) as usize;
    }
    assert_eq!(used_n, 6, "all six used the daemon");
    let st = env.status();
    assert_eq!(st["requests"], 6);
    // losers of the start race end at once
    let t0 = Instant::now();
    let pids = loop {
        let p = daemon_pids(&env.ipc());
        if p.len() <= 1 || t0.elapsed() > Duration::from_secs(5) {
            break p;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(pids, vec![st["pid"].as_u64().unwrap() as u32]);
}

#[test]
fn idle_end_removes_socket_and_lock() {
    let env = Env::new().with("MCAD_DAEMON_IDLE_SECS", "1");
    assert!(used(&env.run(&["check", &ex("spec/housing.mcad"), "--json"])));
    let pid = env.status()["pid"].as_u64().unwrap();
    assert!(env.ipc().join("daemon.sock").exists());
    assert!(env.ipc().join("daemon.lock").exists());
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(env.status()["running"], false);
    assert!(!env.ipc().join("daemon.sock").exists());
    assert!(!env.ipc().join("daemon.lock").exists());
    assert!(!alive(pid as u32));
    let log = std::fs::read_to_string(env.ipc().join("daemon.log")).unwrap();
    assert!(log.contains("ohne Anfrage"), "{log}");
}

#[test]
fn version_change_replaces_daemon() {
    let env = Env::new();
    let f = ex("spec/housing.mcad");
    let with_build = |b: &str, args: &[&str]| env.cmd().env("MCAD_DAEMON_BUILD_ID", b).args(args).output().unwrap();
    assert!(used(&with_build("test-a", &["check", &f, "--json"])));
    let st = env.status();
    assert_eq!(st["build_id"], "test-a");
    let pid_a = st["pid"].as_u64().unwrap() as u32;
    let o = with_build("test-b", &["check", &f, "--json", "--verbose"]);
    assert!(used(&o), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("wird ersetzt"), "{}", text(&o.stderr));
    let st = env.status();
    assert_eq!(st["build_id"], "test-b");
    assert_ne!(st["pid"].as_u64().unwrap() as u32, pid_a);
    assert!(!alive(pid_a));
    // `status` of a client with another build says so
    let t = text(&with_build("test-c", &["daemon", "status"]).stdout);
    assert!(t.contains("anders als dieses mcad"), "{t}");
}

#[test]
fn switched_off_by_flag_env_and_config() {
    let env = Env::new();
    let f = ex("spec/housing.mcad");
    let o = env.run(&["--no-daemon", "check", &f, "--json"]);
    let j = json(&o);
    assert_eq!(j["daemon"]["used"], false);
    assert_eq!(j["daemon"]["reason"], "--no-daemon");
    // flag after the command works too (global flag)
    assert_eq!(json(&env.run(&["check", &f, "--json", "--no-daemon"]))["daemon"]["used"], false);
    let o = env.cmd().env("MCAD_NO_DAEMON", "1").args(["check", &f, "--json"]).output().unwrap();
    let j = json(&o);
    assert_eq!(j["daemon"]["used"], false);
    assert!(j["daemon"]["reason"].as_str().unwrap().contains("MCAD_NO_DAEMON"), "{}", j["daemon"]);
    // old name (before the rename to mcad), still read
    let o = env.cmd().env("CAD_NO_DAEMON", "1").args(["check", &f, "--json"]).output().unwrap();
    let j = json(&o);
    assert_eq!(j["daemon"]["used"], false);
    let reason = j["daemon"]["reason"].as_str().unwrap();
    assert!(reason.contains("CAD_NO_DAEMON") && !reason.contains("MCAD_"), "{reason}");
    // configuration file of the old name, when there is no mcad/config.toml
    let xdg = env.path("xdg");
    std::fs::create_dir_all(xdg.join("controller-cad")).unwrap();
    std::fs::write(xdg.join("controller-cad/config.toml"), "[daemon]\nenabled = false\n").unwrap();
    let o = env
        .cmd()
        .env_remove("MCAD_CONFIG")
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["check", &f, "--json"])
        .output()
        .unwrap();
    let j = json(&o);
    assert!(j["daemon"]["reason"].as_str().unwrap().contains("controller-cad/config.toml"), "{}", j["daemon"]);
    std::fs::create_dir_all(xdg.join("mcad")).unwrap();
    std::fs::write(xdg.join("mcad/config.toml"), "[daemon]\nenabled = false\n").unwrap();
    let o = env
        .cmd()
        .env_remove("MCAD_CONFIG")
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["check", &f, "--json"])
        .output()
        .unwrap();
    let j = json(&o);
    assert!(j["daemon"]["reason"].as_str().unwrap().contains("mcad/config.toml"), "{}", j["daemon"]);
    let cfg = env.path("config.toml");
    std::fs::write(&cfg, "[daemon]\nenabled = false\n").unwrap();
    let o = env.cmd().env("MCAD_CONFIG", &cfg).args(["check", &f, "--json"]).output().unwrap();
    let j = json(&o);
    assert_eq!(j["daemon"]["used"], false);
    assert!(j["daemon"]["reason"].as_str().unwrap().contains("enabled = false"), "{}", j["daemon"]);
    // text output stays quiet, --verbose says why
    let o = env.cmd().env("MCAD_NO_DAEMON", "1").args(["check", &f]).output().unwrap();
    assert!(o.stderr.is_empty());
    let o = env.cmd().env("MCAD_NO_DAEMON", "1").args(["check", &f, "--verbose"]).output().unwrap();
    assert!(text(&o.stderr).contains("ohne Daemon"), "{}", text(&o.stderr));
    // nothing was started
    assert_eq!(env.status()["running"], false);
    assert!(!env.ipc().join("daemon.sock").exists());
    // the new name wins over the old one (this starts the daemon; the guard ends it)
    let o =
        env.cmd().env("CAD_NO_DAEMON", "1").env("MCAD_NO_DAEMON", "0").args(["check", &f, "--json"]).output().unwrap();
    assert_eq!(json(&o)["daemon"]["used"], true, "MCAD_NO_DAEMON=0 before CAD_NO_DAEMON=1");
}

#[test]
fn fallback_when_socket_is_broken() {
    let f = ex("spec/housing.mcad");
    let env = Env::new();
    let reference = env.local(&["check", &f]);
    // 1. socket directory is a file
    let file = env.path("keine-dir");
    std::fs::write(&file, "x").unwrap();
    let o = env.cmd().env("MCAD_IPC_DIR", &file).args(["check", &f, "--json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(json(&o)["daemon"]["used"], false);
    let o = env.cmd().env("MCAD_IPC_DIR", &file).args(["check", &f]).output().unwrap();
    assert_eq!(text(&o.stdout), text(&reference.stdout));
    assert!(o.stderr.is_empty(), "silent: {}", text(&o.stderr));

    // 2. something else listens on the socket and closes every connection
    let sock = env.ipc().join("daemon.sock");
    let l = std::os::unix::net::UnixListener::bind(&sock).unwrap();
    let th = std::thread::spawn(move || {
        for c in l.incoming().take(4) {
            drop(c);
        }
    });
    let t0 = Instant::now();
    let o = env.run(&["check", &f, "--json"]);
    assert_eq!(o.status.code(), Some(0));
    let j = json(&o);
    assert_eq!(j["daemon"]["used"], false, "{}", j["daemon"]);
    assert!(t0.elapsed() < Duration::from_secs(5));
    let o = env.run(&["check", &f]);
    assert_eq!(text(&o.stdout), text(&reference.stdout));
    drop(th);

    // 3. stale socket file (no process): removed, a daemon starts
    let env = Env::new();
    let sock = env.ipc().join("daemon.sock");
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    assert!(sock.exists());
    assert!(used(&env.run(&["check", &f, "--json"])));
    assert_eq!(env.status()["running"], true);
}

#[test]
fn cache_limit_and_project_limit() {
    let env = Env::new().with("MCAD_DAEMON_CACHE_MB", "1").with("MCAD_DAEMON_MAX_PROJECTS", "2");
    let models = [
        "spec/fillet_zones.mcad",
        "spec/bottle.mcad",
        "spec/hole_plate.mcad",
        "spec/housing.mcad",
        "spec/knob_mirror.mcad",
        "spec/angle_anchor.mcad",
        "spec/cable_guide.mcad",
    ];
    let out = env.path("x.stl");
    for m in models {
        let o = env.run(&["export", &ex(m), "-o", &out.display().to_string(), "--json"]);
        assert_eq!(o.status.code(), Some(0), "{m}: {}", text(&o.stderr));
        assert!(used(&o));
    }
    let st = env.status();
    let c = &st["cache"];
    assert_eq!(c["limit_bytes"], 1 << 20);
    assert!(c["evictions"].as_u64().unwrap() > 0, "{c}");
    // within the limit, or only the newest entry (kept even if it alone exceeds the limit)
    assert!(c["bytes"].as_u64().unwrap() <= 1 << 20 || c["entries"] == 1, "{c}");
    assert_eq!(st["projects"], 2);
    assert_eq!(st["project_evictions"], models.len() as u64 - 2);

    // a larger limit keeps everything
    let env = Env::new().with("MCAD_DAEMON_CACHE_MB", "512");
    for m in &models[..3] {
        env.run(&["check", &ex(m)]);
    }
    let st = env.status();
    assert_eq!(st["cache"]["evictions"], 0);
    assert_eq!(st["projects"], 3);
}

#[test]
fn changed_files_are_never_stale() {
    let env = Env::new();
    let main = env.path("part.scad");
    let lib = env.path("lib.scad");
    std::fs::write(&lib, "module block(s) { cube(s); }\n").unwrap();
    std::fs::write(&main, "include <lib.scad>\nw = 10;\nblock(w);\n").unwrap();
    let m = main.display().to_string();
    let vol = |o: &Output| json(o)["volume"]["volume"].as_f64().unwrap();
    let o = env.run(&["measure", &m, "--volume", "--json"]);
    assert!(used(&o));
    assert!((vol(&o) - 1000.0).abs() < 1e-6, "{}", vol(&o));
    // main file changes
    std::fs::write(&main, "include <lib.scad>\nw = 12;\nblock(w);\n").unwrap();
    let o = env.run(&["measure", &m, "--volume", "--json"]);
    assert!((vol(&o) - 1728.0).abs() < 1e-6, "{}", vol(&o));
    // included file changes
    std::fs::write(&lib, "module block(s) { cube([s, s, 1]); }\n").unwrap();
    let o = env.run(&["measure", &m, "--volume", "--json"]);
    assert!((vol(&o) - 144.0).abs() < 1e-6, "{}", vol(&o));
    // back to an earlier state: from the cache, still right
    std::fs::write(&lib, "module block(s) { cube(s); }\n").unwrap();
    let o = env.run(&["measure", &m, "--volume", "--json"]);
    assert!((vol(&o) - 1728.0).abs() < 1e-6, "{}", vol(&o));
    // `set` writes through the client; the next call sees it
    let o = env.run(&["set", &m, "--literal", "2:5=5", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stdout));
    assert!(used(&o));
    assert!(std::fs::read_to_string(&main).unwrap().contains("w = 5;"));
    let o = env.run(&["measure", &m, "--volume", "--json"]);
    assert!((vol(&o) - 125.0).abs() < 1e-6, "{}", vol(&o));
    // an error file and a fixed one
    std::fs::write(&main, "cube(;\n").unwrap();
    assert_eq!(env.run(&["check", &m]).status.code(), Some(1));
    std::fs::write(&main, "cube(3);\n").unwrap();
    assert_eq!(env.run(&["check", &m]).status.code(), Some(0));
}

#[test]
fn status_and_stop() {
    let env = Env::new();
    let o = env.run(&["daemon", "status"]);
    assert!(text(&o.stdout).starts_with("kein Daemon"), "{}", text(&o.stdout));
    env.run(&["render", &ex("spec/housing.mcad"), "-o", &env.path("g.png").display().to_string()]);
    let st = env.status();
    for k in ["pid", "version", "build_id", "uptime_s", "idle_s", "idle_limit_s", "requests", "projects", "rss_bytes"] {
        assert!(!st[k].is_null(), "{k}: {st}");
    }
    for k in ["entries", "bytes", "limit_bytes", "hits", "misses", "hit_rate", "evictions"] {
        assert!(!st["cache"][k].is_null(), "cache.{k}: {st}");
    }
    assert_eq!(st["idle_limit_s"], 120.0);
    assert_eq!(st["cache"]["limit_bytes"], 512u64 << 20);
    let t = text(&env.run(&["daemon", "status"]).stdout);
    assert!(t.contains("Daemon läuft: pid") && t.contains("Trefferquote"), "{t}");
    let pid = st["pid"].as_u64().unwrap();
    let o = env.run(&["daemon", "stop", "--json"]);
    let j = json(&o);
    assert_eq!(j["stopped"], true);
    assert_eq!(j["pid"], pid);
    assert!(!env.ipc().join("daemon.sock").exists());
    assert_eq!(env.status()["running"], false);
    assert!(text(&env.run(&["daemon", "stop"]).stdout).starts_with("kein Daemon"));
}

#[test]
fn watchdog_ends_daemon_and_command_runs_locally() {
    let env = Env::new().with("MCAD_DAEMON_WATCHDOG_SECS", "1");
    let f = ex("spec/housing.mcad");
    let reference = env.local(&["check", &f]);
    assert!(used(&env.run(&["check", &f, "--json"])));
    let pid = env.status()["pid"].as_u64().unwrap();
    let t0 = Instant::now();
    let o = env.cmd().env("MCAD_DAEMON_DEBUG_HANG_MS", "8000").args(["check", &f]).output().unwrap();
    let took = t0.elapsed();
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(text(&o.stdout), text(&reference.stdout));
    let err = text(&o.stderr);
    assert!(err.contains("Watchdog") && err.contains("ohne Daemon"), "{err}");
    assert!(took < Duration::from_secs(6), "{took:?}");
    let t0 = Instant::now();
    while alive(pid as u32) && t0.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(pid as u32));
    assert_eq!(env.status()["running"], false);
    // the next command starts a new daemon
    assert!(used(&env.run(&["check", &f, "--json"])));
    assert_ne!(env.status()["pid"].as_u64().unwrap(), pid);
}

/// Bericht 12: a `MCAD_IPC_DIR` whose socket path exceeds the Unix limit (107 bytes) used to
/// stop the daemon from starting, silently. Now the socket goes to `/tmp/mcad-<uid>/` and
/// `mcad daemon status` names it.
#[test]
fn long_ipc_dir_uses_short_socket_path() {
    let mut env = Env::new();
    let long = env
        .tmp
        .path()
        .join("a-very-long-directory-name-for-the-socket-of-the-background-process-so-that-it-is-enough")
        .join("ipc");
    std::fs::create_dir_all(&long).unwrap();
    assert!(long.join("daemon.sock").as_os_str().len() > 107, "{}", long.display());
    env.vars.push(("MCAD_IPC_DIR".into(), long.display().to_string()));
    let o = env.run(&["check", &ex("spec/housing.mcad"), "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(used(&o), "daemon not used: {}", text(&o.stderr));
    let st = env.status();
    assert_eq!(st["running"], true, "{st}");
    assert_eq!(st["socket_substitute"], true, "{st}");
    let sock = st["socket"].as_str().unwrap().to_string();
    assert!(sock.len() <= 107 && sock.starts_with("/tmp/mcad-") && sock.ends_with("-daemon.sock"), "{sock}");
    assert!(Path::new(&sock).exists());
    // lock and log stay in the long directory
    assert!(long.join("daemon.lock").exists() && long.join("daemon.log").exists());
    let t = text(&env.run(&["daemon", "status"]).stdout);
    assert!(t.contains(&sock) && t.contains("Ersatzpfad"), "{t}");
    // the next call reaches the same daemon
    let pid = st["pid"].as_u64().unwrap();
    assert!(used(&env.run(&["check", &ex("spec/housing.mcad"), "--json"])));
    assert_eq!(env.status()["pid"].as_u64(), Some(pid));
    let o = env.run(&["daemon", "stop", "--json"]);
    assert_eq!(json(&o)["stopped"], true);
    assert!(!Path::new(&sock).exists(), "substitute socket left behind");
    let o = env.run(&["daemon", "status"]);
    assert!(text(&o.stdout).starts_with("kein Daemon"), "{}", text(&o.stdout));
}

/// One daemon, German then English then German: the answers follow the client's language
/// (`MCAD_LANG` or `--lang`) and equal the local run; cached results never carry the other
/// language.
#[test]
fn language_switch_in_one_daemon() {
    let env = Env::new();
    let f = ex("spec/fillet_zones.mcad");
    let bad = data("fillet_too_large.mcad");
    let run = |lang: &str, args: &[&str]| {
        let mut c = env.cmd();
        c.env("MCAD_LANG", lang).args(args);
        c.output().unwrap()
    };
    let local = |lang: &str, args: &[&str]| {
        let mut c = env.cmd();
        c.env("MCAD_LANG", lang).env("MCAD_NO_DAEMON", "1").args(args);
        c.output().unwrap()
    };
    for (lang, word, other) in [("de", "Kanten", " edges"), ("en", " edges", "Kanten"), ("de", "Kanten", " edges")] {
        let args = ["inspect", f.as_str(), "--line", "12"];
        let d = run(lang, &args);
        let l = local(lang, &args);
        assert_same(&l, &d, lang);
        let t = text(&d.stdout);
        assert_eq!(t, text(&l.stdout), "{lang}");
        assert!(t.contains(word) && !t.contains(other), "{lang}: {t}");
        let d = run(lang, &["check", bad.as_str()]);
        let l = local(lang, &["check", bad.as_str()]);
        assert_same(&l, &d, lang);
        assert_eq!(text(&d.stdout), text(&l.stdout), "{lang}");
        let j = run(lang, &["check", bad.as_str(), "--json"]);
        assert!(used(&j), "{lang}");
    }
    // --lang wins over MCAD_LANG
    let d = run("de", &["check", bad.as_str(), "--lang", "en"]);
    assert!(text(&d.stdout).contains(" errors, "), "{}", text(&d.stdout));
}
