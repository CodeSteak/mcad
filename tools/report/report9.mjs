#!/usr/bin/env node
// Test report 9 ("desktop app, file watcher, Git, section view, CLI↔UI"): collects the
// results of the test runs and writes
//
//   docs/reports/09-desktop-schnitt/report.html, img/*
//
// Inputs (gitignored, reports/tmp/report9/):
//   tauri-e2e.json     node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad   (Tauri E2E, Xvfb)
//   scenarios.json,    cd web && pnpm e2e   (section view: e2e/section.spec.ts; also all older scenarios)
//   metrics.json
//   img/*              screenshots of both
//   cargo.txt          cargo test --workspace --no-fail-fast
//   clippy.txt         cargo clippy --workspace --all-targets -- -D warnings
//   fmt.txt            cargo fmt --all --check (empty = ok)
//   vitest.txt         cd web && pnpm test
//   typecheck.txt      cd web && pnpm typecheck
//   actionlint.txt     actionlint .github/workflows/*.yml
// and reports/tmp/report2/playwright.json (Playwright JSON reporter of `pnpm e2e`).
//
//   node tools/report/report9.mjs
//   node tools/report/print-pdf.mjs docs/reports/09-desktop-schnitt/report.html docs/reports/09-desktop-schnitt/report.pdf "Test report 9: desktop app, file watcher, Git, section view, CLI↔UI"

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report9");
const out = join(root, "docs/reports/09-desktop-schnitt");
const img = join(out, "img");

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
const readJson = (f) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const median = (a) => {
  if (!a || !a.length) return null;
  const s = [...a].sort((x, y) => x - y);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

rmSync(img, { recursive: true, force: true });
mkdirSync(img, { recursive: true });

/** Copies a screenshot as JPEG (smaller PDF); returns the relative path or null. */
function pic(name) {
  const base = name.replace(/\.(png|jpg)$/, "");
  for (const ext of ["png", "jpg"]) {
    const src = join(input, "img", `${base}.${ext}`);
    if (!existsSync(src)) continue;
    const dst = join(img, `${base}.jpg`);
    const r = spawnSync("convert", [src, "-resize", "1500x1500>", "-quality", "82", dst]);
    if (r.status !== 0) return null;
    return `img/${base}.jpg`;
  }
  return null;
}
const fig = (name, caption) => {
  const p = pic(name);
  return p ? `<figure><img src="${p}"><figcaption>${esc(caption ?? "")}</figcaption></figure>` : `<figure class="missing">${esc(name)} missing</figure>`;
};

// ---------------------------------------------------------------- inputs

const tauri = readJson(join(input, "tauri-e2e.json"));
const scen = readJson(join(input, "scenarios.json")) ?? [];
const metrics = readJson(join(input, "metrics.json")) ?? {};
const pw = readJson(join(root, "reports/tmp/report2/playwright.json"));
const cargo = readText(join(input, "cargo.txt"));
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const vitest = readText(join(input, "vitest.txt"));
const typecheck = readText(join(input, "typecheck.txt"));
const actionlint = readText(join(input, "actionlint.txt"));

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
const newRust = cargo
  ? cargo
      .split(/Running /)
      .filter((s) => /tests\/backend\.rs|tests\/ipc\.rs|tests\/agent\.rs|src\/lib\.rs \(target.*cad_app|src\/main\.rs \(target.*cad-/.test(s.split("\n")[0]))
      .flatMap((s) => [...s.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ file: s.split("\n")[0].trim().replace(/\(.*$/, "").trim(), name: m[1], res: m[2] })))
      .filter((t) => !t.file.endsWith("agent.rs") || /^ui_/.test(t.name))
  : [];
const vitestSum = vitest ? vitest.match(/Tests\s+(\d+) passed[^\n]*/)?.[0] ?? null : null;
const pwSum = (() => {
  if (!pw) return null;
  const all = [];
  const walk = (s) => {
    for (const sp of s.specs ?? []) for (const t of sp.tests ?? []) all.push({ file: sp.file, title: sp.title, status: t.results?.[t.results.length - 1]?.status ?? t.status });
    for (const c of s.suites ?? []) walk(c);
  };
  for (const s of pw.suites ?? []) walk(s);
  return { total: all.length, passed: all.filter((t) => t.status === "passed").length, skipped: all.filter((t) => t.status === "skipped").length, section: all.filter((t) => t.file.includes("section")), all };
})();
const tauriOk = tauri ? tauri.results.filter((r) => r.ok).length : 0;
const tm = tauri?.metrics ?? {};

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
const webkit = (() => {
  try {
    return execFileSync("pkg-config", ["--modversion", "webkit2gtk-4.1"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

// ---------------------------------------------------------------- sections

const tauriTest = (id) => tauri?.results.find((r) => r.id === id);
const checksHtml = (checks) => (checks?.length ? `<ul class="checks">${checks.map((c) => `<li class="${c.ok === false ? "bad" : "ok"}">${esc(c.text ?? c)}</li>`).join("")}</ul>` : "");
function tauriSection(id, intro) {
  const t = tauriTest(id);
  if (!t) return `<p class="bad">${esc(id)}: not run</p>`;
  const imgs = t.images ?? [];
  return `<div class="step"><h3>${esc(id)} ${esc(t.title)} <span class="${t.ok ? "ok" : "bad"}">${t.ok ? "passed" : "failed"}</span> <span class="small">${num(t.ms)} ms</span></h3>
${intro ? `<p>${intro}</p>` : ""}${checksHtml(t.checks)}${t.error ? `<pre>${esc(t.error)}</pre>` : ""}
${imgs.length ? `<div class="${imgs.length > 1 ? "pair" : ""}">${imgs.map((i) => fig(i.name, i.label)).join("")}</div>` : ""}</div>`;
}
function scenSection(id) {
  const s = scen.find((x) => x.id === id);
  if (!s) return `<p class="bad">${esc(id)} missing</p>`;
  const st = pwSum?.section.find((t) => t.title.startsWith(id));
  return `<div class="step"><h3>${esc(id)} ${esc(s.title)} ${st ? `<span class="${st.status === "passed" ? "ok" : "bad"}">${st.status === "passed" ? "passed" : st.status}</span>` : ""}</h3><p>${esc(s.caption)}</p>${checksHtml(s.checks)}
${s.images?.length ? `<div class="${s.images.length > 1 ? (s.images.length > 2 ? "tri" : "pair") : ""}">${s.images.map((i) => fig(i.name, i.label)).join("")}</div>` : ""}</div>`;
}

const t07 = tauriTest("T07");
const statusText = t07?.details?.status_text ?? null;
const sectionCli = tauriTest("T08")?.details?.cli ?? null;
const kNative = tm.keystrokeToSceneNative ?? [];
const kWasm = metrics.keystrokeToSceneWasm ?? [];
const agentSteps = tm.agentSteps ?? [];

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 9 – desktop app, file watcher, Git, section view, CLI↔UI</title>
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
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 2mm; margin: 2mm 0; align-items: start; }
  .tri { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 2mm; margin: 2mm 0; align-items: start; }
  .pair > *, .tri > * { min-width: 0; }
  figure { margin: 0 0 2mm; break-inside: avoid; }
  figure img { max-width: 100%; max-height: 105mm; border: 1px solid var(--border); display: block; }
  figcaption { font-size: 7.6pt; color: var(--muted); margin-top: 0.6mm; }
  figure.missing { border: 1px dashed var(--border); padding: 4mm; }
  .small { font-size: 8.2pt; color: var(--muted); font-weight: 400; }
  .step { border: 1px solid var(--border); border-radius: 2mm; padding: 2.5mm 3mm; margin: 3mm 0; break-inside: avoid-page; }
  .step h3 { margin-top: 0; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  ul.checks { list-style: none; margin-left: 0; font-size: 8.3pt; }
  ul.checks li::before { content: "✓ "; color: var(--ok); }
  ul.checks li.bad::before { content: "✗ "; color: var(--bad); }
  tr, td { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
</style>
</head>
<body>
<main>
<h1>Test report 9 – desktop app, file watcher, Git, section view, CLI↔UI</h1>
<div class="meta">cad-app (Tauri 2), cad-desktop, cad-ipc, cad-cli, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · WebKitGTK ${esc(webkit)} · ${os.cpus().length} cores (${esc(os.cpus()[0]?.model ?? "")}) · Xvfb without GPU</div>

<div class="summary">
  <div>Tauri-E2E (tauri-driver, Xvfb)<b class="${tauri && tauriOk === tauri.results.length ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>real app, real kernel</div>
  <div>Playwright (Chromium)<b class="${pwSum && pwSum.passed + pwSum.skipped === pwSum.total ? "ok" : "bad"}">${pwSum ? `${pwSum.passed} / ${pwSum.total}` : "–"}</b>of which ${pwSum ? pwSum.section.length : "–"} section, ${pwSum ? pwSum.total - pwSum.section.length : "–"} existing</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${cargoSum ? `${cargoSum.ignored} ignored` : ""}</div>
  <div>clippy · fmt · actionlint<b class="${clippy != null && !/^(error|warning)/m.test(clippy) && fmt != null && fmt.trim() === "" ? "ok" : "bad"}">${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "finding" : "clean"} · ${fmt == null ? "–" : fmt.trim() === "" ? "clean" : "diff"} · ${actionlint == null ? "–" : actionlint.trim() === "" ? "clean" : "finding"}</b></div>
  <div>Start to scene<b>${num(tm.startup?.wallMs)} ms</b>session → first complete scene</div>
  <div>Keystroke → scene (median)<b>${num(median(kNative))} / ${num(median(kWasm))} ms</b>native (Tauri) / Wasm (Chromium)</div>
  <div>Kernel-heavy change<b>${num(median(tm.heavyEvalNative ?? []))} / ${num(median(metrics.heavyEvalWasm ?? []))} ms</b>native / Wasm (median)</div>
  <div>Moving the section<b>${num(metrics.sectionMoveFps?.fps, 1)} fps</b>Chromium SwiftShader; Tauri ${num(tm.sectionFpsTauri?.fps, 1)}</div>
</div>

<h2>1 Scope and architecture</h2>
<p>The desktop version runs the same React app as the browser; new are the Tauri app (<code>crates/cad-app</code>), the backend without Tauri (<code>crates/cad-desktop</code>: engine process with supervisor, binary format, files, watcher, Git), the local IPC (<code>crates/cad-ipc</code>) and, in the web app, <code>TauriTransport</code>, the desktop platform and the section view (browser and desktop).</p>
<pre>cad datei.scad  (app process, feature gui)
  Main thread: Tauri event loop + webview (React app, TauriTransport instead of worker)
  Commands: engine_post · open/save/save_as · watch_files · git_hunks · ui_reply · …
  Channel (Tauri 2, raw): binary batches  ◀────────── stdout ── cad engine-host (engine process)
  Supervisor (cad-desktop::engine) ───── stdin: post [ToEngine…] ─▶  engine thread 256 MiB stack:
    Watchdog 15 s: kill process + restart, BlockKeys                 Session::post / step(16 ms)
  cad-watch: notify → file-changed / file-removed                    SMLib native (FFI)
  cad-ui-ipc: Unix socket ui-&lt;pid&gt; (cad-ipc) ◀── cad ui status | show | screenshot</pre>
<ul>
<li><b>Engine:</b> native <code>cad-session</code> with natively linked SMLib in a separate process (<code>cad engine-host</code>, the same binary), containing an engine thread with a 256 MiB stack and the same loop as the worker (post messages, a <code>SetSource</code> followed by another one is dropped, then <code>step(16 ms)</code>, new messages in between). A process rather than just a thread, because SMLib and its C facade have global state (handle table, messages): a hanging call can only be ended together with the process; a replacement thread next to it would share the state. The watchdog in the app process counts sent and accepted messages (<code>seen</code>) and replaces the engine after 15 s without a response, with the same <code>BlockKeys</code>/<code>SetFiles</code>/<code>SetContext</code>/<code>SetSource</code> as <code>WorkerTransport</code>.</li>
<li><b>Transport:</b> <code>EngineTransport</code> stays; <code>TauriTransport</code> bundles UI messages per microtask into one <code>engine_post</code> command (order is kept) and receives batches over a Tauri 2 channel as <code>ArrayBuffer</code>: <code>[u32 Kopflänge][u32 Datenlänge][Kopf-JSON][Daten]</code> (header length, data length, header JSON, data), mesh arrays as <code>{"$b": [offset, länge, typ]}</code>, typed-array views without copying in the webview. The desktop UI is built without worker and Wasm (<code>pnpm build:tauri</code>) and embedded in the binary.</li>
<li><b>One binary:</b> <code>cad</code> with feature <code>gui</code> (default). <code>cad</code>, <code>cad datei.scad</code>, <code>cad ordner/</code> start the UI; without <code>DISPLAY</code>/<code>WAYLAND_DISPLAY</code> or without <code>gui</code>: message listing the headless commands, exit 2. All subcommands remain.</li>
<li><b>IPC (<code>cad-ipc</code>, reusable):</b> endpoints as Unix sockets in <code>$CAD_IPC_DIR</code>, else <code>$XDG_RUNTIME_DIR/controller-cad</code>, else <code>$TMPDIR/controller-cad-&lt;uid&gt;</code> (folder 0700, owner checked, sockets 0600, peer UID checked); frames with version field <code>v</code>, kinds request/response/event, binary part; stale sockets (connection refused) are removed on bind, connect and list. API: <code>Server::bind(name)</code>, <code>Server::serve(handler)</code>, <code>connect(name)</code>, <code>Conn::request/request_with/respond/send_event/next_event</code>, <code>list(prefix)</code>.</li>
</ul>

<h2>2 Decisions</h2>
<table>
<tr><th>Topic</th><th>Decision</th><th>Reason</th></tr>
<tr><td>Git integration</td><td><code>git</code> command for the HEAD version (<code>rev-parse</code>, <code>show HEAD:pfad</code>), diff in-process with <code>similar</code></td><td>One blob per file and HEAD state (cached, HEAD re-checked at most every 5 s); typing does not start a process. <code>git2</code> would need libgit2 (C build), <code>gix</code> would add well over a hundred crates to every binary; <code>git</code> is present wherever there are repositories. Untracked files or files not in HEAD: no markers.</td></tr>
<tr><td>Feature <code>gui</code></td><td>on by default</td><td>The workspace contains <code>cad-app</code> anyway (<code>cargo test --workspace</code> builds it); the only extra cost is the WebKitGTK packages on Linux (CI: <code>scripts/ci/desktop.sh deps</code>). Headless: <code>--no-default-features</code>. Without <code>web/dist-tauri</code> the build embeds a placeholder page.</td></tr>
<tr><td>Engine</td><td>separate process, containing a thread with a large stack</td><td>see section 1 (global SMLib state, watchdog)</td></tr>
<tr><td>Auto-save</td><td>off by default, delay configurable (1000 ms)</td><td>Whoever works on the file at the same time as an agent should decide when their state reaches the disk; the notification bar protects both sides.</td></tr>
<tr><td>Watcher</td><td><code>notify</code> on the folders (not the files), content comparison by hash</td><td>atomic saving (editors, <code>cad set</code>) replaces the file; the app's own saves register the new content beforehand and never come back as "external".</td></tr>
<tr><td>Reload</td><td>as minimal line changes, separate undo step (<code>isolateHistory</code>)</td><td>Cursor, selection and scroll position are kept; Ctrl+Z restores the user's own state.</td></tr>
</table>

<h2>3 Desktop app (Tauri E2E with tauri-driver and WebKitWebDriver under Xvfb)</h2>
<p>${tauri ? `Binary <code>${esc(relative(root, tauri.bin))}</code>, ${tauriOk}/${tauri.results.length} passed${tauri.fatal ? `, aborted: ${esc(tauri.fatal)}` : ""}.` : "not run"} The run starts the real app with a file via WebDriver, changes files from the shell, uses the real CLI (<code>cad check</code>, <code>cad set</code>, <code>cad ui …</code>) and a temporary Git repository. tauri-driver ran stably under Xvfb; images: WebDriver screenshots of the webview and an X screenshot of the window.</p>
${tauriSection("T01")}
${tauriSection("T02", "The file is changed externally (<code>k: gehaeuse(w = 60)</code> → <code>w = 90</code>); the editor has no unsaved changes.")}
${tauriSection("T03", "Buffer against HEAD: line 21 changed (blue), one new comment line (green), the module <code>deckel</code> deleted (red triangle between the lines). The markers are 3 px wide with opacity 0.5–0.55 (tuned for light/dark); the title (hover) shows the old lines.")}
${tauriSection("T04")}
${tauriSection("T05")}

<h2>4 Workflow "agent changes file → UI updates"</h2>
<p>The user has the file open in the UI; an agent changes it from the shell and checks it with the CLI. Each image is the UI after the step, without any action in the UI. Time from writing to the drawn scene: ${agentSteps.map((s) => `${num(s.ms)} ms`).join(", ") || "–"} (incl. <code>cad check</code>/<code>cad set</code>).</p>
${tauriSection("T06")}
${tauriSection("T07", "Coupling CLI ↔ running UI via <code>cad-ipc</code>.")}
${statusText ? `<div class="small">Output of <code>cad ui status</code>:</div><pre>${esc(statusText.replace(/\/tmp\/cad-desktop-e2e-[^/\s]+/g, "/tmp/…"))}</pre>` : ""}
${tauriSection("T09", "Hanging kernel (test message <code>__debugHang</code> in the engine process, watchdog 3 s for the test): (a) when idle, (b) during a slow kernel job (8×8 holes with fillet, native ~3.6 s).")}
${tauriSection("T10")}
${tauriSection("T11")}

<h2>5 3D section view</h2>
<p>Tool "Section" (German UI: „Schnitt“) in the view (browser and Tauri): plane X/Y/Z through the model center or free (normal from the view direction or entered), handle (blue dot) for dragging along the normal with snapping to grid planes of the context module and to anchors (Alt: free, 0.1 mm), flip, filled cap and contour. Method as in the CLI renderer: clipping plane on all model materials, per opaque body one parity pass (stencil <code>INVERT</code> over all fragments of the clipped mesh) and a large quad in the plane that draws the cap where the bit is set (depth-tested, hatched, direction alternating per body). Convention exactly as <code>cad render --section</code>: plane through p with normal n, the side n points into is removed; "Copy as <code>cad render …</code>" writes <code>--section a=v [--section-flip]</code> or <code>--section-plane p,n</code> with <code>--module</code> and the camera (<code>--azimuth/--elevation</code>).</p>
${["X01", "X02", "X03", "X04", "X05", "X06"].map(scenSection).join("\n")}
${tauriSection("T08")}
${sectionCli ? `<p>Copied call in Tauri: <code>${esc(sectionCli.replace(/\/tmp\/cad-desktop-e2e-[^/\s]+/g, "/tmp/…"))}</code></p>` : ""}

<h2>6 Measurements</h2>
<table>
<tr><th>Measurement</th><th>Value</th><th>Remark</th></tr>
<tr><td>App start to first complete scene</td><td>${num(tm.startup?.wallMs)} ms</td><td>create WebDriver session (process start, webview, engine process, SMLib) until the scene is drawn; of which in the webview until the first scene ${num(tm.startup?.firstSceneMs)} ms</td></tr>
<tr><td>Keystroke → scene, native (Tauri)</td><td>median ${num(median(kNative))} ms (${kNative.map((x) => num(x)).join(", ")})</td><td>Backspace/digit in <code>w = 60</code> (gehaeuse), real keystrokes via WebDriver; time from the editor change until the scene is drawn (incl. 8 ms debounce, WebKitGTK with software GL)</td></tr>
<tr><td>Keystroke → scene, Wasm (Chromium)</td><td>median ${num(median(kWasm))} ms (${kWasm.map((x) => num(x)).join(", ")})</td><td>the same change in the browser (worker, smlib.wasm), headless Chromium with SwiftShader</td></tr>
<tr><td>of which engine (SetSource → complete scene)</td><td>native median ${num(median(tm.keystrokeEvalNative ?? []))} ms, Wasm median ${num(median(metrics.keystrokeEvalWasm ?? []))} ms</td><td>evaluation and kernel without drawing, same keystrokes</td></tr>
<tr><td>Kernel-heavy change (16 holes with fillet, radius 2 → 2.5 → 3)</td><td>native ${(tm.heavyEvalNative ?? []).map((x) => num(x)).join(" / ")} ms, Wasm ${(metrics.heavyEvalWasm ?? []).map((x) => num(x)).join(" / ")} ms</td><td>SetSource → complete scene; first value incl. empty cache for the new model</td></tr>
<tr><td>External change → scene</td><td>${num(tm.externalToScene)} ms</td><td>write file until new scene drawn (debounce 50 ms in the watcher)</td></tr>
<tr><td>Moving the section plane (Chromium)</td><td>${num(metrics.sectionMoveFps?.fps, 1)} fps</td><td>fillet_zonen, 120 frames, each position new (cap, contour); rotating without section ${num(metrics.orbitFpsNoSection?.fps, 1)} fps, with section ${num(metrics.orbitFpsWithSection?.fps, 1)} fps – software GL without GPU, the 60 fps target applies to GPUs</td></tr>
<tr><td>Moving the section plane (Tauri)</td><td>${num(tm.sectionFpsTauri?.fps, 1)} fps</td><td>gehaeuse, WebKitGTK under Xvfb (llvmpipe)</td></tr>
<tr><td>Dragging the handle (Chromium)</td><td>${(metrics.sectionDragFps ?? []).map((f) => `${num(f.fps, 0)} fps`).join(", ") || "–"}</td><td>X02: three real mouse drags (~12 moves each), frame rate of the view while dragging</td></tr>
<tr><td>Cap share UI ↔ CLI</td><td>${metrics.cliCompare ? `${num(metrics.cliCompare.web.share * 100, 1)} % / ${num(metrics.cliCompare.cliPx.share * 100, 1)} %` : "–"}</td><td>share of cap pixels in the drawn pixels, deviation ${metrics.cliCompare ? num(metrics.cliCompare.rel * 100, 0) : "–"} % (limit 35 %)</td></tr>
</table>
<p>For small changes (keystroke in <i>gehaeuse</i>, kernel almost entirely from the cache) the desktop app is not faster: the engine computes for only a few milliseconds; the time is dominated by debounce, the path via Tauri command, engine process and channel (native ${num(median(tm.keystrokeEvalNative ?? []))} ms against ${num(median(metrics.keystrokeEvalWasm ?? []))} ms in the worker) and the software rendering of WebKitGTK under Xvfb. As soon as the kernel computes, the native kernel wins clearly (row "Kernel-heavy change"). All measurements without GPU; meaningful only relative to each other.</p>

<h2>7 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed, ${cargoSum.ignored} ignored` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "finding" : "no findings"}; <code>cargo fmt --check</code>: ${fmt == null ? "–" : fmt.trim() === "" ? "no findings" : "differences"}.</li>
<li>Web: <code>pnpm typecheck</code> ${typecheck == null ? "–" : typecheck.trim() === "" || /error/i.test(typecheck) === false ? "no findings" : "finding"}; Vitest ${esc(vitestSum ?? "–")} (new: <code>tauriTransport.test.ts</code>, <code>textDiff.test.ts</code>, <code>section.test.ts</code>); Playwright ${pwSum ? `${pwSum.passed}/${pwSum.total}` : "–"} (all ${pwSum ? pwSum.total - pwSum.section.length : "–"} existing scenarios and ${pwSum ? pwSum.section.length : "–"} new ones for the section view).</li>
<li>CI: workflow <code>rust</code> installs the Tauri packages (<code>scripts/ci/desktop.sh deps</code>), new workflow <code>desktop</code> (UI for Tauri, release build, Tauri E2E under Xvfb, results as artifact); <code>actionlint</code> 1.7.7 over all workflows: ${actionlint == null ? "–" : actionlint.trim() === "" ? "no findings" : esc(actionlint)}.</li>
</ul>
<table><tr><th>Rust test (new)</th><th>File</th><th>Result</th></tr>${newRust.map((t) => `<tr><td>${esc(t.name)}</td><td>${esc(t.file)}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("")}</table>
<p class="small">Backend (<code>cad-desktop/tests/backend.rs</code>): open file with includes, folder and choice of main file, atomic save and recently opened, watcher (external change, own save ignored, atomic replace, same content, include, unrelated files, deleted/recreated, renamed), Git hunks (kinds, CRLF, end of file) and HEAD base in a temporary repository (untracked, new commit, subfolder), binary format, supervisor (evaluation, meshes, watchdog with block, watchdog when idle). IPC (<code>cad-ipc/tests/ipc.rs</code>): frames, version, request/response/event, stale sockets, permissions, multiple clients. CLI (<code>agent.rs</code>, <code>launch.rs</code>): no display → exit 2 + message, <code>cad ui …</code> without UI → exit 3, <code>cad ui</code> against a test endpoint.</p>

<h2>8 Known limitations</h2>
<ul>
<li>Control IPC only on Unix (Linux, macOS); on Windows <code>cad-ipc</code> reports "not implemented" (named pipes missing), the app itself is untested there. One window per app process; <code>cad datei.scad</code> starts another process instead of using the running window (<code>cad ui show</code> does that).</li>
<li>Watchdog tests block the engine process via a test message; a real hanging SMLib call was not reproduced. The block lasts until the app is restarted.</li>
<li>Watcher: renaming appears as deletion (bar "deleted or renamed"), the new name is not tracked; an external change to exactly the content of the UI goes unnoticed (intended).</li>
<li>Git only against HEAD (not against the index), via the <code>git</code> command; without <code>git</code> no markers.</li>
<li>Native dialogs (Open, Folder, Save as) are not automated in the E2E tests; tested are opening via the command line, <code>cad ui show</code>, recently opened (Rust) and saving.</li>
<li>Section: one plane; caps only for opaque, closed bodies; contour on the CPU per position; handles, grids and anchors are not clipped. The copied CLI call reproduces section and view direction, not the framing (UI perspective, CLI orthographic).</li>
<li>Measurements without GPU (Xvfb/llvmpipe, SwiftShader).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${relative(root, join(out, "report.html"))}: Tauri ${tauriOk}/${tauri?.results.length ?? 0}, Playwright ${pwSum?.passed ?? "–"}/${pwSum?.total ?? "–"}, Rust ${cargoSum?.ok ?? "–"}`);
