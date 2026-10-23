#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use cad_ipc::{Error, Frame, IPC_VERSION, Kind, Server, connect_path, list_in, read_frame, write_frame};
use serde_json::json;

#[test]
fn frame_roundtrip_with_data() {
    let f = Frame::request(7, "status", json!({"a": 1})).with_data(vec![1, 2, 3, 250]);
    let mut buf = Vec::new();
    write_frame(&mut buf, &f).unwrap();
    assert_eq!(u32::from_le_bytes(buf[4..8].try_into().unwrap()), 4);
    let g = read_frame(&mut buf.as_slice()).unwrap().unwrap();
    assert_eq!(f, g);
    // closed before a frame: None
    assert!(read_frame(&mut [].as_slice()).unwrap().is_none());
    // truncated frame: error
    assert!(matches!(read_frame(&mut &buf[..5]), Err(Error::Protocol(_))));
}

#[test]
fn version_mismatch_is_reported() {
    let header = br#"{"v":99,"id":1,"kind":"request","method":"x","body":null}"#;
    let mut buf = Vec::new();
    buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(header);
    match read_frame(&mut buf.as_slice()) {
        Err(Error::Version { ours, theirs }) => assert_eq!((ours, theirs), (IPC_VERSION, 99)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn request_response_events_and_stale_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ui-1.sock");
    let server = Server::bind_path(&path).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    // A second server on the same live endpoint is refused.
    assert!(matches!(Server::bind_path(&path), Err(Error::InUse(_))));

    let t = std::thread::spawn(move || {
        // Liveness probes (bind/list) connect and close at once: skip them.
        let (mut c, f) = loop {
            let mut c = server.accept().unwrap();
            if let Some(f) = c.recv().unwrap() {
                break (c, f);
            }
        };
        assert_eq!(f.header.kind, Kind::Request);
        c.send_event("changed", json!({"file": "a.scad"})).unwrap();
        c.respond(&f.header, json!({"echo": f.header.body}), vec![9, 9]).unwrap();
        let g = c.recv().unwrap().unwrap();
        c.respond_err(&g.header, "geht nicht").unwrap();
        // keep the server alive until the client is done
        let _ = c.recv();
        drop(server);
    });
    let mut c = connect_path(&path).unwrap();
    c.set_timeout(Some(Duration::from_secs(5))).unwrap();
    let r = c.request("status", json!({"x": 1})).unwrap();
    assert_eq!(r.body(), &json!({"echo": {"x": 1}}));
    assert_eq!(r.data, vec![9, 9]);
    let e = c.next_event().unwrap().unwrap();
    assert_eq!(e.header.method, "changed");
    match c.request("show", json!(null)) {
        Err(Error::Remote(m)) => assert_eq!(m, "geht nicht"),
        other => panic!("{other:?}"),
    }
    let names: Vec<String> = list_in(dir.path(), "ui-").unwrap().into_iter().map(|x| x.0).collect();
    assert_eq!(names, vec!["ui-1".to_string()]);
    drop(c);
    t.join().unwrap();
    // The server dropped: socket file removed.
    assert!(!path.exists());
    assert!(matches!(connect_path(&path), Err(Error::NotRunning(_))));
}

#[test]
fn stale_socket_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ui-2.sock");
    {
        // A listener that vanishes without cleanup leaves a stale socket file.
        let l = std::os::unix::net::UnixListener::bind(&path).unwrap();
        drop(l);
    }
    assert!(path.exists());
    assert!(list_in(dir.path(), "ui-").unwrap().is_empty());
    assert!(!path.exists(), "list removes stale sockets");
    let _l = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(_l);
    assert!(matches!(connect_path(&path), Err(Error::NotRunning(_))));
    assert!(!path.exists(), "connect removes stale sockets");
    // bind over a stale socket works
    let _l = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(_l);
    let s = Server::bind_path(&path).unwrap();
    assert!(path.exists());
    drop(s);
}

#[test]
fn serve_handles_requests() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.sock");
    let server = Server::bind_path(&path).unwrap();
    std::thread::spawn(move || {
        let _ = server.serve(|f| match f.header.method.as_str() {
            "add" => {
                let a = f.body()["a"].as_i64().unwrap_or(0);
                let b = f.body()["b"].as_i64().unwrap_or(0);
                Ok((json!(a + b), Vec::new()))
            }
            m => Err(format!("unbekannt: {m}")),
        });
    });
    let mut c = connect_path(&path).unwrap();
    assert_eq!(c.request("add", json!({"a": 2, "b": 3})).unwrap().body(), &json!(5));
    assert!(matches!(c.request("mul", json!({})), Err(Error::Remote(_))));
    // two clients at once
    let mut d = connect_path(&path).unwrap();
    assert_eq!(d.request("add", json!({"a": 1, "b": 1})).unwrap().body(), &json!(2));
}

#[test]
fn runtime_dir_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().join("ipc");
    std::fs::create_dir(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    // SAFETY: the only test that touches this variable.
    unsafe { std::env::set_var("CAD_IPC_DIR", &d) };
    let r = cad_ipc::runtime_dir().unwrap();
    assert_eq!(r, d);
    assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(cad_ipc::endpoint_path("ui-5").unwrap(), d.join("ui-5.sock"));
    assert!(cad_ipc::endpoint_path("../x").is_err());
}

#[test]
fn lock_file_is_exclusive_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.lock");
    let mut a = cad_ipc::LockFile::try_lock_path(&path).unwrap().expect("first lock");
    a.write_info("pid 1").unwrap();
    assert_eq!(cad_ipc::LockFile::read_info(&path).as_deref(), Some("pid 1"));
    // a second open file description does not get it (flock, also within one process)
    assert!(cad_ipc::LockFile::try_lock_path(&path).unwrap().is_none());
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    drop(a);
    assert!(!path.exists());
    assert!(cad_ipc::LockFile::try_lock_path(&path).unwrap().is_some());
}
