//! Native build of the SMLib facade.
//!
//! * `SMLIB_PREBUILT=<dir>`: link `libSMLib.a` / `libSM_API.a` from `<dir>/lib` (or `<dir>`),
//!   e.g. the output of `smlib/build-native.sh`. Fast iteration.
//! * otherwise SMLib + SM_API are built with CMake from `smlib/CMakeLists.txt` into
//!   `OUT_DIR` (cached by cargo; ~3 min with 4 cores the first time).
//! * Sources: `SMLIB_SRC` or `<repo>/third_party/smlib` (see `smlib/fetch.sh`).
//!
//! The facade (`smlib/facade/*.cpp`) is always compiled here with the `cc` crate.
//! Nothing is built for the `wasm` feature: smlib.wasm comes from `smlib/build-wasm.sh`.

use std::env;
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=SMLIB_SRC");
    println!("cargo:rerun-if-env-changed=SMLIB_PREBUILT");
    println!("cargo:rerun-if-changed=build.rs");

    let native = env::var_os("CARGO_FEATURE_NATIVE").is_some();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if native && target_arch != "wasm32" {
        #[cfg(feature = "native")]
        native_build();
    }
}

#[cfg(feature = "native")]
fn native_build() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..").canonicalize().expect("repo root");
    let smlib_dir = root.join("smlib");
    let facade = smlib_dir.join("facade");
    let src = env::var_os("SMLIB_SRC").map(PathBuf::from).unwrap_or_else(|| root.join("third_party/smlib"));
    if !src.join("source/SMLib/inc/SmSmlibAll.h").exists() {
        panic!("SMLib sources not found at {} (run smlib/fetch.sh or set SMLIB_SRC)", src.display());
    }

    let lib_dir: PathBuf = match env::var_os("SMLIB_PREBUILT") {
        Some(p) => {
            let p = PathBuf::from(p);
            let p = if p.is_relative() { root.join(p) } else { p };
            let lib = p.join("lib");
            if lib.join("libSMLib.a").exists() { lib } else { p }
        }
        None => build_smlib_cmake(&smlib_dir, &src),
    };
    for l in ["libSMLib.a", "libSM_API.a"] {
        if !lib_dir.join(l).exists() {
            panic!("{} not found in {}", l, lib_dir.display());
        }
        println!("cargo:rerun-if-changed={}", lib_dir.join(l).display());
    }

    let mut b = cc::Build::new();
    b.cpp(true)
        .std("c++17")
        .flag_if_supported("-w")
        .flag("-fexceptions")
        .flag("-frtti")
        .define("NDEBUG", None)
        .define("NOMINMAX", None)
        .define("SM_GRAPHICS_CALLBACKS", "1")
        .define("_GLIBCXX_USE_CXX11_ABI", "1")
        .include(&facade)
        .include(src.join("source/SM_API/inc"))
        .include(src.join("source/SMLib/inc"))
        .opt_level(2)
        .warnings(false);
    for entry in std::fs::read_dir(&facade).unwrap() {
        let p = entry.unwrap().path();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "cpp" {
            b.file(&p);
        }
        if ext == "cpp" || ext == "h" {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
    b.compile("sc_facade");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=SM_API");
    println!("cargo:rustc-link-lib=static=SMLib");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        println!("cargo:rustc-link-lib=dylib=pthread");
    }
}

#[cfg(feature = "native")]
fn build_smlib_cmake(smlib_dir: &Path, src: &Path) -> PathBuf {
    println!("cargo:rerun-if-changed={}", smlib_dir.join("CMakeLists.txt").display());
    let mut cfg = cmake::Config::new(smlib_dir);
    cfg.define("SMLIB_SRC", src)
        .define("SC_BUILD_FACADE", "OFF")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("CMAKE_CXX_FLAGS_RELEASE", "-O2 -DNDEBUG")
        .profile("Release")
        .build_target("SM_API");
    if which("ninja") {
        cfg.generator("Ninja");
    }
    let dst = cfg.build();
    dst.join("build")
}

#[cfg(feature = "native")]
fn which(cmd: &str) -> bool {
    env::var_os("PATH").map(|paths| env::split_paths(&paths).any(|p| p.join(cmd).is_file())).unwrap_or(false)
}
