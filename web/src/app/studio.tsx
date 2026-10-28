// Studio look of the 3D view (like `mcad render --style studio`): toggle button in the view
// toolbar and a settings panel for lights, environment, shadows, ambient occlusion, edges,
// ground, background and the materials of colours and parts. Changes are kept in localStorage
// (`cad-studio`) and written into the file's `render { … }` block (`editStudio`).

import { useEffect, useRef, useState } from "react";
import { type MessageKey, t } from "../i18n";
import { fmtNum } from "../logic/format";
import {
  DEFAULT_STUDIO,
  ENVIRONMENTS,
  type EnvironmentName,
  KEY_LIGHT,
  type MaterialTarget,
  PRESET_NAMES,
  STUDIO_THEME,
  type StudioLight,
  type StudioSettings,
  aoRadiusFor,
  materialEntry,
  newLightName,
  resolvePbr,
  studioCliCommand,
  withMaterial,
} from "../logic/studio";
import { boxDiagonal, isEmpty } from "../logic/math";
import { absPath, IS_TAURI } from "../platform/desktop";
import { project } from "../state/project";
import { store, useCad } from "../state/store";
import { copyText } from "./clipboard";
import { editStudio, renderBlockLine } from "./renderBlock";
import { getViewer } from "./runtime";
import { Button, IconButton, IconCopy, IconDelete, IconPlus, IconStudio, IconStudioSettings, ToolGroup } from "./ui";

/** `mcad render …` with the studio settings and the current view. */
export function studioCli(): string {
  const st = store.getState();
  const file = IS_TAURI && project.root ? absPath(project.root, project.main) : project.main;
  return studioCliCommand({ ...st.studio, on: true }, file, st.shown?.context.module ?? null, getViewer()?.eyeDir() ?? null, st.theme, "studio.png", renderBlockLine() !== null);
}

async function copyStudioCli(): Promise<void> {
  const cmd = studioCli();
  const ok = await copyText(cmd);
  store.setState({ notice: ok ? t("notice.copied", { text: cmd }) : t("notice.cliCall", { text: cmd }), lastCli: cmd });
}

function Slider({ id, label, hint, value, min, max, step, unit, onChange, disabled }: { id: string; label: string; hint?: string; value: number; min: number; max: number; step: number; unit?: string; onChange: (v: number) => void; disabled?: boolean }) {
  return (
    <label className="setrow studio-row" title={hint}>
      <span>{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} disabled={disabled} aria-label={label} data-testid={`studio-${id}`} onChange={(e) => onChange(Number(e.target.value))} />
      <span className="unit">
        {fmtNum(Math.round(value * 100) / 100)}
        {unit ?? ""}
      </span>
    </label>
  );
}

function Check({ id, label, hint, checked, onChange }: { id: string; label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="studio-check" title={hint}>
      <input type="checkbox" checked={checked} aria-label={label} data-testid={`studio-${id}`} onChange={(e) => onChange(e.target.checked)} />
      <b>{label}</b>
    </label>
  );
}

function Color({ id, label, value, onChange, disabled }: { id: string; label: string; value: string; onChange: (v: string) => void; disabled?: boolean }) {
  return <input type="color" className="studio-color" value={value} disabled={disabled} aria-label={label} title={label} data-testid={`studio-${id}`} onChange={(e) => onChange(e.target.value)} />;
}

/** Light `i` (direction, height, strength, colour, remove). */
function LightRow({ s, i }: { s: StudioSettings; i: number }) {
  const l = s.lights[i];
  const set = (patch: Partial<StudioLight>) => editStudio({ lights: s.lights.map((x, k) => (k === i ? { ...x, ...patch } : x)) });
  return (
    <div className="studio-light" data-testid={`studio-light-${l.name}`} role="group" aria-label={t("studio.lightNamed", { name: l.name })}>
      <div className="studio-line">
        <b>{i === 0 ? t("studio.lightKey", { name: l.name }) : l.name}</b>
        <span className="grow" />
        <Color id={`light${i}-color`} label={t("studio.lightColor")} value={l.color} onChange={(color) => set({ color })} />
        <IconButton icon={IconDelete} label={t("studio.removeLight")} onClick={() => editStudio({ lights: s.lights.filter((_, k) => k !== i) })} data-testid={`studio-light${i}-remove`} />
      </div>
      <Slider id={i === 0 ? "azimuth" : `light${i}-azimuth`} label={t("studio.azimuth")} hint={t("studio.azimuthHint")} value={l.azimuth} min={-180} max={180} step={5} unit="°" onChange={(azimuth) => set({ azimuth })} />
      <Slider id={i === 0 ? "elevation" : `light${i}-elevation`} label={t("studio.elevation")} hint={t("studio.elevationHint")} value={l.elevation} min={5} max={90} step={1} unit="°" onChange={(elevation) => set({ elevation })} />
      <Slider id={`light${i}-intensity`} label={t("studio.lightStrength")} value={l.intensity} min={0} max={2.5} step={0.05} onChange={(intensity) => set({ intensity })} />
    </div>
  );
}

/** Material of a colour or part: preset and the single values. */
function MaterialRow({ s, target, label, swatch, testid }: { s: StudioSettings; target: MaterialTarget; label: string; swatch: string | null; testid: string }) {
  const e = materialEntry(s.materials, target);
  const pbr = resolvePbr(e ?? { preset: null, metalness: null, roughness: null, clearcoat: null });
  const set = (patch: Partial<{ preset: string | null; metalness: number | null; roughness: number | null; clearcoat: number | null }>) => {
    const base = e ?? { preset: null, metalness: null, roughness: null, clearcoat: null };
    editStudio({ materials: withMaterial(s.materials, target, { preset: base.preset, metalness: base.metalness, roughness: base.roughness, clearcoat: base.clearcoat, ...patch }) });
  };
  return (
    <div className="studio-material" data-testid={`studio-material-${testid}`}>
      <div className="studio-line">
        {swatch && <span className="swatch" style={{ background: swatch }} />}
        <span className="studio-mlabel">{label}</span>
        <span className="grow" />
        <select
          className="ui-input"
          aria-label={t("studio.materialOf", { name: label })}
          data-testid={`studio-preset-${testid}`}
          value={e ? (e.preset ?? "custom") : ""}
          onChange={(ev) => {
            const v = ev.target.value;
            if (v === "") editStudio({ materials: withMaterial(s.materials, target, null) });
            else if (v === "custom") set({ preset: null });
            else editStudio({ materials: withMaterial(s.materials, target, { preset: v, metalness: null, roughness: null, clearcoat: null }) });
          }}
        >
          <option value="">{t("studio.presetNone")}</option>
          {PRESET_NAMES.map((p) => (
            <option key={p} value={p}>
              {t(`studio.preset.${p}` as MessageKey)}
            </option>
          ))}
          {e && e.preset === null && <option value="custom">{t("studio.presetCustom")}</option>}
        </select>
      </div>
      {e && (
        <>
          <Slider id={`metalness-${testid}`} label={t("studio.metalness")} hint={t("studio.metalnessHint")} value={pbr.metalness} min={0} max={1} step={0.05} onChange={(metalness) => set({ metalness })} />
          <Slider id={`roughness-${testid}`} label={t("studio.roughness")} hint={t("studio.roughnessHint")} value={pbr.roughness} min={0} max={1} step={0.05} onChange={(roughness) => set({ roughness })} />
          <Slider id={`clearcoat-${testid}`} label={t("studio.clearcoat")} hint={t("studio.clearcoatHint")} value={pbr.clearcoat} min={0} max={1} step={0.05} onChange={(clearcoat) => set({ clearcoat })} />
        </>
      )}
    </div>
  );
}

/** Colours and parts of the shown model that can get a material. */
function MaterialList({ s }: { s: StudioSettings }) {
  const scene = useCad((st) => st.shown);
  const theme = useCad((st) => st.theme);
  if (!scene) return null;
  const colors = scene.colors ?? [];
  const parts = scene.context.module === null ? scene.parts : [];
  const model = getViewer()?.modelColor() ?? "#c3cad4";
  const rows: Array<{ target: MaterialTarget; label: string; swatch: string | null; testid: string }> = [];
  if (!colors.length) rows.push({ target: { color: "default" }, label: t("colors.defaultLabel"), swatch: model, testid: "default" });
  for (const c of colors) {
    const isDefault = c.source === "default";
    const sel = isDefault ? "default" : /^#/.test(c.label) ? c.key.slice(0, 7) : c.label;
    const hex = `#${c.rgba
      .slice(0, 3)
      .map((x) => Math.round(Math.max(0, Math.min(1, x)) * 255).toString(16).padStart(2, "0"))
      .join("")}`;
    rows.push({ target: { color: sel }, label: isDefault ? t("colors.defaultLabel") : c.label, swatch: isDefault ? model : hex, testid: sel.replace(/^#/, "") });
  }
  for (const p of parts) rows.push({ target: { part: p.name }, label: t("studio.partNamed", { name: p.name }), swatch: null, testid: `part-${p.name}` });
  void theme;
  return (
    <>
      {rows.map((r) => (
        <MaterialRow key={r.testid} s={s} target={r.target} label={r.label} swatch={r.swatch} testid={r.testid} />
      ))}
    </>
  );
}

/** Where the settings are kept: the file's render block or only this browser. */
function SourceNote() {
  const line = useCad(() => renderBlockLine());
  return <p className="pbnote" data-testid="studio-source">{line !== null ? t("studio.inFile", { line }) : t("studio.notInFile")}</p>;
}

function StudioPanel({ s }: { s: StudioSettings }) {
  const theme = useCad((st) => st.theme);
  const sectionOn = useCad((st) => st.section.on);
  const th = STUDIO_THEME[theme];
  const box = getViewer()?.modelBox;
  const diag = box && !isEmpty(box) ? boxDiagonal(box) : 50;
  const bg = s.background ?? th.background;
  const [radius, setRadius] = useState(s.aoRadius === null ? "" : String(s.aoRadius));
  useEffect(() => setRadius(s.aoRadius === null ? "" : String(s.aoRadius)), [s.aoRadius]);
  const commitRadius = () => {
    const v = Number(radius.replace(",", "."));
    const aoRadius = radius.trim() === "" || !(v > 0) ? null : v;
    if (aoRadius !== s.aoRadius) editStudio({ aoRadius });
  };
  return (
    <div className="settings-pop studio-pop" role="dialog" aria-label={t("studio.settings")} data-testid="studio-panel">
      <Check id="on" label={t("studio.on")} hint={t("studio.toggleHint")} checked={s.on} onChange={(on) => editStudio({ on })} />
      <SourceNote />
      <div className="studio-line">
        <b>{t("studio.lights")}</b>
        <span className="grow" />
        <Button
          icon={IconPlus}
          onClick={() => editStudio({ lights: [...s.lights, { ...KEY_LIGHT, name: newLightName(s.lights), azimuth: 60, elevation: 30, intensity: 0.5 }] })}
          data-testid="studio-add-light"
        >
          {t("studio.addLight")}
        </Button>
      </div>
      {s.lights.map((l, i) => (
        <LightRow key={l.name} s={s} i={i} />
      ))}
      <Slider id="intensity" label={t("studio.intensity")} hint={t("studio.intensityHint")} value={s.lightIntensity} min={0} max={2.5} step={0.05} onChange={(v) => editStudio({ lightIntensity: v })} />
      <Slider id="ambient" label={t("studio.ambient")} hint={t("studio.ambientHint")} value={s.ambient} min={0} max={1} step={0.05} onChange={(v) => editStudio({ ambient: v })} />
      <label className="setrow studio-row" title={t("studio.environmentHint")}>
        <span>{t("studio.environment")}</span>
        <select className="ui-input" value={s.environment} aria-label={t("studio.environment")} data-testid="studio-environment" onChange={(e) => editStudio({ environment: e.target.value as EnvironmentName })}>
          {ENVIRONMENTS.map((e) => (
            <option key={e} value={e}>
              {t(`studio.env.${e}`)}
            </option>
          ))}
        </select>
        <span />
      </label>
      <Check id="shadows" label={t("studio.shadows")} checked={s.shadows} onChange={(shadows) => editStudio({ shadows })} />
      <Slider id="softness" label={t("studio.softness")} value={s.shadowSoftness} min={0} max={1} step={0.05} disabled={!s.shadows} onChange={(v) => editStudio({ shadowSoftness: v })} />
      <Check id="ao" label={t("studio.ao")} hint={t("studio.aoHint")} checked={s.ao} onChange={(ao) => editStudio({ ao })} />
      {s.ao && sectionOn && <p className="pbnote">{t("studio.sectionNoAo")}</p>}
      <label className="setrow studio-row">
        <span>{t("studio.aoRadius")}</span>
        <input
          className="ui-input"
          value={radius}
          disabled={!s.ao}
          placeholder={t("studio.aoRadiusAuto", { mm: fmtNum(Math.round(aoRadiusFor({ ...s, aoRadius: null }, diag) * 10) / 10) })}
          aria-label={t("studio.aoRadius")}
          data-testid="studio-aoRadius"
          onChange={(e) => setRadius(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && commitRadius()}
          onBlur={commitRadius}
        />
        <span className="unit">mm</span>
      </label>
      <Slider id="aoStrength" label={t("studio.aoStrength")} value={s.aoStrength} min={0} max={2} step={0.05} disabled={!s.ao} onChange={(v) => editStudio({ aoStrength: v })} />
      <Check id="edges" label={t("studio.edges")} hint={t("studio.edgesHint")} checked={s.edges} onChange={(edges) => editStudio({ edges })} />
      <div className="studio-line">
        <Check id="ground" label={t("studio.ground")} hint={t("studio.groundHint")} checked={s.ground} onChange={(ground) => editStudio({ ground })} />
        <span className="grow" />
        <Color id="groundColor" label={t("studio.groundColor")} value={s.groundColor ?? th.ground} disabled={!s.ground} onChange={(v) => editStudio({ groundColor: v })} />
        <Button onClick={() => editStudio({ groundColor: null })} hint={t("studio.fromThemeHint")} disabled={s.groundColor === null}>
          {t("studio.fromTheme")}
        </Button>
      </div>
      <div className="studio-line">
        <b>{t("studio.background")}</b>
        <span className="grow" />
        <Color id="bgTop" label={t("studio.backgroundTop")} value={bg[0]} onChange={(v) => editStudio({ background: [v, bg[1]] })} />
        <Color id="bgBottom" label={t("studio.backgroundBottom")} value={bg[1]} onChange={(v) => editStudio({ background: [bg[0], v] })} />
        <Button onClick={() => editStudio({ background: null })} hint={t("studio.fromThemeHint")} disabled={s.background === null}>
          {t("studio.fromTheme")}
        </Button>
      </div>
      <b>{t("studio.materials")}</b>
      <MaterialList s={s} />
      <div className="setfoot studio-foot">
        <Button onClick={() => editStudio({ ...DEFAULT_STUDIO, on: s.on })} data-testid="studio-defaults">
          {t("studio.defaults")}
        </Button>
        <IconButton icon={IconCopy} label={t("studio.copyCli")} hint={t("studio.copyCliHint")} onClick={() => void copyStudioCli()} data-testid="studio-copy-cli" />
      </div>
    </div>
  );
}

/** Toggle and settings of the studio look (view toolbar). */
export function StudioControls() {
  const s = useCad((st) => st.studio);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", down, true);
    return () => window.removeEventListener("pointerdown", down, true);
  }, [open]);
  return (
    <span
      className="settings-wrap studio-wrap"
      ref={wrap}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
          wrap.current?.querySelector<HTMLButtonElement>("[data-testid=studio-settings]")?.focus();
        }
      }}
    >
      <ToolGroup>
        <IconButton icon={IconStudio} label={t("studio.toggle")} hint={t("studio.toggleHint")} pressed={s.on} onClick={() => editStudio({ on: !s.on })} data-testid="toggle-studio" />
        <IconButton icon={IconStudioSettings} label={t("studio.settings")} hint={t("studio.settingsHint")} pressed={open} aria-expanded={open} aria-haspopup="dialog" onClick={() => setOpen(!open)} data-testid="studio-settings" />
      </ToolGroup>
      {open && <StudioPanel s={s} />}
    </span>
  );
}
