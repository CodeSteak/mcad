//! Backend of the desktop app without Tauri: files, watcher, git markers, wire format, engine
//! supervisor with watchdog (engine in a thread, native SMLib).

use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cad_desktop::engine::{Supervisor, SupervisorOptions, ThreadLauncher};
use cad_desktop::files::{Recent, choose_main, open_file, open_folder, write_atomic};
use cad_desktop::git::{Base, GitBases, HunkKind, diff_hunks, head_base};
use cad_desktop::watcher::{FileWatcher, WatchEvent};
use cad_desktop::wire::{decode_batch, encode_batch};
use cad_session::protocol::{FromEngine, MeshData, ToEngine};
use kernel_smlib::SmlibKernel;
use serde_json::Value as Json;

// ---------------------------------------------------------------------------------- files

#[test]
fn open_file_loads_includes() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("lib")).unwrap();
    std::fs::write(d.path().join("lib/teile.mcad"), "module t() { cube(2); }\n").unwrap();
    std::fs::write(d.path().join("haupt.scad"), "use <lib/teile.mcad>\nt();\ninclude <fehlt.scad>\n").unwrap();
    std::fs::write(d.path().join("anders.scad"), "cube(1);\n").unwrap();
    let p = open_file(&d.path().join("haupt.scad")).unwrap();
    assert_eq!(p.main, "haupt.scad");
    assert!(!p.folder);
    let keys: Vec<&str> = p.files.keys().map(|s| s.as_str()).collect();
    assert_eq!(keys, vec!["haupt.scad", "lib/teile.mcad"], "missing includes and unrelated files are not part of it");
    assert_eq!(p.abs("lib/teile.mcad"), std::path::absolute(d.path().join("lib/teile.mcad")).unwrap());
}

#[test]
fn open_folder_chooses_main() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("lib")).unwrap();
    std::fs::create_dir(d.path().join(".git")).unwrap();
    std::fs::write(d.path().join(".git/x.scad"), "").unwrap();
    std::fs::write(d.path().join("lib/a.scad"), "module a() {}\n").unwrap();
    std::fs::write(d.path().join("b.scad"), "use <lib/a.scad>\na();\n").unwrap();
    let p = open_folder(d.path(), None).unwrap();
    assert!(p.folder);
    assert_eq!(p.main, "b.scad", "the file nobody includes");
    assert_eq!(p.files.len(), 2, "hidden directories are skipped");
    let p = open_folder(d.path(), Some(&d.path().join("lib/a.scad"))).unwrap();
    assert_eq!(p.main, "lib/a.scad");
    std::fs::write(d.path().join("main.scad"), "cube(1);\n").unwrap();
    let p = open_folder(d.path(), None).unwrap();
    assert_eq!(p.main, "main.scad");
    // a new empty folder starts with main.mcad
    let empty = tempfile::tempdir().unwrap();
    let p = open_folder(empty.path(), None).unwrap();
    assert_eq!(p.main, "main.mcad");
    assert_eq!(p.files["main.mcad"], "");
    assert_eq!(choose_main(&Default::default(), None), None);
}

#[test]
fn open_folder_mixes_mcad_and_scad() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("lib")).unwrap();
    std::fs::write(d.path().join("lib/a.scad"), "module a() { cube(1); }\n").unwrap();
    std::fs::write(d.path().join("lib/b.MCAD"), "module b() { cube(2); }\n").unwrap();
    std::fs::write(d.path().join("teil.mcad"), "use <lib/a.scad>\nuse <lib/b.MCAD>\na();\nb();\n").unwrap();
    std::fs::write(d.path().join("notiz.txt"), "kein Modell\n").unwrap();
    let p = open_folder(d.path(), None).unwrap();
    let keys: Vec<&str> = p.files.keys().map(|s| s.as_str()).collect();
    assert_eq!(keys, vec!["lib/a.scad", "lib/b.MCAD", "teil.mcad"], "both extensions, no other files");
    assert_eq!(p.main, "teil.mcad");
    // main.mcad before main.scad
    std::fs::write(d.path().join("main.scad"), "cube(1);\n").unwrap();
    std::fs::write(d.path().join("main.mcad"), "cube(1);\n").unwrap();
    assert_eq!(open_folder(d.path(), None).unwrap().main, "main.mcad");
}

#[test]
fn atomic_write_and_recent() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("sub/x.scad");
    write_atomic(&f, "cube(1);\n").unwrap();
    write_atomic(&f, "cube(2);\n").unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "cube(2);\n");
    let leftovers: Vec<_> = std::fs::read_dir(d.path().join("sub")).unwrap().flatten().collect();
    assert_eq!(leftovers.len(), 1, "no temporary files left");
    let r = Recent::new(d.path().join("cfg/recent.json"));
    r.add(&f, false);
    r.add(d.path(), true);
    let l = r.add(&f, false);
    assert_eq!(l.len(), 2);
    assert_eq!(l[0].path, std::path::absolute(&f).unwrap());
    assert!(l[1].folder);
    assert_eq!(r.load(), l);
}

// -------------------------------------------------------------------------------- watcher

fn watcher() -> (FileWatcher, Receiver<WatchEvent>) {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let w = FileWatcher::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    })
    .unwrap();
    (w, rx)
}

fn next(rx: &Receiver<WatchEvent>, ms: u64) -> Option<WatchEvent> {
    rx.recv_timeout(Duration::from_millis(ms)).ok()
}

#[test]
fn watcher_reports_external_changes_only() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("teil.scad");
    let inc = d.path().join("inc.scad");
    std::fs::write(&f, "cube(1);\n").unwrap();
    std::fs::write(&inc, "x = 1;\n").unwrap();
    let (w, rx) = watcher();
    w.set_files(&[(f.clone(), Some("cube(1);\n".into())), (inc.clone(), None)]);
    std::thread::sleep(Duration::from_millis(100));

    // external change (in place)
    std::fs::write(&f, "cube(2);\n").unwrap();
    match next(&rx, 3000) {
        Some(WatchEvent::Changed { path, text }) => {
            assert_eq!(path, f);
            assert_eq!(text, "cube(2);\n");
        }
        other => panic!("{other:?}"),
    }
    // own save: no event
    w.note_written(&f, "cube(3);\n");
    write_atomic(&f, "cube(3);\n").unwrap();
    assert_eq!(next(&rx, 800), None, "own saves are not reported");
    // external atomic save (temporary file + rename, like `mcad set`): changed, not removed
    std::fs::write(d.path().join(".tmp"), "cube(4);\n").unwrap();
    std::fs::rename(d.path().join(".tmp"), &f).unwrap();
    assert!(matches!(next(&rx, 3000), Some(WatchEvent::Changed { text, .. }) if text == "cube(4);\n"));
    // the same content written again: nothing
    std::fs::write(&f, "cube(4);\n").unwrap();
    assert_eq!(next(&rx, 600), None);
    // included file
    std::fs::write(&inc, "x = 2;\n").unwrap();
    assert!(matches!(next(&rx, 3000), Some(WatchEvent::Changed { path, .. }) if path == inc));
    // other files in the directory are ignored
    std::fs::write(d.path().join("fremd.scad"), "sphere(1);\n").unwrap();
    assert_eq!(next(&rx, 600), None);
    // deleted, then created again
    std::fs::remove_file(&f).unwrap();
    assert!(matches!(next(&rx, 3000), Some(WatchEvent::Removed { path }) if path == f));
    std::fs::write(&f, "cube(5);\n").unwrap();
    assert!(matches!(next(&rx, 3000), Some(WatchEvent::Changed { text, .. }) if text == "cube(5);\n"));
    // renamed away
    std::fs::rename(&f, d.path().join("neu.scad")).unwrap();
    assert!(matches!(next(&rx, 3000), Some(WatchEvent::Removed { .. })));
    assert_eq!(w.watched().len(), 2);
}

// ------------------------------------------------------------------------------------ git

#[test]
fn diff_hunks_kinds() {
    let base = "a\nb\nc\nd\ne\n";
    assert!(diff_hunks(base, base).is_empty());
    // CRLF and a missing final newline are no change
    assert!(diff_hunks(base, "a\r\nb\r\nc\r\nd\r\ne").is_empty());
    let h = diff_hunks(base, "a\nB\nc\nd\nneu\ne\n");
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].kind, h[0].start, h[0].end, h[0].old.clone()), (HunkKind::Modified, 2, 2, vec!["b".to_string()]));
    assert_eq!((h[1].kind, h[1].start, h[1].end), (HunkKind::Added, 5, 5));
    let h = diff_hunks(base, "a\nd\ne\n");
    assert_eq!(h.len(), 1);
    assert_eq!(
        (h[0].kind, h[0].start, h[0].end, h[0].old.clone()),
        (HunkKind::Deleted, 2, 2, vec!["b".into(), "c".into()])
    );
    // deleted at the end: marker after the last line
    let h = diff_hunks(base, "a\nb\nc\n");
    assert_eq!((h[0].kind, h[0].start), (HunkKind::Deleted, 4));
    let h = diff_hunks("", "x\ny\n");
    assert_eq!((h[0].kind, h[0].start, h[0].end), (HunkKind::Added, 1, 2));
}

fn sh_git(dir: &Path, args: &[&str]) {
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
}

#[test]
fn head_base_of_tracked_and_untracked_files() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("teil.scad");
    let neu = d.path().join("neu.scad");
    // not a repository
    std::fs::write(&f, "cube(1);\nsphere(2);\n").unwrap();
    assert_eq!(head_base(&f), Base::None);
    sh_git(d.path(), &["init", "-q"]);
    assert_eq!(head_base(&f), Base::None, "no commit yet");
    sh_git(d.path(), &["add", "teil.scad"]);
    sh_git(d.path(), &["commit", "-q", "-m", "eins"]);
    std::fs::write(&neu, "cube(3);\n").unwrap();
    match head_base(&f) {
        Base::Head { text, .. } => assert_eq!(text, "cube(1);\nsphere(2);\n"),
        Base::None => panic!("tracked file without base"),
    }
    assert_eq!(head_base(&neu), Base::None, "untracked: no markers");
    let mut g = GitBases::default();
    let h = g.hunks(&f, "cube(1);\nsphere(3);\ncylinder(1);\n", false).unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!((h[0].kind, h[0].start, h[0].end), (HunkKind::Modified, 2, 3));
    assert!(g.hunks(&neu, "x", false).is_none());
    // a new commit: refresh gives the new base
    std::fs::write(&f, "cube(9);\n").unwrap();
    sh_git(d.path(), &["commit", "-q", "-am", "zwei"]);
    assert!(g.hunks(&f, "cube(9);\n", true).unwrap().is_empty());
    // file in a subdirectory of the repository
    std::fs::create_dir(d.path().join("lib")).unwrap();
    std::fs::write(d.path().join("lib/a.scad"), "a\n").unwrap();
    sh_git(d.path(), &["add", "lib/a.scad"]);
    sh_git(d.path(), &["commit", "-q", "-m", "drei"]);
    assert!(matches!(head_base(&d.path().join("lib/a.scad")), Base::Head { text, .. } if text == "a\n"));
}

// ----------------------------------------------------------------------------------- wire

#[test]
fn wire_batch_has_binary_meshes() {
    let mesh = MeshData {
        node: 3,
        nodes: vec![3, 4],
        key: "k".into(),
        coarse: false,
        tol: 0.1,
        positions: vec![0.0, 1.5, -2.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
        normals: vec![0.0; 9],
        indices: vec![0, 1, 2],
        tri_tags: vec![7],
        tri_faces: vec![1],
        edge_segments: vec![0.25; 6],
        edge_ids: vec![2],
    };
    let msgs = vec![
        FromEngine::Meshes { version: 5, items: vec![mesh] },
        FromEngine::Error { message: "x".into(), version: Some(5), node: None },
        FromEngine::ExportResult {
            format: cad_session::protocol::ExportFormat::Stl,
            bytes: vec![1, 2, 3],
            triangles: 1,
            parts: None,
            zip: None,
            colors: None,
            notes: Some(vec!["n".into()]),
        },
    ];
    let f = encode_batch(&msgs, true);
    // header: no number arrays, only blob descriptors
    let h = serde_json::to_string(&f.header.body).unwrap();
    assert!(h.contains("\"$b\""));
    assert!(!h.contains("1.5"));
    let bytes = cad_desktop::wire::frame_bytes(&f);
    let hl = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    assert_eq!((8 + hl) % 4, 0, "data part 4-byte aligned");
    let back = cad_ipc::read_frame(&mut bytes.as_slice()).unwrap().unwrap();
    let (m, busy) = decode_batch(&back);
    assert!(busy);
    assert_eq!(m[0]["items"][0]["positions"][1], Json::from(1.5));
    assert_eq!(m[0]["items"][0]["indices"], serde_json::json!([0, 1, 2]));
    assert_eq!(m[0]["items"][0]["nodes"], serde_json::json!([3, 4]));
    assert_eq!(m[1]["type"], "Error");
    assert_eq!(m[2]["bytes"], serde_json::json!([1, 2, 3]));
    // every blob offset is aligned
    for it in ["positions", "normals", "indices", "tri_tags", "tri_faces", "edge_segments", "edge_ids"] {
        assert_eq!(f.header.body["msgs"][0]["items"][0][it]["$b"][0].as_u64().unwrap() % 4, 0);
    }
}

// --------------------------------------------------------------------------------- engine

type Got = Arc<Mutex<Vec<Json>>>;

fn supervisor(watchdog_ms: u64) -> (Arc<Supervisor>, Got) {
    let got: Got = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    let sink: cad_desktop::engine::Sink = Arc::new(move |f: cad_ipc::Frame| {
        let mut g = g.lock().unwrap();
        if f.header.method == "out" {
            g.extend(decode_batch(&f).0);
        } else {
            g.push(serde_json::json!({ "type": "__host", "body": f.header.body }));
        }
    });
    let s = Supervisor::start(
        Arc::new(ThreadLauncher { make_kernel: SmlibKernel::new }),
        sink,
        SupervisorOptions { watchdog: Duration::from_millis(watchdog_ms), startup: Duration::from_secs(30) },
    );
    (s, got)
}

fn wait_for(got: &Got, what: &str, f: impl Fn(&Json) -> bool) -> Json {
    for _ in 0..600 {
        if let Some(m) = got.lock().unwrap().iter().find(|m| f(m)) {
            return m.clone();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("{what} kam nicht: {:?}", got.lock().unwrap().iter().map(|m| m["type"].clone()).collect::<Vec<_>>());
}

#[test]
fn supervisor_evaluates_and_sends_meshes() {
    let (s, got) = supervisor(15_000);
    s.send(vec![
        ToEngine::SetSource { text: "cube(1);".into(), version: 1 },
        ToEngine::SetSource { text: "cube(10);".into(), version: 2 },
    ]);
    let scene = wait_for(&got, "Szene", |m| m["type"] == "Scene" && m["complete"] == true);
    assert_eq!(scene["version"], 2, "a SetSource followed by another is dropped");
    let meshes = wait_for(&got, "Meshes", |m| m["type"] == "Meshes");
    let pos = meshes["items"][0]["positions"].as_array().unwrap();
    let max = pos.iter().filter_map(Json::as_f64).fold(0.0f64, f64::max);
    assert!((max - 10.0).abs() < 1e-6, "cube(10) mesh");
    assert!(!got.lock().unwrap().iter().any(|m| m["type"] == "Scene" && m["version"] == 1));
    let st = s.status();
    assert!(st.ready);
    assert_eq!(st.restarts, 0);
    s.shutdown();
}

#[test]
fn supervisor_watchdog_when_idle_engine_hangs() {
    // A hang without a running job: restart without blocking anything, the source is not
    // sent again (it would hang again).
    let (s, got) = supervisor(800);
    s.send(vec![ToEngine::SetSource { text: "cube(2);".into(), version: 1 }]);
    wait_for(&got, "Szene", |m| m["type"] == "Scene" && m["complete"] == true);
    s.debug_hang(5000);
    let err = wait_for(&got, "Fehler", |m| m["type"] == "Error");
    let msg = err["message"].as_str().unwrap();
    assert!(msg.contains("neu gestartet") && !msg.contains("gesperrt"), "{msg}");
    assert!(s.status().blocked_keys.is_empty());
    s.shutdown();
}

#[test]
fn supervisor_watchdog_replaces_engine_and_blocks_key() {
    let (s, got) = supervisor(1200);
    let src = "difference() { cube(10); translate([2,2,-1]) cylinder(h = 12, r = 2); }\n";
    // The engine "hangs" right after the job started (one batch, like setSource + debugHang
    // in the same tick of the webview).
    s.send_json(vec![
        serde_json::to_value(ToEngine::SetSource { text: src.into(), version: 1 }).unwrap(),
        serde_json::json!({ "type": "__debugHang", "ms": 8000 }),
    ]);
    let prog = wait_for(&got, "Progress", |m| m["type"] == "Progress" && m["current"]["key"].is_string());
    let key = prog["current"]["key"].as_str().unwrap().to_string();
    let err = wait_for(&got, "Watchdog-Fehler", |m| {
        m["type"] == "Error" && m["message"].as_str().unwrap_or("").contains("neu gestartet")
    });
    let msg = err["message"].as_str().unwrap();
    assert!(msg.contains("Kernel hängt"), "{msg}");
    assert!(msg.contains("Knoten gesperrt"), "{msg}");
    let st = s.status();
    assert_eq!(st.restarts, 1);
    assert_eq!(st.blocked_keys, vec![key.clone()]);
    // The new engine gets BlockKeys and the source again: the blocked node reports an error.
    let scene = wait_for(&got, "Szene nach Neustart", |m| {
        m["type"] == "Scene"
            && m["complete"] == true
            && m["nodes"].as_array().is_some_and(|ns| ns.iter().any(|n| n["status"] == "error"))
    });
    assert_eq!(scene["version"], 1);
    let host = wait_for(&got, "host", |m| m["type"] == "__host" && m["body"]["restarts"] == 1);
    assert_eq!(host["body"]["blocked_keys"][0], Json::from(key));
    // A changed source evaluates normally again.
    s.send(vec![ToEngine::SetSource { text: "cube(3);".into(), version: 2 }]);
    let ok = wait_for(&got, "neue Szene", |m| m["type"] == "Scene" && m["complete"] == true && m["version"] == 2);
    assert!(ok["nodes"].as_array().unwrap().iter().all(|n| n["status"] != "error"));
    s.shutdown();
}
