// GPU section view: clipping plane, stencil caps, cut contour, plane outline and grip.
//
// Same principle and convention as the CLI renderer (crates/cad-render): the plane through p
// with normal n removes the half n points into ((q − p) · n ≤ 0 stays). Model materials get a
// three.js clipping plane (normal −n, constant n · p: three keeps distance ≥ 0). For every
// opaque solid a parity pass (stencil INVERT on every fragment of the clipped mesh, both
// faces, no depth test, no colour) marks the pixels where a ray crosses its remaining surface
// an odd number of times — where the plane lies inside the solid; then a large quad on the
// plane draws the cap where the stencil bit is set (depth tested, so other geometry hides it)
// and clears the bit. Caps are hatched in screen space, the direction alternating per solid
// like the CLI. Transparent bodies are clipped but get no cap. Moving the plane only updates
// uniforms, the quads and the contour (no rebuild).

import * as THREE from "three";
import type { Vec3 } from "../engine/protocol";
import { type SectionState, effectiveNormal } from "../logic/section";

export interface CapBody {
  /** Group with the node's placement (the mesh is drawn in it). */
  group: THREE.Group;
  geometry: THREE.BufferGeometry;
  key: string;
}

const CAP_VERT = /* glsl */ `
void main() {
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;

const CAP_FRAG = /* glsl */ `
uniform vec3 color;
uniform float dir;
uniform float pitch;
void main() {
  float h = dir > 0.5 ? gl_FragCoord.x + gl_FragCoord.y : gl_FragCoord.x - gl_FragCoord.y;
  float k = mod(floor(h), pitch) < 1.0 ? 0.72 : 1.0;
  gl_FragColor = vec4(color * 0.92 * k, 1.0);
}`;

export class SectionLayer {
  /** The clipping plane shared by all model materials (updated in place). */
  readonly plane = new THREE.Plane(new THREE.Vector3(0, 0, -1), 0);
  readonly group = new THREE.Group();
  private parity: THREE.MeshBasicMaterial;
  private caps: THREE.ShaderMaterial[] = [];
  private quad = new THREE.PlaneGeometry(1, 1);
  private contour: THREE.LineSegments | null = null;
  private contourMat: THREE.LineBasicMaterial | null = null;
  /** Reused buffer of the contour segments (grows when needed). */
  private contourBuf = new Float32Array(0);
  private outline: THREE.LineLoop | null = null;
  private grip: THREE.Mesh | null = null;
  private bodies: CapBody[] = [];
  private capMeshes: THREE.Mesh[] = [];
  private worldPos = new Map<string, Float32Array>();
  /** Grip position (plane centre on the model) and the box the plane is sized to. */
  center = new THREE.Vector3();
  size = 100;
  /** Pixels covered by caps are counted by tests from the canvas; this is the cut length. */
  contourSegments = 0;
  state: SectionState | null = null;

  constructor(
    private capColor: string,
    private lineColor: string,
    private accent: string,
  ) {
    this.group.name = "section";
    this.parity = new THREE.MeshBasicMaterial({
      colorWrite: false,
      depthWrite: false,
      depthTest: false,
      side: THREE.DoubleSide,
      stencilWrite: true,
      stencilFunc: THREE.AlwaysStencilFunc,
      stencilWriteMask: 1,
      stencilFail: THREE.InvertStencilOp,
      stencilZFail: THREE.InvertStencilOp,
      stencilZPass: THREE.InvertStencilOp,
      clippingPlanes: [this.plane],
    });
  }

  setColors(cap: string, line: string, accent: string): void {
    this.capColor = cap;
    this.lineColor = line;
    this.accent = accent;
    this.contourMat?.color.set(line);
    for (const c of this.caps) (c.uniforms.color.value as THREE.Color).set(cap).convertLinearToSRGB();
  }

  private capMaterial(i: number): THREE.ShaderMaterial {
    while (this.caps.length <= i) {
      const k = this.caps.length;
      this.caps.push(
        new THREE.ShaderMaterial({
          vertexShader: CAP_VERT,
          fragmentShader: CAP_FRAG,
          uniforms: { color: { value: new THREE.Color(this.capColor).convertLinearToSRGB() }, dir: { value: k % 2 === 0 ? 1 : 0 }, pitch: { value: 7 * Math.min(2, window.devicePixelRatio || 1) } },
          side: THREE.DoubleSide,
          depthTest: true,
          depthWrite: true,
          stencilWrite: true,
          stencilRef: 1,
          stencilFuncMask: 1,
          stencilWriteMask: 1,
          stencilFunc: THREE.EqualStencilFunc,
          stencilFail: THREE.ZeroStencilOp,
          stencilZFail: THREE.ZeroStencilOp,
          stencilZPass: THREE.ZeroStencilOp,
          polygonOffset: true,
          polygonOffsetFactor: -1,
          polygonOffsetUnits: -1,
        }),
      );
    }
    return this.caps[i];
  }

  /** Clears everything this layer added to the model groups and its own group. */
  clear(): void {
    for (const m of this.capMeshes) m.parent?.remove(m);
    this.capMeshes = [];
    this.bodies = [];
    for (const c of [...this.group.children]) {
      this.group.remove(c);
      const g = (c as THREE.Mesh).geometry;
      if (g && g !== this.quad) g.dispose();
    }
    this.contour = null;
    this.contourBuf = new Float32Array(0);
    this.outline = null;
    this.grip = null;
  }

  /**
   * Builds the section for the drawn model: `bodies` get caps; `box` sizes plane and quads.
   * Called by the viewer's build (state on).
   */
  build(s: SectionState, bodies: CapBody[], box: { min: Vec3; max: Vec3 }): void {
    this.clear();
    this.state = s;
    this.bodies = bodies;
    const min = new THREE.Vector3(...box.min);
    const max = new THREE.Vector3(...box.max);
    const c = min.clone().add(max).multiplyScalar(0.5);
    this.size = Math.max(1, min.distanceTo(max)) * 1.15;
    this.center.copy(c);
    bodies.forEach((b, i) => {
      const m = new THREE.Mesh(b.geometry, this.parity);
      m.renderOrder = 1000 + i * 2;
      m.name = "section-parity";
      b.group.add(m);
      this.capMeshes.push(m);
      const cap = new THREE.Mesh(this.quad, this.capMaterial(i));
      cap.renderOrder = 1000 + i * 2 + 1;
      cap.name = "section-cap";
      cap.matrixAutoUpdate = false;
      this.group.add(cap);
    });
    const ol = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(-0.5, -0.5, 0), new THREE.Vector3(0.5, -0.5, 0), new THREE.Vector3(0.5, 0.5, 0), new THREE.Vector3(-0.5, 0.5, 0)]);
    this.outline = new THREE.LineLoop(ol, new THREE.LineDashedMaterial({ color: this.accent, dashSize: this.size * 0.02, gapSize: this.size * 0.012, transparent: true, opacity: 0.75, depthTest: false }));
    this.outline.matrixAutoUpdate = false;
    this.outline.renderOrder = 70;
    this.outline.name = "section-outline";
    this.group.add(this.outline);
    this.grip = new THREE.Mesh(new THREE.SphereGeometry(1, 20, 12), new THREE.MeshBasicMaterial({ color: this.accent, depthTest: false, transparent: true, opacity: 0.95 }));
    this.grip.renderOrder = 71;
    this.grip.name = "section-grip";
    this.group.add(this.grip);
    this.update(s);
  }

  /** Moves the plane (uniforms, quads, contour) without a rebuild. */
  update(s: SectionState): void {
    this.state = s;
    const n = new THREE.Vector3(...effectiveNormal(s));
    const p = new THREE.Vector3(...s.point);
    this.plane.normal.copy(n).negate();
    this.plane.constant = n.dot(p);
    // Plane frame: centred where the model centre projects onto the plane.
    const c = this.center.clone().sub(n.clone().multiplyScalar(n.dot(this.center.clone().sub(p))));
    const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 0, 1), n);
    const m = new THREE.Matrix4().compose(c, q, new THREE.Vector3(this.size * 2, this.size * 2, 1));
    const mo = new THREE.Matrix4().compose(c, q, new THREE.Vector3(this.size, this.size, 1));
    for (const ch of this.group.children) {
      if (ch.name === "section-cap") {
        ch.matrix.copy(m);
        ch.matrixWorldNeedsUpdate = true;
      }
    }
    if (this.outline) {
      this.outline.matrix.copy(mo);
      this.outline.matrixWorldNeedsUpdate = true;
      this.outline.computeLineDistances();
    }
    if (this.grip) {
      this.grip.position.copy(c);
      this.grip.scale.setScalar(this.size * 0.018);
    }
    this.gripPos.copy(c);
    this.updateContour(p, n);
  }

  /** Grip position (context coordinates). */
  readonly gripPos = new THREE.Vector3();

  private positions(b: CapBody): Float32Array {
    b.group.updateMatrixWorld(true);
    const key = `${b.key}|${b.group.matrix.elements.join(",")}`;
    let w = this.worldPos.get(key);
    if (!w) {
      if (this.worldPos.size > 64) this.worldPos.clear();
      const src = b.geometry.getAttribute("position") as THREE.BufferAttribute;
      w = new Float32Array(src.count * 3);
      const v = new THREE.Vector3();
      for (let i = 0; i < src.count; i++) {
        v.fromBufferAttribute(src, i).applyMatrix4(b.group.matrix);
        w[i * 3] = v.x;
        w[i * 3 + 1] = v.y;
        w[i * 3 + 2] = v.z;
      }
      this.worldPos.set(key, w);
    }
    return w;
  }

  /** Cut contour: triangle × plane segments of the capped solids. */
  private updateContour(p: THREE.Vector3, n: THREE.Vector3): void {
    const seg: number[] = [];
    const pd = n.dot(p);
    for (const b of this.bodies) {
      const w = this.positions(b);
      const idx = b.geometry.index;
      if (!idx) continue;
      const ia = idx.array;
      for (let t = 0; t + 2 < ia.length; t += 3) {
        const a = ia[t] * 3;
        const bb = ia[t + 1] * 3;
        const cc = ia[t + 2] * 3;
        const da = n.x * w[a] + n.y * w[a + 1] + n.z * w[a + 2] - pd;
        const db = n.x * w[bb] + n.y * w[bb + 1] + n.z * w[bb + 2] - pd;
        const dc = n.x * w[cc] + n.y * w[cc + 1] + n.z * w[cc + 2] - pd;
        if ((da > 0 && db > 0 && dc > 0) || (da <= 0 && db <= 0 && dc <= 0)) continue;
        const pts: number[] = [];
        const edge = (i: number, j: number, di: number, dj: number) => {
          if (di <= 0 !== dj <= 0) {
            const f = di / (di - dj);
            pts.push(w[i] + (w[j] - w[i]) * f, w[i + 1] + (w[j + 1] - w[i + 1]) * f, w[i + 2] + (w[j + 2] - w[i + 2]) * f);
          }
        };
        edge(a, bb, da, db);
        edge(bb, cc, db, dc);
        edge(cc, a, dc, da);
        if (pts.length === 6) seg.push(...pts);
      }
    }
    this.contourSegments = seg.length / 6;
    // One line object, its buffer reused while the plane moves (no allocation per frame).
    this.contourMat ??= new THREE.LineBasicMaterial({ color: this.lineColor, depthFunc: THREE.LessEqualDepth });
    if (!this.contour) {
      this.contour = new THREE.LineSegments(new THREE.BufferGeometry(), this.contourMat);
      this.contour.name = "section-contour";
      this.contour.frustumCulled = false;
      this.group.add(this.contour);
    }
    this.contour.renderOrder = 1000 + this.bodies.length * 2 + 2;
    const g = this.contour.geometry;
    if (this.contourBuf.length < seg.length) {
      this.contourBuf = new Float32Array(Math.max(seg.length, this.contourBuf.length * 2, 192));
      g.setAttribute("position", new THREE.BufferAttribute(this.contourBuf, 3));
    }
    this.contourBuf.set(seg);
    const attr = g.getAttribute("position") as THREE.BufferAttribute | undefined;
    if (attr) {
      attr.needsUpdate = true;
      attr.clearUpdateRanges();
      attr.addUpdateRange(0, seg.length);
    }
    g.setDrawRange(0, seg.length / 3);
    this.contour.visible = seg.length > 0;
  }

  dispose(): void {
    this.clear();
    this.parity.dispose();
    this.contourMat?.dispose();
    for (const c of this.caps) c.dispose();
    this.quad.dispose();
  }
}
