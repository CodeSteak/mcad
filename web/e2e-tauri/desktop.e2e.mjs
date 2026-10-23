#!/usr/bin/env node
// End-to-end tests of the desktop app (Tauri 2) with tauri-driver + WebKitWebDriver, under
// Xvfb when there is no display. A small W3C WebDriver client (fetch) drives the real app
// binary (`cad`, feature gui) with the real native engine process.
//
//   node web/e2e-tauri/desktop.e2e.mjs [--bin target/release/cad] [--only T03]
//
// Needs: tauri-driver (cargo install tauri-driver --locked), WebKitWebDriver
// (webkit2gtk-driver), Xvfb, git. Screenshots, measurements and results go to
// reports/tmp/report9/ (test report 9, tools/report/report9.mjs).

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(here, "../..");
const argv = process.argv.slice(2);
const opt = (name, d) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : d;
};
const BIN = resolve(opt("--bin", join(ROOT, "target/release/cad")));
const ONLY = opt("--only", null);
const OUT = join(ROOT, "reports/tmp/report9");
mkdirSync(join(OUT, "img"), { recursive: true });
const PORT = 4444 + Math.floor(Math.random() * 200) * 2;
const WD = `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const work = mkdtempSync(join(tmpdir(), "cad-desktop-e2e-"));
const ipcDir = join(work, "ipc");
const children = [];
let display = process.env.DISPLAY;

function cleanup() {
  for (const c of children.reverse()) {
    try {
      c.kill("SIGTERM");
    } catch {
      /* gone */
    }
  }
}
process.on("exit", cleanup);
process.on("SIGINT", () => process.exit(130));

// ------------------------------------------------------------------------------- driver

async function wd(method, path, body) {
  const r = await fetch(`${WD}${path}`, { method, headers: { "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) });
  const j = await r.json().catch(() => ({}));
  if (!r.ok || (j.value && j.value.error)) throw new Error(`${method} ${path}: ${JSON.stringify(j.value ?? j).slice(0, 400)}`);
  return j.value;
}

let sid = null;
const exec = (script, ...args) => wd("POST", `/session/${sid}/execute/sync`, { script, args });
const execAsync = (script, ...args) => wd("POST", `/session/${sid}/execute/async`, { script, args });
const state = () => exec("return window.__cad ? window.__cad.state() : null");

async function waitFor(what, fn, ms = 60_000) {
  const t0 = Date.now();
  let last = null;
  while (Date.now() - t0 < ms) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = String(e);
    }
    await sleep(100);
  }
  throw new Error(`Zeitüberschreitung: ${what} (zuletzt: ${JSON.stringify(last)?.slice(0, 300)})`);
}

/** The view shows the complete scene of the sent source with all meshes. */
const sceneReady = (minVersion = 0) =>
  waitFor("Szene", async () => {
    const s = await state();
    return s && s.shownComplete && !s.pendingSend && s.shownVersion === s.sentVersion && s.shownVersion >= minVersion && s.view && s.view.missingMeshes === 0 ? s : null;
  });

async function el(css) {
  const v = await wd("POST", `/session/${sid}/element`, { using: "css selector", value: css });
  return Object.values(v)[0];
}
const click = async (css) => wd("POST", `/session/${sid}/element/${await el(css)}/click`, {});
async function exists(css) {
  const v = await wd("POST", `/session/${sid}/elements`, { using: "css selector", value: css });
  return v.length > 0;
}

async function shot(name) {
  const b64 = await wd("GET", `/session/${sid}/screenshot`);
  writeFileSync(join(OUT, "img", `${name}.png`), Buffer.from(b64, "base64"));
  return `${name}.png`;
}

/** X screenshot of the whole screen (window frame included). */
function xshot(name) {
  const f = join(OUT, "img", `${name}.png`);
  spawnSync("import", ["-window", "root", f], { env: { ...process.env, DISPLAY: display } });
  return `${name}.png`;
}

function cad(...args) {
  const r = spawnSync(BIN, args, { env: { ...process.env, CAD_IPC_DIR: ipcDir, DISPLAY: display }, encoding: "utf8" });
  return { code: r.status, out: r.stdout, err: r.stderr };
}

function git(dir, ...args) {
  const r = spawnSync("git", ["-C", dir, "-c", "user.name=Test", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", ...args], { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr}`);
}

// ---------------------------------------------------------------------------------- app

async function startDriver() {
  if (!display) {
    display = ":97";
    const x = spawn("Xvfb", [display, "-screen", "0", "1600x1000x24"], { stdio: "ignore" });
    children.push(x);
    await sleep(800);
  }
  const td = spawn("tauri-driver", ["--port", String(PORT), "--native-port", String(PORT + 1)], {
    env: { ...process.env, DISPLAY: display, CAD_IPC_DIR: ipcDir, CAD_WATCHDOG_MS: "3000" },
    stdio: ["ignore", "inherit", "inherit"],
  });
  children.push(td);
  await waitFor("tauri-driver", async () => (await fetch(`${WD}/status`).then((r) => r.ok).catch(() => false)), 20_000);
}

async function startApp(args) {
  const t0 = Date.now();
  const v = await wd("POST", "/session", { capabilities: { alwaysMatch: { "tauri:options": { application: BIN, args } } } });
  sid = v.sessionId;
  await waitFor("UI", async () => (await exec("return !!window.__cad")) || null, 60_000);
  const s = await sceneReady();
  return { wallMs: Date.now() - t0, state: s };
}

async function stopApp() {
  if (!sid) return;
  await wd("DELETE", `/session/${sid}`).catch(() => {});
  sid = null;
  await sleep(500);
}

// -------------------------------------------------------------------------------- tests

const results = [];
const metrics = {};

async function test(id, title, f) {
  if (ONLY && !ONLY.split(",").includes(id)) return;
  const t0 = Date.now();
  const entry = { id, title, ok: false, ms: 0, checks: [], images: [], error: null };
  const check = (cond, text) => {
    entry.checks.push({ ok: !!cond, text });
    if (!cond) throw new Error(`Prüfung fehlgeschlagen: ${text}`);
  };
  try {
    await f(check, entry);
    entry.ok = true;
  } catch (e) {
    entry.error = String(e.stack ?? e);
    try {
      if (sid) entry.images.push({ name: await shot(`${id}-fehler`), label: "Zustand beim Fehler" });
    } catch {
      /* no session */
    }
  }
  entry.ms = Date.now() - t0;
  results.push(entry);
  console.log(`${entry.ok ? "✓" : "✗"} ${id} ${title} (${entry.ms} ms)${entry.ok ? "" : `\n  ${entry.error}`}`);
}

const MODEL = readFileSync(join(ROOT, "examples/spec/gehaeuse.scad"), "utf8");

async function main() {
  if (!existsSync(BIN)) throw new Error(`${BIN} fehlt (cargo build --release -p cad-cli)`);
  // Project in a git repository: committed state = the example.
  const repo = join(work, "repo");
  mkdirSync(repo, { recursive: true });
  const file = join(repo, "gehaeuse.scad");
  writeFileSync(file, MODEL);
  git(repo, "init", "-q");
  git(repo, "add", "gehaeuse.scad");
  git(repo, "commit", "-q", "-m", "Gehäuse");
  await startDriver();

  let started = null;
  await test("T01", "App startet mit Datei, Szene erscheint (nativer Kernel)", async (check, e) => {
    started = await startApp([file]);
    const s = started.state;
    check(s.desktop === true, "Tauri-Plattform erkannt (TauriTransport, kein Worker)");
    check(s.fileName === "gehaeuse.scad", "Datei aus der Kommandozeile geöffnet");
    check(s.diagnostics.length === 0, "keine Diagnosen");
    const info = await execAsync("const done = arguments[arguments.length - 1]; window.__cad.appInfo().then(done, (e) => done({ error: String(e) }))");
    const disk = await exec("return window.__cad.disk()");
    check(disk.root === repo, `Projektordner ${disk.root}`);
    const bytes = await exec("return window.__cad.transportBytes()");
    check(bytes > 1000, `Meshes binär über den Kanal (${bytes} Bytes)`);
    metrics.startup = { wallMs: started.wallMs, firstSceneMs: s.metrics.firstSceneMs, appMs: info.started_ms, evalMs: s.metrics.evalMs };
    e.images.push({ name: await shot("t01-start"), label: "Start mit gehaeuse.scad" });
    e.images.push({ name: xshot("t01-fenster"), label: "Fenster unter Xvfb (X-Bildschirmfoto)" });
  });

  await test("T02", "Externe Änderung ohne ungespeicherte Änderungen: still neu geladen, Undo als eigener Schritt", async (check, e) => {
    const s0 = await state();
    // cursor somewhere in the middle (it must stay)
    await exec("window.__cad.setCursorLine(24, 0)");
    const changed = MODEL.replace("k: gehaeuse(w = 60);", "k: gehaeuse(w = 90);");
    check(changed !== MODEL, "Teständerung trifft");
    e.images.push({ name: await shot("t02-vorher"), label: "vorher (w = 60)" });
    const t0 = Date.now();
    writeFileSync(file, changed);
    const s1 = await waitFor("neu geladen", async () => {
      const s = await state();
      return s.notice && s.notice.includes("Extern geändert") ? s : null;
    });
    const text = await exec("return window.__cad.text()");
    check(text === changed, "Editor zeigt den neuen Inhalt");
    const s2 = await sceneReady(s1.sentVersion);
    metrics.externalToScene = Date.now() - t0;
    check(s2.shownVersion > s0.shownVersion, "Ansicht ausgewertet");
    check((await exec("return window.__cad.cursorLine()")) === 24, "Cursor bleibt in Zeile 24");
    check((await exec("return window.__cad.disk().dirty")).length === 0, "nicht als ungespeichert markiert");
    const b = await exec("return window.__cad.rootBounds()");
    check(Math.abs(b.max[0] - b.min[0] - 90) < 1.5 || b.max[0] > 85, `Modell breiter (Hülle x ${b.min[0]}…${b.max[0]})`);
    e.images.push({ name: await shot("t02-nachher"), label: "nach der externen Änderung (w = 90), ohne Zutun neu geladen" });
    await exec("window.__cad.undo()");
    check((await exec("return window.__cad.text()")) === MODEL, "Rückgängig stellt den alten Stand her (eigener Schritt)");
    await exec("window.__cad.redo()");
    check((await exec("return window.__cad.text()")) === changed, "Wiederholen");
    await sceneReady();
  });

  await test("T03", "Git-Markierungen am Rand (neu, geändert, gelöscht)", async (check, e) => {
    const marks = await waitFor("Markierungen", async () => {
      const m = await exec("return window.__cad.gitMarks()");
      return m && m.hunks && m.hunks.length ? m : null;
    }, 15_000);
    check(marks.lines.some((l) => l.kind === "modified"), "geänderte Zeile 21 (w = 90) blau markiert");
    // add a line and delete one in the buffer (typing), markers follow
    await exec("const c = window.__cad; const t = c.text().replace('module deckel() cube([10, 10, 2]);\\n', '').replace('grid h {', '// neu: Hilfsraster\\ngrid h {'); c.setText(t)");
    const m2 = await waitFor("Markierungen nach Tippen", async () => {
      const m = await exec("return window.__cad.gitMarks()");
      return m && m.lines.some((l) => l.kind === "added") && m.lines.some((l) => l.kind.startsWith("deleted")) ? m : null;
    }, 15_000);
    check(m2.lines.some((l) => l.kind === "added"), "neue Zeile grün");
    check(m2.lines.some((l) => l.kind.startsWith("deleted")), "gelöschte Zeile als Dreieck");
    const gutter = await exec("return [...document.querySelectorAll('.cm-git-gutter .cm-git-mark')].map(e => ({ cls: e.className, w: e.getBoundingClientRect().width, op: getComputedStyle(e).opacity, title: e.title }))");
    check(gutter.length >= 3, `${gutter.length} Markierungen im DOM`);
    check(gutter.every((g) => g.w <= 5.5), "schmaler Balken (≤ 5 px)");
    check(gutter.every((g) => Number(g.op) < 0.8), "geringe Deckkraft");
    check(gutter.some((g) => g.title.includes("vorher (HEAD)")), "Hover zeigt alte Zeilen");
    e.details = gutter;
    e.images.push({ name: await shot("t03-git"), label: "Git-Markierungen: geändert (blau), neu (grün), gelöscht (rotes Dreieck)" });
    // dark theme
    await exec("window.__cad.store.setState({ theme: 'dark' })");
    await sleep(300);
    e.images.push({ name: await shot("t03-git-dunkel"), label: "dasselbe im dunklen Thema" });
    await exec("window.__cad.store.setState({ theme: 'light' })");
  });

  await test("T04", "Externe Änderung bei ungespeicherten Änderungen: Hinweisleiste, Vergleichen, Meine behalten, Speichern ohne Schleife", async (check, e) => {
    const mine = await exec("return window.__cad.text()");
    check((await exec("return window.__cad.disk().dirty")).length === 1, "Puffer ist ungespeichert");
    const disk2 = MODEL.replace("k: gehaeuse(w = 60);", "k: gehaeuse(w = 70);");
    writeFileSync(file, disk2);
    await waitFor("Hinweisleiste", () => exists("[data-testid=external-bar]"));
    check((await exec("return window.__cad.text()")) === mine, "Editor behält den eigenen Stand");
    e.images.push({ name: await shot("t04-leiste"), label: "„Datei wurde extern geändert“ mit Neu laden / Meine behalten / Vergleichen" });
    await click("[data-testid=external-compare]");
    await waitFor("Vergleich", () => exists("[data-testid=diff-view]"));
    const rows = await exec("return [...document.querySelectorAll('[data-testid=diff-view] .dl')].map(e => e.className)");
    check(rows.some((r) => r.includes("add")) && rows.some((r) => r.includes("del")), "Vergleich zeigt + und − Zeilen");
    e.images.push({ name: await shot("t04-vergleich"), label: "Vergleichen: Festplatte (−) gegen Editor (+)" });
    await click("[data-testid=diff-keep]");
    check(!(await exists("[data-testid=external-bar]")), "Leiste weg nach „Meine behalten“");
    // save: own write must not come back as an external change
    await exec("window.__cad.focusEditor()");
    await click("button[title='Speichern (Strg+S)']");
    await waitFor("gespeichert", async () => readFileSync(file, "utf8") === mine || null, 10_000);
    await sleep(1500);
    const s = await state();
    check(!s.notice.includes("Extern geändert"), `keine Reload-Schleife (Meldung: ${s.notice})`);
    check(Object.keys(s.external).length === 0, "keine Hinweisleiste nach dem Speichern");
    check(s.dirty.length === 0, "gespeichert, nicht mehr markiert");
    // Neu laden path
    await exec("const c = window.__cad; c.setText(c.text() + '// lokal\\n')");
    writeFileSync(file, MODEL);
    await waitFor("Hinweisleiste 2", () => exists("[data-testid=external-bar]"));
    await click("[data-testid=external-reload]");
    check((await exec("return window.__cad.text()")) === MODEL, "„Neu laden“ übernimmt die Festplatte");
    await sceneReady();
  });

  await test("T05", "Gelöschte und umbenannte Datei", async (check, e) => {
    renameSync(file, file + ".bak");
    await waitFor("gelöscht-Leiste", async () => (await state()).external["gehaeuse.scad"] === "removed" || null);
    e.images.push({ name: await shot("t05-geloescht"), label: "Datei umbenannt/gelöscht: Inhalt bleibt, „Wieder speichern“" });
    await click("[data-testid=external-restore]");
    await waitFor("wieder angelegt", async () => existsSync(file) || null, 10_000);
    check(readFileSync(file, "utf8") === MODEL, "Speichern legt die Datei wieder an");
    rmSync(file + ".bak");
    await sleep(800);
    check(Object.keys((await state()).external).length === 0, "keine Leiste mehr");
  });

  await test("T06", "Agent-Ablauf: Datei per Shell ändern, `cad check`, UI folgt (Bildfolge)", async (check, e) => {
    const steps = [
      { label: "1. Agent ändert w = 60 → 75 und prüft mit `cad check`", edit: (t) => t.replace("k: gehaeuse(w = 60);", "k: gehaeuse(w = 75);") },
      { label: "2. Agent fügt einen zweiten Deckel hinzu", edit: (t) => t + "translate([0, 40, 0]) deckel();\n" },
      { label: "3. Agent setzt h über `cad set --param gehaeuse.h=30`", set: ["--param", "gehaeuse.h=30"] },
    ];
    metrics.agentSteps = [];
    for (const [i, st] of steps.entries()) {
      const before = await state();
      const t0 = Date.now();
      if (st.edit) {
        writeFileSync(file, st.edit(readFileSync(file, "utf8")));
        const c = cad("check", file, "--json");
        check(c.code === 0, `cad check Exit 0 (${c.out.slice(0, 80)})`);
      } else {
        const c = cad("set", file, ...st.set);
        check(c.code === 0, `cad set Exit 0 (${c.err.slice(0, 120)})`);
      }
      await waitFor(`Schritt ${i + 1}`, async () => {
        const s = await state();
        return s.sentVersion > before.sentVersion ? s : null;
      });
      await sceneReady();
      metrics.agentSteps.push({ step: i + 1, ms: Date.now() - t0 });
      check((await exec("return window.__cad.text()")) === readFileSync(file, "utf8"), `Editor = Datei nach Schritt ${i + 1}`);
      e.images.push({ name: await shot(`t06-schritt${i + 1}`), label: st.label });
    }
  });

  await test("T07", "`cad ui status` / `show` / `screenshot` gegen die laufende App", async (check, e) => {
    const st = cad("ui", "status", "--json");
    check(st.code === 0, `status Exit 0 ${st.err}`);
    const j = JSON.parse(st.out);
    const w = j.uis[0].windows[0];
    check(w.project.main === "gehaeuse.scad" && w.project.root === repo, "offene Datei und Ordner");
    check(typeof w.cursor.line === "number", "Cursorzeile");
    const sh = cad("ui", "show", file, "--line", "11", "--json");
    check(sh.code === 0, `show Exit 0 ${sh.err}`);
    check(JSON.parse(sh.out).line === 11, "Cursor in Zeile 11");
    const st2 = JSON.parse(cad("ui", "status", "--json").out).uis[0].windows[0];
    check(st2.cursor.line === 11, "status sieht Zeile 11");
    check(st2.context === "gehaeuse", `Kontextmodul gehaeuse (${st2.context})`);
    check(st2.selection.nodes.length > 0, `ausgewählter Knoten ${JSON.stringify(st2.selection.nodes[0])}`);
    check(Object.keys(st2.params).includes("w"), "Werte der Parameterleiste");
    check(Array.isArray(st2.diagnostics), "Diagnosen");
    const png = join(OUT, "img", "t07-cad-ui-screenshot.png");
    const sc = cad("ui", "screenshot", "-o", png, "--json");
    check(sc.code === 0, `screenshot Exit 0 ${sc.err}`);
    const buf = readFileSync(png);
    check(buf.subarray(1, 4).toString() === "PNG" && buf.length > 5000, `PNG ${buf.length} Bytes`);
    const txt = cad("ui", "status");
    e.details = { status_text: txt.out, status_json: st2 };
    e.images.push({ name: "t07-cad-ui-screenshot.png", label: "`cad ui screenshot`: die 3D-Ansicht, wie der Nutzer sie sieht" });
    e.images.push({ name: await shot("t07-ui-nach-show"), label: "nach `cad ui show gehaeuse.scad --line 11`" });
    await exec("window.__cad.setCursorLine(1, 0)");
    await sceneReady();
  });

  await test("T08", "Schnittansicht in Tauri (Kappe, Kontur, Ziehen, CLI-Aufruf)", async (check, e) => {
    await click("[data-testid=section-toggle]");
    await waitFor("Schnitt", async () => {
      const i = await exec("return window.__cad.sectionInfo()");
      return i && i.on && i.caps > 0 && i.contour > 0 ? i : null;
    });
    const info = await exec("return window.__cad.sectionInfo()");
    check(info.caps > 0, `${info.caps} Körper mit Kappe`);
    check(info.contour > 0, `${info.contour} Kontursegmente`);
    // cap pixels on the canvas
    const cap = await exec(`
      const c = document.querySelector('[data-testid=viewport-canvas]');
      const g = document.createElement('canvas'); g.width = c.width; g.height = c.height;
      const x = g.getContext('2d'); x.drawImage(c, 0, 0);
      const d = x.getImageData(0, 0, g.width, g.height).data; let n = 0;
      for (let i = 0; i < d.length; i += 4) if (d[i] > 150 && d[i] - d[i+1] > 60 && d[i] - d[i+2] > 80) n++;
      return { n, total: g.width * g.height };`);
    check(cap.n > 500, `Kappe sichtbar (${cap.n} rote Pixel)`);
    e.images.push({ name: await shot("t08-schnitt"), label: "Schnitt z in Tauri: gefüllte, schraffierte Kappe und Kontur" });
    const fps = await execAsync("const done = arguments[arguments.length - 1]; window.__cad.sectionBenchmark(120).then(done)");
    metrics.sectionFpsTauri = fps;
    check(fps.fps > 5, `Ebene verschieben: ${fps.fps.toFixed(1)} fps (WebKitGTK, Software-GL)`);
    await click("[data-testid=section-copy]");
    const cli = (await state()).lastCli;
    check(cli && cli.startsWith("cad render ") && cli.includes("--section z="), cli);
    e.details = { cli };
    await click("[data-testid=section-toggle]");
  });

  await test("T09", "Watchdog in Tauri: hängender Kernel → Engine-Prozess ersetzt, Knoten gesperrt", async (check, e) => {
    // (a) the engine hangs while idle: replaced, the next edit evaluates normally
    const before = await state();
    await exec("window.__cad.debugHang(9000)");
    const s = await waitFor("Neustart", async () => {
      const x = await state();
      return x.restarts > before.restarts ? x : null;
    }, 30_000);
    check(s.engineError && s.engineError.includes("neu gestartet"), s.engineError);
    await exec("const c = window.__cad; c.setText(c.text() + '\\ncube(3);\\n')");
    await sceneReady();
    check((await state()).shownComplete, "nächste Änderung wird normal ausgewertet");
    // (b) the engine hangs during a slow kernel job: its node is blocked, the source evaluates again
    const slow = "difference(fillet = 0.4) {\n    cube(40);\n    for (i = [0 : 7]) for (j = [0 : 7]) translate([5 * i + 2.5, 5 * j + 2.5, -1]) cylinder(h = 42, r = 1.2);\n}\n";
    const b2 = await state();
    await exec("window.__cad.setText(arguments[0])", slow);
    await waitFor("Kernel-Job läuft", async () => {
      const x = await state();
      return x.sentVersion > b2.sentVersion && x.progress && x.progress.total > 0 ? x : null;
    }, 20_000);
    await exec("window.__cad.debugHang(9000)");
    const s2 = await waitFor("Neustart mit Sperre", async () => {
      const x = await state();
      return x.restarts > b2.restarts ? x : null;
    }, 30_000);
    check(s2.engineError.includes("Knoten gesperrt"), s2.engineError);
    check(s2.blockedKeys.length >= 1, `${s2.blockedKeys.length} gesperrter Schlüssel`);
    const s3 = await sceneReady();
    const errs = await exec("return window.__cad.store.getState().shown.nodes.filter(n => n.status === 'error').length");
    check(errs >= 1, `gesperrter Knoten meldet Fehler (${errs})`);
    e.images.push({ name: await shot("t09-watchdog"), label: "Watchdog: Engine-Prozess ersetzt, hängender Knoten gesperrt (Zeile rot)" });
    check(s3.shownComplete, "Szene nach dem Neustart");
    writeFileSync(file, readFileSync(file, "utf8"));
    await exec("window.__cad.setText(arguments[0])", readFileSync(file, "utf8"));
    await sceneReady();
  });

  await test("T10", "Ordner als Projekt, Autospeichern", async (check, e) => {
    await stopApp();
    const proj = join(work, "projekt");
    mkdirSync(join(proj, "lib"), { recursive: true });
    for (const f of ["gehaeuse.scad", "konstanten.scad", "lib/teile.scad"]) writeFileSync(join(proj, f), readFileSync(join(ROOT, "examples/projekt-gehaeuse", f)));
    const st = await startApp([proj]);
    check(st.state.fileName === "gehaeuse.scad", `Hauptdatei ${st.state.fileName}`);
    const info = await exec("return window.__cad.projectInfo()");
    check(info.paths.length === 3 && info.folder, `3 Dateien (${info.paths.join(", ")})`);
    // external change of an included file updates the scene
    const before = await state();
    const konst = join(proj, "konstanten.scad");
    writeFileSync(konst, readFileSync(konst, "utf8").replace(/abstand\s*=\s*\d+/, "abstand = 25"));
    await waitFor("include neu ausgewertet", async () => ((await state()).sentVersion > before.sentVersion ? true : null));
    await sceneReady();
    check((await exec("return window.__cad.projectInfo().files['konstanten.scad']")).includes("abstand = 25"), "eingebundene Datei neu geladen");
    // autosave
    await exec("const s = window.__cad.store; s.setState({ settings: { ...s.getState().settings, autoSave: true, autoSaveMs: 300 } })");
    await exec("const c = window.__cad; c.setText(c.text() + '// auto\\n')");
    await waitFor("automatisch gespeichert", async () => readFileSync(join(proj, "gehaeuse.scad"), "utf8").endsWith("// auto\n") || null, 10_000);
    check((await state()).dirty.length === 0, "nach dem Autospeichern sauber");
    e.images.push({ name: await shot("t10-ordner"), label: "Ordner als Projekt (3 Dateien), Autospeichern an" });
  });

  await test("T11", "Tastendruck → Szene nativ (Messung)", async (check) => {
    // after "w = 60" in `k: gehaeuse(w = 60);`: Backspace / digit, always a valid model
    await exec("window.__cad.setText(arguments[0])", MODEL);
    await sceneReady();
    await exec("window.__cad.setCursorLine(21, 18)");
    const keys = [];
    const before = (await state()).metrics.editToScene.length;
    for (let i = 0; i < 8; i++) {
      const id = await el(".cm-content");
      await wd("POST", `/session/${sid}/element/${id}/value`, { text: i % 2 ? "0" : "\uE003" });
      await sleep(250);
      await sceneReady();
    }
    const s = await state();
    keys.push(...s.metrics.editToScene.slice(before));
    metrics.keystrokeToSceneNative = keys;
    check(keys.length > 0, `${keys.length} Messungen`);
  });

  await stopApp();
}

let fatal = null;
try {
  await main();
} catch (e) {
  fatal = String(e.stack ?? e);
  console.error(fatal);
}
writeFileSync(join(OUT, "tauri-e2e.json"), JSON.stringify({ bin: BIN, results, metrics, fatal, at: new Date().toISOString() }, null, 2));
const failed = results.filter((r) => !r.ok).length + (fatal ? 1 : 0);
console.log(`\n${results.length - results.filter((r) => !r.ok).length}/${results.length} Tauri-E2E bestanden${fatal ? " (Abbruch)" : ""}`);
cleanup();
rmSync(work, { recursive: true, force: true });
process.exit(failed ? 1 : 0);
