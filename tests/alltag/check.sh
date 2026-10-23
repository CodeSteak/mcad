#!/usr/bin/env bash
# Regressionsprüfung der Alltagsmodelle (examples/alltag).
#
# Für jedes Modell (jede .scad-Datei direkt in examples/alltag, bei Baugruppen-Ordnern die Datei
# mit dem Namen des Ordners, z. B. gehaeuse/gehaeuse.scad):
#   1. `cad check --json`               – 0 Fehler, Anzahl der Warnungen wie erwartet
#   2. `cad export --all-parts` (STEP)  – Bauteile und ihre Volumina (exakte B-Rep) wie erwartet
#   3. `cad export` als STL und STEP    – Gesamtexport gelingt
# in ein temporäres Verzeichnis. Exit ≠ 0, wenn ein Modell abweicht oder scheitert.
#
#   tests/alltag/check.sh              prüfen gegen tests/alltag/expected.json
#   tests/alltag/check.sh --update     expected.json aus dem aktuellen Stand neu schreiben
#   tests/alltag/check.sh --fehler     zusätzlich die Fehlerbeispiele in tests/alltag/fehler/
#                                      ausführen (nur Bericht: „besteht“ / „behoben“)
#   tests/alltag/check.sh -j 4         parallel (Standard: 4)
#   tests/alltag/check.sh --keep       temporäres Verzeichnis behalten
#   tests/alltag/check.sh MUSTER…      nur Modelle, deren Pfad ein MUSTER enthält
#
# CLI: $CAD, sonst <repo>/target/release/cad, sonst `cad` im PATH. Braucht python3.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
if [ -z "${CAD:-}" ]; then
    if [ -x "$root/target/release/cad" ]; then
        CAD="$root/target/release/cad"
    elif command -v cad >/dev/null 2>&1; then
        CAD=$(command -v cad)
    else
        echo "check.sh: keine CLI gefunden (CAD=… setzen oder cargo build --release -p cad-cli)" >&2
        exit 2
    fi
fi
exec python3 - "$CAD" "$root" "$here" "$@" <<'PY'
import concurrent.futures as cf
import json, os, re, shutil, subprocess, sys, tempfile, time

cad, root, here = sys.argv[1:4]
args = sys.argv[4:]
update = "--update" in args
fehler = "--fehler" in args
keep = "--keep" in args
jobs = 4
muster = []
i = 0
while i < len(args):
    a = args[i]
    if a == "-j":
        jobs = int(args[i + 1]); i += 1
    elif a.startswith("-j") and a[2:].isdigit():
        jobs = int(a[2:])
    elif not a.startswith("--"):
        muster.append(a)
    i += 1

alltag = os.path.join(root, "examples", "alltag")
expected_path = os.path.join(here, "expected.json")
REL_TOL, ABS_TOL = 1e-3, 0.5   # Volumen: 0,1 % oder 0,5 mm³

def modelle():
    out = []
    for n in sorted(os.listdir(alltag)):
        p = os.path.join(alltag, n)
        if n.endswith(".scad") and not n.startswith("_") and os.path.isfile(p):
            out.append(n)
        elif os.path.isdir(p) and os.path.isfile(os.path.join(p, n + ".scad")):
            out.append(f"{n}/{n}.scad")
    return [m for m in out if not muster or any(x in m for x in muster)]

def run(argv, cwd):
    t = time.time()
    r = subprocess.run([cad] + argv + ["--json"], cwd=cwd, capture_output=True, text=True)
    try:
        d = json.loads(r.stdout)
    except ValueError:
        d = None
    return r.returncode, d, r.stderr.strip(), time.time() - t

def pruefe(m, tmp):
    datei = os.path.join(alltag, m)
    cwd = os.path.dirname(datei)
    name = m.replace("/", "_")[:-5]
    res = {"modell": m, "probleme": [], "zeit": 0.0}
    code, d, err, dt = run(["check", datei], cwd)
    res["zeit"] += dt
    if d is None:
        res["probleme"].append(f"check: Exit {code}, keine JSON-Ausgabe ({err[-200:]})")
        return res
    res["errors"], res["warnings"] = d["errors"], d["warnings"]
    res["diagnostics"] = len(d["diagnostics"])
    for g in d["diagnostics"]:
        if g["severity"] == "error":
            res["probleme"].append(f"Fehler Z.{g['range']['line']}: {g['message']}")
    # Bauteile einzeln als STEP
    teile_dir = os.path.join(tmp, name)
    code, d, err, dt = run(["export", datei, "--all-parts", "-o", teile_dir + "/", "--format", "step"], cwd)
    res["zeit"] += dt
    if d is None or code != 0:
        res["probleme"].append(f"export --all-parts: Exit {code} {err[-200:]}")
        res["parts"] = {}
    else:
        res["parts"] = {p["name"]: round(p["volume"], 1) for p in d.get("parts", [])}
        res["volume"] = round(sum(p["volume"] for p in d.get("parts", [])), 1)
        for p in d.get("parts", []):
            if not os.path.getsize(p["output"]):
                res["probleme"].append(f"STEP {p['name']}: leere Datei")
    # Gesamtexport STL und STEP
    for fmt in ("stl", "step"):
        out = os.path.join(tmp, f"{name}.{fmt}")
        code, d, err, dt = run(["export", datei, "-o", out], cwd)
        res["zeit"] += dt
        if d is None or code != 0 or not os.path.isfile(out) or os.path.getsize(out) == 0:
            res["probleme"].append(f"export {fmt}: Exit {code} {err[-200:]}")
        elif fmt == "stl":
            res["volume_stl"] = round(d.get("volume", 0.0), 1)   # Tessellierung, nur Info
            res["triangles"] = d.get("triangles")
    return res

def vergleiche(res, exp):
    p = list(res["probleme"])
    if exp is None:
        return p + ["nicht in expected.json (check.sh --update)"]
    if res.get("warnings") != exp["warnings"]:
        p.append(f"Warnungen {res.get('warnings')} statt {exp['warnings']}")
    if res.get("diagnostics") != exp["diagnostics"]:
        p.append(f"Diagnosen {res.get('diagnostics')} statt {exp['diagnostics']}")
    if set(res.get("parts", {})) != set(exp["parts"]):
        p.append(f"Bauteile {sorted(res.get('parts', {}))} statt {sorted(exp['parts'])}")
    for n, v in exp["parts"].items():
        w = res.get("parts", {}).get(n)
        if w is not None and abs(w - v) > max(ABS_TOL, REL_TOL * abs(v)):
            p.append(f"Volumen {n}: {w} statt {v} mm³")
    return p

def fehlerbeispiele():
    d = os.path.join(here, "fehler")
    print("\nFehlerbeispiele (tests/alltag/fehler, nur Bericht):")
    for n in sorted(os.listdir(d)):
        if not n.endswith(".scad"):
            continue
        f = os.path.join(d, n)
        text = open(f, encoding="utf-8").read()
        code, j, err, _ = run(["measure", f, "--volume"], d)
        befund = []
        if code < 0 or code == 139 or j is None:
            befund.append(f"Absturz/keine Ausgabe (Exit {code})")
        else:
            errs = [g for g in j["diagnostics"] if g["severity"] == "error"]
            warns = [g for g in j["diagnostics"] if g["severity"] == "warning"]
            vol = (j.get("volume") or {}).get("volume")
            if "ERWARTET: fehlerfrei" in text and errs:
                befund.append(errs[0]["message"][:90])
            if "ERWARTET: keine Warnungen" in text and warns:
                befund.append(warns[0]["message"][:90])
            m = re.search(r"ERWARTET-VOLUMEN: ([\d.]+) ([\d.]+)", text)
            if m and (vol is None or abs(vol - float(m.group(1))) > float(m.group(2))):
                befund.append(f"Volumen {vol} statt {m.group(1)}")
            m = re.search(r"ERWARTET-VOLUMEN-MIN: ([\d.]+)", text)
            if m and (vol is None or vol < float(m.group(1))):
                befund.append(f"Volumen {vol} < {m.group(1)}")
        print(f"  {'besteht' if befund else 'behoben'}  {n}" + (f": {'; '.join(befund)}" if befund else ""))

liste = modelle()
tmp = tempfile.mkdtemp(prefix="alltag-")
expected = {}
if os.path.isfile(expected_path):
    expected = json.load(open(expected_path, encoding="utf-8"))
t0 = time.time()
with cf.ThreadPoolExecutor(max_workers=max(1, jobs)) as ex:
    ergebnisse = list(ex.map(lambda m: pruefe(m, tmp), liste))
schlecht = 0
neu = {}
for res in ergebnisse:
    m = res["modell"]
    neu[m] = {"parts": res.get("parts", {}), "volume": res.get("volume"), "errors": res.get("errors"),
              "warnings": res.get("warnings"), "diagnostics": res.get("diagnostics")}
    probleme = res["probleme"] if update else vergleiche(res, expected.get(m))
    if probleme:
        schlecht += 1
    teile = len(res.get("parts", {}))
    print(f"{'ok    ' if not probleme else 'FEHLER'} {m:40s} {teile:2d} Bauteil(e)  "
          f"V {res.get('volume')} mm³  W {res.get('warnings')}  {res['zeit']:5.1f} s")
    for p in probleme:
        print(f"         {p}")
if update:
    if any(r["probleme"] for r in ergebnisse) and muster == []:
        print("expected.json NICHT geschrieben: es gibt Fehler", file=sys.stderr)
    else:
        if muster:
            expected.update(neu)
            neu = expected
        with open(expected_path, "w", encoding="utf-8") as fh:
            json.dump(dict(sorted(neu.items())), fh, indent=2, ensure_ascii=False)
            fh.write("\n")
        print(f"geschrieben: {os.path.relpath(expected_path, root)}")
print(f"{len(liste)} Modelle, {schlecht} mit Problemen, {time.time() - t0:.0f} s, Exporte in {tmp}"
      + ("" if keep else " (gelöscht)"))
if fehler:
    fehlerbeispiele()
if not keep:
    shutil.rmtree(tmp, ignore_errors=True)
sys.exit(1 if schlecht else 0)
PY
