"""Shared helpers for the OpenSCAD comparison scripts (no third-party dependencies).

- `read_stl(path)`: triangles of an ASCII or binary STL.
- `mesh_stats(tris)`: volume (divergence theorem), surface area, bounding box, triangle count.
- `classify(src)`: tolerance class of a model ("polygonal" or "rounded") and whether it sets
  `$fn`/`$fa`/`$fs` globally.
- `fnv1a64(data)`: source hash, identical to the one in `crates/compat-tests`.
"""

from __future__ import annotations

import math
import re
import struct
from pathlib import Path

Vec = tuple[float, float, float]
Tri = tuple[Vec, Vec, Vec]


def read_stl(path: Path) -> list[Tri]:
    data = Path(path).read_bytes()
    if _is_binary(data):
        (n,) = struct.unpack_from("<I", data, 80)
        tris = []
        off = 84
        for _ in range(n):
            v = struct.unpack_from("<12f", data, off)
            tris.append(((v[3], v[4], v[5]), (v[6], v[7], v[8]), (v[9], v[10], v[11])))
            off += 50
        return tris
    verts: list[Vec] = []
    for line in data.decode("ascii", errors="replace").splitlines():
        parts = line.split()
        if len(parts) == 4 and parts[0] == "vertex":
            verts.append((float(parts[1]), float(parts[2]), float(parts[3])))
    if len(verts) % 3:
        raise ValueError(f"{path}: vertex count {len(verts)} is not a multiple of 3")
    return [(verts[i], verts[i + 1], verts[i + 2]) for i in range(0, len(verts), 3)]


def _is_binary(data: bytes) -> bool:
    if len(data) < 84:
        return False
    (n,) = struct.unpack_from("<I", data, 80)
    if 84 + 50 * n == len(data):
        return True
    return not data.lstrip()[:5].lower() == b"solid"


def write_ascii_stl(path: Path, tris: list[Tri]) -> None:
    out = ["solid mesh"]
    for a, b, c in tris:
        out.append("  facet normal 0 0 0\n    outer loop")
        for v in (a, b, c):
            out.append(f"      vertex {v[0]!r} {v[1]!r} {v[2]!r}")
        out.append("    endloop\n  endfacet")
    out.append("endsolid mesh\n")
    Path(path).write_text("\n".join(out))


def mesh_stats(tris: list[Tri]) -> dict:
    vol6 = 0.0
    area2 = 0.0
    lo = [math.inf] * 3
    hi = [-math.inf] * 3
    for a, b, c in tris:
        ux, uy, uz = b[0] - a[0], b[1] - a[1], b[2] - a[2]
        vx, vy, vz = c[0] - a[0], c[1] - a[1], c[2] - a[2]
        cx, cy, cz = uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx
        area2 += math.sqrt(cx * cx + cy * cy + cz * cz)
        # Divergence theorem: signed volume of the tetrahedron (origin, a, b, c).
        vol6 += (
            a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0])
        )
        for v in (a, b, c):
            for k in range(3):
                lo[k] = min(lo[k], v[k])
                hi[k] = max(hi[k], v[k])
    if not tris:
        lo = hi = [0.0, 0.0, 0.0]
    return {
        "volume": vol6 / 6.0,
        "area": area2 / 2.0,
        "bounds": {"min": lo, "max": hi},
        "triangles": len(tris),
    }


def fnv1a64(data: bytes) -> str:
    h = 0xCBF29CE484222325
    for byte in data:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


# --- tolerance classes -----------------------------------------------------------------

_ROUND_CALLS = ("sphere", "cylinder", "circle")
_GLOBAL_FN = re.compile(r"^\s*\$f[nas]\s*=", re.M)
_CLASS_OVERRIDE = re.compile(r"//\s*compat-class:\s*(polygonal|rounded)")


def _strip_comments(src: str) -> str:
    src = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    return re.sub(r"//[^\n]*", " ", src)


def _call_args(src: str, open_paren: int) -> str:
    depth = 0
    for i in range(open_paren, len(src)):
        if src[i] == "(":
            depth += 1
        elif src[i] == ")":
            depth -= 1
            if depth == 0:
                return src[open_paren + 1 : i]
    return src[open_paren + 1 :]


def classify(src: str) -> dict:
    """Tolerance class of a model.

    "polygonal": every sphere/cylinder/circle call has `$fn` = 3…12 directly in its
    arguments, and there is no rotate_extrude, no twisted linear_extrude and no offset(r)
    (rounded joins; offset(delta) keeps polygons). Then
    OpenSCAD and we build the same polygons (plan, section 12 item 3).
    "rounded": anything else; OpenSCAD facets, we are exact.
    A comment `// compat-class: polygonal|rounded` overrides the heuristic.
    """
    override = _CLASS_OVERRIDE.search(src)
    code = _strip_comments(src)
    reasons = []
    for m in re.finditer(r"\b(sphere|cylinder|circle|rotate_extrude|linear_extrude|offset)\s*\(", code):
        name = m.group(1)
        args = _call_args(code, m.end() - 1)
        if name == "offset":
            # offset(r) (also positional or without arguments) rounds with arcs; delta does not
            if not re.search(r"\bdelta\s*=", args) or re.search(r"\br\s*=", args):
                reasons.append("offset(r)")
        elif name == "rotate_extrude":
            reasons.append("rotate_extrude")
        elif name == "linear_extrude":
            t = re.search(r"\btwist\s*=\s*(-?[\d.]+)", args)
            if t and float(t.group(1)) != 0.0:
                reasons.append("linear_extrude(twist)")
        else:
            fn = re.search(r"\$fn\s*=\s*([\d.]+)", args)
            if not (fn and 3 <= float(fn.group(1)) < 13):
                reasons.append(name)
    cls = "rounded" if reasons else "polygonal"
    if override:
        cls = override.group(1)
    return {
        "class": cls,
        "round_features": sorted(set(reasons)),
        "global_fn": bool(_GLOBAL_FN.search(code)),
    }
