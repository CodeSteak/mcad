#!/usr/bin/env node
// Test report 8 ("CLI for agents"): runs the new CLI functions on examples, measures run times,
// replays the case study (tools/report/report8-fallstudie/) and writes
//
//   docs/reports/08-cli-agenten/report.html, img/*
//
// Inputs (gitignored, reports/tmp/report8/):
//   cargo.txt    `cargo test --workspace --no-fail-fast`
//   clippy.txt   `cargo clippy --workspace --all-targets -- -D warnings`
//   fmt.txt      `cargo fmt --all --check` (empty = ok)
// Binary: $CAD (default target/release/mcad).
//
//   node tools/report/report8.mjs
//   node tools/report/print-pdf.mjs docs/reports/08-cli-agenten/report.html docs/reports/08-cli-agenten/report.pdf "Test report 8: CLI for agents"

import { execFileSync, spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report8");
const work = join(input, "work");
const out = join(root, "docs/reports/08-cli-agenten");
const img = join(out, "img");
const cad = process.env.CAD ?? join(root, "target/release/mcad");
const fallstudie = join(here, "report8-fallstudie");

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
/** Number in English format (decimal point, comma as thousands separator). */
const num = (x, d = 0) => Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
const rel = (p) => relative(root, p);

rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
rmSync(img, { recursive: true, force: true });
mkdirSync(img, { recursive: true });

/** Runs `cad` (paths shown relative to the repository). */
function run(args, opts = {}) {
  args = args.map((a) => (a.startsWith(root + "/") ? rel(a) : a));
  const t0 = process.hrtime.bigint();
  const r = spawnSync(cad, args, { cwd: root, encoding: "utf8", maxBuffer: 256 << 20, ...opts });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  return { code: r.status, out: r.stdout ?? "", err: r.stderr ?? "", ms, cmd: "cad " + args.map((a) => (/[\s\[\]*,]/.test(a) && !a.startsWith("-") ? `"${a}"` : a)).join(" ") };
}
function runJson(args) {
  const r = run([...args, "--json"]);
  let j = null;
  try {
    j = JSON.parse(r.out);
  } catch {}
  return { ...r, j };
}
const ex = (p) => rel(join(root, "tests", "models", p));
const td = (p) => rel(join(root, "crates/cad-cli/tests/data", p));
const im = (name) => join(img, `${name}.png`);
const imRel = (name) => `img/${name}.png`;

/** Excerpt of a JSON object: only the given top-level keys, long arrays shortened. */
function excerpt(j, keys, maxArr = 3) {
  const cut = (v, depth = 0) => {
    if (Array.isArray(v)) {
      const a = v.slice(0, maxArr).map((x) => cut(x, depth + 1));
      if (v.length > maxArr) a.push(`… (${v.length - maxArr} more)`);
      return a;
    }
    if (v && typeof v === "object") {
      const o = {};
      for (const [k, x] of Object.entries(v)) o[k] = cut(x, depth + 1);
      return o;
    }
    if (typeof v === "number" && !Number.isInteger(v)) return Math.round(v * 1e4) / 1e4;
    return v;
  };
  const o = {};
  for (const k of keys) if (j && k in j) o[k] = cut(j[k]);
  return compact(o);
}

/** JSON with short arrays and objects on one line. */
function compact(v, ind = "") {
  const flat = JSON.stringify(v);
  if (flat === undefined) return "null";
  if (flat.length + ind.length <= 100) return flat.replace(/,"/g, ', "').replace(/":/g, '": ').replace(/,(?=[\d\-\[n{])/g, ", ");
  const pad = ind + "  ";
  if (Array.isArray(v)) return "[\n" + v.map((x) => pad + compact(x, pad)).join(",\n") + "\n" + ind + "]";
  if (v && typeof v === "object")
    return "{\n" + Object.entries(v).map(([k, x]) => `${pad}${JSON.stringify(k)}: ${compact(x, pad)}`).join(",\n") + "\n" + ind + "}";
  return flat;
}
/** Pixel statistics of a rendered image (python3 with Pillow). */
function pixelStats(name) {
  const py = "import sys\nfrom PIL import Image\nim=Image.open(sys.argv[1]).convert('RGB')\nbg=im.getpixel((0,0))\nd=c=0\nfor p in im.getdata():\n  if max(abs(p[i]-bg[i]) for i in range(3))>6: d+=1\n  if p[0]>p[1]+60 and p[0]>p[2]+60 and p[0]>90: c+=1\nprint(d,c)";
  const r = spawnSync("python3", ["-I", "-c", py, im(name)], { encoding: "utf8" });
  const [d, c] = (r.stdout ?? "").trim().split(" ").map(Number);
  return { d: d || 0, c: c || 0 };
}
const drawn = (name) => pixelStats(name).d;
const capPixels = (name) => pixelStats(name).c;
const lines = (s, n = 40) => {
  const l = s.trimEnd().split("\n");
  return l.length > n ? [...l.slice(0, n), `… (${l.length - n} more lines)`].join("\n") : l.join("\n");
};

// ---------------------------------------------------------------- functions

const sections = [];
const sec = (title, items) => sections.push({ title, items });
const block = (r, opts = {}) => ({ kind: "cmd", r, ...opts });

const G = td("housing.mcad");
const K = td("collision.mcad");
const P = ex("project-housing/housing.mcad");

// outline
{
  const a = run(["outline", P]);
  const b = runJson(["outline", G]);
  sec("1 cad outline", [
    { kind: "p", html: "One call returns the structure of a project: modules with file:line, parameters (default value, inferred type), grids and anchors per module (evaluated with the default values, expression planes with expression and value), functions, top-level variables with values, named instances and parts (bounding box, volume), <code>include</code>/<code>use</code> and the diagnostic count. <code>--no-kernel</code> omits bounding boxes and volumes." },
    block(a, { text: true }),
    block(b, { json: ["modules", "parts", "counts"] }),
  ]);
}

// render: contact sheet
{
  const a = runJson(["render", G, "-o", im("bogen"), "--module", "housing", "--views", "all", "--show", "grids,anchors,labels,dims", "--size", "1500x1100"]);
  const b = runJson(["render", P, "-o", im("bogen-projekt"), "--views", "iso,front,top,right", "--show", "anchors,labels", "--size", "1500x1100"]);
  sec("2 cad render: contact sheet", [
    { kind: "p", html: "<code>--views iso,front,top,right</code> (or <code>all</code>) combines labelled tiles into one image. All tiles have the same scale (px/mm) and the same target point (center of the model), so sizes are comparable between views; <code>--size</code> is the total size. The JSON gives position and camera per tile." },
    block(a, { json: ["width", "height", "views", "overlays"], image: "bogen", caption: "Spec example housing in module context: grid g with plane names and grid points g[a,b,c], anchors as diamonds with direction arrow, dimensions W/D/H." }),
    block(b, { image: "bogen-projekt", caption: "Project of three files in file context: anchors of the named instances (housing.lid_mid, boss[i].top) and part names." }),
  ]);
}

// render: free camera and selection
{
  const a = runJson(["render", ex("spec/fillet_zones.mcad"), "-o", im("kamera-azimut"), "--line", "12", "--azimuth", "120", "--elevation", "30", "--size", "700x500"]);
  const b = runJson(["render", G, "-o", im("kamera-perspektive"), "--perspective", "--fov", "45", "--azimuth", "-30", "--elevation", "20", "--size", "700x500"]);
  const c = runJson(["render", P, "-o", im("auswahl-part"), "--part", "lid", "--show", "dims", "--size", "700x500"]);
  const d = runJson(["render", G, "-o", im("auswahl-zeilen"), "--line", "22", "--line", "23", "--fit", "line:22", "--zoom", "0.8", "--size", "700x500"]);
  sec("3 cad render: free camera and selection", [
    { kind: "p", html: "Camera: <code>--azimuth</code> (0° from the front/−y, 90° from the right/+x), <code>--elevation</code> (90° from above), <code>--zoom</code>, <code>--target x,y,z</code>, <code>--fit all|part:NAME|line:N</code>, <code>--ortho</code>/<code>--perspective [--fov]</code>. Selection: <code>--part NAME</code> (parts only), <code>--line N</code> and <code>--node I</code> repeatable (nodes in full, the rest dimmed, <code>--only</code> omits the rest), combinable with <code>--module</code> and <code>-D</code>." },
    { kind: "pair", items: [
      block(a, { image: "kamera-azimut", caption: "fillet_zones.mcad --line 12 --azimuth 120 --elevation 30: zones blue, selected edges orange." }),
      block(b, { image: "kamera-perspektive", caption: "housing --perspective --fov 45 --azimuth -30 --elevation 20" }),
    ] },
    { kind: "pair", items: [
      block(c, { image: "auswahl-part", caption: "Project --part lid --show dims" }),
      block(d, { image: "auswahl-zeilen", caption: "--line 22 --line 23 --fit line:22: two lines highlighted, fitted to line 22." }),
    ] },
    block(b, { json: ["camera"] }),
    block(d, { json: ["highlighted", "visible_nodes"] }),
  ]);
}

// render: overlays
{
  const shows = ["grids", "anchors", "handles", "edges", "labels", "bbox", "dims"];
  const base = runJson(["render", G, "-o", im("ov-ohne"), "--module", "housing", "--size", "420x320", "--no-axes"]);
  const rows = [];
  const tiles = [];
  for (const s of shows) {
    const sh = s === "labels" ? "grids,labels" : s;
    const r = runJson(["render", G, "-o", im(`ov-${s}`), "--module", "housing", "--size", "420x320", "--no-axes", "--show", sh]);
    rows.push({ s: sh, r });
    tiles.push({ image: `ov-${s}`, caption: `--show ${sh}` });
  }
  sec("4 cad render: overlays", [
    { kind: "p", html: "<code>--show grids,anchors,handles,edges,labels,bbox,dims</code> (or <code>all</code>): grids of the context module (planes as rectangles at the edge of the grid, expression planes dashed, intersections as dots, plane names <code>g.links</code>), anchors as diamonds with name and arrow for <code>dir</code>, handles as squares, hidden edges dashed; <code>labels</code> labels grid points <code>g[a,b,c]</code> (up to 48 points per grid), handles and parts, <code>bbox</code> draws the bounding box, <code>dims</code> additionally width/depth/height in mm. Labels that would overlap are shifted or omitted." },
    { kind: "grid", items: [{ image: "ov-ohne", caption: "no overlay" }, ...tiles] },
    { kind: "table", head: ["--show", "Grids", "Grid points", "Anchors", "Handles", "Labels", "Lines"], rows: rows.map(({ s, r }) => { const o = r.j?.overlays ?? {}; return [s, o.grids, o.grid_points, o.anchors, o.handles, o.labels, o.lines]; }) },
  ]);
  void base;
}

// render: section, xray
{
  const a = runJson(["render", G, "-o", im("schnitt-z"), "--section", "z=10", "--size", "600x450"]);
  const a0 = runJson(["render", G, "-o", im("schnitt-ohne"), "--size", "600x450"]);
  const b = runJson(["render", G, "-o", im("schnitt-flip"), "--section", "z=10", "--section-flip", "--view", "bottom", "--size", "600x450"]);
  const c = runJson(["render", ex("spec/bottle.mcad"), "-o", im("schnitt-ebene"), "--section-plane", "0,0,0,1,1,0", "--perspective", "--size", "600x450"]);
  const d = runJson(["render", join(fallstudie, "v2.scad"), "-o", im("schnitt-fallstudie"), "--section", "x=20", "--size", "600x450"]);
  const e = runJson(["render", ex("spec/fillet_zones.mcad"), "-o", im("xray"), "--xray", "--size", "600x450"]);
  const f = runJson(["render", P, "-o", im("xray-projekt"), "--xray", "--show", "anchors", "--size", "600x450"]);
  sec("5 cad render: section and X-ray", [
    { kind: "p", html: "<b>Section:</b> <code>--section x=VALUE|y=VALUE|z=VALUE</code>, <code>--section-plane px,py,pz,nx,ny,nz</code>, <code>--section-flip</code>. Convention (the same for the later UI): plane through <i>p</i> with normal <i>n</i>; the half that <i>n</i> points into is removed, (q − p)·n ≤ 0 remains. <code>z=10</code> means p = (0,0,10), n = +z; <code>--section-flip</code> reverses n. The section face is drawn as a hatched cap in its own color (hatching alternates per body), the section contour as a line." },
    { kind: "p", html: "<b>Method: clipping in the renderer with a cap, no kernel boolean.</b> The triangles are clipped geometrically at the plane (Sutherland–Hodgman per triangle, edges likewise). The cap follows the GPU stencil principle: for each sample, the number of times the view ray crosses the surface of a body up to the plane is counted (top-left rule, so shared edges count exactly once); if the count is odd, the plane point lies inside the body and the cap is written at plane depth. Reasons: (1) the UI will use the same method with GPU clipping and a stencil cap, so images and convention match; (2) it needs no kernel work – a boolean with a half-space quickly costs seconds for filleted bodies and can fail, clipping costs a few ms and always succeeds; (3) it works for any plane, also for views of several parts. It requires closed meshes; the tessellation of the B-rep bodies is closed. Open surfaces (<code>surface</code>) and transparent bodies get no cap." },
    { kind: "pair", items: [
      block(a0, { image: "schnitt-ohne", caption: "no section" }),
      block(a, { image: "schnitt-z", caption: "--section z=10: upper half removed, walls cut" }),
    ] },
    { kind: "pair", items: [
      block(b, { image: "schnitt-flip", caption: "--section z=10 --section-flip --view bottom: upper half remains" }),
      block(c, { image: "schnitt-ebene", caption: "bottle.mcad --section-plane 0,0,0,1,1,0 --perspective" }),
    ] },
    { kind: "pair", items: [
      block(d, { image: "schnitt-fallstudie", caption: "Case study --section x=20: rod in the bore, two caps with opposite hatching, 0.25 mm gap as contour" }),
      block(e, { image: "xray", caption: "fillet_zones.mcad --xray: everything semi-transparent with edges" }),
    ] },
    block(a, { json: ["section", "bounds"] }),
    { kind: "p", html: `Visible area (pixels ≠ background, 600×450): without section ${num(drawn("schnitt-ohne"))}, with <code>--section z=10</code> ${num(drawn("schnitt-z"))} (${num((100 * drawn("schnitt-z")) / Math.max(1, drawn("schnitt-ohne")), 0)} %), of which cap ${num(capPixels("schnitt-z"))}. The same comparison is the test <code>render_section_reduces_visible_area</code>.` },
    { kind: "figure", image: "xray-projekt", caption: "Project --xray --show anchors: screw bosses and anchors visible through the wall" },
  ]);
  void f;
}

// measure
{
  const a = run(["measure", G, "--between", "k.corner", "k.top", "--bounds", "part:k", "--volume", "part:k", "--area", "part:k"]);
  const b = run(["measure", G, "--module", "housing", "--between", "g[left,front,floor]", "g[right,back,lid]"]);
  const b2 = run(["measure", G, "--module", "housing", "-D", "w=60", "--between", "corner", "[0,0,0]"]);
  const c = run(["measure", K, "--interference", "--clearance", "b", "c"]);
  const c2 = runJson(["measure", K, "--interference"]);
  const d = runJson(["measure", P, "--interference", "--clearance", "housing", "lid"]);
  const e = run(["measure", G, "--between", "g[left,front,floor]", "[0,0,0]"]);
  const kr = runJson(["render", K, "-o", im("kollision"), "--show", "labels", "--xray", "--size", "600x400"]);
  sec("6 cad measure", [
    { kind: "p", html: "<code>--between A B</code> with anchors (<code>k.corner</code>, <code>k[2].corner</code>, in module context <code>corner</code>), grid points of the context module (<code>g[a,b,c]</code>, entries may also be numbers or <code>_</code>) and coordinates <code>[x,y,z]</code>: distance and difference vector. <code>--bounds/--volume/--area TARGET</code> with <code>all</code>, <code>part:NAME</code>, <code>line:N</code>, <code>node:I</code>. <code>--interference</code>: pairwise intersection volume of all parts, exact via the kernel (boolean intersection of the B-rep, bounding boxes tested first); collision from <code>--tolerance</code> (default 0.001 mm³). <code>--clearance A B</code>: smallest distance between two parts as the exact triangle–triangle distance of their tessellations (BVH, tolerance <code>--mesh-tol</code> 0.01 mm, accuracy ± 2·0.01 mm); if the parts intersect, the distance is 0 and <code>collision: true</code>." },
    { kind: "p", html: "Expected values, spec example <i>housing</i>: k.corner = [60, 30, 20], k.top = [0, 0, 20] → √(60² + 30²) = 67.082 mm; in the module g[left,front,floor] → g[right,back,lid] = √(40² + 30² + 20²) = 53.852 mm; volume k = 60·30·20 − 54·30·17 = 8460 mm³. <i>kollision.scad</i>: a and b overlap by 2 × 10 × 10 = 200 mm³, c is 5 mm next to b." },
    block(a, { text: true }),
    block(b, { text: true }),
    block(b2, { text: true }),
    block(e, { text: true, note: "Error expected: grid g is not visible in file context (exit 2)." }),
    block(c, { text: true }),
    block(c2, { json: ["interference"] }),
    block(d, { json: ["clearance"] }),
    { kind: "figure", image: "kollision", caption: "collision.mcad --xray --show labels: a and b overlap, c at 5 mm distance" },
  ]);
  void kr;
}

// set
{
  const f = join(work, "set.scad");
  copyFileSync(G, f);
  const a = run(["set", rel(f), "--param", "housing.w=50", "--literal", "21:16=70", "--dry-run"]);
  const b = runJson(["set", rel(f), "--param", "housing.w=50", "--literal", "21:16=70"]);
  const c = run(["set", rel(f), "--literal", "5:30=4"]);
  const d = run(["set", rel(f), "--param", "housing.t=foo"]);
  sec("7 cad set", [
    { kind: "p", html: "<code>--param module.param=value</code> writes a default value (like \"Apply as default\" in the UI, also for modules from <code>use</code> files), <code>--literal [file:]line:col=value</code> replaces the editable literal at that position – the same literals the UI changes via handles, sliders and grid planes. Before writing, the changed project is evaluated; new errors prevent writing (exit 1, <code>--force</code> overrides). Writing is atomic (temporary file + rename); <code>--dry-run</code> only shows the diff." },
    block(a, { text: true }),
    block(b, { json: ["changes", "written", "errors_before", "errors_after"] }),
    block(c, { text: true, note: "Error expected: 5:30 holds `t`, a reference to a parameter, not a literal (exit 2)." }),
    block(d, { text: true, note: "Rejected: the change produces errors (exit 1), the file stays unchanged." }),
  ]);
}

// UI placeholder and help
{
  const a = run([G]);
  const b = run([]);
  sec("8 Reserved UI entry point", [
    { kind: "p", html: "<code>cad file.scad</code> (a positional argument that is not a command) will later start the desktop UI. Today: message and exit 2. <code>cad</code> without arguments shows the help (exit 0). A typo in the command gives \"unknown command\"." },
    block(a, { text: true }),
    block(b, { text: true, maxLines: 22 }),
  ]);
}

// ---------------------------------------------------------------- watch (async)

/** Starts `cad watch`, applies `edits` one after another, returns the JSON lines. */
function watch(file, edits, extra = []) {
  return new Promise((resolve) => {
    const p = spawn(cad, ["watch", rel(file), "--json", "--timeout", "30", ...extra], { cwd: root });
    const got = [];
    let buf = "";
    let k = 0;
    const next = () => {
      if (k >= edits.length) {
        p.kill("SIGINT");
        return;
      }
      const e = edits[k++];
      setTimeout(() => e(), 250);
    };
    p.stdout.on("data", (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const l = buf.slice(0, i);
        buf = buf.slice(i + 1);
        try {
          got.push(JSON.parse(l));
        } catch {}
        next();
      }
    });
    p.on("exit", () => resolve(got));
  });
}

const watchRuns = [];
{
  const vr = join(work, "variable_radius.mcad");
  copyFileSync(join(root, "tests/models/spec/variable_radius.mcad"), vr);
  const src = readFileSync(vr, "utf8");
  const res = await watch(vr, [
    () => writeFileSync(vr, src + "// comment\n"),
    () => writeFileSync(vr, src.replace("lid = 20", "lid = 22")),
    () => writeFileSync(vr, src.replace("1 + p.x / 40", "1 + p.x / 50")),
  ], ["--render", rel(im("watch")), "--views", "iso,top", "--size", "800x400"]);
  const what = ["start (everything new)", "comment appended (everything from the cache)", "lid = 22 (two fillets rebuilt)", "radius function of the pocket changed"];
  res.forEach((r, i) => watchRuns.push({ file: "variable_radius.mcad", what: what[i] ?? "", r }));
  const dir = join(work, "projekt");
  mkdirSync(join(dir, "lib"), { recursive: true });
  for (const f of ["housing.mcad", "constants.mcad", "lib/parts.mcad"]) copyFileSync(join(root, "tests/models/project-housing", f), join(dir, f));
  const konst = readFileSync(join(dir, "constants.mcad"), "utf8");
  const teile = readFileSync(join(dir, "lib/parts.mcad"), "utf8");
  const res2 = await watch(join(dir, "housing.mcad"), [
    () => writeFileSync(join(dir, "constants.mcad"), konst.replace("spacing = 10", "spacing = 15")),
    () => writeFileSync(join(dir, "lib/parts.mcad"), teile.replace("boss_r = 3;", "boss_r = 3.5;")),
  ]);
  const what2 = ["start (everything new)", "constants.mcad: spacing = 15 (only the position of the lid)", "lib/parts.mcad: boss_r = 3.5 (screw bosses rebuilt)"];
  res2.forEach((r, i) => watchRuns.push({ file: "project-housing", what: what2[i] ?? "", r }));
}
sec("9 cad watch", [
  { kind: "p", html: "<code>cad watch FILE [--json] [--render image.png …]</code> watches the file and all included files (crate <code>notify</code>, inotify on Linux; the directories are watched so that atomic saves by editors are detected too), re-evaluates in the same session on every change and writes one JSON line per run (diagnostics, timings, volume, parts, image). The engine cache is kept: unchanged nodes come from the cache. Stop with Ctrl+C, <code>--once-after-change</code> or <code>--timeout S</code>." },
  { kind: "table", head: ["File", "Run", "Trigger", "Total ms", "Kernel ms", "Image ms", "from cache / nodes", "Kernel calls", "Volume mm³"], rows: watchRuns.map(({ file, what, r }) => [file, `${r.run}: ${what}`, (r.changed ?? []).join(", ") || r.trigger, num(r.timings?.total_ms ?? 0, 1), num(r.timings?.kernel_ms ?? 0, 1), r.timings?.render_ms != null ? num(r.timings.render_ms, 1) : "–", `${r.stats?.cache_hits ?? "–"} / ${r.stats?.nodes ?? "–"}`, r.stats?.kernel_calls ?? "–", r.volume != null ? num(r.volume, 1) : "–"]) },
  { kind: "pre", label: "JSON line (run 3, shortened)", text: watchRuns[2] ? excerpt(watchRuns[2].r, ["command", "run", "trigger", "changed", "ok", "errors", "stats", "volume", "timings", "render"]) : "–" },
  { kind: "figure", image: "watch", caption: "Image after the last run of watch --render … --views iso,top" },
]);

// ---------------------------------------------------------------- timings: repeated calls

const timing = [];
for (const f of ["tests/models/spec/variable_radius.mcad", "tests/models/project-housing/housing.mcad", "tests/models/spec/housing.mcad"]) {
  for (let i = 1; i <= 2; i++) {
    const r = runJson(["render", f, "-o", join(work, "t.png")]);
    timing.push({ f, i, wall: r.ms, t: r.j?.timings ?? {} });
  }
}
const baseline = run(["check", "--no-kernel", "tests/models/spec/housing.mcad"]);
const cmdTimes = [
  ["outline", ["outline", G]],
  ["check", ["check", G]],
  ["render (iso)", ["render", G, "-o", join(work, "a.png")]],
  ["render --views all --show grids,anchors", ["render", G, "-o", join(work, "b.png"), "--module", "housing", "--views", "all", "--show", "grids,anchors"]],
  ["render --section z=10", ["render", G, "-o", join(work, "c.png"), "--section", "z=10"]],
  ["measure --between --volume", ["measure", G, "--between", "k.corner", "k.top", "--volume", "part:k"]],
  ["measure --interference (collision)", ["measure", K, "--interference"]],
  ["measure --clearance b c (collision)", ["measure", K, "--clearance", "b", "c"]],
  ["set --dry-run", ["set", G, "--param", "housing.w=50", "--dry-run"]],
].map(([n, a]) => {
  const r = run(a);
  return [n, num(r.ms, 0), r.code];
});

// ---------------------------------------------------------------- case study

const steps = [];
{
  const f = join(work, "wandhalter.scad");
  const fr = rel(f);
  const step = (title, why, cmds, extra = {}) => steps.push({ title, why, cmds, ...extra });
  copyFileSync(join(fallstudie, "v1.scad"), f);
  step("write the draft", "First version with grid g (planes for plate, arm, bore, screw holes), two anchors and the parts halter and stange.", [], { source: readFileSync(f, "utf8") });
  step("check and understand", "check reports no errors; outline shows grid, anchors and parts with bounding box and volume.", [run(["check", fr]), run(["outline", fr])]);
  step("look", "Contact sheet with anchors and labels: plate, arm, bore and rod are positioned as planned.", [run(["render", fr, "-o", rel(im("fs-1")), "--views", "all", "--show", "anchors,labels", "--size", "1400x1000"])], { image: "fs-1" });
  step("measure", "Collisions and clearance rod ↔ bore: distance 0 – the rod sits in the bore without clearance (Ø 20 in Ø 20). Design error found.", [run(["measure", fr, "--interference", "--clearance", "halter", "stange"]), run(["measure", fr, "--between", "halter.schraube", "halter.stange"])]);
  step("fix with cad set", "Set a clearance of 0.5 mm as default value (diff first, then write); afterwards the distance is 0.25 mm (radial) ± 0.02 mm.", [run(["set", fr, "--param", "halter.spiel=0.5", "--dry-run"]), run(["set", fr, "--param", "halter.spiel=0.5"]), run(["measure", fr, "--clearance", "halter", "stange"])]);
  const before = readFileSync(f, "utf8");
  copyFileSync(join(fallstudie, "v2.scad"), f);
  const diff = spawnSync("diff", ["-u", "--label", "before", "--label", "after", "-", f], { input: before, encoding: "utf8" }).stdout;
  step("fillet the inner edge", "Zone block fillet(4) around the inner edge plate/arm (zone(g[*, platte, arm], d = 2)), inserted with the Edit tool. inspect of the fillet line: exactly 1 edge selected.", [run(["check", fr]), run(["inspect", fr, "--line", "10"])], { source: diff, sourceLabel: "Change (diff)" });
  step("view grid and zone", "Line 10 highlighted in module context, grid with plane names (100 grid points: only planes labelled).", [run(["render", fr, "-o", rel(im("fs-2")), "--line", "10", "--views", "iso,right", "--show", "grids,labels", "--size", "1400x620"])], { image: "fs-2" });
  step("section through the bore", "Section x = 20 shows rod and bore with gap; dimensions of the whole.", [run(["render", fr, "-o", rel(im("fs-3")), "--section", "x=20", "--show", "dims", "--size", "900x650"])], { image: "fs-3" });
  step("export", "Each part as a STEP file.", [run(["export", fr, "--all-parts", "-o", rel(join(work, "teile")), "--format", "step"])]);
}

// ---------------------------------------------------------------- tests

const cargo = readText(join(input, "cargo.txt"));
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const cargoSum = (() => {
  if (!cargo) return null;
  let ok = 0;
  let failed = 0;
  let ignored = 0;
  for (const m of cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)) {
    ok += +m[1];
    failed += +m[2];
    ignored += +m[3];
  }
  return { ok, failed, ignored };
})();
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })) : [];
const agentTests = (() => {
  if (!cargo) return [];
  const sec = cargo.split(/Running /).filter((s) => /tests\/agent\.rs|tests\/features\.rs|src\/main\.rs \(target.*cad-/.test(s.split("\n")[0]));
  return sec.flatMap((s) => [...s.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ file: s.split("\n")[0].trim(), name: m[1], res: m[2] })));
})();
void newTests;

// ---------------------------------------------------------------- HTML

const date = new Date().toISOString().slice(0, 10);
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

const cmdHtml = (b) => {
  const r = b.r;
  let h = `<div class="cmd"><pre class="call">$ ${esc(r.cmd)}<span class="t">  ${num(r.ms, 0)} ms, Exit ${r.code}</span></pre>`;
  if (b.note) h += `<div class="small">${esc(b.note)}</div>`;
  if (b.text) h += `<pre>${esc(lines((r.out + r.err).replaceAll(root + "/", ""), b.maxLines ?? 40))}</pre>`;
  if (b.json) h += `<div class="lbl">JSON (excerpt: ${esc(b.json.join(", "))})</div><pre>${esc(lines(excerpt(r.j, b.json), 60))}</pre>`;
  h += "</div>";
  if (b.image) h += fig(b.image, b.caption);
  return h;
};
const fig = (name, caption) =>
  existsSync(im(name)) ? `<figure><img src="${imRel(name)}"><figcaption>${esc(caption ?? "")}</figcaption></figure>` : `<figure class="missing">${esc(name)} missing</figure>`;
const itemHtml = (it) => {
  switch (it.kind) {
    case "p":
      return `<p>${it.html}</p>`;
    case "cmd":
      return cmdHtml(it);
    case "pair":
      return `<div class="pair">${it.items.map((x) => `<div>${itemHtml(x)}</div>`).join("")}</div>`;
    case "grid":
      return `<div class="tiles">${it.items.map((x) => fig(x.image, x.caption)).join("")}</div>`;
    case "figure":
      return fig(it.image, it.caption);
    case "pre":
      return `<div class="lbl">${esc(it.label)}</div><pre>${esc(it.text)}</pre>`;
    case "table":
      return `<table><tr>${it.head.map((h) => `<th>${esc(h)}</th>`).join("")}</tr>${it.rows.map((r) => `<tr>${r.map((c) => `<td>${esc(c ?? "–")}</td>`).join("")}</tr>`).join("")}</table>`;
    default:
      return "";
  }
};

const timingRows = timing.map(({ f, i, wall, t }) => [f.replace("tests/models/", ""), i, num(wall, 0), num(t.evaluate_ms ?? 0, 0), num(t.kernel_ms ?? 0, 0), num(t.render_ms ?? 0, 0)]);
const maxWall = Math.max(...timing.map((t) => t.wall));
const vrWatch = watchRuns.filter((w) => w.file === "variable_radius.mcad");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 8 – CLI for agents</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 9.6pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 184mm; margin: 0 auto; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 28mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.4pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.2pt; }
  th, td { border: 1px solid var(--border); padding: 1.1mm 1.8mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  pre.call { background: #eef3fb; font-weight: 600; }
  pre.call .t { font-weight: 400; color: var(--muted); float: right; }
  .cmd { margin: 1.5mm 0; }

  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 2mm; margin: 2mm 0; align-items: start; }
  .pair > div { min-width: 0; break-inside: avoid; }
  .tiles { display: grid; grid-template-columns: 1fr 1fr 1fr 1fr; gap: 1.5mm; }
  figure { margin: 0 0 2mm; break-inside: avoid; }
  figure img { max-width: 100%; max-height: 100mm; border: 1px solid var(--border); display: block; }
  figcaption { font-size: 7.6pt; color: var(--muted); margin-top: 0.6mm; }
  figure.missing { border: 1px dashed var(--border); padding: 4mm; }
  .lbl { font-size: 7.4pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1.5mm 0 0.5mm; }
  .small { font-size: 8.4pt; color: var(--muted); }
  .step { border: 1px solid var(--border); border-radius: 2mm; padding: 2.5mm 3mm; margin: 3mm 0; }
  .step h3 { margin-top: 0; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, td { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
</style>
</head>
<body>
<main>
<h1>Test report 8 – CLI for agents</h1>
<div class="meta">cad-cli, cad-render, cad-session · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores (${esc(os.cpus()[0]?.model ?? "")})</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${cargoSum ? `${cargoSum.ignored} ignored` : ""}</div>
  <div>new tests<b>${agentTests.length}</b>agent.rs, features.rs, geom</div>
  <div>cargo clippy -D warnings<b class="${clippy != null && !/^(error|warning)/m.test(clippy) ? "ok" : "bad"}">${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "warnings" : "clean"}</b></div>
  <div>cargo fmt --check<b class="${fmt != null && fmt.trim() === "" ? "ok" : "bad"}">${fmt == null ? "–" : fmt.trim() === "" ? "clean" : "deviations"}</b></div>
  <div>render per call<b>${num(maxWall / 1000, 2)} s</b>max. (2 runs × 3 files)</div>
  <div>watch run<b>${vrWatch[1] ? num(vrWatch[1].r.timings.total_ms, 0) + " ms" : "–"}</b>comment, everything from the cache</div>
</div>

<h2>Scope</h2>
<p>The CLI <code>cad</code> is extended so that an agent (e.g. Claude Code) can develop and check models entirely without the UI – alone or while a human edits the same files in the UI. New:</p>
<table>
<tr><th>Command/option</th><th>Purpose</th></tr>
<tr><td><code>cad outline</code></td><td>Overview of a project in one call</td></tr>
<tr><td><code>render --views</code></td><td>Contact sheet with labelled tiles at the same scale</td></tr>
<tr><td><code>render --azimuth/--elevation/--zoom/--target/--fit/--perspective</code></td><td>Free camera</td></tr>
<tr><td><code>render --part/--line (repeatable)/--node/--only</code></td><td>Selection, combinable with <code>--module</code>, <code>-D</code></td></tr>
<tr><td><code>render --show grids,anchors,handles,edges,labels,bbox,dims</code></td><td>Overlays</td></tr>
<tr><td><code>render --section/--section-plane/--section-flip</code>, <code>--xray</code></td><td>Section with filled cap, X-ray</td></tr>
<tr><td><code>cad measure</code></td><td>Distances (anchors, grid points, coordinates), bounding box, volume, area, collisions, part clearance</td></tr>
<tr><td><code>cad watch</code></td><td>Re-evaluate on file change, one JSON line per run, optional image</td></tr>
<tr><td><code>cad set</code></td><td>Change a default value or literal through the engine, checked, atomic, <code>--dry-run</code></td></tr>
<tr><td><code>cad FILE.scad</code>, <code>cad</code></td><td>UI entry point reserved (exit 2 with message), help without arguments</td></tr>
</table>
<p>Every command has <code>--json</code> with a fixed, versioned schema (<code>schema_version</code> 1, complete in the header of <code>crates/cad-cli/src/main.rs</code>, short form in the skill <code>.claude/skills/cad-cli/SKILL.md</code>) and the existing exit codes (0 ok, 1 model error, 2 invocation, 3 kernel). Recommended workflow for agents: <b>outline → edit → check → render --views all --show grids,anchors → inspect/measure → export</b>. <code>cad-render</code> draws labels with an embedded bitmap font (DejaVu Sans Mono, generated by <code>tools/font/gen-font.py</code>), without a new runtime dependency.</p>

${sections.map((s) => `<h2>${esc(s.title)}</h2>\n${s.items.map(itemHtml).join("\n")}`).join("\n")}

<h2>10 Performance of repeated calls</h2>
<p>Every CLI call starts a new process with an empty cache. Measured: <code>cad render</code> twice in a row (default view 1024×768), wall time of the process and the timings from the JSON (<code>timings</code>: evaluation incl. kernel, of which kernel, image incl. tessellation). Process start with parser, without kernel (<code>check --no-kernel</code>): ${num(baseline.ms, 0)} ms.</p>
<table><tr><th>File</th><th>Run</th><th>Wall time ms</th><th>Evaluation ms</th><th>of which kernel ms</th><th>Image ms</th></tr>
${timingRows.map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join("")}</tr>`).join("")}</table>
<p><b>Decision: no persistent cache between calls.</b> The second call is as fast as the first (no cache); a call takes up to ${num(maxWall / 1000, 2)} s – for <i>variabler_radius</i> at the 1 s limit, almost all of it kernel time (fillets) and tessellation. A file cache of the kernel results would be costly: the results are handles to SMLib objects in the memory of the native library; a cache would have to serialize and re-read the B-rep (together with the provenance of faces and edges), which itself costs kernel time. Instead <code>cad watch</code>: the same session persists, the engine cache holds all node results; a run after a change costs only the changed nodes (table in section 9: ${vrWatch[1] ? num(vrWatch[1].r.timings.total_ms, 0) : "–"} ms for a change without effect on geometry, ${vrWatch[2] ? num(vrWatch[2].r.timings.total_ms, 0) : "–"} ms when two filleted bodies are rebuilt). The skill recommends <code>cad watch</code> for loops with many small steps.</p>
<h3>Run time per command (spec example housing, wall time)</h3>
<table><tr><th>Command</th><th>ms</th><th>Exit</th></tr>${cmdTimes.map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join("")}</tr>`).join("")}</table>

<h2>11 Case study: an agent develops a part using only the CLI</h2>
<p>The agent (this report was produced by Claude) designed a wall bracket for a Ø 20 mm rod without opening the UI: source code with the Edit tool, everything else with <code>cad</code>. The steps below are the ones actually executed, replayed by the generator (versions in <code>tools/report/report8-fallstudie/</code>). The agent viewed the images with the Read tool; a real design error was found (no clearance between rod and bore), visible only through <code>measure --clearance</code>, not in the image.</p>
${steps.map((s, i) => `<div class="step"><h3>Step ${i + 1}: ${esc(s.title)}</h3><p>${esc(s.why)}</p>${s.source ? `<div class="lbl">${esc(s.sourceLabel ?? "Source code")}</div><pre>${esc(s.source)}</pre>` : ""}${s.cmds.map((r) => cmdHtml({ r, text: true, maxLines: 24 })).join("")}${s.image ? fig(s.image, "") : ""}</div>`).join("\n")}
<p>Result: ${steps.reduce((a, s) => a + s.cmds.length, 0)} CLI calls, ${num(steps.reduce((a, s) => a + s.cmds.reduce((b, r) => b + r.ms, 0), 0) / 1000, 1)} s in total. What would have been missing without the new features: the clearance measurement, the grid names in the image (which plane is <code>arm</code>?) and the section through the bore.</p>

<h2>12 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed, ${cargoSum.ignored} ignored` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "warnings" : "no findings"}; <code>cargo fmt --check</code>: ${fmt == null ? "–" : fmt.trim() === "" ? "no findings" : "deviations"}.</li>
<li>New in <code>crates/cad-cli/tests/agent.rs</code> (assert_cmd, per command/option with JSON fields): outline (spec example, functions/variables, project), render (contact sheet: image size and equal scale; free camera; selection; overlays change pixels; section reduces the visible area and produces cap pixels; X-ray), measure (anchors, grid points, coordinates against expected values of the spec example; bounding box, volume; collision on <code>tests/data/collision.mcad</code>; clearance), watch (change file → second JSON line, cache hits increase; text and timeout), set (parameter, literal, diff, rejection, project files), UI entry point.</li>
<li>New in <code>crates/cad-render/tests/features.rs</code>: camera angles = standard views, section with cap on a closed cube (all cap from above, none from below), perspective and fixed scale, markers/labels/title/contact sheet; in <code>geom.rs</code>: triangle–triangle distance on cubes (face, edge, vertex, overlap).</li>
</ul>
<table><tr><th>Test file</th><th>Test</th><th>Result</th></tr>${agentTests.map((t) => `<tr><td>${esc(t.file.replace(/\(.*$/, "").trim())}</td><td>${esc(t.name)}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("")}</table>

<h2>13 Limitations</h2>
<ul>
<li>Section caps require closed meshes; open surfaces (<code>surface</code> without <code>thicken</code>) and transparent bodies get no cap. The cap is flat and single-colored (hatching alternates per body), no distinction by material.</li>
<li><code>--clearance</code> measures on the tessellation (accuracy ± 2·<code>--mesh-tol</code>), <code>--interference</code> exactly via the kernel; parts whose bounding boxes do not overlap are counted as collision-free without the kernel.</li>
<li>Grid points are labelled up to 48 points per grid, above that only the planes (note in <code>notes</code>); overlapped labels are dropped.</li>
<li><code>outline</code> evaluates each module with its default values; modules with required parameters without a default may then show no grids (<code>errors_with_defaults</code>).</li>
<li><code>watch</code> re-reads all project files on every change; changes within the debounce time (<code>--debounce</code>, 60 ms) are merged into one run.</li>
<li>No persistent cache between calls (section 10).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${rel(join(out, "report.html"))}: ${sections.length} sections, ${steps.length} case study steps, ${watchRuns.length} watch runs`);
console.log(`PDF: node tools/report/print-pdf.mjs ${rel(join(out, "report.html"))} ${rel(join(out, "report.pdf"))} "Test report 8: CLI for agents"`);
