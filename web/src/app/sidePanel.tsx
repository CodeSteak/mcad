// Side panel on the right (docs/konzepte/ui-ueberarbeitung, Abschnitt 4): collapsible, tabs
// "Bauteile" (visibility, volume, export per part in the ⋯ menu, "ohne Bauteil", sum),
// "Farben" (the colour list), "Messen" (the measurements) and – in a module context –
// "Parameter" (the parameter bar). The rows "Datei …" and "Bauteile …" above the view are gone.

import { useRef, type KeyboardEvent } from "react";
import { fmtNum } from "../logic/format";
import { type SideTab, setSideOpen, setSideTab, useCad } from "../state/store";
import { ColorsPanel } from "./colors";
import { MeasurePanel } from "./measureUi";
import { ParamBar } from "./panels";
import { exportParts, togglePart } from "./runtime";
import { IconButton, IconMore, IconPanelClose, MenuButton } from "./ui";

/** Name of the material outside the parts (as the engine names it, cad_session::NO_PART). */
const NO_PART = "ohne Bauteil";

function vol(v: number | null | undefined): string {
  return v === null || v === undefined ? "–" : `${fmtNum(Math.round(v * 10) / 10)} mm³`;
}

function PartsPanel() {
  const shown = useCad((s) => s.complete ?? s.shown);
  const hidden = useCad((s) => s.hiddenParts);
  const regions = useCad((s) => s.regions);
  const fileContext = !shown || shown.context.module === null;
  const parts = fileContext ? (shown?.parts ?? []) : [];
  const names = parts.map((p) => p.name);
  if (!parts.length) {
    return (
      <div className="partspanel" role="region" aria-label="Bauteile" data-testid="partsbar">
        <p className="pbnote">{fileContext ? "Keine Bauteile – benannte Instanzen auf oberster Ebene (deckel: deckel();) sind Bauteile." : "Bauteile gibt es nur im Dateikontext."}</p>
      </div>
    );
  }
  // material outside the parts: from the colour regions when they are known for this scene
  const rest = regions && shown && regions.version === shown.version ? regions.items.filter((r) => r.part === NO_PART).reduce((a, r) => a + r.total, 0) : null;
  // (no set "ohne Bauteil" in a known answer: all material is in parts)
  const known = parts.every((p) => p.volume !== null);
  const sum = known ? parts.reduce((a, p) => a + (p.volume ?? 0), 0) + (rest ?? 0) : null;
  return (
    <div className="partspanel" role="region" aria-label="Bauteile" data-testid="partsbar">
      <ul className="pplist">
        {parts.map((pt) => {
          const b = pt.bounds;
          const size = b ? b.max.map((x, i) => fmtNum(Math.round((x - b.min[i]) * 10) / 10)).join(" × ") : "";
          const on = !hidden.includes(pt.name);
          return (
            <li key={pt.name} className={`pprow${on ? "" : " off"}`} data-testid={`part-${pt.name}`}>
              <label className="ppname" title={`Bauteil ${pt.name} (Zeile ${pt.range.line}) ein-/ausblenden`}>
                <input type="checkbox" checked={on} onChange={(e) => togglePart(pt.name, e.target.checked)} aria-label={`${pt.name} sichtbar`} />
                <span>{pt.name}</span>
              </label>
              <span className="ppvol" title={size ? `Hülle ${size} mm` : undefined}>
                {vol(pt.volume)}
              </span>
              <MenuButton
                label={`Export ${pt.name}`}
                text={null}
                icon={IconMore}
                align="end"
                className="ui-btn--small"
                items={[
                  { label: "STL", hint: `${pt.name} als STL`, onSelect: () => void exportParts("stl", [pt.name], false) },
                  { label: "3MF", hint: `${pt.name} als 3MF`, onSelect: () => void exportParts("3mf", [pt.name], false) },
                  { label: "STEP", hint: `${pt.name} als STEP`, onSelect: () => void exportParts("step", [pt.name], false) },
                ]}
              />
            </li>
          );
        })}
        <li className="pprow rest" title="Material außerhalb der Bauteile (im 3MF ein eigenes Objekt; Volumen aus den Farbregionen)" data-testid="part-rest">
          <span className="ppname muted">{NO_PART}</span>
          <span className="ppvol">{rest === null ? "–" : vol(rest)}</span>
          <span className="ppspacer" />
        </li>
      </ul>
      <div className="ppsum">
        <span>Summe</span>
        <span className="ppvol" data-testid="parts-total">
          {vol(sum)}
        </span>
        <MenuButton
          label="Export aller Bauteile"
          text={null}
          icon={IconMore}
          align="end"
          className="ui-btn--small"
          items={[
            { label: "ZIP (STL)", hint: "Jedes Bauteil als STL, zusammen als ZIP", onSelect: () => void exportParts("stl", names, true) },
            { label: "ZIP (STEP)", hint: "Jedes Bauteil als STEP, zusammen als ZIP", onSelect: () => void exportParts("step", names, true) },
            { label: "3MF (ein Objekt je Bauteil)", hint: "Ein 3MF mit einem Objekt je Bauteil", onSelect: () => void exportParts("3mf", names, false) },
          ]}
        />
      </div>
    </div>
  );
}

const TAB_LABELS: Record<SideTab, string> = { parts: "Bauteile", colors: "Farben", measure: "Messen", params: "Parameter" };

export function SidePanel() {
  const open = useCad((s) => s.sideOpen);
  const tab = useCad((s) => s.sideTab);
  const module = useCad((s) => (s.shown ? s.shown.context.module : null));
  const nParts = useCad((s) => {
    const sh = s.complete ?? s.shown;
    return sh && sh.context.module === null ? sh.parts.length : 0;
  });
  const nColors = useCad((s) => (s.complete?.colors ?? []).length);
  const nMeasure = useCad((s) => s.measurements.length);
  const tabsRef = useRef<HTMLDivElement>(null);
  if (!open) return null;
  const tabs: SideTab[] = module !== null ? ["params", "parts", "colors", "measure"] : ["parts", "colors", "measure"];
  const cur = tabs.includes(tab) ? tab : "parts";
  const count: Partial<Record<SideTab, number>> = { parts: nParts, colors: nColors, measure: nMeasure };
  const onKey = (e: KeyboardEvent) => {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d && e.key !== "Home" && e.key !== "End") return;
    e.preventDefault();
    const i = e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : (tabs.indexOf(cur) + d + tabs.length) % tabs.length;
    setSideTab(tabs[i]);
    requestAnimationFrame(() => tabsRef.current?.querySelector<HTMLButtonElement>(`[data-tab="${tabs[i]}"]`)?.focus());
  };
  return (
    <aside className="sidepanel" aria-label="Seitenpanel" data-testid="sidepanel">
      <div className="sp-head">
        <div className="ui-seg sp-tabs" role="tablist" aria-label="Seitenpanel" ref={tabsRef} onKeyDown={onKey}>
          {tabs.map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              id={`sp-tab-${t}`}
              data-tab={t}
              aria-label={TAB_LABELS[t]}
              aria-selected={t === cur}
              aria-controls="sp-body"
              tabIndex={t === cur ? 0 : -1}
              className={`ui-seg-item${t === cur ? " is-on" : ""}`}
              title={t === "params" && module ? `Parameter von ${module}` : TAB_LABELS[t]}
              onClick={() => setSideTab(t)}
            >
              <span className="ui-seg-text">{TAB_LABELS[t]}</span>
              {count[t] ? <span className="sp-count">{count[t]}</span> : null}
            </button>
          ))}
        </div>
        <IconButton icon={IconPanelClose} label="Seitenpanel ausblenden" onClick={() => setSideOpen(false)} />
      </div>
      <div className="sp-body" id="sp-body" role="tabpanel" aria-labelledby={`sp-tab-${cur}`}>
        {cur === "parts" && <PartsPanel />}
        {cur === "colors" && <ColorsPanel />}
        {cur === "measure" && <MeasurePanel />}
        {cur === "params" && <ParamBar />}
      </div>
    </aside>
  );
}
