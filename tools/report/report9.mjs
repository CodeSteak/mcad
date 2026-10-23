#!/usr/bin/env node
// Test report 9 ("Desktop-App, Dateibeobachter, Git, Schnittansicht, CLI↔UI"): collects the
// results of the test runs and writes
//
//   docs/reports/09-desktop-schnitt/report.html, img/*
//
// Inputs (gitignored, reports/tmp/report9/):
//   tauri-e2e.json     node web/e2e-tauri/desktop.e2e.mjs --bin target/release/cad   (Tauri E2E, Xvfb)
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
//   node tools/report/print-pdf.mjs docs/reports/09-desktop-schnitt/report.html docs/reports/09-desktop-schnitt/report.pdf "Testbericht 9: Desktop-App, Dateibeobachter, Git, Schnittansicht, CLI↔UI"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
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
  return p ? `<figure><img src="${p}"><figcaption>${esc(caption ?? "")}</figcaption></figure>` : `<figure class="missing">${esc(name)} fehlt</figure>`;
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
  if (!t) return `<p class="bad">${esc(id)}: nicht gelaufen</p>`;
  const imgs = t.images ?? [];
  return `<div class="step"><h3>${esc(id)} ${esc(t.title)} <span class="${t.ok ? "ok" : "bad"}">${t.ok ? "bestanden" : "fehlgeschlagen"}</span> <span class="small">${de(t.ms)} ms</span></h3>
${intro ? `<p>${intro}</p>` : ""}${checksHtml(t.checks)}${t.error ? `<pre>${esc(t.error)}</pre>` : ""}
${imgs.length ? `<div class="${imgs.length > 1 ? "pair" : ""}">${imgs.map((i) => fig(i.name, i.label)).join("")}</div>` : ""}</div>`;
}
function scenSection(id) {
  const s = scen.find((x) => x.id === id);
  if (!s) return `<p class="bad">${esc(id)} fehlt</p>`;
  const st = pwSum?.section.find((t) => t.title.startsWith(id));
  return `<div class="step"><h3>${esc(id)} ${esc(s.title)} ${st ? `<span class="${st.status === "passed" ? "ok" : "bad"}">${st.status === "passed" ? "bestanden" : st.status}</span>` : ""}</h3><p>${esc(s.caption)}</p>${checksHtml(s.checks)}
${s.images?.length ? `<div class="${s.images.length > 1 ? (s.images.length > 2 ? "tri" : "pair") : ""}">${s.images.map((i) => fig(i.name, i.label)).join("")}</div>` : ""}</div>`;
}

const t07 = tauriTest("T07");
const statusText = t07?.details?.status_text ?? null;
const sectionCli = tauriTest("T08")?.details?.cli ?? null;
const kNative = tm.keystrokeToSceneNative ?? [];
const kWasm = metrics.keystrokeToSceneWasm ?? [];
const agentSteps = tm.agentSteps ?? [];

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 9 – Desktop-App, Dateibeobachter, Git, Schnittansicht, CLI↔UI</title>
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
<h1>Testbericht 9 – Desktop-App, Dateibeobachter, Git, Schnittansicht, CLI↔UI</h1>
<div class="meta">cad-app (Tauri 2), cad-desktop, cad-ipc, cad-cli, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · WebKitGTK ${esc(webkit)} · ${os.cpus().length} Kerne (${esc(os.cpus()[0]?.model ?? "")}) · Xvfb ohne GPU</div>

<div class="summary">
  <div>Tauri-E2E (tauri-driver, Xvfb)<b class="${tauri && tauriOk === tauri.results.length ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>echte App, echter Kernel</div>
  <div>Playwright (Chromium)<b class="${pwSum && pwSum.passed + pwSum.skipped === pwSum.total ? "ok" : "bad"}">${pwSum ? `${pwSum.passed} / ${pwSum.total}` : "–"}</b>davon ${pwSum ? pwSum.section.length : "–"} Schnitt, ${pwSum ? pwSum.total - pwSum.section.length : "–"} bisherige</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${cargoSum ? `${cargoSum.ignored} ignoriert` : ""}</div>
  <div>clippy · fmt · actionlint<b class="${clippy != null && !/^(error|warning)/m.test(clippy) && fmt != null && fmt.trim() === "" ? "ok" : "bad"}">${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "Befund" : "sauber"} · ${fmt == null ? "–" : fmt.trim() === "" ? "sauber" : "Abw."} · ${actionlint == null ? "–" : actionlint.trim() === "" ? "sauber" : "Befund"}</b></div>
  <div>Start bis Szene<b>${de(tm.startup?.wallMs)} ms</b>Sitzung → erste vollständige Szene</div>
  <div>Tastendruck → Szene (Median)<b>${de(median(kNative))} / ${de(median(kWasm))} ms</b>nativ (Tauri) / Wasm (Chromium)</div>
  <div>Schnitt verschieben<b>${de(metrics.sectionMoveFps?.fps, 1)} fps</b>Chromium SwiftShader; Tauri ${de(tm.sectionFpsTauri?.fps, 1)}</div>
</div>

<h2>1 Umfang und Architektur</h2>
<p>Die Desktop-Version läuft mit derselben React-App wie der Browser; neu sind die Tauri-App (<code>crates/cad-app</code>), das Backend ohne Tauri (<code>crates/cad-desktop</code>: Engine-Prozess mit Supervisor, Binärformat, Dateien, Beobachter, Git), die lokale IPC (<code>crates/cad-ipc</code>) und im Web <code>TauriTransport</code>, die Desktop-Plattform und die Schnittansicht (Browser und Desktop).</p>
<pre>cad datei.scad  (App-Prozess, Feature gui)
  Hauptthread: Tauri-Ereignisschleife + Webview (React-App, TauriTransport statt Worker)
  Befehle: engine_post · open/save/save_as · watch_files · git_hunks · ui_reply · …
  Kanal (Tauri 2, roh): binäre Batches  ◀──────────── stdout ── cad engine-host (Engine-Prozess)
  Supervisor (cad-desktop::engine) ───── stdin: post [ToEngine…] ─▶  Engine-Thread 256 MiB Stack:
    Watchdog 15 s: Prozess beenden + neu starten, BlockKeys          Session::post / step(16 ms)
  cad-watch: notify → file-changed / file-removed                    SMLib nativ (FFI)
  cad-ui-ipc: Unix-Socket ui-&lt;pid&gt; (cad-ipc) ◀── cad ui status | show | screenshot</pre>
<ul>
<li><b>Engine:</b> native <code>cad-session</code> mit nativ gelinktem SMLib in einem eigenen Prozess (<code>cad engine-host</code>, dasselbe Binary), darin ein Engine-Thread mit 256 MiB Stack und dieselbe Schleife wie der Worker (Nachrichten posten, ein <code>SetSource</code> vor einem weiteren entfällt, dann <code>step(16 ms)</code>, dazwischen neue Nachrichten). Ein Prozess statt nur eines Threads, weil SMLib und seine C-Fassade globalen Zustand haben (Handle-Tabelle, Meldungen): ein hängender Aufruf lässt sich nur mit dem Prozess beenden, ein Ersatz-Thread daneben teilte den Zustand. Der Watchdog im App-Prozess zählt gesendete und übernommene Nachrichten (<code>seen</code>) und ersetzt die Engine nach 15 s ohne Antwort, mit denselben <code>BlockKeys</code>/<code>SetFiles</code>/<code>SetContext</code>/<code>SetSource</code> wie <code>WorkerTransport</code>.</li>
<li><b>Transport:</b> <code>EngineTransport</code> bleibt; <code>TauriTransport</code> bündelt UI-Nachrichten je Microtask in einen Befehl <code>engine_post</code> (Reihenfolge bleibt) und empfängt Batches über einen Tauri-2-Kanal als <code>ArrayBuffer</code>: <code>[u32 Kopflänge][u32 Datenlänge][Kopf-JSON][Daten]</code>, Mesh-Arrays als <code>{"$b": [offset, länge, typ]}</code>, im Webview Typed-Array-Sichten ohne Kopie. Die Desktop-UI wird ohne Worker und Wasm gebaut (<code>pnpm build:tauri</code>) und ins Binary eingebettet.</li>
<li><b>Ein Binary:</b> <code>cad</code> mit Feature <code>gui</code> (Standard). <code>cad</code>, <code>cad datei.scad</code>, <code>cad ordner/</code> starten die UI; ohne <code>DISPLAY</code>/<code>WAYLAND_DISPLAY</code> oder ohne <code>gui</code>: Meldung mit den headless Befehlen, Exit 2. Alle Unterbefehle bleiben.</li>
<li><b>IPC (<code>cad-ipc</code>, wiederverwendbar):</b> Endpunkte als Unix-Sockets in <code>$CAD_IPC_DIR</code> bzw. <code>$XDG_RUNTIME_DIR/controller-cad</code> bzw. <code>$TMPDIR/controller-cad-&lt;uid&gt;</code> (Ordner 0700, Eigentümer geprüft, Sockets 0600, Peer-UID geprüft); Rahmen mit Versionsfeld <code>v</code>, Arten Anfrage/Antwort/Ereignis, Binärteil; veraltete Sockets (Verbindung abgewiesen) werden beim Binden, Verbinden und Auflisten entfernt. API: <code>Server::bind(name)</code>, <code>Server::serve(handler)</code>, <code>connect(name)</code>, <code>Conn::request/request_with/respond/send_event/next_event</code>, <code>list(prefix)</code>.</li>
</ul>

<h2>2 Entscheidungen</h2>
<table>
<tr><th>Thema</th><th>Entscheidung</th><th>Begründung</th></tr>
<tr><td>Git-Anbindung</td><td><code>git</code>-Befehl für die HEAD-Fassung (<code>rev-parse</code>, <code>show HEAD:pfad</code>), Diff im Prozess mit <code>similar</code></td><td>Ein Blob je Datei und HEAD-Stand (gecacht, HEAD höchstens alle 5 s neu geprüft); Tippen startet keinen Prozess. <code>git2</code> bräuchte libgit2 (C-Build), <code>gix</code> brächte weit über hundert Crates in jedes Binary; <code>git</code> ist überall da, wo es Repositories gibt. Unversionierte oder nicht in HEAD enthaltene Dateien: keine Markierung.</td></tr>
<tr><td>Feature <code>gui</code></td><td>Standard an</td><td>Der Workspace enthält <code>cad-app</code> ohnehin (<code>cargo test --workspace</code> baut es), der Mehraufwand sind nur die WebKitGTK-Pakete unter Linux (CI: <code>scripts/ci/desktop.sh deps</code>). Headless: <code>--no-default-features</code>. Ohne <code>web/dist-tauri</code> bettet der Build eine Platzhalterseite ein.</td></tr>
<tr><td>Engine</td><td>eigener Prozess, darin Thread mit großem Stack</td><td>siehe Abschnitt 1 (globaler SMLib-Zustand, Watchdog)</td></tr>
<tr><td>Automatisch speichern</td><td>Standard aus, Verzögerung einstellbar (1000 ms)</td><td>Wer gleichzeitig mit einem Agenten an der Datei arbeitet, soll entscheiden, wann sein Stand auf der Platte landet; die Hinweisleiste schützt beide Seiten.</td></tr>
<tr><td>Beobachter</td><td><code>notify</code> auf die Ordner (nicht die Dateien), Inhaltsvergleich per Hash</td><td>atomares Speichern (Editoren, <code>cad set</code>) ersetzt die Datei; eigene Speichervorgänge melden den neuen Inhalt vorher an und kommen nie als „extern“ zurück.</td></tr>
<tr><td>Neu laden</td><td>als kleinste Zeilenänderungen, eigener Undo-Schritt (<code>isolateHistory</code>)</td><td>Cursor, Auswahl und Scrollposition bleiben, Strg+Z stellt den eigenen Stand wieder her.</td></tr>
</table>

<h2>3 Desktop-App (Tauri-E2E mit tauri-driver und WebKitWebDriver unter Xvfb)</h2>
<p>${tauri ? `Binary <code>${esc(relative(root, tauri.bin))}</code>, ${tauriOk}/${tauri.results.length} bestanden${tauri.fatal ? `, Abbruch: ${esc(tauri.fatal)}` : ""}.` : "nicht gelaufen"} Der Lauf startet die echte App mit Datei über WebDriver, ändert Dateien per Shell, benutzt die echte CLI (<code>cad check</code>, <code>cad set</code>, <code>cad ui …</code>) und ein temporäres Git-Repository. tauri-driver lief unter Xvfb stabil; Bilder: WebDriver-Bildschirmfotos des Webviews und ein X-Bildschirmfoto des Fensters.</p>
${tauriSection("T01")}
${tauriSection("T02", "Die Datei wird außerhalb geändert (<code>k: gehaeuse(w = 60)</code> → <code>w = 90</code>), der Editor hat keine ungespeicherten Änderungen.")}
${tauriSection("T03", "Puffer gegen HEAD: Zeile 21 geändert (blau), eine neue Kommentarzeile (grün), das Modul <code>deckel</code> gelöscht (rotes Dreieck zwischen den Zeilen). Die Markierungen sind 3 px breit mit Deckkraft 0,5–0,55 (hell/dunkel abgestimmt); der Titel (Hover) zeigt die alten Zeilen.")}
${tauriSection("T04")}
${tauriSection("T05")}

<h2>4 Ablauf „Agent ändert Datei → UI aktualisiert sich“</h2>
<p>Der Nutzer hat die Datei in der UI offen, ein Agent ändert sie per Shell und prüft mit der CLI. Jedes Bild ist die UI nach dem Schritt, ohne dass in der UI etwas getan wurde. Zeit vom Schreiben bis zur gezeichneten Szene: ${agentSteps.map((s) => `${de(s.ms)} ms`).join(", ") || "–"} (inkl. <code>cad check</code>/<code>cad set</code>).</p>
${tauriSection("T06")}
${tauriSection("T07", "Kopplung CLI ↔ laufende UI über <code>cad-ipc</code>.")}
${statusText ? `<div class="small">Ausgabe von <code>cad ui status</code>:</div><pre>${esc(statusText.replace(/\/tmp\/cad-desktop-e2e-[^/\s]+/g, "/tmp/…"))}</pre>` : ""}
${tauriSection("T09", "Hängender Kernel (Testnachricht <code>__debugHang</code> im Engine-Prozess, Watchdog für den Test 3 s): (a) im Leerlauf, (b) während eines langsamen Kernel-Jobs (8×8 Bohrungen mit Verrundung, nativ ~3,6 s).")}
${tauriSection("T10")}
${tauriSection("T11")}

<h2>5 3D-Schnittansicht</h2>
<p>Werkzeug „Schnitt“ in der Ansicht (Browser und Tauri): Ebene X/Y/Z durch die Modellmitte oder frei (Normale aus der Blickrichtung oder per Eingabe), Griff (blauer Punkt) zum Ziehen entlang der Normalen mit Rasten auf Rasterebenen des Kontextmoduls und auf Anker (Alt: frei, 0,1 mm), Umkehren, gefüllte Kappe und Kontur. Verfahren wie im CLI-Renderer: Clipping-Ebene an allen Modellmaterialien, je undurchsichtigem Körper ein Paritäts-Pass (Stencil <code>INVERT</code> über alle Fragmente des abgeschnittenen Netzes) und ein großes Viereck in der Ebene, das die Kappe zeichnet, wo das Bit gesetzt ist (tiefengetestet, schraffiert, Richtung je Körper wechselnd). Konvention exakt wie <code>cad render --section</code>: Ebene durch p mit Normale n, entfernt wird die Seite, in die n zeigt; „Als <code>cad render …</code> kopieren“ schreibt <code>--section a=v [--section-flip]</code> bzw. <code>--section-plane p,n</code> mit <code>--module</code> und der Kamera (<code>--azimuth/--elevation</code>).</p>
${["X01", "X02", "X03", "X04", "X05", "X06"].map(scenSection).join("\n")}
${tauriSection("T08")}
${sectionCli ? `<p>Kopierter Aufruf in Tauri: <code>${esc(sectionCli.replace(/\/tmp\/cad-desktop-e2e-[^/\s]+/g, "/tmp/…"))}</code></p>` : ""}

<h2>6 Messwerte</h2>
<table>
<tr><th>Messung</th><th>Wert</th><th>Bemerkung</th></tr>
<tr><td>Start der App bis erste vollständige Szene</td><td>${de(tm.startup?.wallMs)} ms</td><td>WebDriver-Sitzung anlegen (Prozessstart, Webview, Engine-Prozess, SMLib) bis die Szene gezeichnet ist; davon im Webview bis zur ersten Szene ${de(tm.startup?.firstSceneMs)} ms</td></tr>
<tr><td>Tastendruck → Szene, nativ (Tauri)</td><td>Median ${de(median(kNative))} ms (${kNative.map((x) => de(x)).join(", ")})</td><td>Backspace/Ziffer in <code>w = 60</code> (gehaeuse), echte Tastendrücke über WebDriver; Zeit von der Editoränderung bis die Szene gezeichnet ist (inkl. 8 ms Entprellung, WebKitGTK mit Software-GL)</td></tr>
<tr><td>Tastendruck → Szene, Wasm (Chromium)</td><td>Median ${de(median(kWasm))} ms (${kWasm.map((x) => de(x)).join(", ")})</td><td>dieselbe Änderung im Browser (Worker, smlib.wasm), headless Chromium mit SwiftShader</td></tr>
<tr><td>davon Engine (SetSource → vollständige Szene)</td><td>nativ Median ${de(median(tm.keystrokeEvalNative ?? []))} ms, Wasm Median ${de(median(metrics.keystrokeEvalWasm ?? []))} ms</td><td>Auswertung und Kernel ohne Zeichnen, gleiche Tastendrücke</td></tr>
<tr><td>Externe Änderung → Szene</td><td>${de(tm.externalToScene)} ms</td><td>Datei schreiben bis neue Szene gezeichnet (Entprellung 50 ms im Beobachter)</td></tr>
<tr><td>Schnittebene verschieben (Chromium)</td><td>${de(metrics.sectionMoveFps?.fps, 1)} fps</td><td>fillet_zonen, 120 Bilder, jede Lage neu (Kappe, Kontur); Drehen ohne Schnitt ${de(metrics.orbitFpsNoSection?.fps, 1)} fps, mit Schnitt ${de(metrics.orbitFpsWithSection?.fps, 1)} fps – Software-GL ohne GPU, das 60-fps-Ziel betrifft GPUs</td></tr>
<tr><td>Schnittebene verschieben (Tauri)</td><td>${de(tm.sectionFpsTauri?.fps, 1)} fps</td><td>gehaeuse, WebKitGTK unter Xvfb (llvmpipe)</td></tr>
<tr><td>Ziehen am Griff (Chromium)</td><td>${(metrics.sectionDragFps ?? []).map((f) => `${de(f.fps, 0)} fps`).join(", ") || "–"}</td><td>X02: drei echte Mausziehvorgänge (je ~12 Bewegungen), Bildrate der Ansicht während des Ziehens</td></tr>
<tr><td>Kappenanteil UI ↔ CLI</td><td>${metrics.cliCompare ? `${de(metrics.cliCompare.web.share * 100, 1)} % / ${de(metrics.cliCompare.cliPx.share * 100, 1)} %` : "–"}</td><td>Anteil der Kappenpixel an den gezeichneten Pixeln, Abweichung ${metrics.cliCompare ? de(metrics.cliCompare.rel * 100, 0) : "–"} % (Grenze 35 %)</td></tr>
</table>
<p>Die nativen Kernelzeiten sind kürzer als in Wasm (Bericht 7: nativ ↔ Wasm); bei kleinen Modellen wie <i>gehaeuse</i> bestimmen aber Entprellung, Zeichnen und – im Desktop – das Software-Rendering von WebKitGTK unter Xvfb die Zeit vom Tastendruck bis zur Szene, deshalb liegen beide Werte nahe beieinander. Alle Messungen ohne GPU; nur relativ aussagekräftig.</p>

<h2>7 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen, ${cargoSum.ignored} ignoriert` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : /^(error|warning)/m.test(clippy) ? "Befund" : "ohne Befund"}; <code>cargo fmt --check</code>: ${fmt == null ? "–" : fmt.trim() === "" ? "ohne Befund" : "Abweichungen"}.</li>
<li>Web: <code>pnpm typecheck</code> ${typecheck == null ? "–" : typecheck.trim() === "" || /error/i.test(typecheck) === false ? "ohne Befund" : "Befund"}; Vitest ${esc(vitestSum ?? "–")} (neu: <code>tauriTransport.test.ts</code>, <code>textDiff.test.ts</code>, <code>section.test.ts</code>); Playwright ${pwSum ? `${pwSum.passed}/${pwSum.total}` : "–"} (alle bisherigen ${pwSum ? pwSum.total - pwSum.section.length : "–"} Szenarien und ${pwSum ? pwSum.section.length : "–"} neue zur Schnittansicht).</li>
<li>CI: Workflow <code>rust</code> installiert die Tauri-Pakete (<code>scripts/ci/desktop.sh deps</code>), neuer Workflow <code>desktop</code> (UI für Tauri, Release-Build, Tauri-E2E unter Xvfb, Ergebnisse als Artefakt); <code>actionlint</code> 1.7.7 über alle Workflows: ${actionlint == null ? "–" : actionlint.trim() === "" ? "ohne Befund" : esc(actionlint)}.</li>
</ul>
<table><tr><th>Rust-Test (neu)</th><th>Datei</th><th>Ergebnis</th></tr>${newRust.map((t) => `<tr><td>${esc(t.name)}</td><td>${esc(t.file)}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("")}</table>
<p class="small">Backend (<code>cad-desktop/tests/backend.rs</code>): Datei öffnen mit includes, Ordner und Wahl der Hauptdatei, atomares Speichern und zuletzt geöffnet, Beobachter (externe Änderung, eigene Speicherung ignoriert, atomares Ersetzen, gleicher Inhalt, include, fremde Dateien, gelöscht/wieder angelegt, umbenannt), Git-Hunks (Arten, CRLF, Ende) und HEAD-Basis in einem temporären Repository (unversioniert, neuer Commit, Unterordner), Binärformat, Supervisor (Auswertung, Meshes, Watchdog mit Sperre, Watchdog im Leerlauf). IPC (<code>cad-ipc/tests/ipc.rs</code>): Rahmen, Version, Anfrage/Antwort/Ereignis, veraltete Sockets, Rechte, mehrere Clients. CLI (<code>agent.rs</code>, <code>launch.rs</code>): kein Display → Exit 2 + Meldung, <code>cad ui …</code> ohne UI → Exit 3, <code>cad ui</code> gegen einen Test-Endpunkt.</p>

<h2>8 Bekannte Grenzen</h2>
<ul>
<li>Steuer-IPC nur unter Unix (Linux, macOS); unter Windows meldet <code>cad-ipc</code> „nicht umgesetzt“ (Named Pipes fehlen), die App selbst ist dort ungetestet. Ein Fenster je App-Prozess; <code>cad datei.scad</code> startet einen weiteren Prozess statt das laufende Fenster zu nutzen (<code>cad ui show</code> tut das).</li>
<li>Watchdog-Tests blockieren den Engine-Prozess über eine Testnachricht; ein echter hängender SMLib-Aufruf wurde nicht nachgestellt. Die Sperre gilt bis zum Neustart der App.</li>
<li>Beobachter: Umbenennen erscheint als Löschen (Leiste „gelöscht oder umbenannt“), der neue Name wird nicht verfolgt; eine externe Änderung auf genau den Inhalt der UI bleibt (gewollt) unbemerkt.</li>
<li>Git nur gegen HEAD (nicht gegen den Index), über den <code>git</code>-Befehl; ohne <code>git</code> keine Markierungen.</li>
<li>Native Dialoge (Öffnen, Ordner, Speichern unter) sind in den E2E-Tests nicht automatisiert; geprüft sind Öffnen über die Kommandozeile, <code>cad ui show</code>, zuletzt geöffnet (Rust) und Speichern.</li>
<li>Schnitt: eine Ebene; Kappen nur für undurchsichtige, geschlossene Körper; Kontur auf der CPU je Lage; Griffe, Raster und Anker werden nicht abgeschnitten. Der kopierte CLI-Aufruf trifft Schnitt und Blickrichtung, nicht den Bildausschnitt (UI perspektivisch, CLI orthografisch).</li>
<li>Messwerte ohne GPU (Xvfb/llvmpipe, SwiftShader).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${relative(root, join(out, "report.html"))}: Tauri ${tauriOk}/${tauri?.results.length ?? 0}, Playwright ${pwSum?.passed ?? "–"}/${pwSum?.total ?? "–"}, Rust ${cargoSum?.ok ?? "–"}`);
