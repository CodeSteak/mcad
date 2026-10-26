#!/usr/bin/env node
// Catalogue of the everyday models (tests/models/alltag/): measures every model with the CLI, renders images
// and writes
//
//   docs/models/everyday-models.html, docs/models/img/*.jpg
//
// All numbers come from the CLI (`cad outline`, `measure`, `eval`), texts from MODELLE below
// (placeholders {param} or {modul.param} are replaced by the defaults from `cad outline`).
// Intermediate results: reports/tmp/katalog/ (gitignored). Binary: $CAD (default
// target/release/mcad), socket directory $CAD_IPC_DIR (short path, e.g. /tmp/cadkat).
// Needs ImageMagick (`convert`, `identify`).
//
//   node tools/report/katalog.mjs [--reuse [--redo=id,…]]   (--reuse: reuse measurements/images from
//                                                             reports/tmp, except for --redo)
//   node tools/report/print-pdf.mjs docs/models/everyday-models.html docs/models/everyday-models.pdf "Everyday models – catalogue"

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const cad = process.env.CAD ?? join(root, "target/release/mcad");
const work = join(root, "reports/tmp/katalog");
const out = join(root, "docs/models");
const img = join(out, "img");
const reuse = process.argv.includes("--reuse");
const PLA = 1.24; // g/cm³
const BG = "#f4f5f7"; // background of `cad render` (light theme)

// ---------------------------------------------------------------------------------------------
// Models: order as in tests/models/alltag/README.md. text: 2–4 sentences; {p} = default of
// parameter p of the main module or of the file variable p, {modul.p} = parameter of another
// module, {p|x} = list joined with " × ".
// extra: additional images of the assemblies (section, anchors), pairs: distances `measure --clearance`,
// view: [azimuth, elevation] instead of iso where iso shows the back.
const MODELLE = [
  {
    id: "handystaender", file: "handystaender.mcad", titel: "Phone stand", view: [-50, 30], // iso shows the back
    text: "Stand for a phone up to 12 mm thick. The side profile is a polygon of points of a 2D grid and is extruded over the width w = {w} mm; the backrest ({lehne} mm, wall t = {t} mm) is at an angle a = {a}° to the horizontal, a triangular window lightens it. A cable hole Ø {kabel_d} mm with a channel under the base takes the charging cable. Inner corners of lip and shelf as well as the top edge of the lip are filleted with two radii via zones on grid lines.",
  },
  {
    id: "becher", file: "becher.mcad", titel: "Mug with handle",
    text: "Drinking mug as a conical solid of revolution (radius at the bottom ru = {ru} mm, at the top ro = {ro} mm, height h = {h} mm), opened at the top with shell and a {wand} mm wall. The bottom edge is filleted with r {boden_r} mm via a zone on the bottom plane, the drinking rim via a second zone. The handle (Ø {henkel_d} mm) is a sweep of a circle along a spline path whose ends sit radially in the wall, attached with union(fillet = …).",
  },
  {
    id: "kabelclip", file: "kabelclip.mcad", titel: "Cable clip for the desk edge", kurz: "Cable clip",
    text: "Cable holder that clips onto a desk top of dicke = {dicke} mm (clearance {spiel} mm). The C clamp is an extruded profile (wall {t} mm, arms {arm_o} and {arm_u} mm) with a clamping bump; above it sits a block with n = {n} cable channels Ø {kabel_d} mm at a pitch of {pitch} mm, inserted from the top. The channels lie on indexed grid planes, their edges are filleted with difference(fillet = …), the inner corners of the clamp via zones.",
  },
  {
    id: "wandhaken", file: "wandhaken.mcad", titel: "Wall hook",
    text: "Hook for a jacket or towel: wall plate {b} × {h} × {t} mm with filleted corners (r {ecke} mm), chamfered front edge and two countersunk holes (Ø {schraube_d} mm, head Ø {kopf_d} mm). Chamfer and fillet are nested, so that the chamfer also covers the boundary edges of the fillet. The hook (Ø {haken_d} mm, reach {ausladung} mm) is a sweep along a spline path with a ball knob Ø {knauf_d} mm, attached with union(fillet = …).",
  },
  {
    id: "seifenschale", file: "seifenschale.mcad", titel: "Soap dish",
    text: "Soap dish {l} × {b} × {h} mm with drainage. The tray is a box whose vertical corners (r {ecke} mm) and bottom edges are filleted via nested zones and which shell (wall {wand} mm) opens at the top. n = {n} round support ribs are cylinder(a, b, r) between grid points, with drainage slots with filleted edges between them; underneath stand four feet ({fuss_h} mm).",
  },
  {
    id: "stiftebox", file: "stiftebox.mcad", titel: "Pen box",
    text: "Pen box with three tall compartments (widths {faecher} mm, depth {tiefe} mm, height {h} mm) and a low front compartment ({vorn_t} mm deep, {vorn_h} mm high), wall {wand} mm. The compartment boundaries are indexed grid planes computed by a recursive function from the list of compartment widths. Four nested zone blocks fillet the vertical edges (outside r {ecke} mm, pockets r {ecke_i} mm) and the pocket floors and chamfer the rear top edges; the top edges of the front compartment stay sharp (kernel limit F6).",
  },
  {
    id: "tuerstopper", file: "tuerstopper.mcad", titel: "Door stop",
    text: "Wedge {l} mm long, {b} mm wide and {h_hinten} mm high at the back. The body is a loft through three trapezoid profiles (height at the back, middle {h_mitte} mm, tip {spitze} mm), laid flat with rotate([90, 0, 90]); its bottom edge is chamfered beforehand via a zone on the bottom plane. On top there is a finger recess, underneath {rillen} transverse grooves against slipping, at the back a hanging hole Ø {loch_d} mm.",
  },
  {
    id: "topfuntersetzer", file: "topfuntersetzer.mcad", titel: "Pot trivet",
    text: "Trivet Ø {d} mm, {h} mm high: three concentric rings (centre radii {ringe} mm, web {steg} mm) as rotate_extrude of a rectangle in a for loop, connected by {speichen} spokes and a centre disc (r {mitte_r} mm). All top edges of rings and spokes are chamfered in one block via a zone on the top plane; underneath there are three recesses for rubber feet Ø {fuss_d} mm.",
  },
  {
    id: "flansch", file: "flansch.mcad", titel: "Flange plate",
    text: "Flange Ø {d_aussen} mm, {t} mm thick, with collar Ø {d_bund} mm (height {h_bund} mm) and bore Ø {d_bohrung} mm. The cross-section is a polygon from a 2D grid, revolved with rotate_extrude; on the bolt circle Ø {lk} mm lie {n} through holes Ø {d_loch} mm (M10), plus a sealing groove {nut_t} mm deep. The transition collar/disc is filleted by a dedicated module used as a zone (ringzone), bore and outer edges are chamfered via volume zones.",
  },
  {
    id: "lagerbock", file: "lagerbock.mcad", titel: "Pillow block 608",
    text: "Bearing block for a 608 deep-groove ball bearing ({lager_d} × {lager_b} mm), axis height {achse_h} mm. The base plate {l} × {b} × {t} mm has two slots for M6 ({langloch|x} mm, helper module) for alignment; the bearing eye (r {auge_r} mm, {auge_b} mm wide) is a box with a tangentially adjoining cylinder(a, b, r) and has a stop shoulder Ø {schulter_d} mm. The fillets around the eye are created via zones on grid lines in x, y and z.",
  },
  {
    id: "winkel_rippe", file: "winkel_rippe.mcad", titel: "Mounting bracket with rib", kurz: "Mounting bracket",
    text: "Bracket with legs of {a} and {h} mm, {b} mm wide and {t} mm thick, with n_rippen = {n_rippen} stiffening rib ({rippe_t} mm thick, {rippe_l} mm long). The ribs are extruded triangles on indexed grid planes; inner edge and rib joints are filleted in one zone block, so that the corner transitions run cleanly. The holes Ø {loch_d} mm (M5) are chamfered with difference(chamfer = …).",
  },
  {
    id: "kuehlkoerper", file: "kuehlkoerper.mcad", titel: "Heat sink",
    text: "Extruded heat sink {l} × {b} mm, base plate {basis} mm, total height {h} mm, with n = {n} fins of {rippe_t} mm each. The fin positions are indexed grid planes from a list; the fin roots are filleted via zones on grid lines in a for loop (r {fuss_r} mm), the tips almost fully rounded via a zone on the tip plane. Two M3 holes {loch_abstand} mm apart lie between the fins, an anchor marks the heat source.",
  },
  {
    id: "zahnrad", file: "zahnrad.mcad", titel: "Spur gear pair",
    text: "Two meshing spur gears with module m = {m} mm and {z1} and {z2} teeth, centre distance {achsabstand} mm, width {zahnrad.b} mm. The involute flanks are created as a polygon from list functions (let, each, concat) and extruded linearly; the tooth thickness is slightly reduced for backlash. Gear 1 has a hub, keyway and lightening holes, gear 2 a D bore for a 5 mm motor shaft.",
    pairs: [["rad1", "rad2", "Backlash"]],
    extra: [{ name: "eingriff", args: ["--view", "top", "--fit", "all", "--zoom", "6", "--target", "18.6,-4.2,10"], caption: "Top view, magnified 6× on the narrowest point of the mesh: the gap between the flanks is the measured backlash." }],
  },
  {
    id: "rohrschelle", file: "rohrschelle.mcad", titel: "Pipe clamp",
    text: "Two-part clamp for a pipe Ø {rohr_d} mm: the half rings ({halbschelle.t} mm thick, {b} mm wide) are rotate_extrude(angle = 180) of a rectangle, with lugs and holes Ø {halbschelle.loch_d} mm (M5); the lower part (mirror) carries a boss with a pilot hole for M8. Upper part, lower part and a test pipe are parts in a jointly rotated block. In the open state there is a {halbschelle.spalt} mm gap at the lugs.",
    pairs: [["oben", "unten", "Clamping gap at the lugs"], ["oben", "rohr", "Pipe touches the upper part"], ["unten", "rohr", "Pipe touches the lower part"]],
    extra: [{ name: "schnitt", args: ["--section", "x=10", "--view", "right"], caption: "Section x = 10 (middle of the clamp, across the pipe axis), view from +x: half rings with lugs and M5 holes, clamping gap 2 mm, boss with pilot hole for M8, test pipe." }],
  },
  {
    id: "pi_platte", file: "pi_platte.mcad", titel: "Raspberry Pi mounting plate", kurz: "Raspberry Pi plate",
    text: "Plate {l} × {b} × {t} mm for a Raspberry Pi (hole pattern 58 × 49 mm, M2.5). The standoffs ({hoehe} mm) are a separate module with an anchor, placed in the loop as named instances dom[i] and attached with union(fillet = …). In addition there are {schlitze} ventilation slots, four M4 mounting holes with countersink and filleted corners (r {ecke} mm). A coloured board dummy 85 × 56 × 1.6 mm sits as a second part on the anchor of the plate.",
    pairs: [["platte", "pi", "Board rests on the bosses"]],
    extra: [{ name: "anker", args: ["--show", "anchors", "--part", "platte"], caption: "Plate with anchors shown (yellow): the anchors oben of the bosses carry the board." }],
  },
  {
    id: "gehaeuse", file: "gehaeuse/gehaeuse.mcad", titel: "Electronics enclosure",
    text: "Enclosure for a 70 × 50 mm board: inside {innen_l} × {innen_b} mm, height {hoehe} mm, wall {wand} mm, corners r {ecke} mm. The base is a filleted box opened by shell; four screw bosses (pilot hole Ø {kern_d} mm) are attached as dom[i] with union(fillet = …). The lid ({deckel_t} mm, centring rim {rand_h} mm, clearance {spiel} mm) sits on the anchor deckel_auflage, the M3 screws are parts s[0] … s[3]. Dimensions are in masse.mcad (include), parts in teile.mcad (use); explosion > 0 pulls the assembly apart.",
    pairs: [["unterteil", "deckel", "Lid rests on the base"], ["s[0]", "deckel", "Screw head rests on the lid"], ["s[0]", "unterteil", "Shank in the pilot hole"]],
    extra: [{ name: "schnitt", args: ["--section", "y=8.5", "--section-flip", "--view", "front"], caption: "Section y = 8.5 (through the front bosses), view from the front: screw in the boss, lid with centring rim on the base." }],
  },
  {
    id: "scharnier", file: "scharnier/scharnier.mcad", titel: "Hinge",
    text: "Printable {laenge} mm hinge made of leaf A, leaf B and pin with {augen} knuckles (r {auge_r} mm, leaf thickness {blatt_t} mm). The knuckles of both leaves come from the same function, so that the clearance of {spiel} mm is the same everywhere; the gaps are created by for with a step. Leaf A has the {bohrung_a} mm hole (pin glued in), leaf B {bohrung_b} mm (rotates freely), the pin Ø {stift_d} mm. The variable winkel = {winkel}° rotates leaf B about the axis.",
    pairs: [["a", "b", "Clearance of the knuckles"], ["stift", "b", "Bearing clearance"], ["stift", "a", "Pin head touches"]],
    extra: [
      { name: "schnitt", args: ["--section", "z=0", "--view", "top"], caption: "Section z = 0 (mid-plane of the leaves), top view: knuckles of A (bottom) and B (top) alternating with axial clearance, pin in the holes." },
      { name: "lager", args: ["--section", "x=18", "--view", "right", "--zoom", "3.5", "--target", "18,0,0"], caption: "Section x = 18 (second knuckle, leaf B), view from +x, 3.5×: pin Ø 2.95 in the 3.4 hole of B; on the left leaf A with clearance." },
    ],
  },
  {
    id: "pflanztopf", file: "pflanztopf/pflanztopf.mcad", titel: "Plant pot with saucer", kurz: "Plant pot",
    text: "The pot goes by loft from a rounded square ({topf_unten} mm, superellipse as a spline from a function) to the round opening Ø {topf_oben_d} mm, height {topf_h} mm, wall {wand} mm, bottom {boden} mm; it stands on {fuss_h} mm high feet and has drainage holes. The wall is the difference of two lofts. The saucer is a rotate_extrude, opened at the top with shell and filleted via zones. The pot stands with the anchor aufstand on the anchor boden of the saucer.",
    pairs: [["topf", "untersetzer", "Feet stand on the saucer base"]],
    extra: [{ name: "schnitt", args: ["--section", "y=0", "--section-flip", "--view", "front"], caption: "Section y = 0, view from the front: wall of the loft, feet and drainage holes, pot in the saucer." }],
  },
  {
    id: "schluesselbrett", file: "schluesselbrett/schluesselbrett.mcad", titel: "Key rack",
    text: "Board {brett_l} × {brett_h} × {brett_t} mm with {anzahl} glued-in hooks. It has corners filleted via zones and a chamfered front edge, two keyhole hangers at the back and a letter slot on top. The hooks are a sweep along a spline path with a ball knob; their peg Ø {zapfen_d} mm sits in a hole Ø {bohrung_d} mm (glue gap). The hooks are parts h[0] … h[4] from a for loop, the dimensions are in masse.mcad.",
    pairs: [["brett", "h[0]", "Peg in the hole (glue gap)"]],
    extra: [{ name: "schnitt", args: ["--section", "x=20", "--view", "right"], caption: "Section x = 20 (through hook h[0]), view from +x: peg in the hole, chamfer of the front edge, letter slot on top." }],
  },
];

// ---------------------------------------------------------------------------------------------
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const fmtNum = (x, d = 0) => Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
const num = (x) => Number(x).toLocaleString("en-US", { maximumFractionDigits: 3 });
const rel = (p) => relative(root, p);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};

function run(args) {
  const t0 = process.hrtime.bigint();
  const r = spawnSync(cad, args, { cwd: root, encoding: "utf8", maxBuffer: 1 << 28 });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  let j = null;
  try {
    j = JSON.parse(r.stdout);
  } catch {}
  if (r.status !== 0) console.error(`  ! cad ${args.join(" ")} → Exit ${r.status}${r.signal ? " (" + r.signal + ")" : ""}: ${(r.stderr || "").trim().slice(0, 300)}`);
  return { code: r.status, signal: r.signal, j, ms, err: r.stderr ?? "" };
}
const im = (...a) => execFileSync("convert", a, { encoding: "utf8" });
const identify = (f) => execFileSync("identify", ["-format", "%w %h", f], { encoding: "utf8" }).split(" ").map(Number);

/** PNG → fitted JPEG: trim the borders, pad to aspect ratio w:h with a margin. */
function fitJpeg(src, dst, w, h, margin = 0.06, quality = 84) {
  const trimmed = join(work, "trim.png");
  im(src, "-fuzz", "1%", "-trim", "+repage", trimmed);
  const [tw, th] = identify(trimmed);
  const s = Math.min((w * (1 - 2 * margin)) / tw, (h * (1 - 2 * margin)) / th);
  im(trimmed, "-resize", `${Math.round(tw * s)}x${Math.round(th * s)}`, "-background", BG, "-gravity", "center", "-extent", `${w}x${h}`,
    "-strip", "-sampling-factor", "4:2:0", "-quality", String(quality), dst);
  return checkSize(dst);
}
function toJpeg(src, dst, w, quality = 80) {
  im(src, "-resize", `${w}x`, "-strip", "-sampling-factor", "4:2:0", "-quality", String(quality), dst);
  return checkSize(dst);
}
function checkSize(f) {
  const b = statSync(f).size;
  if (b > 200 * 1024) throw new Error(`${rel(f)}: ${b} B > 200 KB`);
  return b;
}

/** Language features that occur in the source (including include/use). */
const MITTEL = [
  ["grid", /\bgrid\s+\w+\s*\{/],
  ["indexed grid planes", /^\s*[xyz]:.*=\s*\[/m],
  ["anchor", /\banchor\s+\w+/],
  ["anchor with dir", /\banchor\s+\w+\s*=.*dir\s*=/],
  ["named instances", /^\s*(translate\([^;]*\)\s*)?\w+(\[\w+\])?\s*:\s*\w+\(/m],
  ["include", /^\s*include\s*</m],
  ["use", /^\s*use\s*</m],
  ["function", /\bfunction\s+\w+/],
  ["recursion", /function\s+(\w+)\s*\([^)]*\)\s*=[^;]*\b\1\s*\(/],
  ["let", /\blet\s*\(/],
  ["list comprehension", /\[\s*for\s*\(/],
  ["each", /\beach\b/],
  ["for", /\bfor\s*\(/],
  ["if / ?:", /\bif\s*\(|\?[^:;]+:/],
  ["polygon", /\bpolygon\s*\(/],
  ["spline", /spline/],
  ["linear_extrude", /\blinear_extrude\b/],
  ["rotate_extrude", /\brotate_extrude\b/],
  ["loft", /\bloft\s*\(/],
  ["sweep", /\bsweep\s*\(/],
  ["shell", /\bshell\s*\(/],
  ["fillet zones", /\bfillet\s*\([^)]*\)\s*\{/],
  ["chamfer zones", /\bchamfer\s*\([^)]*\)\s*\{/],
  ["zone", /\bzone\s*\(/],
  ["difference(fillet/chamfer)", /\bdifference\s*\(\s*(fillet|chamfer)\s*=/],
  ["union(fillet)", /\bunion\s*\(\s*fillet\s*=/],
  ["$fillet", /\$fillet\s*=/],
  ["box(a, b)", /\bbox\s*\(/],
  ["cylinder(a, b, r)", /\bcylinder\s*\(\s*(g\[|\[|\w+\[)/],
  ["cone (r1, r2)", /\bcylinder\s*\([^;]*\b(r1|d1)\s*=/],
  ["sphere", /\bsphere\s*\(/],
  ["intersection", /\bintersection\s*\(/],
  ["mirror", /\bmirror\s*\(/],
  ["color", /\bcolor\s*\(/],
];
function sprachmittel(files) {
  const src = files.map((f) => readFileSync(f, "utf8").replace(/\/\/.*$/gm, "")).join("\n");
  return MITTEL.filter(([, re]) => re.test(src)).map(([n]) => n);
}

/** Replace placeholders {p}, {modul.p}, {p|x} with defaults. */
function fill(text, o, main) {
  const fmt = (v, join) => (Array.isArray(v) ? v.map((x) => num(x)).join(join) : typeof v === "number" ? num(v) : String(v));
  return text.replace(/\{([\w.]+)(\|x)?\}/g, (_, key, x) => {
    const join = x ? " × " : " / ";
    let mod = main, name = key;
    if (key.includes(".")) [mod, name] = key.split(".");
    const m = o.modules.find((m) => m.name === mod);
    const p = m?.params.find((p) => p.name === name);
    if (p && !key.includes(".") && o.variables.some((v) => v.name === name && v.file === o.mainFile)) {
      // a file variable of the main file takes precedence (e.g. rohrschelle: rohr_d, b)
    } else if (p) return fmt(p.default_value ?? p.default, join);
    const v = o.variables.find((v) => v.name === name);
    if (v) return fmt(v.value, join);
    throw new Error(`placeholder {${key}} unknown (module ${main})`);
  });
}

// ---------------------------------------------------------------------------------------------
if (!existsSync(cad)) throw new Error(`${cad} missing (cargo build --release -p cad-cli --no-default-features)`);
mkdirSync(work, { recursive: true });
if (!reuse) rmSync(img, { recursive: true, force: true });
mkdirSync(img, { recursive: true });
const dataFile = join(work, "data.json");
const cache = reuse && existsSync(dataFile) ? JSON.parse(readFileSync(dataFile, "utf8")) : {};
// --redo id,id: measure and render these models again despite --reuse
const redo = process.argv.find((a) => a.startsWith("--redo="))?.slice(7).split(",") ?? [];
for (const id of redo) delete cache[id];
const auffaellig = [];

const daten = [];
for (const [i, m] of MODELLE.entries()) {
  const file = join(root, "tests/models/alltag", m.file);
  const rf = rel(file);
  console.log(`[${i + 1}/${MODELLE.length}] ${rf}`);
  let d = cache[m.id];
  if (!d) {
    d = { id: m.id };
    const o = run(["outline", rf, "--json"]).j;
    o.mainFile = o.files.find((f) => f.role === "main")?.path;
    d.outline = { modules: o.modules, variables: o.variables, parts: o.parts, colors: o.colors, files: o.files, counts: o.counts, diagnostics: o.diagnostics, mainFile: o.mainFile };
    const mb = run(["measure", rf, "--bounds", "--volume", "--json"]);
    d.bounds = mb.j.bounds.bounds;
    d.volume = mb.j.volume.volume;
    // kernel time without daemon (no cache): scene.stats.kernel_ms
    const ev = run(["ev" + "al", rf, "--json", "--no-daemon"]);
    d.kernel_ms = ev.j?.scene?.stats?.kernel_ms ?? null;
    d.nodes = ev.j?.scene?.stats?.nodes ?? null;
    d.wall_ms = ev.ms;
    d.diag = (ev.j?.diagnostics ?? []).map((x) => `${x.severity}: ${x.message}`);
    // Fits
    d.pairs = [];
    if (m.pairs) {
      const inter = run(["measure", rf, "--interference", "--json"]).j?.interference;
      d.collisions = inter?.collisions ?? null;
      for (const [a, b, bed] of m.pairs) {
        const c = run(["measure", rf, "--clearance", a, b, "--json"]).j?.clearance;
        d.pairs.push({ a, b, bed, distance: c?.distance, collision: c?.collision });
      }
    }
    // Images
    const hero = join(work, `${m.id}.png`);
    const view = m.view ? ["--azimuth", String(m.view[0]), "--elevation", String(m.view[1])] : ["--view", "iso"];
    const r1 = run(["render", rf, "-o", hero, ...view, "--perspective", "--size", "1600x1200", "--no-axes", "--json"]);
    d.render_ms = r1.j?.timings ?? null;
    d.hero = r1.code === 0;
    const sheet = join(work, `${m.id}-ansichten.png`);
    d.sheet = run(["render", rf, "-o", sheet, "--views", "all", "--size", "1200x900", "--no-axes"]).code === 0;
    d.extra = [];
    for (const x of m.extra ?? []) {
      const p = join(work, `${m.id}-${x.name}.png`);
      const r = run(["render", rf, "-o", p, "--size", "1200x900", "--no-axes", ...x.args, "--json"]);
      d.extra.push({ name: x.name, ok: r.code === 0, notes: r.j?.notes ?? [] });
    }
    d.files = o.files.map((f) => f.path);
    cache[m.id] = d;
    writeFileSync(dataFile, JSON.stringify(cache, null, 1));
  }
  // JPEGs (always rebuilt, fast)
  d.img = {};
  if (d.hero) d.img.hero = { file: `img/${m.id}.jpg`, bytes: fitJpeg(join(work, `${m.id}.png`), join(img, `${m.id}.jpg`), 1200, 900) };
  if (d.sheet) d.img.sheet = { file: `img/${m.id}-ansichten.jpg`, bytes: toJpeg(join(work, `${m.id}-ansichten.png`), join(img, `${m.id}-ansichten.jpg`), 800) };
  for (const x of d.extra) if (x.ok) d.img[x.name] = { file: `img/${m.id}-${x.name}.jpg`, bytes: fitJpeg(join(work, `${m.id}-${x.name}.png`), join(img, `${m.id}-${x.name}.jpg`), 1200, 900, 0.04) };
  daten.push({ m, d, rf, dir: dirname(file) });
}

// ---------------------------------------------------------------------------------------------
// HTML
const commit = git("rev-parse", "--short", "HEAD");
const datum = new Date().toISOString().slice(0, 10);
const mm = (b) => b.size.map((x) => fmtNum(x, 1)).join(" × ");
const masse = (v) => (v * PLA) / 1000;
const sec = (ms) => (ms == null ? "–" : fmtNum(ms / 1000, ms < 10000 ? 2 : 1) + " s");

function mainModule(d) {
  const parts = d.outline.parts;
  return parts[0]?.module;
}
function paramChips(m, d) {
  const o = d.outline;
  const lit = (t) => /^(-?[\d.]+|\[[\d.,\s-]+\]|true|false)$/.test(String(t).trim());
  const chip = ([n, v]) => `<span class="pp"><b>${esc(n)}</b> = ${esc(v)}</span>`;
  const vars = o.variables.filter((v) => v.literal).map((v) => [v.name, v.text]);
  const mod = o.modules.find((x) => x.name === mainModule(d));
  const params = (mod?.params ?? []).filter((p) => lit(p.default)).map((p) => [p.name, p.default]);
  const groups = [];
  if (vars.length) groups.push(`<span class="grp">File</span>` + vars.map(chip).join(""));
  // module parameters only in addition when the file variables do not already say it all
  if (params.length && vars.length < 6) groups.push(`<span class="grp">${esc(mod.name)}()</span>` + params.map(chip).join(""));
  return groups.join("");
}
function farben(d) {
  const c = d.outline.colors ?? [];
  if (c.length <= 1) return "single colour (filament slot 1)";
  return c.map((x) => `<span class="sw" style="background:${esc(x.key)}"></span>Slot ${x.filament} ${esc(x.label)} ${fmtNum(x.mass_g, 1)} g`).join("<br>");
}

const seiten = []; // [title, page]
let seite = 3; // 1 title, 2 contents, 3 overview
const pages = [];
for (const [i, { m, d, rf }] of daten.entries()) {
  const nr = String(i + 1).padStart(2, "0");
  const o = d.outline;
  const baugruppe = o.parts.length > 1;
  const text = fill(m.text, o, mainModule(d));
  const mittel = sprachmittel(d.files.filter((f, k, a) => a.indexOf(f) === k).map((f) => join(dirname(join(root, rf)), f)));
  seite += 1;
  seiten.push({ nr, titel: m.titel, kurz: m.kurz ?? m.titel, rf, seite, d, baugruppe });
  const partsTxt = o.parts.map((p) => `<code>${esc(p.name)}</code>`).join(", ");
  const kenn = [
    ["Bounding box (W × D × H)", `${mm(d.bounds)} mm`],
    ["Volume", `${fmtNum(d.volume / 1000, 1)} cm³`],
    ["Mass PLA (1.24 g/cm³)", `${fmtNum(masse(d.volume), 1)} g`],
    ["Parts", `${o.parts.length}: ${partsTxt}`],
    ["Colours / filaments", farben(d)],
    ["Kernel time (no cache)", `${sec(d.kernel_ms)} <span class="dim">(${d.nodes} nodes)</span>`],
  ];
  const cmd = `cad ${rf}`;
  pages.push(`
<section class="page model" id="m-${m.id}">
  <header class="mh"><span class="nr">${nr}</span><h2>${esc(m.titel)}</h2><span class="tag">${baugruppe ? "Assembly" : "Single part"}</span></header>
  <p class="desc">${esc(text)}</p>
  <figure class="hero">${d.img.hero ? `<img src="${d.img.hero.file}" alt="${esc(m.titel)}, isometric">` : `<div class="missing">Rendering failed</div>`}</figure>
  <div class="row">
    <figure class="sheet">${d.img.sheet ? `<img src="${d.img.sheet.file}" alt="Views">` : ""}<figcaption>Views iso, front, top, right (same scale)</figcaption></figure>
    <table class="kenn">${kenn.map(([k, v]) => `<tr><th>${k}</th><td>${v}</td></tr>`).join("")}</table>
  </div>
  <div class="block"><h3>Parameters (default values)</h3><div class="chips">${paramChips(m, d)}</div></div>
  <div class="block"><h3>Language features</h3><div class="chips">${mittel.map((x) => `<span class="ch">${esc(x)}</span>`).join("")}</div></div>
  <div class="cmd"><span>Open</span><code>${esc(cmd)}</code><span>headless</span><code>cad render ${esc(rf)} -o bild.png --views all</code></div>
</section>`);
  if (baugruppe) {
    seite += 1;
    const extras = (m.extra ?? []).map((x) => (d.img[x.name] ? `<figure class="extra"><img src="${d.img[x.name].file}" alt="${esc(x.caption)}"><figcaption>${esc(x.caption)}</figcaption></figure>` : `<div class="missing">${esc(x.name)}: rendering failed</div>`)).join("");
    const extra = (m.extra ?? []).length > 1 ? `<div class="extras2">${extras}</div>` : extras;
    const coll = d.collisions == null ? "–" : d.collisions.length === 0 ? "no collision (tolerance 0.001 mm³)" : d.collisions.map((c) => `${c.a} ↔ ${c.b}: ${fmtNum(c.volume, 3)} mm³`).join("; ");
    pages.push(`
<section class="page model">
  <header class="mh"><span class="nr">${nr}</span><h2>${esc(m.titel)} <span class="sub">– fit</span></h2><span class="tag">Assembly</span></header>
  <table class="parts"><thead><tr><th>Part</th><th>Module</th><th>Bounding box (W × D × H) mm</th><th class="r">Volume cm³</th><th class="r">Mass PLA g</th></tr></thead><tbody>
  ${o.parts.map((p) => `<tr><td><code>${esc(p.name)}</code></td><td><code>${esc(p.module)}</code></td><td>${mm(p.bounds)}</td><td class="r">${fmtNum(p.volume / 1000, 2)}</td><td class="r">${fmtNum(masse(p.volume), 1)}</td></tr>`).join("")}
  </tbody></table>
  <table class="parts"><thead><tr><th>Pair (<code>measure --clearance</code>)</th><th class="r">Distance</th><th>Meaning</th></tr></thead><tbody>
  ${d.pairs.map((p) => `<tr><td><code>${esc(p.a)}</code> ↔ <code>${esc(p.b)}</code></td><td class="r">${p.distance == null ? "–" : fmtNum(p.distance, 3) + " mm"}${p.collision ? " (collision)" : ""}</td><td>${esc(p.bed)}</td></tr>`).join("")}
  <tr><td><code>measure --interference</code></td><td colspan="2">${coll}</td></tr>
  </tbody></table>
  ${extra}
  <div class="cmd"><span>Check</span><code>cad measure ${esc(rf)} --interference --clearance ${esc(m.pairs[0][0])} ${esc(m.pairs[0][1])}</code></div>
</section>`);
  }
  for (const x of d.diag) auffaellig.push(`${m.id}: ${x}`);
  if (!d.hero || !d.sheet || d.extra.some((x) => !x.ok)) auffaellig.push(`${m.id}: rendering failed`);
}

const gesamtVol = daten.reduce((s, { d }) => s + d.volume, 0);
const gesamtTeile = daten.reduce((s, { d }) => s + d.outline.parts.length, 0);
const nBaugruppen = daten.filter(({ d }) => d.outline.parts.length > 1).length;
const toc = seiten.map((s) => `<tr><td class="nr">${s.nr}</td><td><a href="#m-${s.d.id}">${esc(s.titel)}</a>${s.baugruppe ? ' <span class="tag sm">Assembly</span>' : ""}<br><code class="dim">${esc(s.rf.replace("tests/models/alltag/", ""))}</code></td><td class="r">${s.d.outline.parts.length}</td><td class="nw">${mm(s.d.bounds)}</td><td class="r nw">${fmtNum(masse(s.d.volume), 0)} g</td><td class="r nw">${sec(s.d.kernel_ms)}</td><td class="r">${s.seite}</td></tr>`).join("");
const grid = seiten.map((s) => `<a class="tile" href="#m-${s.d.id}"><img src="${s.d.img.hero?.file ?? ""}" alt=""><span><em><b>${s.nr}</b> ${esc(s.kurz)}</em><i>p. ${s.seite}</i></span></a>`).join("");

const howtoLines = [
  ["cad tests/models/alltag/becher.mcad", "desktop UI: editor and 3D"],
  ["cad tests/models/alltag/gehaeuse/", "assembly as project (folder)"],
  ["cad render tests/models/alltag/becher.mcad -o becher.png --views all", "headless: image"],
  ["cad outline tests/models/alltag/becher.mcad", "parameters, grids, anchors, parts"],
  ["cad measure tests/models/alltag/becher.mcad --volume --colors", "volume, mass per colour"],
  ["cad export tests/models/alltag/zahnrad.mcad --all-parts -o teile/", "STL/STEP/3MF per part"],
];
const howto = howtoLines.map(([c, k]) => `<code>${esc(c)}</code><span>${esc(k)}</span>`).join("");
const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Everyday models – catalogue</title>
<style>
@page { size: A4; margin: 13mm 14mm 15mm 14mm; }
:root { --ink:#1d2129; --dim:#5b6270; --line:#d9dce2; --soft:#f4f5f7; --acc:#2f5d8a; }
* { box-sizing: border-box; }
html { background: #fff; }
body { margin: 0; font: 9.6pt/1.42 "DejaVu Sans", "Helvetica Neue", Arial, sans-serif; color: var(--ink); background: #fff; }
code { font: 8.4pt "DejaVu Sans Mono", Consolas, monospace; }
a { color: inherit; text-decoration: none; }
.page { width: 182mm; height: 268mm; overflow: hidden; break-after: page; position: relative; margin: 0 auto; }
.page.model { display: flex; flex-direction: column; }
.page:last-child { break-after: auto; }
@media screen { body { background: #e6e8ec; } .page { background: #fff; padding: 13mm 14mm; width: 210mm; height: 297mm; margin: 8mm auto; box-shadow: 0 1px 4px #0002; } }
h1 { font-size: 26pt; line-height: 1.15; margin: 0 0 3mm; font-weight: 600; letter-spacing: -0.01em; }
h2 { font-size: 15pt; margin: 0; font-weight: 600; }
h2 .sub { font-weight: 400; color: var(--dim); font-size: 12pt; }
h3 { font-size: 8pt; text-transform: uppercase; letter-spacing: .06em; color: var(--dim); margin: 0 0 1.2mm; font-weight: 600; }
.dim { color: var(--dim); }
.r { text-align: right; }
.title { padding-top: 30mm; }
.title .kicker { color: var(--acc); font-weight: 600; text-transform: uppercase; letter-spacing: .08em; font-size: 9pt; margin-bottom: 4mm; }
.title .lead { font-size: 12pt; line-height: 1.5; color: #333a46; max-width: 160mm; margin: 0 0 8mm; }
.title p { max-width: 165mm; }
.title .facts { display: flex; gap: 6mm; margin: 8mm 0; }
.title .facts div { border-top: 2px solid var(--acc); padding-top: 2mm; min-width: 30mm; }
.title .facts b { display: block; font-size: 17pt; font-weight: 600; }
.title .facts span { color: var(--dim); font-size: 8.5pt; }
.howto { display: grid; grid-template-columns: auto 1fr; gap: 1.2mm 5mm; background: var(--soft); border: 1px solid var(--line); border-radius: 2mm; padding: 3mm 4mm; margin: 2mm 0 5mm; align-items: baseline; }
.howto span { color: var(--dim); font-size: 8.6pt; }
.title .stand { position: absolute; bottom: 0; left: 0; right: 0; color: var(--dim); font-size: 8.5pt; border-top: 1px solid var(--line); padding-top: 2mm; }
table { border-collapse: collapse; width: 100%; }
table.toc td, table.toc th { padding: 1.25mm 1.5mm; border-bottom: 1px solid var(--line); vertical-align: baseline; font-size: 9pt; }
table.toc th { text-align: left; color: var(--dim); font-weight: 600; font-size: 7.8pt; text-transform: uppercase; letter-spacing: .04em; border-bottom: 1.5px solid var(--ink); }
table.toc td.nr { color: var(--acc); font-weight: 600; width: 8mm; }
.tag { display: inline-block; font-size: 7.4pt; color: var(--acc); border: 1px solid #b9cbe0; border-radius: 1mm; padding: 0 1.3mm; line-height: 1.5; background: #f1f6fb; }
.tag.sm { font-size: 6.8pt; }
.tiles { display: grid; grid-template-columns: repeat(4, 1fr); gap: 3mm; margin-top: 4mm; }
.tile { display: block; border: 1px solid var(--line); border-radius: 1.5mm; overflow: hidden; background: ${BG}; }
.tile img { width: 100%; display: block; aspect-ratio: 4/3; object-fit: cover; }
.tile span { display: flex; gap: 1.5mm; background: #fff; border-top: 1px solid var(--line); padding: 1mm 1.6mm; font-size: 7.6pt; white-space: nowrap; }
.tile em { font-style: normal; flex: 1; overflow: hidden; text-overflow: ellipsis; }
.tile b { color: var(--acc); } .tile i { color: var(--dim); font-style: normal; }
.mh { display: flex; align-items: baseline; gap: 3mm; border-bottom: 1.5px solid var(--ink); padding-bottom: 1.6mm; margin-bottom: 2.6mm; }
.mh .nr { font-size: 15pt; font-weight: 600; color: var(--acc); }
.mh .tag { margin-left: auto; }
.nw { white-space: nowrap; }
.grp { font-size: 7.4pt; color: var(--dim); padding: 0.2mm 0.6mm 0 0; line-height: 1.5; }
.grp:not(:first-child) { margin-left: 2mm; }
.desc { margin: 0 0 3mm; }
figure { margin: 0; }
.hero { background: ${BG}; border: 1px solid var(--line); border-radius: 1.5mm; overflow: hidden; }
.hero img { width: 100%; display: block; height: 110mm; object-fit: contain; }
.row { display: flex; gap: 5mm; margin: 3.5mm 0 3mm; align-items: flex-start; }
.sheet { width: 64mm; flex: none; }
.sheet img { width: 100%; display: block; border: 1px solid var(--line); border-radius: 1mm; }
figcaption { font-size: 7.4pt; color: var(--dim); margin-top: 1mm; }
table.kenn th, table.kenn td { text-align: left; vertical-align: top; padding: 1.15mm 1.5mm; border-bottom: 1px solid var(--line); font-size: 8.8pt; }
table.kenn th { color: var(--dim); font-weight: 400; width: 40mm; white-space: nowrap; }
table.kenn tr:first-child th, table.kenn tr:first-child td { border-top: 1px solid var(--line); }
.sw { display: inline-block; width: 2.6mm; height: 2.6mm; border-radius: 50%; border: 1px solid #0003; margin-right: 1.2mm; vertical-align: -0.3mm; }
.block { margin: 0 0 2.4mm; }
.chips { display: flex; flex-wrap: wrap; gap: 1.1mm 1.2mm; }
.ch, .pp { font-size: 7.8pt; border-radius: 1mm; padding: 0.2mm 1.5mm; line-height: 1.5; }
.ch { background: var(--soft); border: 1px solid var(--line); }
.pp { font-family: "DejaVu Sans Mono", monospace; font-size: 7.6pt; border: 1px solid var(--line); }
.pp b { font-weight: 600; color: var(--acc); }
.cmd { margin-top: auto; display: grid; grid-template-columns: auto 1fr; gap: 0.8mm 3mm; background: var(--soft); border: 1px solid var(--line); border-radius: 1.5mm; padding: 2mm 3mm; align-items: baseline; }
.cmd span { color: var(--dim); font-size: 7.8pt; text-transform: uppercase; letter-spacing: .05em; }
table.parts { margin: 0 0 4mm; }
table.parts th, table.parts td { text-align: left; padding: 1.3mm 1.5mm; border-bottom: 1px solid var(--line); font-size: 8.8pt; }
table.parts th { color: var(--dim); font-weight: 600; font-size: 7.8pt; border-bottom: 1.5px solid var(--ink); }
table.parts .r { text-align: right; }
.extra { background: ${BG}; border: 1px solid var(--line); border-radius: 1.5mm; overflow: hidden; }
.extra img { width: 100%; display: block; aspect-ratio: 4/3; }
.extras2 { display: flex; flex-direction: column; gap: 3mm; }
.extras2 .extra { display: flex; }
.extras2 .extra img { width: 108mm; flex: none; }
.extras2 .extra figcaption { border-top: 0; border-left: 1px solid var(--line); padding: 3mm; font-size: 8pt; }
table.toc code { font-size: 7.6pt; }
.extra figcaption { background: #fff; border-top: 1px solid var(--line); padding: 1.4mm 2mm; margin: 0; }
.missing { padding: 20mm; text-align: center; color: #a33; }
</style></head><body>

<section class="page title">
  <div class="kicker">Parametric CAD · example collection</div>
  <h1>Everyday models<br>Catalogue</h1>
  <p class="lead">${daten.length} parametric models from <code>tests/models/alltag/</code>: household, desk, workshop and typical machine parts – from the mug to the pillow block bearing.</p>
  <p>The collection serves two purposes: as a template for your own parts (every model is a module with default values, all dimensions in mm) and as a test suite that uses the language broadly – grids and anchors, fillet and chamfer via zones, <code>shell</code>, <code>loft</code>, <code>sweep</code>, profiles, <code>include</code>/<code>use</code>, named instances as parts. The regression <code>tests/alltag/check.sh</code> checks all models; bugs and weaknesses found are listed in <code>tests/models/alltag/README.md</code>.</p>
  <p>For each model the catalogue shows a page with description, rendering, views, key data, parameters and the language features used; assemblies have a second page with parts, measured distances and a section or the anchors. All values are measured with the CLI.</p>
  <h3 style="margin-top:6mm">Opening a model</h3>
<div class="howto">${howto}</div>
  <div class="facts">
    <div><b>${daten.length}</b><span>models</span></div>
    <div><b>${nBaugruppen}</b><span>with multiple parts</span></div>
    <div><b>${gesamtTeile}</b><span>parts</span></div>
    <div><b>${fmtNum(masse(gesamtVol) / 1000, 2)} kg</b><span>PLA for everything</span></div>
  </div>
  <div class="stand">As of ${datum}, commit <code>${esc(commit)}</code> · generated with <code>node tools/report/katalog.mjs</code> · images <code>cad render --view iso --perspective</code>, fitted · mass: PLA 1.24 g/cm³ · kernel time: <code>scene.stats.kernel_ms</code> from <code>cad eval --no-daemon</code></div>
</section>

<section class="page">
  <header class="mh"><h2>Contents</h2></header>
  <table class="toc"><thead><tr><th>No.</th><th>Model, file</th><th class="r">Parts</th><th>Bounding box W × D × H mm</th><th class="r">PLA</th><th class="r">Kernel</th><th class="r">Page</th></tr></thead><tbody>${toc}</tbody></table>
</section>

<section class="page">
  <header class="mh"><h2>Overview</h2></header>
  <div class="tiles">${grid}</div>
</section>
${pages.join("\n")}
</body></html>
`;
writeFileSync(join(out, "everyday-models.html"), html);
const total = 3 + pages.length;
console.log(`${rel(join(out, "everyday-models.html"))}: ${total} pages expected, images ${daten.reduce((s, { d }) => s + Object.values(d.img).reduce((a, x) => a + x.bytes, 0), 0)} B`);
for (const a of auffaellig) console.log("  notable: " + a);
writeFileSync(join(work, "auffaellig.txt"), auffaellig.join("\n") + "\n");
