// Clickable view cube in the lower right corner of the 3D view (docs/konzepte/ui-ueberarbeitung,
// Frage 4, Option A), instead of the axis gizmo: it turns with the camera; a click on a face
// looks onto it, on an edge or a corner along the diagonal (3 × 3 cells per face: centre =
// face, edge bands = edges, corners = corners). The small axis triad next to it shows x, y, z.
// Drawn as SVG: an orthographic projection maps each face affinely, so the face labels are
// simply transformed text.

import { useEffect, useState } from "react";
import type { Vec3 } from "../engine/protocol";
import { t } from "../i18n";
import type { Viewer } from "../view/viewer";

type V = [number, number, number];

export interface Face {
  id: "front" | "back" | "right" | "left" | "top" | "bottom";
  /** Label (current language). */
  readonly name: string;
  n: V;
  /** Right on the face seen from outside (z up). */
  u: V;
}

const face = (id: Face["id"], n: V, u: V): Face => ({
  id,
  n,
  u,
  get name() {
    return t(`view.${id}`);
  },
});

export const FACES: Face[] = [
  face("front", [0, -1, 0], [1, 0, 0]),
  face("back", [0, 1, 0], [-1, 0, 0]),
  face("right", [1, 0, 0], [0, 1, 0]),
  face("left", [-1, 0, 0], [0, -1, 0]),
  face("top", [0, 0, 1], [1, 0, 0]),
  face("bottom", [0, 0, -1], [1, 0, 0]),
];

const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
/** Down on the face seen from outside: v = u × n (with screen y pointing down). */
const down = (f: Face): V => cross(f.u, f.n);

/** Cell borders along u and v (face from −1 to 1; edge bands 0.35 wide). */
const T = [-1, -0.65, 0.65, 1];

/** Direction of cell (i, j) of a face (towards the camera). */
export function cellDirection(f: Face, i: number, j: number): Vec3 {
  const v = down(f);
  const a = i - 1;
  const b = j - 1;
  const d: V = [0, 1, 2].map((k) => f.n[k] + a * f.u[k] + b * v[k]) as V;
  const l = Math.hypot(...d);
  return d.map((x) => x / l) as Vec3;
}

const AXES: Array<{ label: string; dir: V; cls: string }> = [
  { label: "X", dir: [1, 0, 0], cls: "x" },
  { label: "Y", dir: [0, 1, 0], cls: "y" },
  { label: "Z", dir: [0, 0, 1], cls: "z" },
];

/** Iso view until the viewer reports its camera. */
const ISO: number[] = (() => {
  // camera looking from (1, -1.25, 0.9) with z up
  const f: V = [1, -1.25, 0.9];
  const l = Math.hypot(...f);
  const z = f.map((x) => x / l) as V;
  const xr = cross([0, 0, 1], z);
  const lx = Math.hypot(...xr);
  const x = xr.map((c) => c / lx) as V;
  const y = cross(z, x);
  return [...x, ...y, ...z];
})();

export function ViewCube({ viewer }: { viewer: Viewer | null }) {
  const [rot, setRot] = useState<number[]>(ISO);
  const [hover, setHover] = useState<string | null>(null);
  useEffect(() => (viewer ? viewer.onCamera(setRot) : undefined), [viewer]);
  const S = 25;
  const cam = (w: V): V => [rot[0] * w[0] + rot[1] * w[1] + rot[2] * w[2], rot[3] * w[0] + rot[4] * w[1] + rot[5] * w[2], rot[6] * w[0] + rot[7] * w[1] + rot[8] * w[2]];
  const look = (d: Vec3) => viewer?.viewFrom(d);
  return (
    <div className="viewcube" data-testid="viewcube" role="group" aria-label={t("cube.label")} title={t("cube.hint")}>
      <svg width="132" height="108" viewBox="-78 -54 132 108">
        {FACES.map((f) => {
          const rn = cam(f.n);
          if (rn[2] <= 1e-3) return null;
          const ru = cam(f.u);
          const rv = cam(down(f));
          // local (x along u, y along v) → screen: affine, y of the screen points down
          const m = [S * ru[0], -S * ru[1], S * rv[0], -S * rv[1], S * rn[0], -S * rn[1]].map((x) => +x.toFixed(3));
          const shade = 0.55 + 0.45 * rn[2];
          return (
            <g key={f.id} transform={`matrix(${m.join(" ")})`} className="vc-face" style={{ ["--vc-shade" as string]: shade }}>
              <rect x={-1} y={-1} width={2} height={2} className="vc-base" vectorEffect="non-scaling-stroke" />
              {[0, 1, 2].flatMap((i) =>
                [0, 1, 2].map((j) => {
                  const key = `${f.id}-${i}-${j}`;
                  const center = i === 1 && j === 1;
                  const dir = cellDirection(f, i, j);
                  const id = dir.map((x) => Math.sign(Math.round(x * 100))).join(",");
                  return (
                    <rect
                      key={key}
                      x={T[i]}
                      y={T[j]}
                      width={T[i + 1] - T[i]}
                      height={T[j + 1] - T[j]}
                      className={`vc-cell${hover === id ? " is-hover" : ""}${center ? " vc-center" : ""}`}
                      vectorEffect="non-scaling-stroke"
                      role={center ? "button" : undefined}
                      aria-label={center ? t("cube.face", { name: f.name }) : undefined}
                      aria-hidden={center ? undefined : true}
                      onMouseEnter={() => setHover(id)}
                      onMouseLeave={() => setHover((h) => (h === id ? null : h))}
                      onClick={() => look(dir)}
                    />
                  );
                }),
              )}
              <text x={0} y={0} className="vc-label" fontSize={0.42} textAnchor="middle" dominantBaseline="central" pointerEvents="none">
                {f.name}
              </text>
            </g>
          );
        })}
        <g className="vc-axes" transform="translate(-62 38)" aria-hidden="true">
          {AXES.map((a) => {
            const c = cam(a.dir);
            const x = 13 * c[0];
            const y = -13 * c[1];
            return (
              <g key={a.label} className={`vc-axis ${a.cls}`}>
                <line x1={0} y1={0} x2={x} y2={y} />
                <text x={x * 1.45} y={y * 1.45} textAnchor="middle" dominantBaseline="central">
                  {a.label}
                </text>
              </g>
            );
          })}
        </g>
      </svg>
    </div>
  );
}
