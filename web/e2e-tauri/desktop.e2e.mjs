#!/usr/bin/env node
// End-to-end tests of the desktop app (Tauri 2) with tauri-driver + WebKitWebDriver, under
// Xvfb when there is no display. A small W3C WebDriver client (fetch) drives the real app
// binary (`mcad`, feature gui) with the real native engine process.
//
//   node web/e2e-tauri/desktop.e2e.mjs [--bin target/release/mcad] [--only T03] [--out reports/tmp/report11/tauri]
//
// Needs: tauri-driver (cargo install tauri-driver --locked), WebKitWebDriver
// (webkit2gtk-driver), Xvfb, git. Screenshots, measurements and results go to
// reports/tmp/report9/ (test report 9, tools/report/report9.mjs) or `--out`. T12–T14: colours,
// filaments and the dark theme (test report 11). T16: sketch editor (test report 18,
// `--only T16 --out reports/tmp/report18/tauri`). T17: measure mode (test report 19,
// `--only T17 --out reports/tmp/report19/tauri`). T18: UI overhaul – bars at 1280/1440/1920 px in
// both themes, menus, view cube, side panel (test report 20, `--out reports/tmp/report20/tauri`).
// T19: collision asserts – keep-out volume, violation, framing, export question (Plan 21 C,
// `--only T19 --out reports/tmp/report21/phase-c/tauri`).

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
const BIN = resolve(opt("--bin", join(ROOT, "target/release/mcad")));
const ONLY = opt("--only", null);
const OUT = resolve(opt("--out", join(ROOT, "reports/tmp/report9")));
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

/** Top-level X windows: title (_NET_WM_NAME, UTF-8) and WM_CLASS (libX11 over ctypes; no x11-utils needed). */
function xwindows() {
  const py = `
import ctypes, ctypes.util, json
x = ctypes.cdll.LoadLibrary(ctypes.util.find_library("X11"))
class H(ctypes.Structure): _fields_ = [("n", ctypes.c_char_p), ("c", ctypes.c_char_p)]
x.XOpenDisplay.restype = ctypes.c_void_p
x.XDefaultRootWindow.restype = ctypes.c_ulong; x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong] + [ctypes.POINTER(ctypes.c_ulong)] * 2 + [ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
x.XGetClassHint.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(H)]
x.XInternAtom.restype = ctypes.c_ulong; x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x.XGetWindowProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long, ctypes.c_int, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ubyte))]
d = x.XOpenDisplay(None)
NET = x.XInternAtom(d, b"_NET_WM_NAME", 0); UTF8 = x.XInternAtom(d, b"UTF8_STRING", 0)
def name(w):
    t, f, n, a = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong(); p = ctypes.POINTER(ctypes.c_ubyte)()
    if x.XGetWindowProperty(d, w, NET, 0, 1024, 0, UTF8, ctypes.byref(t), ctypes.byref(f), ctypes.byref(n), ctypes.byref(a), ctypes.byref(p)) == 0 and n.value:
        return bytes(p[:n.value]).decode(errors="replace")
    return ""
r, pa = ctypes.c_ulong(), ctypes.c_ulong(); k = ctypes.POINTER(ctypes.c_ulong)(); n = ctypes.c_uint()
x.XQueryTree(d, x.XDefaultRootWindow(d), ctypes.byref(r), ctypes.byref(pa), ctypes.byref(k), ctypes.byref(n))
out = []
for i in range(n.value):
    h = H()
    if x.XGetClassHint(d, k[i], ctypes.byref(h)):
        out.append({"name": name(k[i]), "res_name": h.n.decode(), "res_class": h.c.decode()})
print(json.dumps(out, ensure_ascii=False))
`;
  const r = spawnSync("python3", ["-I", "-c", py], { env: { ...process.env, DISPLAY: display }, encoding: "utf8" });
  try {
    return JSON.parse(r.stdout);
  } catch {
    return [];
  }
}

function mcad(...args) {
  // MCAD_NO_DAEMON: no CLI daemon (it would outlive the test run; tests/daemon.rs covers it).
  const r = spawnSync(BIN, args, { env: { ...process.env, MCAD_IPC_DIR: ipcDir, DISPLAY: display, MCAD_NO_DAEMON: "1" }, encoding: "utf8" });
  return { code: r.status, out: r.stdout, err: r.stderr };
}

function git(dir, ...args) {
  const r = spawnSync("git", ["-C", dir, "-c", "user.name=Test", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", ...args], { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr}`);
}

// ---------------------------------------------------------------------------------- app

let driver = null;

async function startDriver(extraEnv = {}) {
  if (!display) {
    display = ":97";
    const x = spawn("Xvfb", [display, "-screen", "0", "1600x1000x24"], { stdio: "ignore" });
    children.push(x);
    await sleep(800);
  }
  const td = spawn("tauri-driver", ["--port", String(PORT), "--native-port", String(PORT + 1)], {
    env: { ...process.env, DISPLAY: display, MCAD_IPC_DIR: ipcDir, MCAD_WATCHDOG_MS: "3000", ...extraEnv },
    stdio: ["ignore", "inherit", "inherit"],
  });
  children.push(td);
  driver = td;
  await waitFor("tauri-driver", async () => (await fetch(`${WD}/status`).then((r) => r.ok).catch(() => false)), 20_000);
}

async function stopDriver() {
  if (!driver) return;
  driver.kill("SIGTERM");
  await new Promise((r) => (driver.exitCode !== null ? r() : driver.once("exit", r)));
  driver = null;
  await sleep(500);
}

/** Selects an option of a React-controlled <select> (native setter + change event). */
async function selectOption(css, value) {
  await exec(
    "const el = document.querySelector(arguments[0]); const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set; set.call(el, arguments[1]); el.dispatchEvent(new Event('change', { bubbles: true }));",
    css,
    value,
  );
}

/** Runs an async page function `__cad.<name>(...args)` and returns its result. */
async function hookAsync(name, ...args) {
  return execAsync(
    "const done = arguments[arguments.length - 1]; const a = arguments[0]; window.__cad[a.name](...a.args).then(done, (e) => done({ error: String(e) }));",
    { name, args },
  );
}

async function startApp(args) {
  const t0 = Date.now();
  const v = await wd("POST", "/session", { capabilities: { alwaysMatch: { "tauri:options": { application: BIN, args } } } });
  sid = v.sessionId;
  await waitFor("UI", async () => (await exec("return !!window.__cad")) || null, 60_000);
  // The checks use the German names of the UI (the webview's language depends on the system).
  await exec("window.__cad.setLangPref('de')");
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

const MODEL = readFileSync(join(ROOT, "examples/spec/gehaeuse.mcad"), "utf8");

async function main() {
  if (!existsSync(BIN)) throw new Error(`${BIN} fehlt (cargo build --release -p cad-cli)`);
  // Project in a git repository: committed state = the example.
  const repo = join(work, "repo");
  mkdirSync(repo, { recursive: true });
  const file = join(repo, "gehaeuse.mcad");
  writeFileSync(file, MODEL);
  git(repo, "init", "-q");
  git(repo, "add", "gehaeuse.mcad");
  git(repo, "commit", "-q", "-m", "Gehäuse");
  await startDriver();

  let started = null;
  await test("T01", "App startet mit Datei, Szene erscheint (nativer Kernel)", async (check, e) => {
    started = await startApp([file]);
    const s = started.state;
    check(s.desktop === true, "Tauri-Plattform erkannt (TauriTransport, kein Worker)");
    check(s.fileName === "gehaeuse.mcad", "Datei aus der Kommandozeile geöffnet");
    check(s.diagnostics.length === 0, "keine Diagnosen");
    const info = await execAsync("const done = arguments[arguments.length - 1]; window.__cad.appInfo().then(done, (e) => done({ error: String(e) }))");
    const disk = await exec("return window.__cad.disk()");
    check(disk.root === repo, `Projektordner ${disk.root}`);
    const bytes = await exec("return window.__cad.transportBytes()");
    check(bytes > 1000, `Meshes binär über den Kanal (${bytes} Bytes)`);
    metrics.startup = { wallMs: started.wallMs, firstSceneMs: s.metrics.firstSceneMs, appMs: info.started_ms, evalMs: s.metrics.evalMs };
    e.images.push({ name: await shot("t01-start"), label: "Start mit gehaeuse.mcad" });
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
    await click("[data-testid=external-bar] button[aria-label=Vergleichen]");
    await waitFor("Vergleich", () => exists("[data-testid=diff-view]"));
    const rows = await exec("return [...document.querySelectorAll('[data-testid=diff-view] .dl')].map(e => e.className)");
    check(rows.some((r) => r.includes("add")) && rows.some((r) => r.includes("del")), "Vergleich zeigt + und − Zeilen");
    e.images.push({ name: await shot("t04-vergleich"), label: "Vergleichen: Festplatte (−) gegen Editor (+)" });
    await click("[role=dialog] button[aria-label='Meine behalten']");
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
    await click("[data-testid=external-bar] button[aria-label='Neu laden']");
    check((await exec("return window.__cad.text()")) === MODEL, "„Neu laden“ übernimmt die Festplatte");
    await sceneReady();
  });

  await test("T05", "Gelöschte und umbenannte Datei", async (check, e) => {
    renameSync(file, file + ".bak");
    await waitFor("gelöscht-Leiste", async () => (await state()).external["gehaeuse.mcad"] === "removed" || null);
    e.images.push({ name: await shot("t05-geloescht"), label: "Datei umbenannt/gelöscht: Inhalt bleibt, „Wieder speichern“" });
    await click("[data-testid=external-bar] button[aria-label='Wieder speichern']");
    await waitFor("wieder angelegt", async () => existsSync(file) || null, 10_000);
    check(readFileSync(file, "utf8") === MODEL, "Speichern legt die Datei wieder an");
    rmSync(file + ".bak");
    await sleep(800);
    check(Object.keys((await state()).external).length === 0, "keine Leiste mehr");
  });

  await test("T06", "Agent-Ablauf: Datei per Shell ändern, `mcad check`, UI folgt (Bildfolge)", async (check, e) => {
    const steps = [
      { label: "1. Agent ändert w = 60 → 75 und prüft mit `mcad check`", edit: (t) => t.replace("k: gehaeuse(w = 60);", "k: gehaeuse(w = 75);") },
      { label: "2. Agent fügt einen zweiten Deckel hinzu", edit: (t) => t + "translate([0, 40, 0]) deckel();\n" },
      { label: "3. Agent setzt h über `mcad set --param gehaeuse.h=30`", set: ["--param", "gehaeuse.h=30"] },
    ];
    metrics.agentSteps = [];
    for (const [i, st] of steps.entries()) {
      const before = await state();
      const t0 = Date.now();
      if (st.edit) {
        writeFileSync(file, st.edit(readFileSync(file, "utf8")));
        const c = mcad("check", file, "--json");
        check(c.code === 0 && JSON.parse(c.out).errors === 0, "`mcad check --json`: Exit 0, 0 Fehler");
      } else {
        const c = mcad("set", file, ...st.set);
        check(c.code === 0, `\`mcad set ${st.set.join(" ")}\`: Exit 0${c.err ? ` (${c.err.slice(0, 120)})` : ""}`);
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

  await test("T07", "`mcad ui status` / `show` / `screenshot` gegen die laufende App", async (check, e) => {
    const st = mcad("ui", "status", "--json");
    check(st.code === 0, `status Exit 0 ${st.err}`);
    const j = JSON.parse(st.out);
    const w = j.uis[0].windows[0];
    check(w.project.main === "gehaeuse.mcad" && w.project.root === repo, "offene Datei und Ordner");
    check(typeof w.cursor.line === "number", "Cursorzeile");
    const sh = mcad("ui", "show", file, "--line", "11", "--json");
    check(sh.code === 0, `show Exit 0 ${sh.err}`);
    check(JSON.parse(sh.out).line === 11, "Cursor in Zeile 11");
    const st2 = JSON.parse(mcad("ui", "status", "--json").out).uis[0].windows[0];
    check(st2.cursor.line === 11, "status sieht Zeile 11");
    check(st2.context === "gehaeuse", `Kontextmodul gehaeuse (${st2.context})`);
    check(st2.selection.nodes.length > 0, `ausgewählter Knoten ${JSON.stringify(st2.selection.nodes[0])}`);
    check(Object.keys(st2.params).includes("w"), "Werte der Parameterleiste");
    check(Array.isArray(st2.diagnostics), "Diagnosen");
    const png = join(OUT, "img", "t07-cad-ui-screenshot.png");
    const sc = mcad("ui", "screenshot", "-o", png, "--json");
    check(sc.code === 0, `screenshot Exit 0 ${sc.err}`);
    const buf = readFileSync(png);
    check(buf.subarray(1, 4).toString() === "PNG" && buf.length > 5000, `PNG ${buf.length} Bytes`);
    const txt = mcad("ui", "status");
    e.details = { status_text: txt.out, status_json: st2 };
    e.images.push({ name: "t07-cad-ui-screenshot.png", label: "`mcad ui screenshot`: die 3D-Ansicht, wie der Nutzer sie sieht" });
    e.images.push({ name: await shot("t07-ui-nach-show"), label: "nach `mcad ui show gehaeuse.mcad --line 11`" });
    const shm = mcad("ui", "show", file, "--module", "deckel", "--json");
    check(shm.code === 0 && JSON.parse(shm.out).module === "deckel", "show --module deckel: Kontext deckel");
    const shx = mcad("ui", "show", file, "--module", "gibtsnicht", "--json");
    check(shx.code === 1, `unbekanntes Modul: Exit ${shx.code} („${JSON.parse(shx.out).error}“)`);
    await exec("window.__cad.setCursorLine(1, 0)");
    await sceneReady();
  });

  await test("T08", "Schnittansicht in Tauri (Kappe, Kontur, Ziehen, CLI-Aufruf)", async (check, e) => {
    await click("[role=radiogroup][aria-label=Modus] [role=radio][aria-label=Schnitt]");
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
    await click("[role=toolbar][aria-label=Schnitt] button[aria-label='Als mcad render … kopieren']");
    const cli = (await state()).lastCli;
    check(cli && cli.startsWith("mcad render ") && cli.includes("--section z="), cli);
    e.details = { cli };
    await click("[role=radiogroup][aria-label=Modus] [role=radio][aria-label=Modell]");
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
    for (const f of ["gehaeuse.mcad", "konstanten.mcad", "lib/teile.mcad"]) writeFileSync(join(proj, f), readFileSync(join(ROOT, "examples/projekt-gehaeuse", f)));
    const st = await startApp([proj]);
    check(st.state.fileName === "gehaeuse.mcad", `Hauptdatei ${st.state.fileName}`);
    const info = await exec("return window.__cad.projectInfo()");
    check(info.paths.length === 3 && info.folder, `3 Dateien (${info.paths.join(", ")})`);
    // external change of an included file updates the scene
    const before = await state();
    const konst = join(proj, "konstanten.mcad");
    writeFileSync(konst, readFileSync(konst, "utf8").replace(/abstand\s*=\s*\d+/, "abstand = 25"));
    await waitFor("include neu ausgewertet", async () => ((await state()).sentVersion > before.sentVersion ? true : null));
    await sceneReady();
    check((await exec("return window.__cad.projectInfo().files['konstanten.mcad']")).includes("abstand = 25"), "eingebundene Datei neu geladen");
    // autosave
    await exec("const s = window.__cad.store; s.setState({ settings: { ...s.getState().settings, autoSave: true, autoSaveMs: 300 } })");
    await exec("const c = window.__cad; c.setText(c.text() + '// auto\\n')");
    await waitFor("automatisch gespeichert", async () => readFileSync(join(proj, "gehaeuse.mcad"), "utf8").endsWith("// auto\n") || null, 10_000);
    check((await state()).dirty.length === 0, "nach dem Autospeichern sauber");
    e.images.push({ name: await shot("t10-ordner"), label: "Ordner als Projekt (3 Dateien), Autospeichern an" });
  });

  await test("T11", "Tastendruck → Szene nativ (Messung)", async (check) => {
    // after "w = 60" in `k: gehaeuse(w = 60);`: Backspace / digit, always a valid model
    await exec("window.__cad.setText(arguments[0])", MODEL);
    await sceneReady();
    await exec("window.__cad.setCursorLine(21, 18)");
    const keys = [];
    const m0 = (await state()).metrics;
    const before = m0.editToScene.length;
    const beforeEval = m0.evalMs.length;
    for (let i = 0; i < 8; i++) {
      const id = await el(".cm-content");
      await wd("POST", `/session/${sid}/element/${id}/value`, { text: i % 2 ? "0" : "\uE003" });
      await sleep(250);
      await sceneReady();
    }
    const s = await state();
    keys.push(...s.metrics.editToScene.slice(before));
    metrics.keystrokeToSceneNative = keys;
    metrics.keystrokeEvalNative = s.metrics.evalMs.slice(beforeEval);
    // kernel-heavy edit: 4×4 holes with fillet, radius changed (all fillets recompute)
    const heavy = (r) => `difference(fillet = 0.4) {\n    cube(40);\n    for (i = [0 : 3]) for (j = [0 : 3]) translate([10 * i + 5, 10 * j + 5, -1]) cylinder(h = 42, r = ${r});\n}\n`;
    metrics.heavyEvalNative = [];
    for (const r of [2, 2.5, 3]) {
      const v0 = (await state()).sentVersion;
      await exec("window.__cad.setText(arguments[0])", heavy(r));
      await waitFor("schwere Szene", async () => ((await state()).sentVersion > v0 ? true : null));
      const sc = await sceneReady();
      metrics.heavyEvalNative.push(sc.metrics.evalMs[sc.metrics.evalMs.length - 1]);
    }
    check(keys.length > 0, `${keys.length} Messungen`);
  });

  // ------------------------------------------------------------ colours, filaments, theme (report 11)

  const MEHR = readFileSync(join(ROOT, "examples/spec/mehrfarbig.mcad"), "utf8");
  const ensureApp = async () => {
    if (!sid) await startApp([file]);
  };

  await test("T12", "Farben & Filamente in Tauri: Liste, Slot ändern, 3MF mehrfarbig (nativer Kernel)", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('light')");
    await click("[role=tablist][aria-label=Seitenpanel] [role=tab][aria-label=Farben]");
    await exec("window.__cad.setText(arguments[0])", MEHR);
    await sceneReady();
    await waitFor("Farbliste", async () => (await exists("[data-testid=colors-panel]")) || null);
    const regions = await waitFor("Farbregionen", async () => {
      const s = await state();
      return s.regions && s.regions.version === s.shownVersion ? s : null;
    });
    const labels = regions.colors.map((c) => `${c.label}:${c.filament}`).join(", ");
    check(labels === "white:1, teal:2, red:3, blue:4, yellow:5", `Farben ${labels}`);
    for (const set of regions.regions.items) {
      const sum = set.regions.reduce((a, r) => a + r.volume, 0);
      check(Math.abs(sum - set.total) < 1e-6 * sum, `${set.part}: Summe ${sum.toFixed(3)} = ${set.total.toFixed(3)} mm³`);
    }
    e.images.push({ name: await shot("t12-farben"), label: "Farbliste in der Desktop-App (helles Thema)" });
    // slot change writes the code
    await selectOption("select[aria-label='Filament für teal']", "5");
    await waitFor("filament = 5 im Text", async () => ((await exec("return window.__cad.text()")).includes('color("teal", filament = 5)') ? true : null));
    await sceneReady();
    const s2 = await state();
    const teal = s2.colors.find((c) => c.label === "teal");
    check(teal.filament === 5 && teal.explicit, "teal fest auf Slot 5");
    check(s2.dirty.length >= 1, "Datei als geändert markiert");
    // multi-colour 3MF over the native engine
    const sum = await hookAsync("exportSummary", "3mf", undefined, ["schild", "figur"]);
    // Bericht 12: no colour group (Bambu Studio keeps the slots without its colour dialog)
    check(!sum.colorGroup && sum.mColors === 0 && sum.items === 2 && sum.components === 6 && sum.modelSettings, `3MF: ${JSON.stringify(sum)}`);
    check(sum.extruders.includes(5) && sum.extruders.includes(3), `extruder je Teil ${sum.extruders.join(",")}`);
    const zip = await hookAsync("exportSummary", "stl", true, ["schild", "figur"]);
    check(zip.files.length === 6, `STL je Farbe: ${zip.files.join(", ")}`);
    e.details = { colors: s2.colors, export3mf: sum, stlZip: zip };
    e.images.push({ name: await shot("t12-slot"), label: "teal auf Slot 5: Code geändert" });
  });

  await test("T13", "Dunkles Thema in Tauri: Kontrast (computed styles), Ansicht, Editor", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('dark')");
    await waitFor("dunkel", async () => ((await exec("return document.documentElement.dataset.theme")) === "dark" ? true : null));
    await sleep(400);
    const pairs = {
      Text: ["body", "body"],
      Editor: [".cm-content", ".editor .cm-editor"],
      Statusleiste: ["[data-testid=status-progress]", ".statusbar"],
      Farbliste: ["[data-testid=colors-panel]", ".sidepanel"],
      Kopfleiste: [".hd .ui-btn--quiet", ".hd"],
      "Speichern (primär)": ["[data-testid=save]", "[data-testid=save]"],
      Werkzeugleiste: [".vtoolbar .ui-seg-item.is-on", ".vtoolbar .ui-seg-item.is-on"],
    };
    const ratios = {};
    for (const [k, [a, b]] of Object.entries(pairs)) {
      ratios[k] = await exec("return window.__cad.contrastOf(arguments[0], arguments[1])", a, b);
      check(ratios[k] >= 4.5, `${k}: Kontrast ${Number(ratios[k]).toFixed(2)} ≥ 4,5`);
    }
    const swatch = await exec("return getComputedStyle(document.querySelector('[data-testid=color-swatch-teal]')).backgroundColor");
    check(swatch.replace(/ /g, "") === "rgb(48,126,128)", `teal dunkle Variante (${swatch})`);
    const v = (await state()).view;
    check(v.faceColors > 0, "Farbe je Fläche in der Ansicht");
    e.details = { contrast: ratios };
    e.images.push({ name: await shot("t13-dunkel"), label: "dunkles Thema in der Desktop-App" });
  });

  await test("T15", "Bericht 12 in Tauri: Farbregionen je Instanz, filament()-Tabelle, Objekt „ohne Bauteil“, 3MF ohne Farbgruppe", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('light'); window.__cad.store.setState({ sideOpen: true, sideTab: 'colors', showRegions: true })");
    const pal2 = `// pal2\n${readFileSync(join(ROOT, "tests/farben-pruefung/cases/pal2.scad"), "utf8")}`;
    await exec("window.__cad.setText(arguments[0])", pal2);
    await sceneReady();
    const v = await waitFor("drei Farbregionen", async () => {
      const s = await state();
      return s.view && s.view.regionsDrawn === 3 ? s : null;
    });
    check(v.colors.map((c) => c.label).join(",") === "red,blue,green", `Farben ${v.colors.map((c) => c.label).join(", ")}`);
    e.images.push({ name: await shot("t15-pal2"), label: "pal2.scad: drei gleiche Würfel in drei Farben (Farbregionen)" });
    // slot of a parameter colour: filament("red", 4); at the top, not filament = 4 at color(c)
    const slotMod = `// slot_mod\n${readFileSync(join(ROOT, "tests/farben-pruefung/cases/slot_mod.scad"), "utf8")}`;
    await exec("window.__cad.setText(arguments[0])", slotMod);
    await sceneReady();
    await waitFor("Farbliste", async () => (await exists("select[aria-label='Filament für red']")) || null);
    await selectOption("select[aria-label='Filament für red']", "4");
    await waitFor("filament(\"red\", 4)", async () => ((await exec("return window.__cad.text()")).includes('filament("red", 4);') ? true : null));
    const t = await exec("return window.__cad.text()");
    check(!t.includes("filament = 4"), "kein filament = 4 an color(c)");
    await sceneReady();
    const s2 = await state();
    const slots = s2.colors.map((c) => `${c.label}:${c.filament}`).join(", ");
    check(slots === "red:4, blue:1, green:2", `Slots ${slots}`);
    // material outside the parts: its own object, the same file as mcad export
    await exec("window.__cad.setText(arguments[0])", '// F4\nmodule a() color("white") cube([20, 10, 2]);\nteil: a();\ntranslate([0, 20, 0]) color("yellow") cube(5);\n');
    await sceneReady();
    const sum = await hookAsync("exportSummary", "3mf", true);
    check(sum.objects.join(",") === "teil,ohne Bauteil", `Objekte ${sum.objects.join(", ")}`);
    check(!sum.colorGroup && sum.mColors === 0, "keine Farbgruppe");
    check(sum.notes.some((n) => n.includes("ohne Farbdialog")), `Hinweise ${sum.notes.join("; ")}`);
    e.details = { slots, export3mf: sum };
  });

  await test("T16", "Skizzen-Editor in Tauri: Zeichenmodus, Rechteck auf Rasterpunkten, Snap-Anzeige, Undo (TauriTransport, nativer Kernel)", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('light'); window.__cad.store.setState({ sideOpen: false })");
    const SRC = "grid g {\n    x: a = 0, b = 10, c = 30;\n    y: u = 0, v = 15;\n}\nlinear_extrude(3) sketch {\n}\n";
    const s0 = await state();
    await exec("window.__cad.setText(arguments[0])", SRC);
    await sceneReady(s0.sentVersion + 1);
    await exec("window.__cad.setCursorLine(6, 0)");
    await sceneReady();
    const sk = () => exec("return window.__cad.sketch()");
    await click("[role=radiogroup][aria-label=Modus] [role=radio][aria-label=Skizze]");
    await waitFor("Zeichenmodus", async () => {
      const s = await sk();
      return s && s.raster && s.answers > 0 ? s : null;
    });
    const cam = await exec("return window.__cad.camera()");
    check(cam.dir[2] < -0.9999, `Blick senkrecht auf die Skizzenebene (z = ${cam.dir[2].toFixed(5)})`);
    await click("[role=toolbar][aria-label=Skizze] button[aria-label=Rechteck]");
    await waitFor("Werkzeug Rechteck", async () => ((await sk())?.tool === "rect" ? true : null));
    // pointer events on the canvas (the view's listeners: capture on the container, move on the canvas)
    const fire = (type, x, y) =>
      exec(
        "const c = document.querySelector('[data-testid=viewport-canvas]'); c.dispatchEvent(new PointerEvent(arguments[0], { clientX: arguments[1], clientY: arguments[2], bubbles: true, cancelable: true, composed: true, button: 0, buttons: arguments[0] === 'pointerdown' ? 1 : 0, pointerId: 1, pointerType: 'mouse', isPrimary: true })); return true;",
        type,
        x,
        y,
      );
    const at = (x, y) => exec("return window.__cad.sketchScreen(arguments[0], arguments[1])", x, y);
    const hover = async (x, y, kind) => {
      const p = await at(x, y);
      const n = (await sk()).answers;
      await fire("pointermove", p.x, p.y);
      return waitFor(`Snap ${kind}`, async () => {
        const s = await sk();
        return s.answers > n && s.snap && s.snap.kind === kind ? s : null;
      });
    };
    const press = async (x, y) => {
      const p = await at(x, y);
      const n = (await sk()).answers;
      await fire("pointerdown", p.x, p.y);
      await fire("pointerup", p.x, p.y);
      return waitFor("Klick beantwortet", async () => {
        const s = await sk();
        return s.answers > n ? s : null;
      });
    };
    const h = await hover(10.2, 0.2, "grid");
    check(h.snap.text === "g[b, u]", `Snap auf Rasterpunkt: ${h.snap.text}`);
    const label = await exec("const l = document.querySelector('[data-testid=sketch-snap-label]'); return l && !l.hidden ? l.textContent : null");
    check(label === "g[b, u]", `Snap-Beschriftung sichtbar: ${label}`);
    await press(10.2, 0.2);
    const h2 = await hover(30, 15, "grid");
    check(h2.preview === 1, "Vorschau des Rechtecks");
    e.images.push({ name: await shot("t16-vorschau"), label: "Rechteck: zweite Ecke g[c, v], Vorschau und Snap-Anzeige" });
    await press(30, 15);
    const s1 = await waitFor("Code geschrieben", async () => {
      const s = await sk();
      return s.actions >= 1 ? s : null;
    });
    const text = await exec("return window.__cad.text()");
    check(text.includes("    start(g[b, u]);\n    line(to = g[c, u]);\n    line(to = g[c, v]);\n    line(to = g[b, v]);\n    close();\n}"), "Code im sketch-Block mit Rasterreferenzen");
    const s2 = await sceneReady();
    check(s2.diagnostics.filter((d) => d.severity === "error").length === 0, "keine Fehler, Körper nativ berechnet");
    e.images.push({ name: await shot("t16-rechteck"), label: "Rechteck auf Rasterpunkten, Code links" });
    await exec("window.__cad.undo()");
    const back = await waitFor("Undo", async () => ((await exec("return window.__cad.text()")) === SRC ? true : null));
    check(back === true, "Undo nimmt die Aktion als einen Schritt zurück");
    await click("[role=toolbar][aria-label=Skizze] button[aria-label=Fertig]");
    check((await sk()) === null, "Zeichenmodus beendet");
    const ms = s1.rtt.filter((r) => !r.click).map((r) => r.ms).sort((a, b) => a - b);
    metrics.sketchTauri = { hoverMedianMs: ms[Math.floor(ms.length / 2)] ?? null, answers: s1.answers };
    e.details = { text, rtt: s1.rtt };
  });

  await test("T17", "Messen in Tauri: Radius, Abstand paralleler Flächen, Radius im Schnitt (TauriTransport, nativer Kernel)", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('light'); window.__cad.store.setState({ sideOpen: false })");
    const SRC = "cube([20, 10, 8]);\ntranslate([30, 5, 0]) cylinder(h = 12, r = 5);\n";
    const s0 = await state();
    await exec("window.__cad.setText(arguments[0])", SRC);
    await sceneReady(s0.sentVersion + 1);
    await exec("window.__cad.setView('iso')");
    const ms = () => exec("return window.__cad.measure()");
    await click("[role=radiogroup][aria-label=Modus] [role=radio][aria-label=Messen]");
    await waitFor("Messmodus", async () => {
      const m = await ms();
      return m && m.active && m.answers > 0 ? m : null;
    });
    // Faces with Alt (without snapping): in the small window the circle centres of the cylinder
    // are within the snap radius of its side.
    const fire = (type, x, y, alt = false) =>
      exec(
        "const c = document.querySelector('[data-testid=viewport-canvas]'); c.dispatchEvent(new PointerEvent(arguments[0], { clientX: arguments[1], clientY: arguments[2], altKey: arguments[3], bubbles: true, cancelable: true, composed: true, button: 0, buttons: arguments[0] === 'pointerdown' ? 1 : 0, pointerId: 1, pointerType: 'mouse', isPrimary: true })); return true;",
        type,
        x,
        y,
        alt,
      );
    const at = (p) => exec("return window.__cad.screenOf(arguments[0])", p);
    const hover = async (p, kind) => {
      const q = await at(p);
      const n = (await ms()).answers;
      await fire("pointermove", q.x + 0.3, q.y + 0.3, kind === "face");
      return waitFor(`Element ${kind}`, async () => {
        const m = await ms();
        if (m.answers > n && m.hover && m.hover.kind === kind) return m;
        // shown on a timeout (last state)
        throw new Error(JSON.stringify({ at: q, answers: [n, m.answers], hover: m.hover && { kind: m.hover.kind, label: m.hover.label }, message: m.message }));
      });
    };
    const press = async (p, kind) => {
      await hover(p, kind);
      const q = await at(p);
      const n = (await ms()).answers;
      const before = await ms();
      await fire("pointerdown", q.x + 0.3, q.y + 0.3, kind === "face");
      await fire("pointerup", q.x + 0.3, q.y + 0.3, kind === "face");
      // the click's own effect (a hover answer still on its way also counts as an answer)
      return waitFor("Klick beantwortet", async () => {
        const m = await ms();
        return m.answers > n && (m.picks.length !== before.picks.length || m.results.length !== before.results.length || m.message !== null) ? m : null;
      });
    };
    const c = Math.SQRT1_2;
    await click("[role=radiogroup][aria-label=Messart] [role=radio][aria-label=Radius]");
    await waitFor("Radius", async () => ((await ms()).kind === "radius" ? true : null));
    const m1 = await press([30 + 5 * c, 5 - 5 * c, 6], "face");
    check(m1.results.length === 1 && m1.results[0].label === "R 5 · ⌀ 10", `Radius des Zylinders: ${m1.results[0]?.label}`);
    await click("[role=radiogroup][aria-label=Messart] [role=radio][aria-label=Abstand]");
    await waitFor("Abstand", async () => ((await ms()).kind === "distance" ? true : null));
    await press([10, 5, 8], "face");
    const m2 = await press([30, 5, 12], "face");
    const d = m2.results[1];
    check(d && Math.abs(d.value - 4) < 1e-9 && d.exact, `Normalabstand der Deckflächen: ${d?.label} (exakt)`);
    const label = await exec("const l = document.querySelector('[data-testid=measure-label-' + arguments[0] + ']'); return l && !l.hidden ? l.textContent : null", d.id);
    check(label === "4 mm", `Beschriftung in der Ansicht: ${label}`);
    e.images.push({ name: await shot("t17-messen"), label: "Radius des Zylinders und Normalabstand der Deckflächen" });
    // in the section z = 4: the cylinder cut gives an exact circle
    await exec("window.__cad.setSection({ on: true, axis: 'z', point: [0, 0, 4], normal: [0, 0, 1], flip: false, snap: null })");
    await click("[role=radiogroup][aria-label=Messart] [role=radio][aria-label=Radius]");
    await click("[role=toolbar][aria-label=Messen] button[aria-label='Im Schnitt']");
    const sec = await waitFor("Schnitt", async () => {
      const m = await ms();
      return m.inSection && m.section && m.section.regions === 2 ? m : null;
    });
    check(sec.section.exact, `Schnitt exakt: ${sec.section.regions} Bereiche, ${sec.section.area.toFixed(4)} mm²`);
    const m3 = await press([35, 5, 4], "section_edge");
    const r3 = m3.results[2];
    check(r3 && r3.label === "R 5 · ⌀ 10", `Kreis im Schnitt: ${r3?.label}`);
    e.images.push({ name: await shot("t17-schnitt"), label: "Radius des Schnittkreises (Schnitt z = 4)" });
    await click("[role=toolbar][aria-label=Messen] button[aria-label=Fertig]");
    check((await ms()).active === false, "Messmodus beendet");
    const rtt = m3.rtt.filter((r) => !r.click).map((r) => r.ms).sort((a, b) => a - b);
    metrics.measureTauri = { hoverMedianMs: rtt[Math.floor(rtt.length / 2)] ?? null, answers: m3.answers };
    e.details = { results: m3.results.map((r) => ({ kind: r.kind, label: r.label, cli: r.cli })) };
    await exec("window.__cad.setSection({ on: false })");
  });

  // ------------------------------------------------------------ UI overhaul (report 20)

  await test("T18", "UI-Überarbeitung in Tauri: Leisten ohne Umbruch bei 1280/1440/1920 (hell/dunkel), Menüs, Ansichtswürfel, Seitenpanel", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.store.setState({ sideOpen: true, sideTab: 'parts' }); window.__cad.setText(arguments[0])", MEHR);
    await sceneReady();
    // bars: content not wider than the box, nothing sticks out, one row, mode and camera apart
    const bars = () =>
      exec(`
        const sel = [["Kopfleiste", ".hd"], ["Kopf links", ".hd-left"], ["Kopf Mitte", ".hd-title"], ["Kopf rechts", ".hd-right"], ["Dateileiste", ".filebar"],
          ["Modus", ".vtoolbar > [role=toolbar]:first-child"], ["Kamera", ".vt-end > [role=toolbar]"], ["Kontextleiste", ".ctxslot > [role=toolbar]"],
          ["Seitenpanel", ".sp-head"], ["Statuszeile", ".statusbar"]];
        const out = [];
        for (const [name, s] of sel) for (const e of document.querySelectorAll(s)) {
          const r = e.getBoundingClientRect();
          let outside = 0;
          for (const c of e.querySelectorAll("button, [role=radio], [role=tab], input")) {
            const q = c.getBoundingClientRect();
            if (q.width && (q.right > r.right + 0.5 || q.left < r.left - 0.5 || q.right > innerWidth + 0.5)) outside++;
          }
          out.push({ name, scroll: e.scrollWidth, client: e.clientWidth, height: Math.round(r.height), left: Math.round(r.left), right: Math.round(r.right), outside });
        }
        return { bars: out, inner: innerWidth, doc: [document.documentElement.scrollWidth, document.documentElement.clientWidth] };`);
    const problems = (rep, label) => {
      const p = [];
      if (rep.doc[0] > rep.doc[1]) p.push(`${label}: Fenster scrollt waagerecht`);
      for (const b of rep.bars) {
        if (b.scroll > b.client + 1) p.push(`${label}: ${b.name} abgeschnitten (${b.scroll} > ${b.client})`);
        if (b.outside) p.push(`${label}: ${b.name}: ${b.outside} Elemente ragen heraus`);
        const max = b.name === "Kopfleiste" ? 41 : b.name === "Dateileiste" ? 34 : b.name === "Statuszeile" ? 26 : 40;
        if (b.height > max) p.push(`${label}: ${b.name} umgebrochen (${b.height} px)`);
      }
      const m = rep.bars.find((b) => b.name === "Modus");
      const k = rep.bars.find((b) => b.name === "Kamera");
      if (m && k && m.right > k.left) p.push(`${label}: Modus und Kamera überlappen`);
      return p;
    };
    const sizes = {};
    const all = [];
    for (const theme of ["light", "dark"]) {
      await exec("window.__cad.setThemePref(arguments[0])", theme);
      for (const w of [1280, 1440, 1920]) {
        await wd("POST", `/session/${sid}/window/rect`, { width: w, height: 900 }).catch(() => null);
        await sleep(500);
        for (const m of ["Modell", "Messen"]) {
          await click(`[role=radiogroup][aria-label=Modus] [role=radio][aria-label=${m}]`);
          await sleep(250);
          const rep = await bars();
          sizes[`${w} ${theme} ${m}`] = rep.inner;
          all.push(...problems(rep, `${w} ${theme} ${m} (innen ${rep.inner} px)`));
        }
        await click("[role=radiogroup][aria-label=Modus] [role=radio][aria-label=Modell]");
        if (w !== 1440) e.images.push({ name: await shot(`t18-${w}-${theme === "light" ? "hell" : "dunkel"}`), label: `${w} px, ${theme === "light" ? "hell" : "dunkel"}` });
      }
      await wd("POST", `/session/${sid}/window/rect`, { width: 1440, height: 900 }).catch(() => null);
      await sleep(400);
      // menu "Datei" and the export menu
      await click("button[aria-label=Datei]");
      await waitFor("Datei-Menü", async () => (await exists("[role=menu][aria-label=Datei]")) || null);
      e.images.push({ name: await shot(`t18-datei-${theme === "light" ? "hell" : "dunkel"}`), label: `Datei-Menü, ${theme === "light" ? "hell" : "dunkel"}` });
      await click("button[aria-label=Datei]");
      check(!(await exists("[role=menu]")), "Menü schließt beim zweiten Klick");
      await click("button[aria-label=Exportformat]");
      const formats = await exec("return [...document.querySelectorAll('[role=menu][aria-label=Exportformat] [role^=menuitem]')].map((x) => x.getAttribute('aria-label'))");
      check(formats.join("|") === "3MF mehrfarbig|3MF einfarbig|STL|STL je Farbe (ZIP)|STEP|Je Bauteil", `Exportformate: ${formats.join(", ")}`);
      e.images.push({ name: await shot(`t18-export-${theme === "light" ? "hell" : "dunkel"}`), label: `Export-Menü, ${theme === "light" ? "hell" : "dunkel"}` });
      await click("button[aria-label=Exportformat]");
    }
    e.details = { sizes, problems: all };
    check(all.length === 0, all.length ? all.join("; ") : `keine abgeschnittenen oder umgebrochenen Leisten (${Object.keys(sizes).length} Zustände, Fensterbreiten ${[...new Set(Object.values(sizes))].join("/")} px)`);
    // the view cube turns the camera; the side panel shows the colours
    await exec("window.__cad.setThemePref('light')");
    await click("[data-testid=viewcube] [aria-label='Ansicht Oben']");
    const cam = await waitFor("Blick von oben", async () => {
      const c = await exec("return window.__cad.camera()");
      return c && c.dir[2] < -0.999 ? c : null;
    });
    check(cam.dir[2] < -0.999, `Ansichtswürfel „Oben“: Blickrichtung ${cam.dir.map((x) => x.toFixed(3)).join(", ")}`);
    await click("[role=tablist][aria-label=Seitenpanel] [role=tab][aria-label=Farben]");
    await waitFor("Farbliste", async () => (await exists("[data-testid=colors-panel] .cptable")) || null);
    e.images.push({ name: await shot("t18-oben-farben"), label: "Ansichtswürfel „Oben“, Reiter Farben" });
    await exec("window.__cad.setView('iso'); window.__cad.store.setState({ sideTab: 'parts' })");
  });

  await test("T19", "Asserts in Tauri: Sperrvolumen, Verletzung einrahmen, Export-Rückfrage (nativer Kernel)", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('light')");
    // the keep-out toggle is remembered: start with it on
    if (!(await exec("return window.__cad.asserts().showKeepout"))) {
      await exec("window.__cad.setText('cube(1);\\nassert_keepout() cube(1);\\n')");
      await sceneReady();
      await click("[data-testid=toggle-keepout]");
    }
    const BAD = "cube([40, 30, 3]);\ntranslate([0, 0, 3]) cube([12, 30, 6]);\nassert_keepout(clearance = 0.5) translate([10, 10, 2]) cube([10, 10, 5]);\n";
    await exec("window.__cad.setText(arguments[0])", BAD);
    await sceneReady();
    const a = await waitFor("Assert-Ergebnisse", async () => {
      const x = await exec("return window.__cad.asserts()");
      return x && x.version !== null && x.version === x.completeVersion && x.view ? x : null;
    });
    const p = a.items[0].pairs[0];
    check(p.problem === "overlap" && Math.abs(p.volume - 180) < 1e-6, `Schnittvolumen ${p.volume} mm³ (10·10·1 + 2·10·4 = 180)`);
    check(a.view.keepout === 1, "Sperrvolumen gezeichnet");
    check((a.view.drawn["assert-overlap"] ?? 0) >= 1, "Schnittkörper rot gezeichnet");
    e.images.push({ name: await shot("t19-verletzung"), label: "Sperrvolumen mit Verletzung (Tauri, nativer Kernel)" });
    await click("[data-testid=status-asserts]");
    const f = await waitFor("Einrahmen", async () => (await exec("return window.__cad.asserts().focus")) || null);
    check(f.pair === 0, "Verletzung fokussiert");
    e.images.push({ name: await shot("t19-eingerahmt"), label: "Klick in der Statusleiste: Schnittkörper eingerahmt" });
    await exec("window.dispatchEvent(new KeyboardEvent('keydown', { key: 'e', ctrlKey: true }))");
    const c = await waitFor("Export-Rückfrage", async () => (await exec("return window.__cad.asserts().exportConfirm")) || null);
    check(c.violations.length === 1 && c.violations[0].startsWith("Z.3 assert_keepout: "), `Rückfrage: ${c.violations[0]}`);
    e.images.push({ name: await shot("t19-export-dialog"), label: "Export trotz Verletzung? (Trotzdem exportieren / Abbrechen)" });
    await click("[data-testid=export-cancel]");
    await waitFor("Dialog zu", async () => ((await exec("return window.__cad.asserts().exportConfirm")) === null ? true : null));
    // (with the cursor on its statement the keep-out volume stays visible: move it away)
    await exec("window.__cad.setCursorLine(1)");
    await click("[data-testid=toggle-keepout]");
    await waitFor("Sperrvolumen aus", async () => {
      const x = await exec("return window.__cad.asserts()");
      if (x.view.keepout === 0) return true;
      throw new Error(`show ${x.showKeepout}, keepout ${x.view.keepout}`);
    });
    await click("[data-testid=toggle-keepout]");
    e.details = { volume: p.volume, message: p.message, labels: a.view.labels };
  });

  await test("T14", "Thema „System“ in Tauri folgt dem GTK-Farbschema (prefers-color-scheme)", async (check, e) => {
    await ensureApp();
    await exec("window.__cad.setThemePref('system')");
    const sysLight = await exec("return window.__cad.systemDark()");
    const th = await exec("return document.documentElement.dataset.theme");
    check(th === (sysLight ? "dark" : "light"), `System (${sysLight ? "dunkel" : "hell"}) → data-theme ${th}`);
    // the same app with a dark GTK theme (GTK_THEME=Adwaita:dark)
    await stopApp();
    await stopDriver();
    await startDriver({ GTK_THEME: "Adwaita:dark" });
    await startApp([file]);
    const sysDark = await exec("return window.__cad.systemDark()");
    const th2 = await exec("return document.documentElement.dataset.theme");
    e.details = { systemDarkWithoutGtkTheme: sysLight, systemDarkWithAdwaitaDark: sysDark };
    check(sysDark === true, `WebKitGTK meldet prefers-color-scheme: dark mit GTK_THEME=Adwaita:dark (${sysDark})`);
    check(th2 === "dark", `Thema System → ${th2}`);
    e.images.push({ name: await shot("t14-system-dunkel"), label: "Thema System bei dunklem GTK-Schema" });
    await stopApp();
    await stopDriver();
    await startDriver();
  });

  await test("T20", "Umbenennung mcad: Fenstertitel, App-ID (WM_CLASS), Kopfleiste; .scad öffnet mit use <….mcad>", async (check, e) => {
    await stopApp();
    const dir = join(work, "scad");
    mkdirSync(join(dir, "lib"), { recursive: true });
    writeFileSync(join(dir, "lib/teil.mcad"), "module teil(h = 5) { cube([20, 10, h]); }\n");
    const alt = join(dir, "alt.scad");
    writeFileSync(alt, "// OpenSCAD-Datei mit einer .mcad-Bibliothek\nuse <lib/teil.mcad>\nteil(4);\n");
    const st = await startApp([alt]);
    check(st.state.fileName === "alt.scad", `Datei ${st.state.fileName}`);
    check(st.state.diagnostics.length === 0, `keine Diagnosen (${JSON.stringify(st.state.diagnostics)})`);
    const p = await exec("return window.__cad.projectInfo()");
    check(JSON.stringify(p.paths) === JSON.stringify(["alt.scad", "lib/teil.mcad"]), `Projektdateien ${JSON.stringify(p.paths)}`);
    const brand = await exec("return document.querySelector('.hd-brand')?.textContent");
    check(brand === "mcad", `Kopfleiste „${brand}“`);
    const docTitle = await exec("return document.title");
    const win = await waitFor("Fenster mit Titel … – mcad", async () => xwindows().find((w) => w.name.endsWith("– mcad")) || null, 15_000);
    check(win.name === "alt.scad – mcad", `Fenstertitel „${win.name}“`);
    // GTK: res_name = program name (app id), res_class = the same with a capital first letter
    check(win.res_name === "dev.mcad.desktop" && win.res_class.toLowerCase() === "dev.mcad.desktop", `WM_CLASS ${win.res_name}/${win.res_class}`);
    e.details = { window: win, docTitle, brand, paths: p.paths };
    e.images.push({ name: await shot("t20-mcad"), label: "alt.scad mit use <lib/teil.mcad>, Kopfleiste „mcad“" });
    e.images.push({ name: xshot("t20-fenster"), label: `Fenster „${win.name}“, WM_CLASS ${win.res_class} (X-Bildschirmfoto)` });
    await stopApp();
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
