"""Check a multi-colour 3MF: structure, colours, extruders, meshes (closed/manifold),
volumes per part, overlaps between parts of one object (sampled), lib3mf readability."""
import sys, zipfile, re, json
import xml.etree.ElementTree as ET
import numpy as np
import trimesh

NS = {"c": "http://schemas.microsoft.com/3dmanufacturing/core/2015/02",
      "m": "http://schemas.microsoft.com/3dmanufacturing/material/2015/02"}


def main(path, overlap_samples=20000):
    z = zipfile.ZipFile(path)
    names = z.namelist()
    print("files:", names)
    model = z.read("3D/3dmodel.model").decode()
    root = ET.fromstring(model)
    print("unit:", root.get("unit"))
    cg = root.findall(".//m:colorgroup", NS)
    colors = [c.get("color") for g in cg for c in g.findall("m:color", NS)]
    print("colorgroups:", len(cg), "colors:", colors)
    objs = {}
    for o in root.findall(".//c:object", NS):
        objs[o.get("id")] = o
    meshes = {}
    for oid, o in objs.items():
        m = o.find("c:mesh", NS)
        if m is None:
            continue
        v = np.array([[float(x.get(a)) for a in "xyz"] for x in m.findall("c:vertices/c:vertex", NS)])
        t = np.array([[int(x.get(a)) for a in ("v1", "v2", "v3")] for x in m.findall("c:triangles/c:triangle", NS)])
        tri_pid = {x.get("pid") for x in m.findall("c:triangles/c:triangle", NS)}
        tm = trimesh.Trimesh(v, t, process=False)
        meshes[oid] = tm
        print(f" mesh obj {oid} name={o.get('name')!r} pid={o.get('pid')} pindex={o.get('pindex')} "
              f"verts={len(v)} tris={len(t)} watertight={tm.is_watertight} winding={tm.is_winding_consistent} "
              f"vol={tm.volume:.4f} tri-pids={tri_pid} bounds={tm.bounds.round(3).tolist()}")
        # manifold edge check: every edge exactly two faces
        e = np.sort(tm.edges, axis=1)
        _, cnt = np.unique(e, axis=0, return_counts=True)
        if (cnt != 2).any():
            print("   NON-MANIFOLD edges:", int((cnt != 2).sum()))
    settings = None
    if "Metadata/model_settings.config" in names:
        settings = ET.fromstring(z.read("Metadata/model_settings.config"))
    for oid, o in objs.items():
        comps = o.findall("c:components/c:component", NS)
        if not comps:
            continue
        print(f" object {oid} name={o.get('name')!r} components={[ (c.get('objectid'), c.get('transform')) for c in comps]}")
        cm = [meshes[c.get("objectid")] for c in comps]
        print(f"   sum vol={sum(m.volume for m in cm):.4f}")
        # overlap check between components by sampling points inside each and testing containment in others
        for i, a in enumerate(cm):
            for j, b in enumerate(cm):
                if j <= i:
                    continue
                if not (a.is_watertight and b.is_watertight):
                    continue
                lo = np.maximum(a.bounds[0], b.bounds[0]); hi = np.minimum(a.bounds[1], b.bounds[1])
                if (hi - lo <= 1e-6).any():
                    continue
                pts = np.random.default_rng(0).uniform(lo, hi, size=(overlap_samples, 3))
                ina = a.contains(pts); inb = b.contains(pts)
                both = (ina & inb).sum()
                if both:
                    frac = both / overlap_samples * np.prod(hi - lo)
                    print(f"   OVERLAP comp {i}/{j}: ~{frac:.4f} mm3 ({both} samples)")
    for it in root.findall(".//c:build/c:item", NS):
        print(" build item", it.attrib)
    if settings is not None:
        for o in settings.findall("object"):
            md = {m.get("key"): m.get("value") for m in o.findall("metadata")}
            print(" settings object", o.get("id"), md)
            for p in o.findall("part"):
                pm = {m.get("key"): m.get("value") for m in p.findall("metadata")}
                print("   part", p.get("id"), p.get("subtype"), pm)
    try:
        import lib3mf
        w = lib3mf.get_wrapper()
        mdl = w.CreateModel()
        r = mdl.QueryReader("3mf")
        r.ReadFromFile(path)
        for i in range(r.GetWarningCount()):
            print(" lib3mf warning:", r.GetWarning(i))
        it = mdl.GetBuildItems()
        n = 0
        while it.MoveNext():
            n += 1
        mi = mdl.GetMeshObjects(); k = 0
        while mi.MoveNext():
            k += 1
        print(f" lib3mf ok: build items={n} mesh objects={k}")
    except Exception as ex:
        print(" lib3mf ERROR", ex)


if __name__ == "__main__":
    main(sys.argv[1])
