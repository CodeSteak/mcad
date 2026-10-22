//! `web/src/engine/protocol.ts` must match the Rust protocol types.
//! Regenerate with `UPDATE_PROTOCOL_TS=1 cargo test -p cad-session --test protocol_ts`.

use std::path::PathBuf;

#[test]
fn protocol_ts_is_current() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/engine/protocol.ts");
    let want = cad_session::protocol::typescript();
    if std::env::var_os("UPDATE_PROTOCOL_TS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let got = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        got == want,
        "{} is out of date; run UPDATE_PROTOCOL_TS=1 cargo test -p cad-session --test protocol_ts",
        path.display()
    );
}
