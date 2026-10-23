// Section view tool: toggle in the view toolbar, a bar with axis, flip, value/normal input
// and "Als `cad render …` kopieren", and the status line text.

import { useEffect, useState } from "react";
import type { Vec3 } from "../engine/protocol";
import { fmtNum } from "../logic/format";
import { type SectionState, axisSection, cliCommand, effectiveNormal, freeSection, normalize, offset, withOffset } from "../logic/section";
import { absPath, IS_TAURI } from "../platform/desktop";
import { project } from "../state/project";
import { store, useCad } from "../state/store";
import { getViewer } from "./runtime";

function center(): Vec3 {
  const b = getViewer()?.modelBox;
  if (!b || !Number.isFinite(b.min[0])) return [0, 0, 0];
  return [0, 1, 2].map((k) => (b.min[k] + b.max[k]) / 2) as Vec3;
}

const round = (v: number) => Math.round(v * 10) / 10;

export function setSectionOn(on: boolean): void {
  const s = store.getState().section;
  if (!on) return store.setState({ section: { ...s, on: false } });
  if (s.point.some((x) => x !== 0) || s.axis === "free") return store.setState({ section: { ...s, on: true } });
  store.setState({ section: axisSection("z", round(center()[2])) });
}

export function setSectionAxis(a: "x" | "y" | "z"): void {
  const s = store.getState().section;
  const k = "xyz".indexOf(a);
  store.setState({ section: axisSection(a, round(center()[k]), s.flip) });
}

/** Free plane facing the viewer through the orbit target (removes the half with the camera). */
export function sectionFromView(): void {
  const v = getViewer();
  if (!v) return;
  const t = v.controls.target;
  store.setState({ section: freeSection([t.x, t.y, t.z], v.eyeDir()) });
}

export function flipSection(): void {
  const s = store.getState().section;
  store.setState({ section: { ...s, flip: !s.flip } });
}

export function setSectionValue(t: number): void {
  if (!Number.isFinite(t)) return;
  store.setState({ section: withOffset(store.getState().section, t) });
}

export function setSectionNormal(n: Vec3): void {
  if (n.some((x) => !Number.isFinite(x)) || Math.hypot(...n) < 1e-9) return;
  const s = store.getState().section;
  store.setState({ section: freeSection(s.point, normalize(n), s.flip) });
}

/** File name for the CLI call: the absolute path in the desktop app, else the file name. */
function cliFile(): string {
  return IS_TAURI && project.root ? absPath(project.root, project.main) : project.main;
}

/** `cad render …` with the current section, context module and view direction. */
export function sectionCli(): string {
  const st = store.getState();
  return cliCommand(st.section, cliFile(), st.shown?.context.module ?? null, getViewer()?.eyeDir() ?? null);
}

export async function copySectionCli(): Promise<string> {
  const cmd = sectionCli();
  store.setState({ lastCli: cmd });
  let ok = false;
  try {
    await navigator.clipboard.writeText(cmd);
    ok = true;
  } catch {
    // Fallback without clipboard permission.
    const ta = document.createElement("textarea");
    ta.value = cmd;
    document.body.appendChild(ta);
    ta.select();
    try {
      ok = document.execCommand("copy");
    } catch {
      ok = false;
    }
    ta.remove();
  }
  store.setState({ notice: ok ? `Kopiert: ${cmd}` : `CLI-Aufruf: ${cmd}`, lastCli: cmd });
  return cmd;
}

const v3 = (v: Vec3) => `[${v.map((x) => fmtNum(Math.round(x * 1000) / 1000)).join(", ")}]`;

/** Status line text of the section. */
export function sectionText(s: SectionState): string {
  const n = effectiveNormal(s);
  const axis = ["x", "y", "z"].find((_, k) => Math.abs(Math.abs(n[k]) - 1) < 1e-9);
  if (axis) {
    const k = "xyz".indexOf(axis);
    const v = fmtNum(Math.round(s.point[k] * 1000) / 1000);
    return `Schnitt ${axis} = ${v} · entfernt ${axis} ${n[k] > 0 ? ">" : "<"} ${v}${s.snap ? ` · ${s.snap}` : ""}`;
  }
  return `Schnitt p = ${v3(s.point)}, n = ${v3(n)} · entfernt die Seite, in die n zeigt${s.snap ? ` · ${s.snap}` : ""}`;
}

export function SectionToggle() {
  const on = useCad((s) => s.section.on);
  return (
    <button aria-pressed={on} className={on ? "on" : ""} onClick={() => setSectionOn(!on)} title="3D-Schnittansicht (Ebene ziehen am Griff, Alt: ohne Rasten)" data-testid="section-toggle">
      Schnitt
    </button>
  );
}

export function SectionBar() {
  const s = useCad((st) => st.section);
  const [val, setVal] = useState("");
  const [nrm, setNrm] = useState("");
  useEffect(() => setVal(fmtNum(Math.round(offset(s) * 1000) / 1000)), [s]);
  useEffect(() => setNrm(s.normal.map((x) => fmtNum(Math.round(x * 1000) / 1000)).join(", ")), [s.normal]);
  if (!s.on) return null;
  return (
    <div className="sectionbar" role="toolbar" aria-label="Schnitt" data-testid="sectionbar">
      <span className="stitle">Schnitt</span>
      {(["x", "y", "z"] as const).map((a) => (
        <button key={a} aria-pressed={s.axis === a} className={s.axis === a ? "on" : ""} onClick={() => setSectionAxis(a)} data-testid={`section-axis-${a}`}>
          {a.toUpperCase()}
        </button>
      ))}
      <button aria-pressed={s.axis === "free"} className={s.axis === "free" ? "on" : ""} onClick={sectionFromView} title="Freie Ebene: Normale aus der Blickrichtung" data-testid="section-view">
        Aus Blickrichtung
      </button>
      <button onClick={flipSection} title="Andere Seite behalten (--section-flip)" data-testid="section-flip">
        Umkehren
      </button>
      <label title={s.axis === "free" ? "Abstand der Ebene vom Ursprung entlang n" : `Lage der Ebene (${s.axis})`}>
        {s.axis === "free" ? "p·n" : `${s.axis} =`}
        <input
          value={val}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && setSectionValue(Number(val.replace(",", ".")))}
          onBlur={() => setSectionValue(Number(val.replace(",", ".")))}
          size={7}
          data-testid="section-value"
        />
      </label>
      {s.axis === "free" && (
        <label title="Normale (nx, ny, nz)">
          n =
          <input
            value={nrm}
            onChange={(e) => setNrm(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              const v = nrm.split(/[,;\s]+/).filter(Boolean).map(Number);
              if (v.length === 3) setSectionNormal(v as Vec3);
            }}
            size={14}
            data-testid="section-normal"
          />
        </label>
      )}
      <button onClick={() => void copySectionCli()} title="Gleichen Schnitt in der CLI rendern" data-testid="section-copy">
        Als <code>cad render …</code> kopieren
      </button>
    </div>
  );
}

export function SectionStatus() {
  const s = useCad((st) => st.section);
  if (!s.on) return null;
  return (
    <span className="st-note section" data-testid="status-section" title="Konvention wie cad render --section: es bleibt (q − p) · n ≤ 0">
      {sectionText(s)}
    </span>
  );
}
