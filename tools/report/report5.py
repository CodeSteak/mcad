#!/usr/bin/env python3
"""Testbericht 5 „STEP-Export und Kernel-Vergleich“ erzeugen.

    python3 tools/report/report5.py --run     # alles messen (Tests, verify.py, diff-kernels, Bilder), HTML + PDF
    python3 tools/report/report5.py           # nur HTML/PDF aus target/step/ (vorheriger --run)

Ausgabe: docs/reports/05-step-export/{report.html, report.pdf, img/*.png}.
Zwischenstände: target/step/ (verify-report.json, diff-kernels.json, report5-tests.json, readback/).

Voraussetzungen:
  * SMLIB_PREBUILT (Standard /home/user/controller-cad/smlib/build-native) oder eigener SMLib-Build,
    third_party/smlib (smlib/fetch.sh), emsdk für smlib/build-wasm.sh (Node-Test),
  * ein venv mit cadquery-ocp (OCCT-Python), Standard /opt/tools/venv-step (STEP_PY),
  * Node 22 und playwright in tools/report (``npm i playwright@1.56.1 --no-save``) für das PDF.
"""

from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "reports" / "05-step-export"
IMG = OUT / "img"
WORK = ROOT / "target" / "step"
CAD = ROOT / "target" / "release" / "cad"
STEP_PY = os.environ.get("STEP_PY", "/opt/tools/venv-step/bin/python")
RENDERS = [
    ("14_pipe_bend", "Rohrbogen: Rotationsflächen und Zylinder"),
    ("22_gear", "Zahnrad: 82 Ebenen, 5 Zylinder"),
    ("dose_shell", "Dose (Shell, Verrundungen): Zylinder und Kugeln"),
    ("freiformdeckel", "Freiformdeckel: B-Spline-Flächen"),
    ("knauf_mirror", "Knauf (gespiegelt): Rotation, B-Spline"),
    ("flasche", "Flasche: Rotationsflächen, Kegel"),
]
TITLE = "Testbericht 5 – STEP-Export und Kernel-Vergleich"


def sh(cmd, env: dict | None = None) -> subprocess.CompletedProcess:
    env = dict(env or os.environ)
    env.setdefault("SMLIB_PREBUILT", "/home/user/controller-cad/smlib/build-native")
    print("$", " ".join(map(str, cmd)), flush=True)
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, env=env)


def run_all() -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    IMG.mkdir(parents=True, exist_ok=True)
    tests: dict = {}
    r = sh(["cargo", "build", "--release", "-p", "cad-cli", "--bins", "--examples"])
    if r.returncode:
        sys.exit(r.stderr)
    r = sh(["cargo", "test", "--workspace"])
    passed = sum(int(m) for m in re.findall(r"test result: \w+\. (\d+) passed", r.stdout))
    failed = sum(int(m) for m in re.findall(r"(\d+) failed;", r.stdout))
    tests["cargo test --workspace"] = {"ok": r.returncode == 0, "text": f"{passed} bestanden, {failed} fehlgeschlagen"}
    r = sh(["cargo", "clippy", "--workspace", "--all-targets"])
    warns = re.findall(r"^warning: (?!`)(.*)\n\s*--> ([^:]+)", r.stderr, re.M)
    files = sorted({f for _, f in warns})
    tests["cargo clippy --workspace --all-targets"] = {
        "ok": r.returncode == 0,
        "text": f"{len(warns)} Warnung(en)" + (f" in {', '.join(files)}" if files else ""),
    }
    r = sh(["node", "crates/kernel-smlib/js/test-node.mjs"])
    last = (r.stdout.strip().splitlines() or [""])[-1]
    n_ok = r.stdout.count("\nok ")
    tests["node crates/kernel-smlib/js/test-node.mjs"] = {"ok": r.returncode == 0, "text": f"{n_ok} Tests, {last}"}
    r = sh(["python3", "tests/openscad-compat/compare.py"], env={**os.environ, "CAD": str(CAD)})
    m = re.search(r"(\d+/\d+) files match", r.stdout)
    tests["tests/openscad-compat/compare.py"] = {"ok": r.returncode == 0, "text": (m.group(0) if m else r.stdout[-200:])}
    r = sh([STEP_PY, "tests/step/verify.py", "--cad", str(CAD), "--out", str(WORK), "--stl-dir", str(WORK / "readback")])
    m = re.search(r"(\d+/\d+) passed", r.stdout)
    tests["tests/step/verify.py"] = {"ok": r.returncode == 0, "text": (m.group(0) if m else r.stdout[-200:])}
    models = sorted((ROOT / "tests" / "models" / "openscad-compat").glob("*.scad")) + sorted((ROOT / "tests" / "models" / "spec").glob("*.scad"))
    r = sh([str(CAD), "diff-kernels", *map(str, models), "--json"])
    (WORK / "diff-kernels.json").write_text(r.stdout)
    tests["cad diff-kernels (alle Beispiele)"] = {"ok": r.returncode in (0, 1), "text": f"Exit {r.returncode} (1 = nicht alle Knoten gleich)"}
    for stem, _ in RENDERS:
        sh([str(ROOT / "target" / "release" / "examples" / "render_stl"), str(WORK / "readback" / f"{stem}.stl"),
            str(IMG / f"readback-{stem}.png"), str(WORK / "readback" / f"{stem}.edges.json"), "iso"])
    env = {}
    env["Datum"] = dt.datetime.now().strftime("%Y-%m-%d %H:%M")
    env["Commit"] = sh(["git", "rev-parse", "--short", "HEAD"]).stdout.strip() + " (" + sh(["git", "rev-parse", "--abbrev-ref", "HEAD"]).stdout.strip() + ")"
    env["Betriebssystem"] = f"{platform.system()} {platform.release()}, {os.cpu_count()} Kerne"
    env["Rust"] = sh(["rustc", "--version"]).stdout.strip()
    env["C++"] = (sh(["g++", "--version"]).stdout.splitlines() or [""])[0]
    env["SMLib"] = "tangency-labs/smlib capi-tess @ 7eba8aa, Fassade Version 3 (sc_brep)"
    env["solidcore"] = "tangency-labs/solidcore main @ 3a6a50b (git-Abhängigkeit, Feature `solidcore`)"
    occ = sh([STEP_PY, "-c", "import importlib.metadata as m, sys; print(m.version('cadquery-ocp'), sys.version.split()[0])"]).stdout.split()
    env["STEP-Leser"] = f"OpenCASCADE über cadquery-ocp {occ[0] if occ else '?'} (Python {occ[1] if len(occ) > 1 else '?'}, venv {Path(STEP_PY).parent.parent})"
    env["Node"] = sh(["node", "--version"]).stdout.strip()
    (WORK / "report5-tests.json").write_text(json.dumps({"tests": tests, "env": env}, indent=1))


CSS = (Path(__file__).with_name("report1.py").read_text().split('CSS = """', 1)[1].split('"""', 1)[0])


def esc(x) -> str:
    return html.escape(str(x))


def fmt_e(x) -> str:
    return "–" if x is None else f"{x:.1e}"


def build_html() -> str:
    ver = json.loads((WORK / "verify-report.json").read_text())
    dk = json.loads((WORK / "diff-kernels.json").read_text())
    meta = json.loads((WORK / "report5-tests.json").read_text())
    res = ver["results"]
    n_ok = sum(r["ok"] for r in res)
    surf: dict[str, int] = {}
    curv: dict[str, int] = {}
    for r in res:
        for k, v in (r.get("step_surfaces") or {}).items():
            surf[k] = surf.get(k, 0) + v
        for k, v in (r.get("step_curves") or {}).items():
            curv[k] = curv.get(k, 0) + v
    max_dv = max(r.get("volume_rel", 0) or 0 for r in res)
    max_da = max(r.get("area_rel", 0) or 0 for r in res)
    max_db = max(r.get("bounds_rel", 0) or 0 for r in res)
    approx = sum(r.get("approximated", 0) for r in res)
    s = dk["summary"]
    out: list[str] = []
    a = out.append
    a(f"<!doctype html><html lang='de'><head><meta charset='utf-8'><title>{esc(TITLE)}</title>")
    a(f"<style>{CSS}</style></head><body>")
    a(f"<h1>{esc(TITLE)}</h1>")
    a(f"<div class='sub'>{esc(meta['env']['Datum'])} · Commit {esc(meta['env']['Commit'])}</div>")
    a("<div class='kpis'>")
    a(f"<div class='kpi'><b class='{'ok' if n_ok == len(res) else 'bad'}'>{n_ok}/{len(res)}</b><span>Beispiele: STEP gültig, geschlossen, Volumen/Fläche/Hülle wie Kernel</span></div>")
    a(f"<div class='kpi'><b>{fmt_e(max_dv)}</b><span>größte rel. Volumenabweichung OCCT ↔ SMLib</span></div>")
    a(f"<div class='kpi'><b>{approx}</b><span>approximierte Flächen/Kanten (alles exakt geschrieben)</span></div>")
    a(f"<div class='kpi'><b>{s['same']}/{s['nodes']}</b><span>Knoten mit solidcore gleich (Rest unsupported/blockiert)</span></div>")
    a("</div>")

    a("<h2>1. Umfang</h2><ul>")
    a("<li><b>Fassade</b> (<code>smlib/facade</code>, ABI-Version 2 → 3): neue Abfrage <code>sc_brep(h)</code> liefert Topologie "
      "(Körper → Schalen → Flächen → Schleifen → orientierte Kanten, Vertices) und exakte Geometrie: jede Fläche und Kante als "
      "NURBS (Grad, Knoten, Gewichte, Kontrollpunkte), analytische Flächen (Ebene, Zylinder, Kegel, Kugel, Torus, Rotations-, "
      "Extrusionsfläche) zusätzlich mit ihren Parametern. Nicht-B-Spline-Geometrie (Blends, Offsets) würde auf die Modelltoleranz "
      "approximiert und markiert. Native Seite (FFI) und Wasm-Brücke (<code>smlib-bridge.js</code>, <code>exports.txt</code>).</li>")
    a("<li><b>Neutrale B-Rep</b> <code>cad_kernel::brep</code> mit <code>Kernel::brep</code> (Standard: Unsupported). "
      "<code>kernel-smlib</code> prüft die analytischen Parameter gegen das NURBS (9×9 Stichproben), schneidet Kantenkurven "
      "auf ihr Intervall zu (Knoteneinfügung), erkennt exakte Linien und Kreise und bestimmt <code>same_sense</code> aus "
      "SMLibs Normale an einem inneren Flächenpunkt.</li>")
    a("<li><b>Writer</b> <code>cad-step</code> (kernelunabhängig): AP242 (Standard) oder AP214, Produktstruktur "
      "(<code>PRODUCT</code> … <code>SHAPE_DEFINITION_REPRESENTATION</code>), Einheiten mm und Grad, "
      "<code>MANIFOLD_SOLID_BREP</code>/<code>BREP_WITH_VOIDS</code> je Körper (mehrere Schalen → mehrere Solids), offene "
      "Flächen als <code>SHELL_BASED_SURFACE_MODEL</code>, Farben aus <code>color()</code> als <code>STYLED_ITEM</code> je Fläche.</li>")
    a("<li><b>Anbindung</b>: <code>Engine::export_step</code>, <code>cad export -o x.step [--schema ap214]</code> (JSON mit "
      "Volumen, Fläche, Hülle und Statistik), Session <code>Export{format:\"step\"}</code> (additiv, <code>protocol.ts</code> neu erzeugt), Wasm.</li>")
    a("<li><b>Prüfung</b> <code>tests/step/verify.py</code>: liest jede Datei mit OpenCASCADE (nicht unser Code), "
      "<code>BRepCheck_Analyzer</code>, geschlossene Schalen, Anzahl Solids/Flächen, Volumen und Hülle gegen den Kernel "
      f"(relativ {ver['tolerance_rel']:g}), Fläche ({ver.get('area_tolerance_rel', 1e-4):g}, siehe Abschnitt 7). Rust-Tests des Writers "
      "(Syntax, Referenzen, Zahlen zurückgelesen mit eigenem Part-21-Leser) laufen ohne Python.</li>")
    a("<li><b>solidcore</b>: Crate <code>kernel-solidcore</code>, CLI <code>cad diff-kernels</code>, Doku <code>docs/solidcore-umstieg.md</code>.</li></ul>")
    a("<p><b>Warum AP242:</b> AP242 ist die aktuelle Ausgabe, die AP203 und AP214 zusammenführt und ersetzt; die B-Rep-Entitäten "
      "(Teil 42) sind in beiden gleich, die Wahl ändert nur Kopf und Anwendungskontext. AP214 bleibt per <code>--schema ap214</code> für "
      "ältere Leser, die den AP242-Schemanamen nicht kennen.</p>")

    a("<h2>2. Umgebung</h2><table>")
    for k, v in meta["env"].items():
        a(f"<tr><th style='width:20%'>{esc(k)}</th><td>{esc(v)}</td></tr>")
    a("</table>")

    a("<h2>3. Prüfergebnisse</h2><table>")
    for k, v in meta["tests"].items():
        a(f"<tr><th style='width:42%'><code>{esc(k)}</code></th><td class='{'ok' if v['ok'] else 'bad'}'>{'ok' if v['ok'] else 'FEHLER'}</td><td>{esc(v['text'])}</td></tr>")
    a("</table>")

    a("<h2 class='pagebreak'>4. Alle Beispiele: Kernel ↔ wieder eingelesene STEP-Datei</h2>")
    a("<p class='small'>V: Volumen in mm³ (SMLib <code>mass_props</code> bzw. OCCT, adaptiv integriert 1e-9). dV, dA: relative Abweichung "
      "von Volumen und Fläche; dBox: größte Abweichung einer Hüllkoordinate bezogen auf die Diagonale. „gültig“: BRepCheck gültig, alle "
      "Schalen geschlossen, Solid- und Flächenzahl wie geschrieben. Flächentypen: wie geschrieben (OCCT liest dieselben Typen).</p>")
    a("<table><tr><th>Modell</th><th class='n'>Solids</th><th class='n'>Flächen</th><th class='n'>V Kernel</th><th class='n'>V STEP (OCCT)</th>"
      "<th class='n'>dV</th><th class='n'>dA</th><th class='n'>dBox</th><th>gültig</th><th>Flächentypen</th><th>Kurventypen</th></tr>")
    for r in res:
        c = r.get("checks", {})
        valid = all(c.get(k, False) for k in ("read", "valid", "closed", "solids", "faces"))
        name = Path(r["model"]).stem
        st = ", ".join(f"{k} {v}" for k, v in sorted((r.get("step_surfaces") or {}).items()))
        ct = ", ".join(f"{k} {v}" for k, v in sorted((r.get("step_curves") or {}).items()))
        cls = "ok" if r["ok"] else "bad"
        a(f"<tr><td>{esc(name)}</td><td class='n'>{r.get('occ_solids', '–')}</td><td class='n'>{r.get('occ_faces', '–')}</td>"
          f"<td class='n'>{r.get('kernel_volume') or 0:.3f}</td><td class='n'>{r.get('occ_volume') or 0:.3f}</td>"
          f"<td class='n'>{fmt_e(r.get('volume_rel'))}</td><td class='n'>{fmt_e(r.get('area_rel'))}</td><td class='n'>{fmt_e(r.get('bounds_rel'))}</td>"
          f"<td class='{cls}'>{'ja' if valid else 'nein'}</td><td class='small'>{esc(st)}</td><td class='small'>{esc(ct)}</td></tr>")
    a("</table>")
    a(f"<p>Größte Abweichungen: Volumen {fmt_e(max_dv)}, Fläche {fmt_e(max_da)}, Hülle {fmt_e(max_db)}. "
      "Die Volumenabweichungen über 1e-7 betreffen B-Spline- und Rotationsflächen, bei denen beide Seiten numerisch integrieren.</p>")
    a("<h3>Summen über alle Beispiele</h3><table><tr><th>Flächentyp (STEP-Entität)</th><th class='n'>Anzahl</th><th>Kurventyp</th><th class='n'>Anzahl</th></tr>")
    sn = {"plane": "PLANE", "cylinder": "CYLINDRICAL_SURFACE", "cone": "CONICAL_SURFACE", "sphere": "SPHERICAL_SURFACE",
          "torus": "TOROIDAL_SURFACE", "revolution": "SURFACE_OF_REVOLUTION", "extrusion": "SURFACE_OF_LINEAR_EXTRUSION",
          "bspline": "B_SPLINE_SURFACE_WITH_KNOTS", "nurbs": "… + RATIONAL_B_SPLINE_SURFACE"}
    cn = {"line": "LINE", "circle": "CIRCLE", "ellipse": "ELLIPSE", "bspline": "B_SPLINE_CURVE_WITH_KNOTS", "nurbs": "… + RATIONAL_B_SPLINE_CURVE"}
    sl, cl = sorted(surf.items(), key=lambda x: -x[1]), sorted(curv.items(), key=lambda x: -x[1])
    for i in range(max(len(sl), len(cl))):
        s1 = sl[i] if i < len(sl) else ("", "")
        c1 = cl[i] if i < len(cl) else ("", "")
        a(f"<tr><td>{esc(s1[0])} <span class='small'>{esc(sn.get(s1[0], ''))}</span></td><td class='n'>{s1[1]}</td>"
          f"<td>{esc(c1[0])} <span class='small'>{esc(cn.get(c1[0], ''))}</span></td><td class='n'>{c1[1]}</td></tr>")
    a("</table>")

    a("<h2>5. Wieder eingelesene STEP-Geometrie</h2>")
    a("<p class='small'>OpenCASCADE liest die STEP-Datei, vernetzt sie (Sehnenfehler 0,2 % der Diagonale) und schreibt STL; die Kantenlinien "
      "sind die B-Rep-Kanten der eingelesenen Form. Gerendert mit dem Software-Rasterizer der CLI (<code>cad-render</code>, Beispiel "
      "<code>render_stl</code>). Die Bilder zeigen also die Geometrie aus Sicht eines fremden Kernels.</p><div class='grid2'>")
    for stem, cap in RENDERS:
        if (IMG / f"readback-{stem}.png").exists():
            a(f"<figure><img src='img/readback-{stem}.png' alt='{esc(stem)}'><figcaption><b>{esc(stem)}</b> – {esc(cap)}</figcaption></figure>")
    a("</div>")

    a("<h2>6. Kernel-Vergleich <code>cad diff-kernels</code> (SMLib ↔ solidcore)</h2>")
    a(f"<p>solidcore @ <code>{esc(dk['solidcore_rev'][:7])}</code>, alle {len(dk['files'])} Beispiele, {s['nodes']} ausgewertete Knoten: "
      f"<b>{s['same']}</b> gleich, <b>{s['unsupported']}</b> unsupported, <b>{s['blocked']}</b> blockiert (Eingang unsupported), "
      f"<b>{s['differ']}</b> abweichend, <b>{s['failed']}</b> gescheitert. Gleich sind nur 2D-Knoten (Profile werden in Rust ohne Kernel "
      "berechnet). Jedes Primitiv bricht in solidcore mit <code>Unported: SmSurface::DropCurve (intersect.surface_drop_lib_p3)</code> ab; "
      "Booleans, Abfragen und Massen sind deshalb noch nicht prüfbar. Der Vergleich läuft über alle Dateien ohne Absturz.</p>")
    cov = dk["coverage"]
    kernel_ops = ["box", "sphere", "cylinder", "torus", "polyhedron", "linear_extrude", "rotate_extrude", "loft", "sweep", "surface",
                  "difference", "union", "intersection", "translate", "rotate", "fillet", "chamfer", "shell", "zone"]
    a("<table><tr><th>Operation (Knoten)</th><th class='n'>Knoten</th><th class='n'>gleich</th><th class='n'>unsupported</th><th class='n'>blockiert</th><th class='n'>abweichend</th></tr>")
    for op in kernel_ops + sorted(k for k in cov if k not in kernel_ops and cov[k].get("same", 0) and cov[k].get("nodes", 0) > 3):
        if op not in cov:
            continue
        c = cov[op]
        a(f"<tr><td><code>{esc(op)}</code></td><td class='n'>{c.get('nodes', 0)}</td><td class='n'>{c.get('same', 0)}</td><td class='n'>{c.get('unsupported', 0)}</td>"
          f"<td class='n'>{c.get('blocked', 0)}</td><td class='n'>{c.get('differ', 0) + c.get('failed', 0)}</td></tr>")
    a("</table><p class='small'>Weitere Labels (Modulaufrufe, Gruppen, 2D-Primitive) im JSON <code>target/step/diff-kernels.json</code>. Abnahme "
      "und fehlende Teile: <code>docs/solidcore-umstieg.md</code>.</p>")

    a("<h2>7. Bekannte Grenzen</h2><ul>")
    a("<li><b>Fläche:</b> SMLib integriert Flächen nur auf etwa 1e-4 genau (<code>SmApiBrepComputeArea</code> klemmt die Genauigkeit auf [1e-4, 1e-1]). "
      "Beim <code>freiformdeckel</code> weicht die Kernel-Fläche um 1,9e-4 ab; OCCT adaptiv (6658,641 mm²) und eine feine OCCT-Vernetzung "
      "(6658,656 mm²) stimmen untereinander überein, der Fehler liegt also im Kernel-Referenzwert. Die Flächenprüfung nutzt daher 5e-4, "
      "Volumen und Hülle 1e-4.</li>")
    a("<li>OCCTs Standard-Integration (feste Gauss-Regel) weicht bei B-Spline- und Rotationsflächen um bis zu 1e-4 ab; verify.py integriert adaptiv (1e-9).</li>")
    a("<li>Keine p-Kurven (2D-Kurven im Flächenparameterraum): Leser projizieren die 3D-Kanten. OCCT tut das ohne Fehler für alle Beispiele; "
      "andere Systeme könnten bei B-Spline-Flächen mit Naht langsamer lesen.</li>")
    a("<li>Kantenkurven von Booleans sind B-Splines (Schnittkurven); exakte Kreise/Linien werden erkannt (Toleranz 1e-9 der Modellgröße), "
      "Ellipsen nur, wenn SMLib sie als solche hält (in den Beispielen keine).</li>")
    a("<li>Flächen ohne B-Spline-Form (Blend-, Offsetflächen) würden approximiert; in allen Beispielen liefert SMLib exakte B-Splines oder analytische Flächen (0 approximiert).</li>")
    a("<li>Hohlräume (<code>BREP_WITH_VOIDS</code>) sind im Rust-Test und am Kernel (Kugel minus Kugel) geprüft, in den Beispielen kommen keine vor.</li>")
    a("<li>Farben: je Fläche aus <code>color()</code> als <code>STYLED_ITEM</code>; OCCT liest sie nur über XCAF, verify.py prüft sie nicht.</li>")
    a("<li>Export der Wurzel in ihrem eigenen System (wie STL/3MF); 2D-Ergebnisse werden abgelehnt.</li>")
    a("<li>Im Browser schreibt die Session den Zeitstempel fest (<code>1970-01-01T00:00:00</code>), die CLI die aktuelle UTC-Zeit.</li>")
    a("<li>solidcore: siehe Abschnitt 6; <code>kernel-solidcore</code> deckt nur die heute vorhandenen API-Einstiege ab.</li></ul>")
    a("</body></html>")
    return "\n".join(out)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="alles neu messen")
    ap.add_argument("--no-pdf", action="store_true")
    a = ap.parse_args()
    if a.run:
        run_all()
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "report.html").write_text(build_html())
    print(f"{OUT / 'report.html'}")
    if not a.no_pdf:
        r = sh(["node", "tools/report/print-pdf.mjs", str(OUT / "report.html"), str(OUT / "report.pdf"), TITLE])
        print(r.stdout, r.stderr)


if __name__ == "__main__":
    main()
