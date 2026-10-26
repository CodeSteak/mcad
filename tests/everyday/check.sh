#!/usr/bin/env bash
# Regression check of the everyday models (tests/models/everyday).
#
# For each model (each .mcad/.scad file directly in tests/models/everyday; for assembly folders the file
# named after the folder, e.g. housing/housing.mcad):
#   1. `mcad check --json`               – 0 errors, number of warnings as expected
#   2. `mcad export --all-parts` (STEP)  – parts and their volumes (exact B-rep) as expected
#   3. `mcad export` as STL and STEP    – full export succeeds
# into a temporary directory. Exit ≠ 0 if a model deviates or fails.
#
#   tests/everyday/check.sh              check against tests/everyday/expected.json
#   tests/everyday/check.sh --update     rewrite expected.json from the current state
#   tests/everyday/check.sh --bugs       also check the bug examples in tests/everyday/bugs/
#                                      against their target state (head comment `// EXPECT…`)
#                                      (report only: "fixed" / "open")
#   tests/everyday/check.sh -j 4         parallel (default: 4)
#   tests/everyday/check.sh --keep       keep the temporary directory
#   tests/everyday/check.sh PATTERN…     only models whose path contains a PATTERN
#
# CLI: $MCAD (old name $CAD), else <repo>/target/release/mcad, else `mcad` in PATH. Needs python3.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
CAD="${MCAD:-${CAD:-}}"
if [ -z "$CAD" ]; then
    if [ -x "$root/target/release/mcad" ]; then
        CAD="$root/target/release/mcad"
    elif command -v mcad >/dev/null 2>&1; then
        CAD=$(command -v mcad)
    else
        echo "check.sh: no CLI found (set MCAD=… or run cargo build --release -p cad-cli)" >&2
        exit 2
    fi
fi
exec python3 - "$CAD" "$root" "$here" "$@" <<'PY'
import concurrent.futures as cf
import json, os, re, shutil, subprocess, sys, tempfile, time

mcad, root, here = sys.argv[1:4]
args = sys.argv[4:]
update = "--update" in args
bugs = "--bugs" in args
keep = "--keep" in args
jobs = 4
patterns = []
i = 0
while i < len(args):
    a = args[i]
    if a == "-j":
        jobs = int(args[i + 1]); i += 1
    elif a.startswith("-j") and a[2:].isdigit():
        jobs = int(a[2:])
    elif not a.startswith("--"):
        patterns.append(a)
    i += 1

everyday = os.path.join(root, "tests", "models", "everyday")
expected_path = os.path.join(here, "expected.json")
MODEL_EXT = (".mcad", ".scad")
REL_TOL, ABS_TOL = 1e-3, 0.5   # volume: 0.1 % or 0.5 mm³

def models():
    out = []
    for n in sorted(os.listdir(everyday)):
        p = os.path.join(everyday, n)
        if n.endswith(MODEL_EXT) and not n.startswith("_") and os.path.isfile(p):
            out.append(n)
        elif os.path.isdir(p):
            out += [f"{n}/{n}{e}" for e in MODEL_EXT if os.path.isfile(os.path.join(p, n + e))][:1]
    return [m for m in out if not patterns or any(x in m for x in patterns)]

def run(argv, cwd):
    t = time.time()
    r = subprocess.run([mcad] + argv + ["--json"], cwd=cwd, capture_output=True, text=True)
    try:
        d = json.loads(r.stdout)
    except ValueError:
        d = None
    return r.returncode, d, r.stderr.strip(), time.time() - t

def check(m, tmp):
    path = os.path.join(everyday, m)
    cwd = os.path.dirname(path)
    name = m.replace("/", "_")[:-5]
    res = {"model": m, "problems": [], "time": 0.0}
    code, d, err, dt = run(["check", path], cwd)
    res["time"] += dt
    if d is None:
        res["problems"].append(f"check: exit {code}, no JSON output ({err[-200:]})")
        return res
    res["errors"], res["warnings"] = d["errors"], d["warnings"]
    res["diagnostics"] = len(d["diagnostics"])
    for g in d["diagnostics"]:
        if g["severity"] == "error":
            res["problems"].append(f"error l. {g['range']['line']}: {g['message']}")
    # parts individually as STEP
    parts_dir = os.path.join(tmp, name)
    code, d, err, dt = run(["export", path, "--all-parts", "-o", parts_dir + "/", "--format", "step"], cwd)
    res["time"] += dt
    if d is None or code != 0:
        res["problems"].append(f"export --all-parts: exit {code} {err[-200:]}")
        res["parts"] = {}
    else:
        res["parts"] = {p["name"]: round(p["volume"], 1) for p in d.get("parts", [])}
        res["volume"] = round(sum(p["volume"] for p in d.get("parts", [])), 1)
        for p in d.get("parts", []):
            if not os.path.getsize(p["output"]):
                res["problems"].append(f"STEP {p['name']}: empty file")
    # full export STL and STEP
    for fmt in ("stl", "step"):
        out = os.path.join(tmp, f"{name}.{fmt}")
        code, d, err, dt = run(["export", path, "-o", out], cwd)
        res["time"] += dt
        if d is None or code != 0 or not os.path.isfile(out) or os.path.getsize(out) == 0:
            res["problems"].append(f"export {fmt}: exit {code} {err[-200:]}")
        elif fmt == "stl":
            res["volume_stl"] = round(d.get("volume", 0.0), 1)   # tessellation, info only
            res["triangles"] = d.get("triangles")
    return res

def compare(res, exp):
    p = list(res["problems"])
    if exp is None:
        return p + ["not in expected.json (check.sh --update)"]
    if res.get("warnings") != exp["warnings"]:
        p.append(f"warnings {res.get('warnings')} instead of {exp['warnings']}")
    if res.get("diagnostics") != exp["diagnostics"]:
        p.append(f"diagnostics {res.get('diagnostics')} instead of {exp['diagnostics']}")
    if set(res.get("parts", {})) != set(exp["parts"]):
        p.append(f"parts {sorted(res.get('parts', {}))} instead of {sorted(exp['parts'])}")
    for n, v in exp["parts"].items():
        w = res.get("parts", {}).get(n)
        if w is not None and abs(w - v) > max(ABS_TOL, REL_TOL * abs(v)):
            p.append(f"volume {n}: {w} instead of {v} mm³")
    return p

def bug_examples():
    """Minimal examples of the findings (tests/everyday/bugs). Their head comment states the
    target state:
      // EXPECT: error-free           no error
      // EXPECT: no warnings          no warning
      // EXPECT-VOLUME: V TOL         volume V ± TOL mm³
      // EXPECT-VOLUME-MIN: V         volume at least V
      // EXPECT-ERROR: TEXT           an error whose message contains TEXT (no correct result
                                      exists, e.g. operands touching tangentially)
    Always: no crash, JSON output. Reported per file: "fixed" (target reached) or "open"
    (with the first deviation and, if any, the error the CLI reports now)."""
    d = os.path.join(here, "bugs")
    print("\nBug examples (tests/everyday/bugs):")
    n_ok = n_open = 0
    for n in sorted(os.listdir(d)):
        if not n.endswith(MODEL_EXT):
            continue
        f = os.path.join(d, n)
        text = open(f, encoding="utf-8").read()
        code, j, err, _ = run(["measure", f, "--volume"], d)
        findings = []
        reported = ""
        if code < 0 or code == 139 or j is None:
            findings.append(f"crash/no output (exit {code})")
        else:
            errs = [g for g in j["diagnostics"] if g["severity"] == "error"]
            warns = [g for g in j["diagnostics"] if g["severity"] == "warning"]
            vol = (j.get("volume") or {}).get("volume")
            if errs:
                reported = errs[0]["message"][:110]
            if "EXPECT: error-free" in text and errs:
                findings.append("error")
            if "EXPECT: no warnings" in text and warns:
                findings.append(warns[0]["message"][:90])
            m = re.search(r"EXPECT-VOLUME: ([\d.]+) ([\d.]+)", text)
            if m and (vol is None or abs(vol - float(m.group(1))) > float(m.group(2))):
                findings.append(f"volume {vol} instead of {m.group(1)}")
            m = re.search(r"EXPECT-VOLUME-MIN: ([\d.]+)", text)
            if m and (vol is None or vol < float(m.group(1))):
                findings.append(f"volume {vol} < {m.group(1)}")
            m = re.search(r"EXPECT-ERROR: (.+)", text)
            if m and not any(m.group(1).strip() in g["message"] for g in errs):
                findings.append(f'no error message with "{m.group(1).strip()}"')
        if findings:
            n_open += 1
            extra = f" – reported: {reported}" if reported and "error" in findings else ""
            print(f"  open     {n}: {'; '.join(findings)}{extra}")
        else:
            n_ok += 1
            print(f"  fixed    {n}" + (f" (error as intended: {reported})" if reported else ""))
    print(f"  {n_ok} fixed, {n_open} open")

todo = models()
tmp = tempfile.mkdtemp(prefix="everyday-")
expected = {}
if os.path.isfile(expected_path):
    expected = json.load(open(expected_path, encoding="utf-8"))
t0 = time.time()
with cf.ThreadPoolExecutor(max_workers=max(1, jobs)) as ex:
    results = list(ex.map(lambda m: check(m, tmp), todo))
bad = 0
new = {}
for res in results:
    m = res["model"]
    new[m] = {"parts": res.get("parts", {}), "volume": res.get("volume"), "errors": res.get("errors"),
              "warnings": res.get("warnings"), "diagnostics": res.get("diagnostics")}
    problems = res["problems"] if update else compare(res, expected.get(m))
    if problems:
        bad += 1
    nparts = len(res.get("parts", {}))
    print(f"{'ok    ' if not problems else 'ERROR '} {m:40s} {nparts:2d} part(s)  "
          f"V {res.get('volume')} mm³  W {res.get('warnings')}  {res['time']:5.1f} s")
    for p in problems:
        print(f"         {p}")
if update:
    if any(r["problems"] for r in results) and patterns == []:
        print("expected.json NOT written: there are errors", file=sys.stderr)
    else:
        if patterns:
            expected.update(new)
            new = expected
        with open(expected_path, "w", encoding="utf-8") as fh:
            json.dump(dict(sorted(new.items())), fh, indent=2, ensure_ascii=False)
            fh.write("\n")
        print(f"written: {os.path.relpath(expected_path, root)}")
print(f"{len(todo)} models, {bad} with problems, {time.time() - t0:.0f} s, exports in {tmp}"
      + ("" if keep else " (deleted)"))
if bugs:
    bug_examples()
if not keep:
    shutil.rmtree(tmp, ignore_errors=True)
sys.exit(1 if bad else 0)
PY
