# controller-cad – development

Overview: [`README.md`](../README.md). This file is the former German README with all details on building, installing, tests and structure.

Program `mcad` (up to 0.2: `cad`), model files `*.mcad` (`*.scad` still opens).
Parametric CAD in the browser and as a desktop app (Tauri 2) for small hobby projects. Models are written as text in an
OpenSCAD dialect, extended with grids, anchors, named instances and fillets
via zones. Geometry is exact B-Rep (kernel SMLib), not meshes. Code and 3D view are
linked: cursor and hover highlight the corresponding geometry, handles in the view
write literals or grid references back into the code.

Plan and decisions: [`docs/technical-plan.md`](technical-plan.md).

## Status

Milestones M0–M7 are done, plus STEP export, `include`/`use`, multiple parts and
variable fillet radius, a CLI with which agents develop and check models entirely without a UI,
a desktop app (Tauri 2: native kernel, real files, file watcher as in
VS Code, Git markers in the gutter, control via `mcad ui …`), a 3D section view, a
CLI daemon that keeps the kernel cache between CLI calls, as well as multi-colour support with
colour palette, colour regions, filament slots, multi-colour 3MF for Bambu Studio/OrcaSlicer and a
reworked dark theme, corrected after an independent review, sketches with
sketch editor, text and SVG import, and a measuring tool on the exact B-Rep (distance with
foot points, radius, angle, length, area, body; in a section contours, circles, wall thickness) in
the UI and in `mcad measure` (test reports 1–19, [`docs/reports/`](reports/README.md)). Still open is the switch to the
Rust kernel solidcore (M8, waiting for the porting status of solidcore, see
[`docs/solidcore-migration.md`](solidcore-migration.md)). Browser: no backend, data is stored
in the browser (IndexedDB) or via the File System Access API; desktop: file system.

## Architecture in brief

```
Browser:  React/TS (CodeMirror 6, three.js) ── EngineTransport ── Web Worker
Worker:   cad-wasm → cad-session → cad-engine ── trait Kernel ── kernel-smlib ── smlib.wasm (Emscripten)
CLI:      cad-cli  → cad-session → cad-engine ── trait Kernel ── kernel-smlib ── SMLib native (FFI)
          (computes in the daemon `mcad daemon serve` via cad-ipc, cache persists between calls)
Desktop:  mcad (Tauri 2: same React app, TauriTransport) ── channel (binary) ── mcad engine-host
          (engine process: cad-session, SMLib native); watcher, Git, control socket in cad-desktop/cad-ipc
Language: cad-syntax (parser) → cad-eval (values with provenance, grids, anchors, handles) → cad-ir (tree)
```

- The same Rust crates run in the worker (Wasm) and in the CLI (native). The engine holds
  state and cache (content-based keys), evaluates incrementally with a time budget and
  delivers per evaluation a preliminary scene (without kernel) and a complete one.
- Kernel: SMLib/SM_API (C++, Apache-2.0) from the fork `tangency-labs/smlib` (branch
  `capi-tess` @ `7eba8aa`) plus `smlib/patches/0001-…`, behind its own C facade
  (`smlib/facade/`). Two Wasm modules: `cad_wasm_bg.wasm` (Rust, wasm-bindgen) and
  `smlib.wasm` (Emscripten), connected via a small JS bridge.
- Export: STL and 3MF from the tessellation, STEP (AP242/AP214) with a dedicated writer
  (`cad-step`) from the exact B-Rep. 3MF is multi-colour: one object per part, inside it one part
  per colour (colour regions via kernel Booleans, cached) with filament slot in
  `Metadata/model_settings.config` (Bambu Studio, OrcaSlicer); in addition "one STL per colour" as ZIP.
- No COOP/COEP headers needed (no threads, no `SharedArrayBuffer`).
- CLI daemon: `check`, `eval`, `inspect`, `render`, `export`, `outline`, `measure`, `set`
  run in a background process with one engine for all projects (starts on the first
  call, exits after 15 min without a request; limits 512 MiB cache, 8 projects, watchdog 30 min).
  Output is the same as without the daemon; disable with `--no-daemon`, `MCAD_NO_DAEMON=1` or `[daemon] enabled =
  false` in `$XDG_CONFIG_HOME/mcad/config.toml`; `mcad daemon status|stop|serve`.
- Desktop: one binary `mcad` (feature `gui`, default). `mcad` without a command starts the UI, the
  engine runs in a separate process (a watchdog terminates it if the kernel hangs), meshes
  arrive in binary form over a Tauri 2 channel. Without a graphical session: message, exit 2.

## Quick start

Prerequisites (tested versions): Rust 1.97 with target `wasm32-unknown-unknown`, C++17
compiler (g++ 13), CMake ≥ 3.20 (Ninja optional), emsdk 6.0.13, Node 22.18 or newer, pnpm
10.28, wasm-bindgen-cli in the version from `Cargo.lock` (0.2.129; `web/scripts/build-wasm.sh`
installs it if needed). For the comparison tests additionally OpenSCAD 2021.01 and Python 3
with `cadquery-ocp==8.0.1.1.0`.

```sh
# 1. SMLib sources (fork + patches) into third_party/smlib
smlib/fetch.sh

# 2. Build SMLib natively (~3 min with 4 cores) and build the CLI
smlib/build-native.sh                       # -> smlib/build-native/lib
export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo build --release -p cad-cli            # -> target/release/mcad
```

Without `SMLIB_PREBUILT`, `crates/kernel-smlib/build.rs` builds SMLib itself (once per profile).

CLI for humans and agents (details: `mcad --help`, `mcad usage` – manual for
agents/LLMs from `manuell/*.md`, English, compiled into the binary –, `.claude/skills/cad-cli/SKILL.md`,
report 8). Recommended workflow: `outline` → edit → `check` →
`render --views all --show grids,anchors` → `inspect`/`measure` → `export`.

```sh
mcad=target/release/mcad
$mcad outline tests/models/project-housing/housing.mcad          # modules, parameters, grids, anchors, parts
$mcad check tests/models/spec/housing.mcad                         # diagnostics with line:column
$mcad eval tests/models/spec/housing.mcad --json                   # scene, grids, anchors, echo
$mcad inspect tests/models/spec/fillet_zones.mcad --line 12         # what line 12 produces
$mcad render tests/models/spec/housing.mcad -o x.png --module housing --views all --show grids,anchors,labels
$mcad render tests/models/spec/housing.mcad -o s.png --section z=10 --show dims   # section with cap
$mcad render tests/models/spec/fillet_zones.mcad -o f.png --line 12 --azimuth 120 --elevation 30 --xray
$mcad measure tests/models/spec/housing.mcad --between k.corner k.top --volume part:k
$mcad measure tests/models/project-housing/housing.mcad --interference --clearance housing lid
$mcad measure tests/models/spec/hole_plate.mcad --radius "face@[8,6,2]" --at "[18,6,4]"   # R 2 · ⌀ 4, elements at P
$mcad measure tests/models/spec/housing.mcad --section z=10 --wall "[0,15,10]"         # section: regions, wall thickness 3 mm
$mcad set tests/models/spec/housing.mcad --param housing.w=50 --dry-run   # default value, diff
$mcad watch teil.mcad --json --render teil.png --views all      # re-evaluate on every change
$mcad export tests/models/spec/bottle.mcad -o bottle.step         # also .stl, .3mf
$mcad export tests/models/spec/multicolor.mcad -o druck.3mf          # multi-colour (Bambu/Orca), --single-color
$mcad measure tests/models/spec/multicolor.mcad --colors            # volume/mass per colour and slot
$mcad render tests/models/spec/multicolor.mcad -o m.png --theme dark --show regions
$mcad export tests/models/project-housing/housing.mcad --all-parts -o parts/ --format step
$mcad daemon status                                             # daemon: PID, uptime, cache, hit rate
```

The first computing command starts the daemon (`mcad daemon serve`, detached, log
`daemon.log` in the socket directory); further calls use its cache (measurements: report 10).
If the socket path would exceed the Unix limit (107 bytes, e.g. a deep `MCAD_IPC_DIR`),
the socket is placed at `/tmp/mcad-<uid>/<hash>-daemon.sock` (same permission check);
`mcad daemon status` reports it ("Ersatzpfad" (fallback path)).
Settings (default values; environment before file, arguments of `mcad daemon serve` before both):

```toml
# $XDG_CONFIG_HOME/mcad/config.toml  (otherwise ~/.config/…, path also via MCAD_CONFIG)
[daemon]
enabled = true        # false or MCAD_NO_DAEMON=1: never use the daemon; per call: --no-daemon
cache_mb = 512        # node results + meshes (MCAD_DAEMON_CACHE_MB, serve --cache-mb)
max_projects = 8      # projects whose results the cache keeps (MCAD_DAEMON_MAX_PROJECTS)
idle_minutes = 15     # exit without a request (MCAD_DAEMON_IDLE_MINUTES or MCAD_DAEMON_IDLE_SECS)
watchdog_secs = 1800  # longer-running command: daemon exits, command runs without it (MCAD_DAEMON_WATCHDOG_SECS)
rss_factor = 1.5      # RSS above start + factor × cache_mb: clear cache (MCAD_DAEMON_RSS_FACTOR)
cache_entries = 16384 # maximum number of entries (MCAD_DAEMON_CACHE_ENTRIES)
```

Every command has `--json` with a fixed, versioned schema (`schema_version`, header of
`crates/cad-cli/src/main.rs`) and exit codes 0 (ok), 1 (error in the model), 2 (invocation),
3 (kernel/internal).

Desktop app (Linux: `libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev libxdo-dev`, see `scripts/ci/desktop.sh deps`):

```sh
(cd web && pnpm install && pnpm build:tauri)   # UI without worker/Wasm -> web/dist-tauri (gets embedded)
cargo build --release -p cad-cli               # target/release/mcad with UI (without UI: --no-default-features)
mcad                                            # desktop UI
mcad tests/models/spec/housing.mcad                # with file (watched, Git gutter)
mcad tests/models/project-housing/                 # folder as project
mcad ui status --json                           # what is open (file, cursor, context, parameters, diagnostics)
mcad ui show tests/models/spec/housing.mcad --line 11
mcad ui screenshot -o ansicht.png               # 3D view of the running UI
```

Without `web/dist-tauri`, `cargo build` embeds a placeholder page (the workspace still
builds). Workflow "user in the UI, agent edits and checks": `.claude/skills/cad-cli/SKILL.md`.

Browser (build the Wasm modules, then Vite):

```sh
EMSDK=/pfad/zu/emsdk smlib/build-wasm.sh    # -> web/public/smlib/smlib.{js,wasm} (~3 min)
web/scripts/build-wasm.sh                   # cad-wasm + wasm-bindgen + wasm-opt -> web/src/wasm-pkg/
cd web
pnpm install
pnpm dev                                    # http://localhost:5173
pnpm build                                  # -> web/dist (static, e.g. GitHub Pages)
```

## Building and installing

The `Makefile` combines the steps above and calls the same scripts as the CI
(`scripts/ci/*.sh`, `smlib/*.sh`, `web/scripts/build-wasm.sh`). `make` alone lists the targets;
if a tool is missing (cargo, pnpm, node, cmake, emcc, docker …), the target aborts with a
message saying what to install.

| Target | Effect |
| --- | --- |
| `make deps` | `smlib` + `web-deps`: SMLib native (`smlib-native` → `SMLIB_PREBUILT`) and as Wasm (`smlib-wasm` → `web/public/smlib`, needs emcc: `make emsdk`), `pnpm install --frozen-lockfile` |
| `make wasm` / `make web` | cad-wasm → `web/src/wasm-pkg`; browser app → `web/dist` |
| `make ui` | UI of the desktop app → `web/dist-tauri` (without Wasm) |
| `make cli` | `target/release/mcad` without desktop UI (`--no-default-features`, without WebKitGTK) |
| `make app` | `target/release/mcad` with embedded desktop UI (Tauri 2) |
| `make build` | SMLib native + Wasm, browser app, desktop app |
| `make test` | `cargo test --workspace` and Vitest (`test-rust`, `test-web`; plus `test-node`) |
| `make e2e` / `make e2e-desktop` | Playwright (Chromium) / Tauri E2E (tauri-driver, Xvfb) |
| `make lint` | `cargo fmt --check`, Clippy native and wasm32 (`-D warnings`), `tsc` |
| `make fmt` | `cargo fmt --all` |
| `make install` | `mcad`, desktop file, app icon (`assets/icon`: PNG 16–512 px, SVG), licences, docs and examples |
| `make uninstall` | removes these files again |
| `make dist` | source archive of `HEAD` → `target/arch/mcad-<pkgver>.tar.gz` |
| `make arch-pkg` / `make arch-test` | build Arch package / install and check it in a fresh container |
| `make clean` / `make distclean` | build output / additionally `node_modules`, SMLib build directories |

Variables (environment or `make VAR=…`): `CARGO`, `PNPM`, `NODE`, `GIT`, `DOCKER`;
`SMLIB_PREBUILT` (default `smlib/build-native`), `SMLIB_SRC` (`third_party/smlib`), `EMSDK`,
`CARGO_TARGET_DIR`, `CARGO_FLAGS` (`--locked`); `PREFIX` (`/usr/local`), `DESTDIR`, `BINDIR`,
`DATADIR`, `LICENSEDIR`, `DOCDIR`; for the package `ARCH_BUILD`, `ARCH_OUT`, `ARCH_IMAGE`,
`ARCH_DOCKER_NETWORK`, `ARCH_CA_BUNDLE`, `ARCH_CACHE_VOLUME`.

```sh
make app SMLIB_PREBUILT=$PWD/smlib/build-native   # SMLib, UI, mcad with UI
sudo make install PREFIX=/usr/local               # or: make install DESTDIR=/tmp/root PREFIX=/usr
```

`make install` creates: `$PREFIX/bin/mcad`, `share/applications/dev.mcad.desktop.desktop`
(`mcad %f`, without a file it starts the UI; the file name is the app ID from `crates/cad-app/tauri.conf.json`,
which the app also sets as Wayland `app_id` and X11 `WM_CLASS` – this is how the taskbar matches the icon; file types `text/x-mcad` and `application/x-openscad`), `share/mime/packages/mcad.xml`
(`*.mcad`, `*.scad`), `share/icons/hicolor/<n>x<n>/apps/mcad.png` (n = 16 … 512 from `assets/icon/png`) and
`share/icons/hicolor/scalable/apps/mcad.svg` (`assets/icon/cad.svg`),
`share/licenses/mcad/` (`NOTICE`, OFL texts of the fonts with `fonts-README.md`,
SMLib `LICENSE` and `THIRD_PARTY_NOTICES.md`), `share/doc/mcad/` (README, CLI guide
`cad-cli.md`, `examples/`). It installs whatever `target/release/mcad` currently is (`make app`
or `make cli`); `install` does not build by itself. `make uninstall` also removes the desktop file,
icons, licences and docs under the old names (`controller-cad`), but not an old `bin/cad`.

### Arch package

`packaging/arch/PKGBUILD` builds the package `mcad` (replaces the former package `controller-cad`): `mcad` with desktop UI and CLI,
desktop file, icons, licences (project MIT, SMLib Apache-2.0, fonts
OFL-1.1) and examples. Runtime: `webkit2gtk-4.1`, `gtk3`, `libsoup3`, `glib2`, `cairo`,
`gdk-pixbuf2`, `dbus`, `libgcc`, `libstdc++`, `glibc`, `hicolor-icon-theme`, `shared-mime-info`; build: `cargo`, `nodejs`, `pnpm`,
`cmake`, `ninja`, `git` (and `base-devel`). SMLib comes as a Git source at the commit from
`smlib/fetch.sh` (other location: `SMLIB_URL=…`), `prepare()` applies the patches from `smlib/patches`;
`build()` is `make app`, `package()` is `make install DESTDIR=$pkgdir PREFIX=/usr`.

```sh
make arch-pkg    # -> target/arch/mcad-<pkgver>-1-x86_64.pkg.tar.zst, namcap.txt
make arch-test   # fresh archlinux container: pacman -U, CLI headless, UI under Xvfb
sudo pacman -U target/arch/mcad-*.pkg.tar.zst
```

- `pkgver` = Cargo version + Git state (`0.3.0.r<commits>.g<hash>`); the archive is `git archive
  HEAD`, uncommitted changes are missing from it (`make` points this out).
- `ARCH_BUILD=auto` (default): on Arch with `makepkg` (`MAKEPKG_FLAGS`, default `--syncdeps
  --force`), otherwise in Docker (`packaging/arch/docker.sh`): a builder image from `archlinux:latest`
  with `base-devel`, `depends`, `makedepends` and `namcap`, `makepkg` as user `builder`,
  then `namcap` over PKGBUILD and package (`target/arch/namcap.txt`). Cargo and pnpm caches
  live in the volume `controller-cad-arch-cache`; `make arch-clean` removes image, volume and
  `target/arch`.
- Behind a proxy: `HTTPS_PROXY`/`NO_PROXY` are passed through, `ARCH_DOCKER_NETWORK=host`
  (default) reaches a proxy on `127.0.0.1`, `ARCH_CA_BUNDLE=/pfad/ca.crt` adds an
  extra CA to the container (trust store and `NODE_EXTRA_CA_CERTS` for pnpm).
- `make arch-test` (`packaging/arch/smoke-test.sh`): installation with dependencies from the
  repositories, `pacman -Qi/-Ql/-Qkk`, missing libraries (`ldd`), `mcad --version`, `check`,
  `outline`, `render`, `export` (STEP/STL/3MF), `measure`, daemon, exit 2 without display,
  desktop UI under Xvfb (`mcad ui status`, `mcad ui screenshot`), `desktop-file-validate`;
  results in `target/arch/test/`.

## Language in brief

- OpenSCAD syntax: variables, functions, modules, `for`/`if`/`let`, primitives, transformations,
  Booleans, extrusions, `include`/`use`. Lengths in mm, exact geometry (`$fn` only affects display,
  except `$fn` = 3…12 at the call).
- `grid g { x: …; y: …; z: … }` defines named planes; `g[x, y, z]` is a point, an
  entry can also be a number; `g.name` is a number or plane, `*` yields a line/plane.
- `anchor name = punkt, dir = v;` in a module, `k: modul();` names the instance, `k.name` and
  `dir(k.name)` access it from outside; `k[i]: …` in loops.
- `fillet(r) { körper; zone(…); … }` fillets the edges of the body that lie
  in the zones (`chamfer` and `shell` likewise); `difference(fillet = r)` the new edges,
  `$fillet = r` all edges of a primitive; `r` may be a function of position.
- Named instances at top level are parts (export per part).
- `color("teal")` takes a small palette of 16 muted colours (`red, orange, yellow, lime,
  green, teal, cyan, blue, purple, magenta, pink, brown, black, gray, silver, white`, case-insensitive,
  `grey`/`aqua`/`fuchsia` as CSS spellings); unknown names warn and get
  a stable random colour, hex and vectors stay exact. `color("red", filament = 3)` sets
  the filament slot, otherwise slots are assigned in order of first appearance (uncoloured
  material: slot 1). The UI shows "Farben & Filamente" (colours & filaments) (slot via selection, writes
  `filament = n` into the code, volume and mass PLA) and the colour regions as printed.
  If the colour comes from a parameter or a loop, `filament("red", 4);` at
  top level sets the slot (the UI then writes that). `alpha` only affects the view,
  `transparent` does not colour. The 3MF has one object per part (remainder: "no part") and
  no colour group, so that Bambu Studio takes over the slots without a colour dialog (report 12).

- `sketch { start(p); line(to = …); arc(to = …, r = …); bezier(c1 = …, to = …); spline(…);
  close(); }` is a 2D profile of exact segments (lines, circular arcs, Bézier, splines) for
  `linear_extrude`, `rotate_extrude`, `loft` and `sweep` (STEP writes circles and lines exactly).
  Each line is a segment with display, handles and its own errors (not closed,
  impossible arc, self-intersection); multiple contours with `start`, holes by the
  winding rule or in `hole { … }`, `for`/`if`/`let` and grids in the sketch;
  `sketch(closed = false)` is a `sweep` path. `module profil(b = 40) sketch { … }` is a
  named sketch (plan 17, report 14).
- `corner(r = …)` / `corner(chamfer = …)` rounds or chamfers the corner between two segments
  (exact for lines and arcs, numerical for Bézier/splines; radius handle), `fillet2d(r)` /
  `chamfer2d(d)` all corners of a 2D profile or only those in 2D zones, `offset(r | delta,
  chamfer)` as in OpenSCAD (exact arcs instead of facets), `stroke(w, cap, join)` turns
  an open path into an area (ribs, slots, cable ducts; plan 17.3, report 15).
- Modules are namespaces: `module teile { module schraube(d = 3) { … } }` (braces may
  be omitted), call from outside `teile.schraube(d = 4);`, multi-level `a.b.c();`; functions and
  constants of a module as `teile.kopf(d)`, `teile.M3`; `mcad --module teile.schraube`.
- `text("Schild", size = 10, font = "Inter:style=Bold", halign = "center")` as in OpenSCAD
  (size, tracking, alignment, `spacing`, `direction`, `language`, `script`; hull and
  area equal to OpenSCAD 2021.01), exact outlines (Bézier), kerning and ligatures via
  `rustybuzz`. **Default font** is the project's own stencil font `"cad stencil mono"`
  (Regular/Bold): monospaced, made of lines and circular arcs, with bridges – no glyph has a
  hole, nothing falls out when cut; `stroke_width` (stroke width, default
  0.14·size, Bold 0.2·size) and `bridge_width` (bridge width, default 0.1·size, 0 = no bridges)
  in mm. Bundled (SIL OFL): JetBrains Mono (Regular/Bold), Inter (Regular/Bold), Allerta
  Stencil; custom fonts with `font = "datei.ttf"` (project file, in the UI "⇡ Datei
  laden" (load file)). Required character set with umlauts, ß/ẞ and punctuation; if a character is missing in a
  font, it is taken from JetBrains Mono with a warning.
- `import("logo.svg", id = "…", scale = 1, dpi = 96, center = false)` reads SVG as an exact
  2D profile: `path` (all commands), `rect`, `circle`, `ellipse`, `polygon`, `polyline`,
  `line`, groups with `transform`, `viewBox`/units, `fill-rule`; circles stay circles.
  Unsupported features (text, images, `use`, CSS classes, masks, strokes) report a warning
  with the SVG line. The file is resolved relative to the including file (CLI: file system,
  `mcad watch` watches it; browser: project file, editable in the editor) (plan 17.4/17.5,
  report 16).
- `import("teil.stl", center, scale, as = "solid" | "mesh")` reads STL (ASCII and binary):
  welded, orientation corrected, coplanar triangles merged into planar faces.
  Closed meshes up to 500 faces automatically become B-Rep (Booleans, STEP), larger
  or open ones stay a pure mesh (display, export STL/3MF, measuring; no Booleans) (plan
  21, phase B).

```
linear_extrude(5) sketch {
    start([0, 0]);
    line(to = [40, 0]);
    arc(to = [50, 10], r = 10);          // also: center + angle, through, tangential
    line(by = [0, 20]);
    bezier(c1 = [45, 40], to = [20, 35]);
    close();
    hole { start([12, 12]); arc(to = [22, 12], r = 5); arc(to = [12, 12], r = 5); }
}
```

```
module halter(w = 40, d = 20, h = 10) {
    grid g {
        x: links = 0, mitte = w / 2, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }
    fillet(2) {
        difference() {
            box(g[links, vorne, boden], g[rechts, hinten, deckel]);
            translate(g[mitte, 10, -1]) cylinder(h = h + 2, r = 4);   // number instead of plane: y = 10
        }
        zone(g.deckel, d = 1);              // all edges in the lid plane
    }
    anchor oben = g[mitte, 10, deckel], dir = [0, 0, 1];
}

k: halter(w = 50);
translate(k.oben) rotate(to = dir(k.oben)) cylinder(h = 8, r = 3);
```

More examples: `tests/models/spec/` (language extensions, sketches: `sketch_*.mcad`, text: `text_sign.mcad`,
SVG: `svg_logo.mcad` with `tests/models/svg/*.svg`, STL: `stl_import.mcad` with `tests/models/stl/`), `tests/models/openscad-compat/`
(36 OpenSCAD models), `tests/models/project-housing/` (project of three files).

## Directory structure

```
crates/
  cad-syntax/        lexer, parser, AST with spans (file IDs)
  cad-eval/          evaluation: values with provenance, grids, anchors, handles, write-back, include/use
  cad-ir/            geometry tree, cache keys, radius law (Law)
  cad-kernel/        trait Kernel, shared types, neutral B-Rep
  kernel-smlib/      kernel via the SMLib facade (native via FFI, Wasm via JS bridge)
  kernel-solidcore/  kernel via solidcore (opt-in, feature `solidcore`)
  kernel-mock/       test kernel in Rust
  cad-engine/        evaluation with cache, provenance, zones, parts, export STL/3MF/STEP
  cad-step/          STEP writer (AP242/AP214)
  cad-measure/       measuring on the exact B-Rep (distance, radius, angle, length, area, section contours, wall thickness)
  cad-outline/       neutral 2D contours (line, circular/elliptical arc, Bézier), fill rules
  cad-svg/           SVG import (subset) → contours in mm
  cad-font/          text: bundled OFL fonts (fonts/), shaping (rustybuzz), glyph outlines
  cad-render/        software rasterizer for `mcad render` (section with cap, labels)
  cad-session/       protocol (worker and CLI), TS types
  cad-wasm/          wasm-bindgen entry point for the worker
  cad-ipc/           local IPC (Unix socket per user, versioned frames, lock files) – UI control, CLI daemon
  cad-desktop/       desktop backend without Tauri: engine process, watchdog, files, watcher, Git hunks
  cad-app/           Tauri 2 app (embedded UI from web/dist-tauri)
  cad-cli/           CLI `mcad` (without a command: desktop UI)
  compat-tests/      OpenSCAD comparison stage 1 (Rust tests)
smlib/               fetch.sh, patches, CMakeLists, facade, build scripts native/Wasm
third_party/smlib/   SMLib sources (not checked in, smlib/fetch.sh)
web/                 React app (src/), Playwright tests (e2e/), Tauri E2E (e2e-tauri/), build scripts (scripts/)
examples/            example models (also test cases)
tests/               openscad-compat/ (reference, compare.py), step/verify.py
tools/report/        generators of the test reports, 3MF check (verify3mf.py)
tools/palette/       check of the colour palette (ΔE in OKLab, colour vision deficiency, contrast)
tools/font/          generator of the bitmap font of cad-render
scripts/ci/          check and build steps (also called by the Makefile)
packaging/           desktop file and NOTICE (linux/), Arch package (arch/: PKGBUILD, Docker build, check script)
Makefile             targets for building, checking, installing, Arch package (make help)
docs/                plan, solidcore migration, test reports
```

## Tests

```sh
export SMLIB_PREBUILT=$PWD/smlib/build-native
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                          # ~450 tests (daemon: tests/daemon.rs)
cargo check -p cad-wasm --target wasm32-unknown-unknown

node crates/kernel-smlib/js/test-node.mjs                       # smlib.wasm via the bridge
node web/scripts/smoke-node.mjs                                 # browser engine in Node
cd web && pnpm typecheck && pnpm test && pnpm e2e               # tsc, Vitest, Playwright (Chromium)
node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad    # desktop: tauri-driver + WebKitWebDriver (Xvfb)

python3 tests/openscad-compat/make-reference.py                 # reference STLs (OpenSCAD 2021.01)
python3 tests/openscad-compat/compare.py --self-test
MCAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval   # 33/33
/pfad/zum/venv/bin/python tests/step/verify.py --cad target/release/mcad  # STEP with OpenCASCADE
python3 tools/palette/palette.py                                # palette: ΔE, colour vision deficiency, contrast
python3 tools/report/verify3mf.py teil.3mf                      # 3MF: lib3mf, trimesh, Bambu import model
```

There are no GitHub workflows (removed 2026-10-30). The checks run locally with the scripts
in `scripts/ci/`: `rust.sh` (Tauri packages, fmt, clippy, tests, Wasm check), `web.sh`
(smlib.wasm, cad-wasm, typecheck, Vitest, Node tests, build, Playwright), `desktop.sh` (UI for
Tauri, release build, Tauri E2E under Xvfb), `compat.sh` (OpenSCAD reference and comparison,
include/use against OpenSCAD, STEP check), `solidcore.sh` (`diff-kernels` against solidcore
`main`), e.g. `scripts/ci/smlib-native.sh && SMLIB_PREBUILT=smlib/build-native scripts/ci/rust.sh all`.
If the SMLib fork is not publicly readable, `SMLIB_URL` gives `smlib/fetch.sh` another URL.

## Test reports

Overview with generator commands: [`docs/reports/README.md`](reports/README.md).

- [Report 1](reports/01-kernel-sprache-engine-cli/report.pdf): kernel, language, engine, CLI; OpenSCAD comparison 33/33.
- [Report 2](reports/02-ui-phase-a/report.pdf): browser UI phase A – editor, view, mapping code ↔ view.
- [Report 3](reports/03-ui-phase-b/report.pdf): UI phase B – handles, snap, parameter bar.
- [Report 4](reports/04-fillet-profile/report.pdf): fillet, chamfer, shell, profile operations, draft mode, watchdog lock.
- [Report 5](reports/05-step-export/report.pdf): STEP export and kernel comparison SMLib ↔ solidcore.
- [Report 6](reports/06-projekte-bauteile/report.pdf): projects, parts, `include`/`use`.
- [Report 7](reports/07-kernel-variabler-radius/report.pdf): kernel fixes and variable radius.
- [Report 8](reports/08-cli-agenten/report.pdf): CLI for agents (outline, render views/section, measure, watch, set).
- [Report 9](reports/09-desktop-schnitt/report.pdf): desktop app, file watcher, Git, section view, CLI↔UI.
- [Report 10](reports/10-cli-daemon/report.pdf): CLI daemon (cache between calls, limits, measurement).
- [Report 11](reports/11-farben-filamente/report.pdf): colours, multi-filament 3MF (Bambu Studio/OrcaSlicer), dark theme.
- [Report 12](reports/12-farben-korrekturen/report.pdf): fixes for colours and daemon (findings F1–F9 of the independent review, Bambu colour dialog, socket path).
- [Report 14](reports/14-skizzen/report.pdf): sketches with mixed contours (lines, arcs, Bézier, splines) and modules as namespaces.
- [Report 15](reports/15-skizzen-ecken-offset/report.pdf): sketches – corners (`corner`, `fillet2d`), `offset` (OpenSCAD comparison), `stroke`.
- [Report 16](reports/16-text-svg/report.pdf): text with bundled fonts and SVG import, comparison with OpenSCAD.
- [Report 17](reports/17-schablonenschrift/report.pdf): stencil font "cad stencil mono" as default font.
- [Report 18](reports/18-skizzen-editor/report.pdf): sketch editor (drawing mode with grid, snap, code per action).
- [Report 19](reports/19-messen/report.pdf): measuring tool in UI and CLI (exact on the B-Rep, in a section, relation to the language).

## Licences

- This project: MIT (`LICENSE`), `publish = false`.
- SMLib/SM_API (NVIDIA usd-brep, fork tangency-labs/smlib): Apache-2.0.
- Tauri 2 (Apache-2.0/MIT), `notify` (CC0/MIT/Apache-2.0), `similar` (Apache-2.0). The sources are
  not part of the repository; `smlib/patches/` contains changes to them.
- solidcore (tangency-labs/solidcore, optional dependency): Apache-2.0.
- Bundled fonts in `crates/cad-font/fonts/` (JetBrains Mono, Inter, Allerta Stencil):
  SIL Open Font License 1.1, licence texts and sources alongside (`fonts/README.md`). The
  default font "cad stencil mono" is part of the source code (`crates/cad-font/src/stencil.rs`).
  `rustybuzz`/`ttf-parser` (MIT), `roxmltree` (MIT/Apache-2.0).
- Further dependencies via Cargo and pnpm (including React, three.js, CodeMirror) under their
  respective licences (mostly MIT/Apache-2.0).
