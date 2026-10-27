// Studio look of the 3D view (like `mcad render --style studio`): toggle button in the view
// toolbar and a settings panel for light, shadows, ambient occlusion, ground and background.
// The settings are kept in localStorage (`cad-studio`, see `setStudio`).

import { useEffect, useRef, useState } from "react";
import { t } from "../i18n";
import { fmtNum } from "../logic/format";
import { DEFAULT_STUDIO, STUDIO_THEME, type StudioSettings, aoRadiusFor, studioCliCommand } from "../logic/studio";
import { boxDiagonal, isEmpty } from "../logic/math";
import { absPath, IS_TAURI } from "../platform/desktop";
import { project } from "../state/project";
import { setStudio, store, useCad } from "../state/store";
import { copyText } from "./clipboard";
import { getViewer } from "./runtime";
import { Button, IconButton, IconCopy, IconStudio, IconStudioSettings, ToolGroup } from "./ui";

/** `mcad render …` with the studio settings and the current view. */
export function studioCli(): string {
  const st = store.getState();
  const file = IS_TAURI && project.root ? absPath(project.root, project.main) : project.main;
  return studioCliCommand({ ...st.studio, on: true }, file, st.shown?.context.module ?? null, getViewer()?.eyeDir() ?? null, st.theme);
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
    setStudio({ aoRadius: radius.trim() === "" || !(v > 0) ? null : v });
  };
  return (
    <div className="settings-pop studio-pop" role="dialog" aria-label={t("studio.settings")} data-testid="studio-panel">
      <Check id="on" label={t("studio.on")} hint={t("studio.toggleHint")} checked={s.on} onChange={(on) => setStudio({ on })} />
      <b>{t("studio.light")}</b>
      <Slider id="azimuth" label={t("studio.azimuth")} hint={t("studio.azimuthHint")} value={s.lightAzimuth} min={-180} max={180} step={5} unit="°" onChange={(v) => setStudio({ lightAzimuth: v })} />
      <Slider id="elevation" label={t("studio.elevation")} hint={t("studio.elevationHint")} value={s.lightElevation} min={5} max={90} step={1} unit="°" onChange={(v) => setStudio({ lightElevation: v })} />
      <Slider id="intensity" label={t("studio.intensity")} value={s.lightIntensity} min={0} max={2.5} step={0.05} onChange={(v) => setStudio({ lightIntensity: v })} />
      <Slider id="ambient" label={t("studio.ambient")} hint={t("studio.ambientHint")} value={s.ambient} min={0} max={1} step={0.05} onChange={(v) => setStudio({ ambient: v })} />
      <Check id="shadows" label={t("studio.shadows")} checked={s.shadows} onChange={(shadows) => setStudio({ shadows })} />
      <Slider id="softness" label={t("studio.softness")} value={s.shadowSoftness} min={0} max={1} step={0.05} disabled={!s.shadows} onChange={(v) => setStudio({ shadowSoftness: v })} />
      <Check id="ao" label={t("studio.ao")} hint={t("studio.aoHint")} checked={s.ao} onChange={(ao) => setStudio({ ao })} />
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
      <Slider id="aoStrength" label={t("studio.aoStrength")} value={s.aoStrength} min={0} max={2} step={0.05} disabled={!s.ao} onChange={(v) => setStudio({ aoStrength: v })} />
      <div className="studio-line">
        <Check id="ground" label={t("studio.ground")} hint={t("studio.groundHint")} checked={s.ground} onChange={(ground) => setStudio({ ground })} />
        <span className="grow" />
        <Color id="groundColor" label={t("studio.groundColor")} value={s.groundColor ?? th.ground} disabled={!s.ground} onChange={(v) => setStudio({ groundColor: v })} />
        <Button onClick={() => setStudio({ groundColor: null })} hint={t("studio.fromThemeHint")} disabled={s.groundColor === null}>
          {t("studio.fromTheme")}
        </Button>
      </div>
      <div className="studio-line">
        <b>{t("studio.background")}</b>
        <span className="grow" />
        <Color id="bgTop" label={t("studio.backgroundTop")} value={bg[0]} onChange={(v) => setStudio({ background: [v, bg[1]] })} />
        <Color id="bgBottom" label={t("studio.backgroundBottom")} value={bg[1]} onChange={(v) => setStudio({ background: [bg[0], v] })} />
        <Button onClick={() => setStudio({ background: null })} hint={t("studio.fromThemeHint")} disabled={s.background === null}>
          {t("studio.fromTheme")}
        </Button>
      </div>
      <div className="setfoot studio-foot">
        <Button onClick={() => setStudio({ ...DEFAULT_STUDIO, on: s.on })} data-testid="studio-defaults">
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
        <IconButton icon={IconStudio} label={t("studio.toggle")} hint={t("studio.toggleHint")} pressed={s.on} onClick={() => setStudio({ on: !s.on })} data-testid="toggle-studio" />
        <IconButton icon={IconStudioSettings} label={t("studio.settings")} hint={t("studio.settingsHint")} pressed={open} aria-expanded={open} aria-haspopup="dialog" onClick={() => setOpen(!open)} data-testid="studio-settings" />
      </ToolGroup>
      {open && <StudioPanel s={s} />}
    </span>
  );
}
