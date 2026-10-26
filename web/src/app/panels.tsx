// Parameter bar (Spec: Parameterleiste) and settings (language, Schrittweiten beim Ziehen).

import { LANG_PREFS, type LangPref, t } from "../i18n";
import { IS_TAURI } from "../platform/desktop";
import { useEffect, useRef, useState } from "react";
import type { Value } from "../engine/protocol";
import { fmtNum } from "../logic/format";
import { type ParamRow, contextModule, dropOverride, paramRows, resetAll, resetOverride, setOverride } from "../logic/params";
import { DEFAULT_SETTINGS, type Settings, saveSettings, setLangPref, store, useCad } from "../state/store";
import { client } from "./runtime";
import { Button, IconButton, IconReset, IconSettings } from "./ui";

/** "Als Standard übernehmen" waiting for its edit: the temporary value is dropped afterwards. */
let pendingDefault: { module: string; param: string } | null = null;
client.onEdits(() => {
  const p = pendingDefault;
  if (!p) return;
  pendingDefault = null;
  // The drag controller has applied the edit (it listens first); the new default now equals
  // the temporary value.
  queueMicrotask(() => store.setState({ paramOverrides: dropOverride(store.getState().paramOverrides, p.module, p.param) }));
});

/** Throttles override updates to one per animation frame (slider drags). */
function useFrameThrottle(): (f: () => void) => void {
  const pending = useRef<(() => void) | null>(null);
  return (f) => {
    const first = pending.current === null;
    pending.current = f;
    if (!first) return;
    requestAnimationFrame(() => {
      const g = pending.current;
      pending.current = null;
      g?.();
    });
  };
}

function numText(v: Value): string {
  return typeof v === "number" ? fmtNum(v) : "";
}

function NumberField({ value, onChange, label, testid }: { value: number; onChange: (v: number) => void; label: string; testid?: string }) {
  const [text, setText] = useState(fmtNum(value));
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(fmtNum(value));
  }, [value]);
  return (
    <input
      className="pnum"
      type="text"
      inputMode="decimal"
      aria-label={label}
      data-testid={testid}
      value={text}
      onFocus={() => (focused.current = true)}
      onBlur={() => {
        focused.current = false;
        setText(fmtNum(value));
      }}
      onChange={(e) => {
        setText(e.target.value);
        const v = Number(e.target.value.replace(",", "."));
        if (e.target.value.trim() !== "" && Number.isFinite(v)) onChange(v);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
      }}
    />
  );
}

function Row({ row, module }: { row: ParamRow; module: string }) {
  const throttle = useFrameThrottle();
  const set = (v: Value) => store.setState({ paramOverrides: setOverride(store.getState().paramOverrides, module, row.name, v) });
  const sig = contextModule(store.getState().shown);
  const param = sig?.params.find((p) => p.name === row.name);
  const reset = () => {
    if (param) store.setState({ paramOverrides: resetOverride(store.getState().paramOverrides, module, param, row.control) });
  };
  const adopt = () => {
    pendingDefault = { module, param: row.name };
    client.post({ type: "SetDefault", module, param: row.name, value: row.value });
  };
  let control: React.ReactNode;
  if (row.control === "number") {
    const v = typeof row.value === "number" ? row.value : 0;
    control = (
      <div className="pctl">
        <NumberField value={v} onChange={set} label={t("param.value", { name: row.name })} testid={`param-${row.name}-input`} />
        <input
          type="range"
          aria-label={t("param.slider", { name: row.name })}
          data-testid={`param-${row.name}-slider`}
          min={row.min}
          max={row.max}
          step={row.step}
          value={v}
          onChange={(e) => {
            const x = Number(e.target.value);
            throttle(() => set(x));
          }}
        />
      </div>
    );
  } else if (row.control === "bool") {
    control = (
      <label className="pswitch">
        <input type="checkbox" checked={row.value === true} aria-label={t("param.switch", { name: row.name })} data-testid={`param-${row.name}-switch`} onChange={(e) => set(e.target.checked)} />
        <span className="cm-switch-knob" />
      </label>
    );
  } else if (row.control === "vector") {
    const vec = Array.isArray(row.value) ? (row.value as Value[]) : [0, 0, 0];
    control = (
      <div className="pvec">
        {[0, 1, 2].map((k) => (
          <NumberField
            key={k}
            value={typeof vec[k] === "number" ? (vec[k] as number) : 0}
            label={`${row.name}[${k}]`}
            testid={`param-${row.name}-${k}`}
            onChange={(x) => {
              const nv = [0, 1, 2].map((i) => (typeof vec[i] === "number" ? (vec[i] as number) : 0));
              nv[k] = x;
              set(nv);
            }}
          />
        ))}
      </div>
    );
  } else {
    control = <span className="pother">{JSON.stringify(row.value)}</span>;
  }
  return (
    <div className={`prow${row.changed ? " changed" : ""}`} data-testid={`param-${row.name}`}>
      <div className="phead">
        <span className="pname" title={row.hasDefault ? t("param.defaultIs", { value: row.defaultText ?? "" }) : t("param.noDefaultHint")}>
          {row.changed && <span className="pmark" aria-label={t("param.changed")}>●</span>}
          {row.name}
        </span>
        <span className="pdefault">{row.hasDefault ? `= ${row.defaultText}` : t("param.noDefault")}</span>
        <IconButton
          icon={IconReset}
          className="ui-btn--small"
          label={t("param.reset", { name: row.name })}
          title={t("param.resetTo", { value: row.hasDefault ? (row.defaultText ?? "") : numText(row.reset) || JSON.stringify(row.reset) })}
          data-testid={`param-${row.name}-reset`}
          disabled={!row.overridden}
          onClick={reset}
        />
      </div>
      {control}
      {row.changed && row.control !== "other" && (
        <Button className="padopt" data-testid={`param-${row.name}-adopt`} onClick={adopt} title={t("param.adoptHint")}>
          {t("param.adopt")}
        </Button>
      )}
    </div>
  );
}

export function ParamBar() {
  const shown = useCad((s) => s.shown);
  const overrides = useCad((s) => s.paramOverrides);
  const sig = contextModule(shown);
  if (!shown || !sig || shown.context.module === null) return null;
  const module = shown.context.module;
  const rows = paramRows(sig, shown.context.params, overrides[module]);
  const anyChanged = rows.some((r) => r.overridden);
  return (
    <div className="parambar" role="region" aria-label={t("param.bar")} data-testid="parambar">
      <div className="pbtitle">
        <span>
          {t("param.title")} <b>{sig.name}</b>
        </span>
        <Button variant="quiet" data-testid="param-reset-all" disabled={!anyChanged} onClick={() => store.setState({ paramOverrides: resetAll(store.getState().paramOverrides, sig, shown.context.params) })}>
          {t("param.resetAll")}
        </Button>
      </div>
      <p className="pbnote">{t("param.previewOnly")}</p>
      {rows.length === 0 && <p className="pbnote">{t("param.none")}</p>}
      {rows.map((r) => (
        <Row key={r.name} row={r} module={module} />
      ))}
    </div>
  );
}

/** Names of the language choices: the languages in their own language, "System" translated. */
const LANG_NAMES: Record<LangPref, () => string> = { system: () => t("settings.langSystem"), en: () => "English", de: () => "Deutsch" };

export function SettingsButton() {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  const settings = useCad((s) => s.settings);
  const langPref = useCad((s) => s.langPref);
  const set = (patch: Partial<Settings>) => {
    const s = { ...store.getState().settings, ...patch };
    saveSettings(s);
    store.setState({ settings: s });
  };
  // close on a click outside and with Esc (focus back to the button)
  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", down, true);
    return () => window.removeEventListener("pointerdown", down, true);
  }, [open]);
  const field = (key: "stepMm" | "stepDeg" | "snapPx" | "autoSaveMs", label: string, unit: string) => (
    <label className="setrow">
      <span>{label}</span>
      <input
        type="number"
        min={0}
        step="any"
        value={settings[key]}
        aria-label={label}
        data-testid={`setting-${key}`}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isFinite(v) && v >= 0) set({ [key]: v } as Partial<Settings>);
        }}
      />
      <span className="unit">{unit}</span>
    </label>
  );
  return (
    <span
      className="settings-wrap"
      ref={wrap}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
          wrap.current?.querySelector<HTMLButtonElement>("button")?.focus();
        }
      }}
    >
      <IconButton icon={IconSettings} label={t("settings.title")} onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="dialog" pressed={open} data-testid="settings" />
      {open && (
        <div className="settings-pop" role="dialog" aria-label={t("settings.title")}>
          <label className="setrow setrow--wide">
            <span>{t("settings.language")}</span>
            <select value={langPref} onChange={(e) => setLangPref(e.target.value as LangPref)} aria-label={t("settings.language")} data-testid="setting-lang">
              {LANG_PREFS.map((p) => (
                <option key={p} value={p}>
                  {LANG_NAMES[p]()}
                </option>
              ))}
            </select>
          </label>
          <b>{t("settings.drag")}</b>
          {field("stepMm", t("settings.stepMm"), "mm")}
          {field("stepDeg", t("settings.stepDeg"), "°")}
          {field("snapPx", t("settings.snapPx"), "px")}
          <p className="pbnote">{t("settings.dragNote")}</p>
          {IS_TAURI && (
            <>
              <b>{t("settings.files")}</b>
              <label className="setrow">
                <span>{t("settings.autoSave")}</span>
                <input type="checkbox" checked={settings.autoSave} onChange={(e) => set({ autoSave: e.target.checked })} aria-label={t("settings.autoSave")} data-testid="setting-autoSave" />
                <span className="unit" />
              </label>
              {field("autoSaveMs", t("settings.autoSaveMs"), "ms")}
            </>
          )}
          <div className="setfoot">
            <Button onClick={() => set({ ...DEFAULT_SETTINGS })}>{t("settings.defaults")}</Button>
          </div>
        </div>
      )}
    </span>
  );
}
