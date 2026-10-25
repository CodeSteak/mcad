#!/usr/bin/env python3
"""Independent check of the STEP export with OpenCASCADE (OCP, `pip install cadquery-ocp`).

For every model: `mcad export <model> -o <x>.step --merge --json` writes the STEP file (the
root as one fused body, so that it compares with the root) and reports the kernel's volume,
area and bounds (SMLib `mass_props`/`bounds` of the root) and what the writer wrote. With
`--bodies` the default export is checked instead (one body per part / top-level object): read,
valid, closed, solids, faces and bounds as below, the volume unless overlapping bodies had to be
written apart (note "getrennt geschrieben" from `mcad export`; overlapping bodies are otherwise
fused into one), the area not (touching bodies keep their contact faces). The file is then read back with OCCT's STEP reader (not our code) and checked:

  read      STEPControl_Reader reads the file and transfers a non-empty shape
  valid     BRepCheck_Analyzer reports the shape valid
  closed    every shell of every solid is closed (BRep_Tool::IsClosed)
  solids    number of solids (and open shells) equals what the writer reported
  faces     number of faces equals the writer's
  volume    |V_occ - V_kernel| <= 1e-4 * V_kernel       (solids only; OCCT integrates
            adaptively to 1e-9, its default Gauss rule is off by up to 1e-4 on B-spline faces)
  area      |A_occ - A_kernel| <= 5e-4 * A_kernel  (SMLib's own area is only ~1e-4 accurate,
            see AREA_REL)
  bounds    every coordinate of OCCT's optimal box within 1e-4 * diagonal of the kernel's

Surface types are counted from OCCT's adaptor (GeomAbs_*) for the table.

Usage (the venv lives outside the repository, e.g. /opt/tools/venv-step):
  /opt/tools/venv-step/bin/python tests/step/verify.py [--cad target/release/mcad]
      [--out target/step] [--json report.json] [--stl-dir DIR] [models...]
Default models: examples/openscad-compat/*.scad and examples/spec/*.mcad. $MCAD ($CAD) overrides the
binary like compare.py. Exit code 0 if every model passes, 1 otherwise, 2 for setup errors.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
REL = 1e-4
# SMLib integrates areas to about 1e-4 only (SmApiBrepComputeArea clamps its accuracy to
# [1e-4, 1e-1]); measured: freiformdeckel kernel 6659.89, OCCT adaptive 6658.641, OCCT mesh
# (0.002 mm) 6658.656, i.e. the kernel's area is the one that is off (1.9e-4).
AREA_REL = 5e-4
OCC_EPS = 1e-9

try:
    from OCP.Bnd import Bnd_Box
    from OCP.BRep import BRep_Tool
    from OCP.BRepAdaptor import BRepAdaptor_Curve, BRepAdaptor_Surface
    from OCP.BRepBndLib import BRepBndLib
    from OCP.BRepCheck import BRepCheck_Analyzer
    from OCP.BRepGProp import BRepGProp
    from OCP.BRepMesh import BRepMesh_IncrementalMesh
    from OCP.GeomAbs import (
        GeomAbs_Line,
        GeomAbs_BezierSurface,
        GeomAbs_BSplineSurface,
        GeomAbs_Cone,
        GeomAbs_Cylinder,
        GeomAbs_OffsetSurface,
        GeomAbs_Plane,
        GeomAbs_Sphere,
        GeomAbs_SurfaceOfExtrusion,
        GeomAbs_SurfaceOfRevolution,
        GeomAbs_Torus,
    )
    from OCP.GProp import GProp_GProps
    from OCP.IFSelect import IFSelect_RetDone
    from OCP.STEPControl import STEPControl_Reader
    from OCP.StlAPI import StlAPI_Writer
    from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_SHELL, TopAbs_SOLID
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopoDS import TopoDS
except ImportError as e:  # pragma: no cover
    print(f"verify.py: OCP not available ({e}); install with `pip install cadquery-ocp` in a venv",
          file=sys.stderr)
    sys.exit(2)

SURF = {
    GeomAbs_Plane: "plane",
    GeomAbs_Cylinder: "cylinder",
    GeomAbs_Cone: "cone",
    GeomAbs_Sphere: "sphere",
    GeomAbs_Torus: "torus",
    GeomAbs_BezierSurface: "bezier",
    GeomAbs_BSplineSurface: "bspline",
    GeomAbs_SurfaceOfRevolution: "revolution",
    GeomAbs_SurfaceOfExtrusion: "extrusion",
    GeomAbs_OffsetSurface: "offset",
}


def explore(shape, kind):
    ex = TopExp_Explorer(shape, kind)
    while ex.More():
        yield ex.Current()
        ex.Next()


def read_step(path: Path):
    r = STEPControl_Reader()
    if r.ReadFile(str(path)) != IFSelect_RetDone:
        return None
    r.TransferRoots()
    s = r.OneShape()
    return None if s.IsNull() else s


def rel_ok(a: float, b: float) -> bool:
    return abs(a - b) <= REL * max(abs(b), 1e-12)


def check(model: Path, mcad: str, out: Path, stl_dir: Path | None, bodies: bool = False) -> dict:
    res: dict = {"model": str(model.relative_to(ROOT)) if model.is_relative_to(ROOT) else str(model)}
    step = out / (model.stem + ".step")
    t0 = time.time()
    p = subprocess.run([mcad, "export", str(model), "-o", str(step), "--json"] + ([] if bodies else ["--merge"]),
                       capture_output=True, text=True, timeout=600)
    res["export_s"] = round(time.time() - t0, 3)
    try:
        j = json.loads(p.stdout)
    except json.JSONDecodeError:
        res["error"] = f"mcad export: exit {p.returncode}: {p.stderr.strip()[:300]}"
        return res
    if not step.exists() or "step" not in j:
        res["error"] = f"mcad export: {j.get('error') or 'no STEP written'}"
        return res
    st = j["step"]
    res.update(kernel_volume=j.get("volume"), kernel_area=j.get("area"), kernel_bounds=j.get("bounds"),
               step_bytes=st["bytes"], step_solids=st["solids"], step_sheets=st["sheets"],
               step_voids=st["voids"], step_faces=st["faces"], step_edges=st["edges"],
               step_surfaces=st["surfaces"], step_curves=st["curves"],
               approximated=st["approximated_faces"] + st["approximated_edges"],
               eval_errors=not j.get("ok", True))
    t0 = time.time()
    shape = read_step(step)
    res["read_s"] = round(time.time() - t0, 3)
    checks: dict[str, bool] = {}
    checks["read"] = shape is not None
    if shape is None:
        res["checks"] = checks
        return res
    checks["valid"] = BRepCheck_Analyzer(shape).IsValid()
    solids = list(explore(shape, TopAbs_SOLID))
    shells_closed = [BRep_Tool.IsClosed_s(sh) for so in solids for sh in explore(so, TopAbs_SHELL)]
    checks["closed"] = all(shells_closed) if st["solids"] else True
    open_shells = sum(1 for sh in explore(shape, TopAbs_SHELL) if not BRep_Tool.IsClosed_s(sh))
    res["occ_solids"] = len(solids)
    checks["solids"] = len(solids) == st["solids"] and (st["sheets"] == 0 or open_shells >= st["sheets"])
    faces = list(explore(shape, TopAbs_FACE))
    res["occ_faces"] = len(faces)
    checks["faces"] = len(faces) == st["faces"]
    kinds: dict[str, int] = {}
    for f in faces:
        k = SURF.get(BRepAdaptor_Surface(TopoDS.Face(f)).GetType(), "other")
        kinds[k] = kinds.get(k, 0) + 1
    res["occ_surfaces"] = dict(sorted(kinds.items()))

    # adaptive integration (OCCT's default Gauss rule is off by up to 1e-4 on B-spline faces)
    g = GProp_GProps()
    BRepGProp.VolumeProperties_s(shape, g, OCC_EPS)
    res["occ_volume"] = g.Mass()
    g = GProp_GProps()
    BRepGProp.SurfaceProperties_s(shape, g, OCC_EPS)
    res["occ_area"] = g.Mass()
    several = bodies and len(st.get("bodies", [])) > 1
    res["bodies"] = st.get("bodies", [])
    overlap = any("getrennt geschrieben" in n for n in j.get("notes") or [])
    if st["solids"] and res["kernel_volume"] is not None and not (several and overlap):
        res["volume_rel"] = abs(res["occ_volume"] - res["kernel_volume"]) / max(abs(res["kernel_volume"]), 1e-12)
        checks["volume"] = rel_ok(res["occ_volume"], res["kernel_volume"])
    if res["kernel_area"] is not None and not several:
        res["area_rel"] = abs(res["occ_area"] - res["kernel_area"]) / max(abs(res["kernel_area"]), 1e-12)
        checks["area"] = res["area_rel"] <= AREA_REL
    box = Bnd_Box()
    BRepBndLib.AddOptimal_s(shape, box, False, False)
    lo = box.CornerMin()
    hi = box.CornerMax()
    res["occ_bounds"] = {"min": [lo.X(), lo.Y(), lo.Z()], "max": [hi.X(), hi.Y(), hi.Z()]}
    kb = res["kernel_bounds"]
    if kb:
        kmin, kmax = kb["min"], kb["max"]
        diag = sum((b - a) ** 2 for a, b in zip(kmin, kmax)) ** 0.5
        dev = max(max(abs(a - b) for a, b in zip(kmin, res["occ_bounds"]["min"])),
                  max(abs(a - b) for a, b in zip(kmax, res["occ_bounds"]["max"])))
        res["bounds_rel"] = dev / max(diag, 1e-12)
        checks["bounds"] = dev <= REL * diag
    if stl_dir is not None:
        stl_dir.mkdir(parents=True, exist_ok=True)
        BRepMesh_IncrementalMesh(shape, 0.002 * max(diag if kb else 1.0, 1.0), False, 0.2, True)
        w = StlAPI_Writer()
        w.Write(shape, str(stl_dir / (model.stem + ".stl")))
        # B-rep edges of the read-back shape as polylines (for renderings)
        polys = []
        for e in explore(shape, TopAbs_EDGE):
            e = TopoDS.Edge(e)
            if BRep_Tool.Degenerated_s(e):
                continue
            c = BRepAdaptor_Curve(e)
            t0, t1 = c.FirstParameter(), c.LastParameter()
            n = 1 if c.GetType() == GeomAbs_Line else 32
            pts = []
            for i in range(n + 1):
                p = c.Value(t0 + (t1 - t0) * i / n)
                pts.append([p.X(), p.Y(), p.Z()])
            polys.append(pts)
        (stl_dir / (model.stem + ".edges.json")).write_text(json.dumps(polys))
    res["checks"] = checks
    return res


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("models", nargs="*", type=Path)
    ap.add_argument("--cad", default=os.environ.get("MCAD", os.environ.get("CAD", str(ROOT / "target" / "release" / "mcad"))))
    ap.add_argument("--out", type=Path, default=ROOT / "target" / "step")
    ap.add_argument("--json", type=Path, default=None, help="report file (default <out>/verify-report.json)")
    ap.add_argument("--stl-dir", type=Path, default=None, help="write the read-back shapes as STL here")
    ap.add_argument("--bodies", action="store_true", help="check the default export with separate bodies")
    a = ap.parse_args()
    models = a.models or sorted((ROOT / "examples" / "openscad-compat").glob("*.scad")) + sorted(
        (ROOT / "examples" / "spec").glob("*.mcad"))
    if not Path(a.cad).exists() and "/" in a.cad:
        print(f"verify.py: {a.cad} not found (cargo build --release -p cad-cli)", file=sys.stderr)
        return 2
    a.out.mkdir(parents=True, exist_ok=True)
    results = []
    failed = 0
    print(f"{'model':<28} {'ok':<4} {'solids':>6} {'faces':>5} {'V kernel':>14} {'V OCCT':>14} {'dV rel':>9} "
          f"{'dA rel':>9} {'dBox':>9}  surfaces (OCCT)")
    for m in models:
        m = m.resolve()
        r = check(m, a.cad, a.out, a.stl_dir, a.bodies)
        ok = "error" not in r and all(r.get("checks", {}).values()) and bool(r.get("checks"))
        r["ok"] = ok
        failed += not ok
        results.append(r)
        bad = [k for k, v in r.get("checks", {}).items() if not v]
        fmt = lambda x: "-" if x is None else f"{x:.3e}"  # noqa: E731
        kv = r.get("kernel_volume")
        print(f"{m.stem:<28} {'ok' if ok else 'FAIL':<4} {r.get('occ_solids', '-'):>6} {r.get('occ_faces', '-'):>5} "
              f"{'-' if kv is None else f'{kv:.6g}':>14} {'-' if 'occ_volume' not in r else format(r['occ_volume'], '.6g'):>14} "
              f"{fmt(r.get('volume_rel')):>9} {fmt(r.get('area_rel')):>9} {fmt(r.get('bounds_rel')):>9}  "
              f"{r.get('occ_surfaces', '')} {r.get('error', '')} {' '.join(bad)}")
    report = a.json or a.out / "verify-report.json"
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps({"tolerance_rel": REL, "area_tolerance_rel": AREA_REL, "results": results}, indent=1))
    print(f"\n{len(results) - failed}/{len(results)} passed; report {report}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
