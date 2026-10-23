// Tauri build script. The web assets (`web/dist-tauri`, built by `pnpm build:tauri`) are
// embedded into the binary. Without them (a plain `cargo build`/`cargo test` of the
// workspace, CI without Node) a placeholder page is created so the crate still compiles;
// the app then shows how to build the UI.

fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist-tauri");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(&dist).expect("web/dist-tauri");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><meta charset=\"utf-8\"><title>controller-cad</title>\
             <body style=\"font:15px system-ui;padding:2em\"><h1>UI nicht eingebettet</h1>\
             <p>Dieser Build enthält die Web-Oberfläche nicht. Bauen mit:</p>\
             <pre>cd web &amp;&amp; pnpm install &amp;&amp; pnpm build:tauri\ncargo build --release -p cad-cli</pre>\
             <p>oder <code>scripts/build-desktop.sh</code>.</p></body>",
        )
        .expect("placeholder");
        // A marker so the app can tell the placeholder apart.
        std::fs::write(dist.join("placeholder"), "").ok();
    }
    println!("cargo:rerun-if-changed=../../web/dist-tauri");
    tauri_build::build()
}
