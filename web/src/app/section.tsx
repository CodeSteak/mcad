// Section view tool: mode "Schnitt" of the view toolbar, the context bar (axis or plane from the
// view direction, value/normal input, flip, "Als `cad render …` kopieren") and the status text.

import { useEffect, useState } from "react";
import type { Vec3 } from "../engine/protocol";
import { fmtNum } from "../logic/format";
import { type SectionState, axisSection, cliCommand, effectiveNormal, freeSection, normalize, offset, withOffset } from "../logic/section";
import { absPath, IS_TAURI } from "../platform/desktop";
import { project } from "../state/project";
import { store, useCad } from "../state/store";
import { getViewer } from "./runtime";
import { IconButton, IconCopy, IconFlip, Segmented, Toolbar, ToolGroup, ToolSep } from "./ui";

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

export function SectionBar() {
  const s = useCad((st) => st.section);
  const [val, setVal] = useState("");
  const [nrm, setNrm] = useState("");
  useEffect(() => setVal(fmtNum(Math.round(offset(s) * 1000) / 1000)), [s]);
  useEffect(() => setNrm(s.normal.map((x) => fmtNum(Math.round(x * 1000) / 1000)).join(", ")), [s.normal]);
  if (!s.on) return null;
  const axis = s.axis === "free" ? "view" : s.axis;
  return (
    <Toolbar label="Schnitt" float className="contextbar sectionbar" data-testid="sectionbar">
      <Segmented
        label="Schnittebene"
        value={axis}
        onChange={(v) => (v === "view" ? sectionFromView() : setSectionAxis(v))}
        options={[
          { value: "x", label: "X", hint: "Ebene senkrecht zur x-Achse durch die Modellmitte" },
          { value: "y", label: "Y", hint: "Ebene senkrecht zur y-Achse durch die Modellmitte" },
          { value: "z", label: "Z", hint: "Ebene senkrecht zur z-Achse durch die Modellmitte" },
          { value: "view", label: "Aus Blickrichtung", text: "Blick", hint: "Freie Ebene: Normale aus der Blickrichtung" },
        ]}
      />
      <ToolSep />
      <label className="ui-field" title={s.axis === "free" ? "Abstand der Ebene vom Ursprung entlang n" : `Lage der Ebene (${s.axis})`}>
        <span className="ui-label">{s.axis === "free" ? "p·n =" : `${s.axis} =`}</span>
        <input
          className="ui-input"
          value={val}
          aria-label={s.axis === "free" ? "Lage der Ebene (p·n)" : `Lage der Ebene (${s.axis})`}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && setSectionValue(Number(val.replace(",", ".")))}
          onBlur={() => setSectionValue(Number(val.replace(",", ".")))}
          size={6}
          data-testid="section-value"
        />
      </label>
      {s.axis === "free" && (
        <label className="ui-field" title="Normale (nx, ny, nz)">
          <span className="ui-label">n =</span>
          <input
            className="ui-input"
            value={nrm}
            aria-label="Normale der Ebene"
            onChange={(e) => setNrm(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              const v = nrm.split(/[,;\s]+/).filter(Boolean).map(Number);
              if (v.length === 3) setSectionNormal(v as Vec3);
            }}
            size={12}
            data-testid="section-normal"
          />
        </label>
      )}
      <ToolSep />
      <ToolGroup>
        <IconButton icon={IconFlip} label="Umkehren" hint="Andere Seite behalten (--section-flip)" pressed={s.flip} onClick={flipSection} />
        <IconButton icon={IconCopy} label="Als cad render … kopieren" hint="Gleichen Schnitt in der CLI rendern" onClick={() => void copySectionCli()} />
      </ToolGroup>
    </Toolbar>
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
