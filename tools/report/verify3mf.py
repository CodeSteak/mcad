#!/usr/bin/env python3
"""Independent check of a multi-colour 3MF from `cad export` (report 11).

Usage: verify3mf.py FILE.3mf [--expect measure.json] [--json out.json]

1. lib3mf (official 3MF consortium library, `pip install lib3mf`): reads the package and
   reports warnings, objects, components (transforms), the colour group, object-level
   properties, manifoldness and mesh volumes.
2. trimesh (`pip install trimesh networkx lxml`): loads the scene, volume per geometry.
3. A model of the Bambu Studio / OrcaSlicer importer, written from
   `src/libslic3r/Format/bbs_3mf.cpp` (2026-10-23): tags compared literally (`m:colorgroup`,
   `m:color`), build item → components → sub-objects (`_generate_current_object_list`),
   `Metadata/model_settings.config` `<part id>` matched to the sub-object id
   (`_generate_volumes_new`), volume `extruder` from the part metadata; objects with a single
   volume use the object's `extruder` (the volume value is erased after loading).
4. PrusaSlicer CLI (`prusa-slicer --info`), if installed: number of objects and volumes.
5. With `--expect` (`cad measure --colors --json`): every region (part, colour, filament) has a
   volume with this colour and extruder whose mesh volume agrees within 0.5 %.

Exit code 0 if all checks pass, 1 otherwise.
"""

import argparse
import json
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
import xml.parsers.expat
import zipfile


def mesh_volume(verts, tris):
    v = 0.0
    for a, b, c in tris:
        ax, ay, az = verts[a]
        bx, by, bz = verts[b]
        cx, cy, cz = verts[c]
        v += (ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx)) / 6.0
    return v


def check_lib3mf(path, out):
    import lib3mf

    w = lib3mf.get_wrapper()
    m = w.CreateModel()
    r = m.QueryReader("3mf")
    r.ReadFromFile(path)
    res = {"version": ".".join(map(str, w.GetLibraryVersion())), "warnings": [r.GetWarning(i)[1] if isinstance(r.GetWarning(i), tuple) else str(r.GetWarning(i)) for i in range(r.GetWarningCount())]}
    colors = {}
    cg = m.GetColorGroups()
    while cg.MoveNext():
        g = cg.GetCurrentColorGroup()
        for pid in g.GetAllPropertyIDs():
            c = g.GetColor(pid)
            colors[(g.GetResourceID(), pid)] = "#%02X%02X%02X%02X" % (c.Red, c.Green, c.Blue, c.Alpha)
    res["colors"] = list(colors.values())
    meshes, comps = {}, {}
    it = m.GetObjects()
    while it.MoveNext():
        o = it.GetCurrentObject()
        rid = o.GetResourceID()
        if o.IsMeshObject():
            mo = m.GetMeshObjectByID(rid)
            n = mo.GetVertexCount()
            verts = [tuple(mo.GetVertex(i).Coordinates) for i in range(n)]
            tris = [tuple(mo.GetTriangle(i).Indices) for i in range(mo.GetTriangleCount())]
            res_id, prop, has = mo.GetObjectLevelProperty()
            meshes[rid] = {
                "name": o.GetName(),
                "triangles": len(tris),
                "manifold": bool(mo.IsManifoldAndOriented()),
                "volume": mesh_volume(verts, tris),
                "color": colors.get((res_id, prop)) if has else None,
            }
        elif o.IsComponentsObject():
            co = m.GetComponentsObjectByID(rid)
            comps[rid] = {
                "name": o.GetName(),
                "components": [co.GetComponent(k).GetObjectResourceID() for k in range(co.GetComponentCount())],
                "transforms": [bool(co.GetComponent(k).HasTransform()) for k in range(co.GetComponentCount())],
            }
    items = []
    bi = m.GetBuildItems()
    while bi.MoveNext():
        b = bi.GetCurrent()
        items.append({"object": b.GetObjectResourceID(), "transform": bool(b.HasObjectTransform())})
    res.update(meshes=meshes, components=comps, items=items)
    ok = not res["warnings"] and all(x["manifold"] for x in meshes.values())
    ok = ok and all(not any(c["transforms"]) for c in comps.values()) and all(not i["transform"] for i in items)
    # colours are optional (report 12: no colour group by default); if there is a group, every
    # colour mesh needs one
    ok = ok and (not colors or all(x["color"] for x in meshes.values()))
    res["ok"] = ok
    out["lib3mf"] = res
    return ok


def check_trimesh(path, out):
    import trimesh

    sc = trimesh.load(path, force="scene")
    geoms = {name: float(g.volume) for name, g in sc.geometry.items()}
    out["trimesh"] = {"version": trimesh.__version__, "geometries": len(geoms), "volumes": geoms,
                      "watertight": all(g.is_watertight for g in sc.geometry.values())}
    return out["trimesh"]["watertight"]


def bambu_import(path, out):
    """What the Bambu Studio / OrcaSlicer importer assigns (model of bbs_3mf.cpp)."""
    z = zipfile.ZipFile(path)
    names = z.namelist()
    # Like Bambu: expat without namespace processing, element names as written (`m:colorgroup`).
    groups = {}
    objects = {}
    items = []
    state = {"app": None, "meta": None, "group": None, "obj": None, "text": ""}

    def start(name, attrs):
        if name == "metadata":
            state["meta"] = attrs.get("name")
            state["text"] = ""
        elif name == "m:colorgroup":
            state["group"] = int(attrs["id"])
            groups[state["group"]] = []
        elif name == "m:color" and state["group"] is not None:
            groups[state["group"]].append(attrs.get("color"))
        elif name == "object":
            oid = int(attrs["id"])
            objects[oid] = {
                "name": attrs.get("name"),
                "pid": int(attrs["pid"]) if "pid" in attrs else None,
                "pindex": int(attrs["pindex"]) if "pindex" in attrs else None,
                "components": [],
            }
            state["obj"] = oid
        elif name == "component" and state["obj"] is not None:
            objects[state["obj"]]["components"].append(int(attrs["objectid"]))
        elif name == "item":
            items.append(int(attrs["objectid"]))

    def end(name):
        if name == "metadata" and state["meta"] == "Application":
            state["app"] = state["text"].strip()
        if name == "metadata":
            state["meta"] = None

    def chars(t):
        if state["meta"] is not None:
            state["text"] += t

    px = xml.parsers.expat.ParserCreate()
    px.StartElementHandler = start
    px.EndElementHandler = end
    px.CharacterDataHandler = chars
    px.Parse(z.read("3D/3dmodel.model"), True)
    app = state["app"]
    cfg = {}
    if "Metadata/model_settings.config" in names:
        root = ET.fromstring(z.read("Metadata/model_settings.config"))
        for o in root.iter("object"):
            meta = {m.get("key"): m.get("value") for m in o.findall("metadata")}
            parts = {}
            for p in o.findall("part"):
                parts[int(p.get("id"))] = {m.get("key"): m.get("value") for m in p.findall("metadata")}
            cfg[int(o.get("id"))] = {"meta": meta, "parts": parts}
    is_bbl = bool(app and app.startswith("BambuStudio-"))
    result = []
    for oid in items:
        o = objects[oid]
        subs = o["components"] or [oid]
        c = cfg.get(oid)
        vols = []
        for sid in subs:
            s = objects[sid]
            meta = (c or {}).get("parts", {}).get(sid, {})
            color = None
            if s["pid"] in groups and s["pindex"] is not None:
                color = groups[s["pid"]][s["pindex"]]
            vols.append({"sub_object": sid, "name": meta.get("name", s["name"]), "extruder": meta.get("extruder"),
                         "color": color})
        obj_ext = (c or {}).get("meta", {}).get("extruder")
        if len(vols) == 1:
            # bbs_3mf.cpp: a single volume's extruder is erased, the object's counts
            vols[0]["extruder"] = obj_ext or "1"
        result.append({"object": oid, "name": (c or {}).get("meta", {}).get("name", o["name"]),
                       "extruder": obj_ext, "volumes": vols})
    # Plater.cpp (report 12): a foreign 3MF with a colour group and triangle colours opens the
    # colour dialog, whose OK replaces the extruders; without it the slots stay.
    dialog = bool(groups) and any(objects[s]["pid"] in groups for o in objects.values() for s in (o["components"] or []))
    out["bambu_model"] = {"application": app, "treated_as_bambu_project": is_bbl,
                          "project_settings": "Metadata/project_settings.config" in names,
                          "color_dialog": dialog, "objects": result}
    ok = all(v["extruder"] for o in result for v in o["volumes"]) and not is_bbl
    out["bambu_model"]["ok"] = ok
    return ok


def check_prusa(path, out):
    exe = shutil.which("prusa-slicer")
    if not exe:
        out["prusaslicer"] = None
        return True
    ver = subprocess.run([exe, "--help"], capture_output=True, text=True, timeout=60).stdout.splitlines()[:1]
    p = subprocess.run([exe, "--info", path], capture_output=True, text=True, timeout=300)
    blocks = []
    cur = None
    for line in p.stdout.splitlines():
        if line.startswith("["):
            cur = {}
            blocks.append(cur)
        elif "=" in line and cur is not None:
            k, v = [x.strip() for x in line.split("=", 1)]
            cur[k] = v
    out["prusaslicer"] = {"version": ver[0] if ver else "", "objects": len(blocks),
                          "volumes": [float(b.get("volume", "nan")) for b in blocks],
                          "manifold": [b.get("manifold") for b in blocks], "exit": p.returncode}
    return p.returncode == 0 and all(b.get("manifold") == "yes" for b in blocks)


def check_expect(expect_path, out):
    exp = json.load(open(expect_path))
    regions = exp["colors"]["parts"]
    labels = {c["key"]: c for c in exp["colors"]["colors"]}
    meshes = list(out["lib3mf"]["meshes"].values())
    bm = out["bambu_model"]["objects"]
    rows = []
    ok = True
    for set_ in regions:
        part = set_["part"]
        for r in set_["regions"]:
            label = labels[r["color"]]["label"]
            name = f"{part} – {label}" if part else None
            mesh = next((m for m in meshes if name is None or m["name"] == name), None)
            vol = next((v for o in bm for v in o["volumes"] if v["name"] == name), None)
            want_color = "#" + r["color"].lstrip("#").upper() + "FF"
            row = {"part": part, "color": label, "filament": r["filament"], "volume_brep": r["volume"],
                   "volume_mesh": mesh["volume"] if mesh else None,
                   "extruder_bambu": vol["extruder"] if vol else None, "color_3mf": mesh["color"] if mesh else None}
            good = (mesh is not None and vol is not None and str(r["filament"]) == str(vol["extruder"])
                    and (mesh["color"] is None or mesh["color"] == want_color)
                    and abs(mesh["volume"] - r["volume"]) <= 0.005 * r["volume"])
            row["ok"] = good
            ok &= good
            rows.append(row)
    out["expect"] = rows
    return ok


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--expect")
    ap.add_argument("--json")
    a = ap.parse_args()
    out = {"file": a.file}
    results = {
        "lib3mf": check_lib3mf(a.file, out),
        "trimesh": check_trimesh(a.file, out),
        "bambu_model": bambu_import(a.file, out),
        "prusaslicer": check_prusa(a.file, out),
    }
    if a.expect:
        results["expect"] = check_expect(a.expect, out)
    out["results"] = results
    out["ok"] = all(results.values())
    if a.json:
        json.dump(out, open(a.json, "w"), indent=1)
    lm = out["lib3mf"]
    print(f"lib3mf {lm['version']}: {len(lm['meshes'])} meshes, {len(lm['components'])} component objects, "
          f"{len(lm['items'])} build items, {len(lm['colors'])} colours, warnings {len(lm['warnings'])}, "
          f"all manifold: {all(m['manifold'] for m in lm['meshes'].values())}")
    tm = out["trimesh"]
    print(f"trimesh {tm['version']}: {tm['geometries']} geometries, watertight: {tm['watertight']}")
    for o in out["bambu_model"]["objects"]:
        vols = ", ".join(f"{v['name']} → extruder {v['extruder']} {v['color']}" for v in o["volumes"])
        print(f"Bambu/Orca import (model): object {o['name']}: {vols}")
    if out.get("prusaslicer"):
        ps = out["prusaslicer"]
        print(f"PrusaSlicer ({ps['version']}): {ps['objects']} objects, exit {ps['exit']}")
    for r in out.get("expect", []):
        print(f"  {r['part']} {r['color']}: F{r['filament']} Bambu {r['extruder_bambu']}, B-rep {r['volume_brep']:.2f} "
              f"mesh {r['volume_mesh']:.2f} mm³ {'ok' if r['ok'] else 'ERROR'}")
    print("all checks passed" if out["ok"] else f"ERROR: {results}")
    sys.exit(0 if out["ok"] else 1)


if __name__ == "__main__":
    main()
