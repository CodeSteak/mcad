#!/usr/bin/env node
// Test report 8 ("CLI für Agenten"): runs the new CLI functions on examples, measures run times,
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
//   node tools/report/print-pdf.mjs docs/reports/08-cli-agenten/report.html docs/reports/08-cli-agenten/report.pdf "Testbericht 8: CLI für Agenten"

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
const de = (x, d = 0) => Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d });
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
      if (v.length > maxArr) a.push(`… (${v.length - maxArr} weitere)`);
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
  return l.length > n ? [...l.slice(0, n), `… (${l.length - n} Zeilen mehr)`].join("\n") : l.join("\n");
};

// ---------------------------------------------------------------- functions

const sections = [];
const sec = (title, items) => sections.push({ title, items });
const block = (r, opts = {}) => ({ kind: "cmd", r, ...opts });

const G = td("gehaeuse.scad");
const K = td("kollision.scad");
const P = ex("projekt-gehaeuse/gehaeuse.scad");

// outline
{
  const a = run(["outline", P]);
  const b = runJson(["outline", G]);
  sec("1 cad outline", [
    { kind: "p", html: "Ein Aufruf liefert die Struktur eines Projekts: Module mit Datei:Zeile, Parametern (Standardwert, geratener Typ), Raster und Anker je Modul (mit den Standardwerten ausgewertet, Ausdrucksebenen mit Ausdruck und Wert), Funktionen, Variablen der obersten Ebene mit Werten, benannte Instanzen und Bauteile (Hülle, Volumen), <code>include</code>/<code>use</code> und die Diagnosenanzahl. <code>--no-kernel</code> lässt Hüllen und Volumen weg." },
    block(a, { text: true }),
    block(b, { json: ["modules", "parts", "counts"] }),
  ]);
}

// render: contact sheet
{
  const a = runJson(["render", G, "-o", im("bogen"), "--module", "gehaeuse", "--views", "all", "--show", "grids,anchors,labels,dims", "--size", "1500x1100"]);
  const b = runJson(["render", P, "-o", im("bogen-projekt"), "--views", "iso,front,top,right", "--show", "anchors,labels", "--size", "1500x1100"]);
  sec("2 cad render: Kontaktbogen", [
    { kind: "p", html: "<code>--views iso,front,top,right</code> (oder <code>all</code>) setzt beschriftete Teilbilder zu einem Bild zusammen. Alle Teilbilder haben dieselbe Skalierung (px/mm) und denselben Blickpunkt (Mitte des Modells), damit Größen zwischen den Ansichten vergleichbar sind; <code>--size</code> ist die Gesamtgröße. Das JSON nennt je Teilbild Lage und Kamera." },
    block(a, { json: ["width", "height", "views", "overlays"], image: "bogen", caption: "Spec-Beispiel gehaeuse im Modulkontext: Raster g mit Ebenennamen und Rasterpunkten g[a,b,c], Anker als Rauten mit Richtungspfeil, Maße B/T/H." }),
    block(b, { image: "bogen-projekt", caption: "Projekt aus drei Dateien im Dateikontext: Anker der benannten Instanzen (gehaeuse.deckel_mitte, dom[i].oben) und Bauteilnamen." }),
  ]);
}

// render: free camera and selection
{
  const a = runJson(["render", ex("spec/fillet_zonen.scad"), "-o", im("kamera-azimut"), "--line", "12", "--azimuth", "120", "--elevation", "30", "--size", "700x500"]);
  const b = runJson(["render", G, "-o", im("kamera-perspektive"), "--perspective", "--fov", "45", "--azimuth", "-30", "--elevation", "20", "--size", "700x500"]);
  const c = runJson(["render", P, "-o", im("auswahl-part"), "--part", "deckel", "--show", "dims", "--size", "700x500"]);
  const d = runJson(["render", G, "-o", im("auswahl-zeilen"), "--line", "22", "--line", "23", "--fit", "line:22", "--zoom", "0.8", "--size", "700x500"]);
  sec("3 cad render: freie Kamera und Auswahl", [
    { kind: "p", html: "Kamera: <code>--azimuth</code> (0° von vorn/−y, 90° von rechts/+x), <code>--elevation</code> (90° von oben), <code>--zoom</code>, <code>--target x,y,z</code>, <code>--fit all|part:NAME|line:N</code>, <code>--ortho</code>/<code>--perspective [--fov]</code>. Auswahl: <code>--part NAME</code> (nur Bauteile), <code>--line N</code> und <code>--node I</code> mehrfach (Knoten voll, Rest gedimmt, <code>--only</code> lässt den Rest weg), kombinierbar mit <code>--module</code> und <code>-D</code>." },
    { kind: "pair", items: [
      block(a, { image: "kamera-azimut", caption: "fillet_zonen.scad --line 12 --azimuth 120 --elevation 30: Zonen blau, ausgewählte Kanten orange." }),
      block(b, { image: "kamera-perspektive", caption: "gehaeuse --perspective --fov 45 --azimuth -30 --elevation 20" }),
    ] },
    { kind: "pair", items: [
      block(c, { image: "auswahl-part", caption: "Projekt --part deckel --show dims" }),
      block(d, { image: "auswahl-zeilen", caption: "--line 22 --line 23 --fit line:22: zwei Zeilen hervorgehoben, eingepasst auf Zeile 22." }),
    ] },
    block(b, { json: ["camera"] }),
    block(d, { json: ["highlighted", "visible_nodes"] }),
  ]);
}

// render: overlays
{
  const shows = ["grids", "anchors", "handles", "edges", "labels", "bbox", "dims"];
  const base = runJson(["render", G, "-o", im("ov-ohne"), "--module", "gehaeuse", "--size", "420x320", "--no-axes"]);
  const rows = [];
  const tiles = [];
  for (const s of shows) {
    const sh = s === "labels" ? "grids,labels" : s;
    const r = runJson(["render", G, "-o", im(`ov-${s}`), "--module", "gehaeuse", "--size", "420x320", "--no-axes", "--show", sh]);
    rows.push({ s: sh, r });
    tiles.push({ image: `ov-${s}`, caption: `--show ${sh}` });
  }
  sec("4 cad render: Überlagerungen", [
    { kind: "p", html: "<code>--show grids,anchors,handles,edges,labels,bbox,dims</code> (oder <code>all</code>): Raster des Kontextmoduls (Ebenen als Rechtecke am Rand des Rasters, Ausdrucksebenen gestrichelt, Schnittpunkte als Punkte, Ebenennamen <code>g.links</code>), Anker als Rauten mit Namen und Pfeil bei <code>dir</code>, Griffe als Quadrate, verdeckte Kanten gestrichelt, <code>labels</code> beschriftet Rasterpunkte <code>g[a,b,c]</code> (bis 48 Punkte je Raster), Griffe und Bauteile, <code>bbox</code> den Hüllquader, <code>dims</code> zusätzlich Breite/Tiefe/Höhe in mm. Beschriftungen, die sich überdecken würden, werden versetzt oder weggelassen." },
    { kind: "grid", items: [{ image: "ov-ohne", caption: "ohne Überlagerung" }, ...tiles] },
    { kind: "table", head: ["--show", "Raster", "Rasterpunkte", "Anker", "Griffe", "Beschriftungen", "Linien"], rows: rows.map(({ s, r }) => { const o = r.j?.overlays ?? {}; return [s, o.grids, o.grid_points, o.anchors, o.handles, o.labels, o.lines]; }) },
  ]);
  void base;
}

// render: section, xray
{
  const a = runJson(["render", G, "-o", im("schnitt-z"), "--section", "z=10", "--size", "600x450"]);
  const a0 = runJson(["render", G, "-o", im("schnitt-ohne"), "--size", "600x450"]);
  const b = runJson(["render", G, "-o", im("schnitt-flip"), "--section", "z=10", "--section-flip", "--view", "bottom", "--size", "600x450"]);
  const c = runJson(["render", ex("spec/flasche.scad"), "-o", im("schnitt-ebene"), "--section-plane", "0,0,0,1,1,0", "--perspective", "--size", "600x450"]);
  const d = runJson(["render", join(fallstudie, "v2.scad"), "-o", im("schnitt-fallstudie"), "--section", "x=20", "--size", "600x450"]);
  const e = runJson(["render", ex("spec/fillet_zonen.scad"), "-o", im("xray"), "--xray", "--size", "600x450"]);
  const f = runJson(["render", P, "-o", im("xray-projekt"), "--xray", "--show", "anchors", "--size", "600x450"]);
  sec("5 cad render: Schnitt und Röntgen", [
    { kind: "p", html: "<b>Schnitt:</b> <code>--section x=WERT|y=WERT|z=WERT</code>, <code>--section-plane px,py,pz,nx,ny,nz</code>, <code>--section-flip</code>. Konvention (gleich für die spätere UI): Ebene durch <i>p</i> mit Normale <i>n</i>; entfernt wird die Hälfte, in die <i>n</i> zeigt, es bleibt (q − p)·n ≤ 0. <code>z=10</code> heißt p = (0,0,10), n = +z; <code>--section-flip</code> kehrt n um. Die Schnittfläche wird als schraffierte Kappe in eigener Farbe gezeichnet (Schraffur je Körper abwechselnd), die Schnittkontur als Linie." },
    { kind: "p", html: "<b>Verfahren: Clipping im Renderer mit Kappe, kein Kernel-Boolean.</b> Die Dreiecke werden an der Ebene geometrisch abgeschnitten (Sutherland–Hodgman je Dreieck, ebenso Kanten). Die Kappe folgt dem Stencil-Prinzip der GPU: für jede Abtastung wird gezählt, wie oft der Sichtstrahl bis zur Ebene die Oberfläche eines Körpers kreuzt (Top-Left-Regel, damit gemeinsame Kanten genau einmal zählen); ist die Zahl ungerade, liegt der Ebenenpunkt im Körper und die Kappe wird in Ebenentiefe geschrieben. Gründe: (1) dasselbe Verfahren wird die UI mit GPU-Clipping und Stencil-Kappe verwenden, Bilder und Konvention stimmen also überein; (2) es braucht keine Kernelarbeit – ein Boolean mit einem Halbraum kostet bei verrundeten Körpern schnell Sekunden und kann scheitern, das Clipping kostet einige ms und gelingt immer; (3) es funktioniert für jede Ebene, auch für Ansichten mehrerer Bauteile. Voraussetzung sind geschlossene Netze; die Tessellierung der B-Rep-Körper ist geschlossen. Offene Flächen (<code>surface</code>) und transparente Körper bekommen keine Kappe." },
    { kind: "pair", items: [
      block(a0, { image: "schnitt-ohne", caption: "ohne Schnitt" }),
      block(a, { image: "schnitt-z", caption: "--section z=10: obere Hälfte entfernt, Wände geschnitten" }),
    ] },
    { kind: "pair", items: [
      block(b, { image: "schnitt-flip", caption: "--section z=10 --section-flip --view bottom: obere Hälfte bleibt" }),
      block(c, { image: "schnitt-ebene", caption: "flasche.scad --section-plane 0,0,0,1,1,0 --perspective" }),
    ] },
    { kind: "pair", items: [
      block(d, { image: "schnitt-fallstudie", caption: "Fallstudie --section x=20: Stange in der Bohrung, zwei Kappen mit gegenläufiger Schraffur, Spalt 0,25 mm als Kontur" }),
      block(e, { image: "xray", caption: "fillet_zonen.scad --xray: alles halbtransparent mit Kanten" }),
    ] },
    block(a, { json: ["section", "bounds"] }),
    { kind: "p", html: `Sichtbare Fläche (Pixel ≠ Hintergrund, 600×450): ohne Schnitt ${de(drawn("schnitt-ohne"))}, mit <code>--section z=10</code> ${de(drawn("schnitt-z"))} (${de((100 * drawn("schnitt-z")) / Math.max(1, drawn("schnitt-ohne")), 0)} %), davon Kappe ${de(capPixels("schnitt-z"))}. Derselbe Vergleich ist der Test <code>render_section_reduces_visible_area</code>.` },
    { kind: "figure", image: "xray-projekt", caption: "Projekt --xray --show anchors: Schraubdome und Anker sichtbar durch die Wand" },
  ]);
  void f;
}

// measure
{
  const a = run(["measure", G, "--between", "k.ecke", "k.oben", "--bounds", "part:k", "--volume", "part:k", "--area", "part:k"]);
  const b = run(["measure", G, "--module", "gehaeuse", "--between", "g[links,vorne,boden]", "g[rechts,hinten,deckel]"]);
  const b2 = run(["measure", G, "--module", "gehaeuse", "-D", "w=60", "--between", "ecke", "[0,0,0]"]);
  const c = run(["measure", K, "--interference", "--clearance", "b", "c"]);
  const c2 = runJson(["measure", K, "--interference"]);
  const d = runJson(["measure", P, "--interference", "--clearance", "gehaeuse", "deckel"]);
  const e = run(["measure", G, "--between", "g[links,vorne,boden]", "[0,0,0]"]);
  const kr = runJson(["render", K, "-o", im("kollision"), "--show", "labels", "--xray", "--size", "600x400"]);
  sec("6 cad measure", [
    { kind: "p", html: "<code>--between A B</code> mit Ankern (<code>k.ecke</code>, <code>k[2].ecke</code>, im Modulkontext <code>ecke</code>), Rasterpunkten des Kontextmoduls (<code>g[a,b,c]</code>, Einträge auch Zahlen oder <code>_</code>) und Koordinaten <code>[x,y,z]</code>: Abstand und Differenzvektor. <code>--bounds/--volume/--area ZIEL</code> mit <code>all</code>, <code>part:NAME</code>, <code>line:N</code>, <code>node:I</code>. <code>--interference</code>: paarweise Schnittvolumen aller Bauteile, exakt über den Kernel (Boolean-Schnitt der B-Rep, vorher Test der Hüllquader); Kollision ab <code>--tolerance</code> (Standard 0,001 mm³). <code>--clearance A B</code>: kleinster Abstand zweier Bauteile als exakter Dreieck-Dreieck-Abstand ihrer Tessellierungen (BVH, Toleranz <code>--mesh-tol</code> 0,01 mm, Genauigkeit ± 2·0,01 mm); überschneiden sich die Bauteile, ist der Abstand 0 und <code>collision: true</code>." },
    { kind: "p", html: "Sollwerte Spec-Beispiel <i>gehaeuse</i>: k.ecke = [60, 30, 20], k.oben = [0, 0, 20] → √(60² + 30²) = 67,082 mm; im Modul g[links,vorne,boden] → g[rechts,hinten,deckel] = √(40² + 30² + 20²) = 53,852 mm; Volumen k = 60·30·20 − 54·30·17 = 8460 mm³. <i>kollision.scad</i>: a und b überlappen 2 × 10 × 10 = 200 mm³, c steht 5 mm neben b." },
    block(a, { text: true }),
    block(b, { text: true }),
    block(b2, { text: true }),
    block(e, { text: true, note: "Fehler erwartet: Raster g ist im Dateikontext nicht sichtbar (Exit 2)." }),
    block(c, { text: true }),
    block(c2, { json: ["interference"] }),
    block(d, { json: ["clearance"] }),
    { kind: "figure", image: "kollision", caption: "kollision.scad --xray --show labels: a und b überlappen, c mit 5 mm Abstand" },
  ]);
  void kr;
}

// set
{
  const f = join(work, "set.scad");
  copyFileSync(G, f);
  const a = run(["set", rel(f), "--param", "gehaeuse.w=50", "--literal", "21:17=70", "--dry-run"]);
  const b = runJson(["set", rel(f), "--param", "gehaeuse.w=50", "--literal", "21:17=70"]);
  const c = run(["set", rel(f), "--literal", "5:30=4"]);
  const d = run(["set", rel(f), "--param", "gehaeuse.t=foo"]);
  sec("7 cad set", [
    { kind: "p", html: "<code>--param modul.param=wert</code> schreibt einen Standardwert (wie „Als Standard übernehmen“ in der UI, Module auch aus <code>use</code>-Dateien), <code>--literal [datei:]zeile:spalte=wert</code> ersetzt das editierbare Literal an der Stelle – dieselben Literale, die die UI über Griffe, Regler und Rasterebenen ändert. Vor dem Schreiben wird das geänderte Projekt ausgewertet; neue Fehler verhindern das Schreiben (Exit 1, <code>--force</code> überstimmt). Geschrieben wird atomar (temporäre Datei + Umbenennen); <code>--dry-run</code> zeigt nur den Diff." },
    block(a, { text: true }),
    block(b, { json: ["changes", "written", "errors_before", "errors_after"] }),
    block(c, { text: true, note: "Fehler erwartet: an 5:30 steht `t`, eine Referenz auf einen Parameter, kein Literal (Exit 2)." }),
    block(d, { text: true, note: "Abgelehnt: die Änderung erzeugt Fehler (Exit 1), die Datei bleibt unverändert." }),
  ]);
}

// UI placeholder and help
{
  const a = run([G]);
  const b = run([]);
  sec("8 Reservierter UI-Einstieg", [
    { kind: "p", html: "<code>cad datei.scad</code> (Positionsargument, das kein Befehl ist) startet später die Desktop-UI. Heute: Meldung und Exit 2. <code>cad</code> ohne Argumente zeigt die Hilfe (Exit 0). Ein Tippfehler im Befehl ergibt „unbekannter Befehl“." },
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
  const vr = join(work, "variabler_radius.scad");
  copyFileSync(join(root, "tests/models/spec/variabler_radius.scad"), vr);
  const src = readFileSync(vr, "utf8");
  const res = await watch(vr, [
    () => writeFileSync(vr, src + "// Kommentar\n"),
    () => writeFileSync(vr, src.replace("deckel = 20", "deckel = 22")),
    () => writeFileSync(vr, src.replace("1 + p.x / 40", "1 + p.x / 50")),
  ], ["--render", rel(im("watch")), "--views", "iso,top", "--size", "800x400"]);
  const what = ["Start (alles neu)", "Kommentar angehängt (alles aus dem Cache)", "deckel = 22 (zwei Verrundungen neu)", "Radiusfunktion der Tasche geändert"];
  res.forEach((r, i) => watchRuns.push({ file: "variabler_radius.scad", what: what[i] ?? "", r }));
  const dir = join(work, "projekt");
  mkdirSync(join(dir, "lib"), { recursive: true });
  for (const f of ["gehaeuse.scad", "konstanten.scad", "lib/teile.scad"]) copyFileSync(join(root, "tests/models/projekt-gehaeuse", f), join(dir, f));
  const konst = readFileSync(join(dir, "konstanten.scad"), "utf8");
  const teile = readFileSync(join(dir, "lib/teile.scad"), "utf8");
  const res2 = await watch(join(dir, "gehaeuse.scad"), [
    () => writeFileSync(join(dir, "konstanten.scad"), konst.replace("abstand = 10", "abstand = 15")),
    () => writeFileSync(join(dir, "lib/teile.scad"), teile.replace("dom_r = 3;", "dom_r = 3.5;")),
  ]);
  const what2 = ["Start (alles neu)", "konstanten.scad: abstand = 15 (nur Lage des Deckels)", "lib/teile.scad: dom_r = 3.5 (Schraubdome neu)"];
  res2.forEach((r, i) => watchRuns.push({ file: "projekt-gehaeuse", what: what2[i] ?? "", r }));
}
sec("9 cad watch", [
  { kind: "p", html: "<code>cad watch DATEI [--json] [--render bild.png …]</code> beobachtet die Datei und alle eingebundenen Dateien (Crate <code>notify</code>, inotify unter Linux; beobachtet werden die Verzeichnisse, damit auch atomares Speichern der Editoren erkannt wird), wertet bei jeder Änderung in derselben Sitzung neu aus und schreibt je Lauf eine JSON-Zeile (Diagnosen, Zeiten, Volumen, Bauteile, Bild). Der Engine-Cache bleibt erhalten: unveränderte Knoten kommen aus dem Cache. Ende mit Strg+C, <code>--once-after-change</code> oder <code>--timeout S</code>." },
  { kind: "table", head: ["Datei", "Lauf", "Auslöser", "Gesamt ms", "Kernel ms", "Bild ms", "aus Cache / Knoten", "Kernel-Aufrufe", "Volumen mm³"], rows: watchRuns.map(({ file, what, r }) => [file, `${r.run}: ${what}`, (r.changed ?? []).join(", ") || r.trigger, de(r.timings?.total_ms ?? 0, 1), de(r.timings?.kernel_ms ?? 0, 1), r.timings?.render_ms != null ? de(r.timings.render_ms, 1) : "–", `${r.stats?.cache_hits ?? "–"} / ${r.stats?.nodes ?? "–"}`, r.stats?.kernel_calls ?? "–", r.volume != null ? de(r.volume, 1) : "–"]) },
  { kind: "pre", label: "JSON-Zeile (Lauf 3, gekürzt)", text: watchRuns[2] ? excerpt(watchRuns[2].r, ["command", "run", "trigger", "changed", "ok", "errors", "stats", "volume", "timings", "render"]) : "–" },
  { kind: "figure", image: "watch", caption: "Bild nach dem letzten Lauf von watch --render … --views iso,top" },
]);

// ---------------------------------------------------------------- timings: repeated calls

const timing = [];
for (const f of ["tests/models/spec/variabler_radius.scad", "tests/models/projekt-gehaeuse/gehaeuse.scad", "tests/models/spec/gehaeuse.scad"]) {
  for (let i = 1; i <= 2; i++) {
    const r = runJson(["render", f, "-o", join(work, "t.png")]);
    timing.push({ f, i, wall: r.ms, t: r.j?.timings ?? {} });
  }
}
const baseline = run(["check", "--no-kernel", "tests/models/spec/gehaeuse.scad"]);
const cmdTimes = [
  ["outline", ["outline", G]],
  ["check", ["check", G]],
  ["render (iso)", ["render", G, "-o", join(work, "a.png")]],
  ["render --views all --show grids,anchors", ["render", G, "-o", join(work, "b.png"), "--module", "gehaeuse", "--views", "all", "--show", "grids,anchors"]],
  ["render --section z=10", ["render", G, "-o", join(work, "c.png"), "--section", "z=10"]],
  ["measure --between --volume", ["measure", G, "--between", "k.ecke", "k.oben", "--volume", "part:k"]],
  ["measure --interference (kollision)", ["measure", K, "--interference"]],
  ["measure --clearance b c (kollision)", ["measure", K, "--clearance", "b", "c"]],
  ["set --dry-run", ["set", G, "--param", "gehaeuse.w=50", "--dry-run"]],
].map(([n, a]) => {
  const r = run(a);
  return [n, de(r.ms, 0), r.code];
});

// ---------------------------------------------------------------- case study

const steps = [];
{
  const f = join(work, "wandhalter.scad");
  const fr = rel(f);
  const step = (title, why, cmds, extra = {}) => steps.push({ title, why, cmds, ...extra });
  copyFileSync(join(fallstudie, "v1.scad"), f);
  step("Entwurf schreiben", "Erste Fassung mit Raster g (Ebenen für Platte, Ausleger, Bohrung, Schraublöcher), zwei Ankern und den Bauteilen halter und stange.", [], { source: readFileSync(f, "utf8") });
  step("Prüfen und verstehen", "check meldet keine Fehler; outline zeigt Raster, Anker und Bauteile mit Hülle und Volumen.", [run(["check", fr]), run(["outline", fr])]);
  step("Ansehen", "Kontaktbogen mit Ankern und Beschriftung: Platte, Ausleger, Bohrung und Stange liegen wie geplant.", [run(["render", fr, "-o", rel(im("fs-1")), "--views", "all", "--show", "anchors,labels", "--size", "1400x1000"])], { image: "fs-1" });
  step("Nachmessen", "Kollisionen und Abstand Stange ↔ Bohrung: Abstand 0 – die Stange liegt ohne Spiel in der Bohrung (Ø 20 in Ø 20). Fehler im Entwurf gefunden.", [run(["measure", fr, "--interference", "--clearance", "halter", "stange"]), run(["measure", fr, "--between", "halter.schraube", "halter.stange"])]);
  step("Korrigieren mit cad set", "Spiel 0,5 mm als Standardwert setzen (erst Diff, dann schreiben); danach Abstand 0,25 mm (radial) ± 0,02 mm.", [run(["set", fr, "--param", "halter.spiel=0.5", "--dry-run"]), run(["set", fr, "--param", "halter.spiel=0.5"]), run(["measure", fr, "--clearance", "halter", "stange"])]);
  const before = readFileSync(f, "utf8");
  copyFileSync(join(fallstudie, "v2.scad"), f);
  const diff = spawnSync("diff", ["-u", "--label", "vorher", "--label", "nachher", "-", f], { input: before, encoding: "utf8" }).stdout;
  step("Innenkante verrunden", "Zonenblock fillet(4) um die Innenkante Platte/Ausleger (zone(g[*, platte, arm], d = 2)), mit dem Edit-Werkzeug eingefügt. inspect der fillet-Zeile: genau 1 Kante ausgewählt.", [run(["check", fr]), run(["inspect", fr, "--line", "10"])], { source: diff, sourceLabel: "Änderung (diff)" });
  step("Raster und Zone ansehen", "Zeile 10 hervorgehoben im Modulkontext, Raster mit Ebenennamen (100 Rasterpunkte: nur Ebenen beschriftet).", [run(["render", fr, "-o", rel(im("fs-2")), "--line", "10", "--views", "iso,right", "--show", "grids,labels", "--size", "1400x620"])], { image: "fs-2" });
  step("Schnitt durch die Bohrung", "Schnitt x = 20 zeigt Stange und Bohrung mit Spalt; Maße des Ganzen.", [run(["render", fr, "-o", rel(im("fs-3")), "--section", "x=20", "--show", "dims", "--size", "900x650"])], { image: "fs-3" });
  step("Ausgeben", "Jedes Bauteil als STEP-Datei.", [run(["export", fr, "--all-parts", "-o", rel(join(work, "teile")), "--format", "step"])]);
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

const date = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
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
  let h = `<div class="cmd"><pre class="call">$ ${esc(r.cmd)}<span class="t">  ${de(r.ms, 0)} ms, Exit ${r.code}</span></pre>`;
  if (b.note) h += `<div class="small">${esc(b.note)}</div>`;
  if (b.text) h += `<pre>${esc(lines((r.out + r.err).replaceAll(root + "/", ""), b.maxLines ?? 40))}</pre>`;
  if (b.json) h += `<div class="lbl">JSON (Auszug: ${esc(b.json.join(", "))})</div><pre>${esc(lines(excerpt(r.j, b.json), 60))}</pre>`;
  h += "</div>";
  if (b.image) h += fig(b.image, b.caption);
  return h;
};
const fig = (name, caption) =>
  existsSync(im(name)) ? `<figure><img src="${imRel(name)}"><figcaption>${esc(caption ?? "")}</figcaption></figure>` : `<figure class="missing">${esc(name)} fehlt</figure>`;
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

const timingRows = timing.map(({ f, i, wall, t }) => [f.replace("tests/models/", ""), i, de(wall, 0), de(t.evaluate_ms ?? 0, 0), de(t.kernel_ms ?? 0, 0), de(t.render_ms ?? 0, 0)]);
const maxWall = Math.max(...timing.map((t) => t.wall));
const vrWatch = watchRuns.filter((w) => w.file === "variabler_radius.scad");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 8 – CLI für Agenten</title>
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
<h1>Testbericht 8 – CLI für Agenten</h1>
<div class="meta">cad-cli, cad-render, cad-session · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne (${esc(os.cpus()[0]?.model ?? "")})</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${cargoSum ? `${cargoSum.ignored} ignoriert` : ""}</div>
  <div>neue Tests<b>${agentTests.length}</b>agent.rs, features.rs, geom</div>
  <div>cargo clippy -D warnings<b class="${clippy != null && !/^(error|warning)/m.test(clippy) ? "ok" : "bad"}">${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "Warnungen" : "sauber"}</b></div>
  <div>cargo fmt --check<b class="${fmt != null && fmt.trim() === "" ? "ok" : "bad"}">${fmt == null ? "–" : fmt.trim() === "" ? "sauber" : "Abweichungen"}</b></div>
  <div>render je Aufruf<b>${de(maxWall / 1000, 2)} s</b>max. (2 Läufe × 3 Dateien)</div>
  <div>watch-Lauf<b>${vrWatch[1] ? de(vrWatch[1].r.timings.total_ms, 0) + " ms" : "–"}</b>Kommentar, alles aus dem Cache</div>
</div>

<h2>Umfang</h2>
<p>Die CLI <code>cad</code> ist so erweitert, dass ein Agent (z. B. Claude Code) Modelle ganz ohne UI entwickeln und prüfen kann – allein oder während ein Mensch dieselben Dateien in der UI bearbeitet. Neu:</p>
<table>
<tr><th>Befehl/Option</th><th>Zweck</th></tr>
<tr><td><code>cad outline</code></td><td>Übersicht eines Projekts in einem Aufruf</td></tr>
<tr><td><code>render --views</code></td><td>Kontaktbogen mit beschrifteten Teilbildern gleicher Skalierung</td></tr>
<tr><td><code>render --azimuth/--elevation/--zoom/--target/--fit/--perspective</code></td><td>Freie Kamera</td></tr>
<tr><td><code>render --part/--line (mehrfach)/--node/--only</code></td><td>Auswahl, kombinierbar mit <code>--module</code>, <code>-D</code></td></tr>
<tr><td><code>render --show grids,anchors,handles,edges,labels,bbox,dims</code></td><td>Überlagerungen</td></tr>
<tr><td><code>render --section/--section-plane/--section-flip</code>, <code>--xray</code></td><td>Schnitt mit gefüllter Kappe, Röntgen</td></tr>
<tr><td><code>cad measure</code></td><td>Abstände (Anker, Rasterpunkte, Koordinaten), Hülle, Volumen, Fläche, Kollisionen, Bauteilabstand</td></tr>
<tr><td><code>cad watch</code></td><td>Neu auswerten bei Dateiänderung, eine JSON-Zeile je Lauf, optional Bild</td></tr>
<tr><td><code>cad set</code></td><td>Standardwert oder Literal über die Engine ändern, geprüft, atomar, <code>--dry-run</code></td></tr>
<tr><td><code>cad DATEI.scad</code>, <code>cad</code></td><td>UI-Einstieg reserviert (Exit 2 mit Meldung), ohne Argumente Hilfe</td></tr>
</table>
<p>Jeder Befehl hat <code>--json</code> mit festem, versioniertem Schema (<code>schema_version</code> 1, vollständig im Kopf von <code>crates/cad-cli/src/main.rs</code>, Kurzfassung im Skill <code>.claude/skills/cad-cli/SKILL.md</code>) und die bisherigen Exit-Codes (0 ok, 1 Modellfehler, 2 Aufruf, 3 Kernel). Empfohlener Ablauf für Agenten: <b>outline → editieren → check → render --views all --show grids,anchors → inspect/measure → export</b>. Beschriftungen zeichnet <code>cad-render</code> mit einer eingebetteten Bitmap-Schrift (DejaVu Sans Mono, erzeugt von <code>tools/font/gen-font.py</code>), ohne neue Laufzeitabhängigkeit.</p>

${sections.map((s) => `<h2>${esc(s.title)}</h2>\n${s.items.map(itemHtml).join("\n")}`).join("\n")}

<h2>10 Leistung bei wiederholten Aufrufen</h2>
<p>Jeder CLI-Aufruf startet einen neuen Prozess mit leerem Cache. Gemessen: <code>cad render</code> zweimal hintereinander (Standardansicht 1024×768), Wandzeit des Prozesses und die Zeiten aus dem JSON (<code>timings</code>: Auswerten inkl. Kernel, davon Kernel, Bild inkl. Tessellierung). Prozessstart mit Parser ohne Kernel (<code>check --no-kernel</code>): ${de(baseline.ms, 0)} ms.</p>
<table><tr><th>Datei</th><th>Lauf</th><th>Wandzeit ms</th><th>Auswerten ms</th><th>davon Kernel ms</th><th>Bild ms</th></tr>
${timingRows.map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join("")}</tr>`).join("")}</table>
<p><b>Entscheidung: kein persistenter Cache zwischen Aufrufen.</b> Der zweite Aufruf ist so schnell wie der erste (kein Cache), ein Aufruf dauert bis ${de(maxWall / 1000, 2)} s – bei <i>variabler_radius</i> an der Grenze von 1 s, fast alles Kernelzeit (Verrundungen) und Tessellierung. Ein Datei-Cache der Kernel-Ergebnisse wäre aufwendig: die Ergebnisse sind Handles auf SMLib-Objekte im Speicher der nativen Bibliothek, ein Cache müsste die B-Rep serialisieren und wieder einlesen (und mit der Provenienz der Flächen und Kanten), was selbst Kernelzeit kostet. Stattdessen <code>cad watch</code>: dieselbe Sitzung bleibt bestehen, der Engine-Cache hält alle Knotenergebnisse; ein Lauf nach einer Änderung kostet nur die geänderten Knoten (Tabelle in Abschnitt 9: ${vrWatch[1] ? de(vrWatch[1].r.timings.total_ms, 0) : "–"} ms für eine Änderung ohne Geometrieeinfluss, ${vrWatch[2] ? de(vrWatch[2].r.timings.total_ms, 0) : "–"} ms wenn zwei verrundete Körper neu entstehen). Der Skill empfiehlt <code>cad watch</code> für Schleifen mit vielen kleinen Schritten.</p>
<h3>Laufzeit je Befehl (Spec-Beispiel gehaeuse, Wandzeit)</h3>
<table><tr><th>Befehl</th><th>ms</th><th>Exit</th></tr>${cmdTimes.map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join("")}</tr>`).join("")}</table>

<h2>11 Fallstudie: Agent entwickelt ein Teil nur mit der CLI</h2>
<p>Der Agent (dieser Bericht ist von Claude erzeugt) hat einen Wandhalter für eine Stange Ø 20 mm entworfen, ohne die UI zu öffnen: Quelltext mit dem Edit-Werkzeug, alles Weitere mit <code>cad</code>. Die Schritte unten sind die tatsächlich ausgeführten, vom Generator nachgespielt (Fassungen in <code>tools/report/report8-fallstudie/</code>). Der Agent hat dabei die Bilder mit dem Read-Werkzeug angesehen; gefunden wurde ein echter Entwurfsfehler (kein Spiel zwischen Stange und Bohrung), sichtbar nur durch <code>measure --clearance</code>, nicht im Bild.</p>
${steps.map((s, i) => `<div class="step"><h3>Schritt ${i + 1}: ${esc(s.title)}</h3><p>${esc(s.why)}</p>${s.source ? `<div class="lbl">${esc(s.sourceLabel ?? "Quelltext")}</div><pre>${esc(s.source)}</pre>` : ""}${s.cmds.map((r) => cmdHtml({ r, text: true, maxLines: 24 })).join("")}${s.image ? fig(s.image, "") : ""}</div>`).join("\n")}
<p>Bilanz: ${steps.reduce((a, s) => a + s.cmds.length, 0)} CLI-Aufrufe, zusammen ${de(steps.reduce((a, s) => a + s.cmds.reduce((b, r) => b + r.ms, 0), 0) / 1000, 1)} s. Was gefehlt hätte ohne die neuen Funktionen: die Abstandsmessung (Spiel), die Rasternamen im Bild (welche Ebene ist <code>arm</code>?) und der Schnitt durch die Bohrung.</p>

<h2>12 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen, ${cargoSum.ignored} ignoriert` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "Warnungen" : "ohne Befund"}; <code>cargo fmt --check</code>: ${fmt == null ? "–" : fmt.trim() === "" ? "ohne Befund" : "Abweichungen"}.</li>
<li>Neu in <code>crates/cad-cli/tests/agent.rs</code> (assert_cmd, je Befehl/Option mit JSON-Feldern): outline (Spec-Beispiel, Funktionen/Variablen, Projekt), render (Kontaktbogen: Bildgröße und gleiche Skalierung; freie Kamera; Auswahl; Überlagerungen ändern Pixel; Schnitt verkleinert die sichtbare Fläche und erzeugt Kappenpixel; Röntgen), measure (Anker, Rasterpunkte, Koordinaten gegen Sollwerte des Spec-Beispiels; Hülle, Volumen; Kollision an <code>tests/data/kollision.scad</code>; Abstand), watch (Datei ändern → zweite JSON-Zeile, Cache-Treffer steigen; Text und Timeout), set (Parameter, Literal, Diff, Ablehnung, Projektdateien), UI-Einstieg.</li>
<li>Neu in <code>crates/cad-render/tests/features.rs</code>: Kamerawinkel = Standardansichten, Schnitt mit Kappe an einem geschlossenen Würfel (von oben ganz Kappe, von unten keine), Perspektive und feste Skalierung, Marker/Beschriftung/Titel/Kontaktbogen; in <code>geom.rs</code>: Dreieck-Dreieck-Abstand an Würfeln (Fläche, Kante, Ecke, Überlappung).</li>
</ul>
<table><tr><th>Testdatei</th><th>Test</th><th>Ergebnis</th></tr>${agentTests.map((t) => `<tr><td>${esc(t.file.replace(/\(.*$/, "").trim())}</td><td>${esc(t.name)}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("")}</table>

<h2>13 Grenzen</h2>
<ul>
<li>Schnittkappen setzen geschlossene Netze voraus; bei offenen Flächen (<code>surface</code> ohne <code>thicken</code>) und transparenten Körpern gibt es keine Kappe. Die Kappe ist eben und einfarbig (Schraffur je Körper abwechselnd), keine Materialunterscheidung.</li>
<li><code>--clearance</code> misst an der Tessellierung (Genauigkeit ± 2·<code>--mesh-tol</code>), <code>--interference</code> exakt über den Kernel; Bauteile, deren Hülle sich nicht überlappt, werden ohne Kernel als kollisionsfrei gezählt.</li>
<li>Rasterpunkte werden bis 48 Punkte je Raster beschriftet, darüber nur die Ebenen (Hinweis in <code>notes</code>); überdeckte Beschriftungen fallen weg.</li>
<li><code>outline</code> wertet jedes Modul mit seinen Standardwerten aus; Module mit Pflichtparametern ohne Standardwert zeigen dann ggf. keine Raster (<code>errors_with_defaults</code>).</li>
<li><code>watch</code> liest bei jeder Änderung alle Projektdateien neu; Änderungen innerhalb der Entprellzeit (<code>--debounce</code>, 60 ms) werden zu einem Lauf zusammengefasst.</li>
<li>Kein persistenter Cache zwischen Aufrufen (Abschnitt 10).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${rel(join(out, "report.html"))}: ${sections.length} Abschnitte, ${steps.length} Schritte Fallstudie, ${watchRuns.length} watch-Läufe`);
console.log(`PDF: node tools/report/print-pdf.mjs ${rel(join(out, "report.html"))} ${rel(join(out, "report.pdf"))} "Testbericht 8: CLI für Agenten"`);
