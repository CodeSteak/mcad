// Parameter bar (Spec: Parameterleiste) and settings (Schrittweiten beim Ziehen).

import { useEffect, useRef, useState } from "react";
import type { Value } from "../engine/protocol";
import { fmtNum } from "../logic/format";
import { type ParamRow, contextModule, dropOverride, paramRows, resetAll, resetOverride, setOverride } from "../logic/params";
import { DEFAULT_SETTINGS, type Settings, saveSettings, store, useCad } from "../state/store";
import { client } from "./runtime";

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
        <NumberField value={v} onChange={set} label={`${row.name} Wert`} testid={`param-${row.name}-input`} />
        <input
          type="range"
          aria-label={`${row.name} Schieberegler`}
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
        <input type="checkbox" checked={row.value === true} aria-label={`${row.name} Schalter`} data-testid={`param-${row.name}-switch`} onChange={(e) => set(e.target.checked)} />
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
        <span className="pname" title={row.hasDefault ? `Standard: ${row.defaultText}` : "ohne Standardwert"}>
          {row.changed && <span className="pmark" aria-label="geändert">●</span>}
          {row.name}
        </span>
        <span className="pdefault">{row.hasDefault ? `= ${row.defaultText}` : "kein Standard"}</span>
        <button className="picon" title={`Zurücksetzen auf ${row.hasDefault ? row.defaultText : numText(row.reset) || JSON.stringify(row.reset)}`} aria-label={`${row.name} zurücksetzen`} data-testid={`param-${row.name}-reset`} disabled={!row.overridden} onClick={reset}>
          ↺
        </button>
      </div>
      {control}
      {row.changed && row.control !== "other" && (
        <button className="padopt" data-testid={`param-${row.name}-adopt`} onClick={adopt} title="Wert als Standardwert in die Moduldefinition schreiben">
          Als Standard übernehmen
        </button>
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
    <aside className="parambar" aria-label="Parameterleiste" data-testid="parambar">
      <div className="pbtitle">
        <span>
          Parameter <b>{sig.name}</b>
        </span>
        <button data-testid="param-reset-all" disabled={!anyChanged} onClick={() => store.setState({ paramOverrides: resetAll(store.getState().paramOverrides, sig, shown.context.params) })}>
          Alle zurücksetzen
        </button>
      </div>
      <p className="pbnote">Nur Vorschau – der Code bleibt unverändert.</p>
      {rows.length === 0 && <p className="pbnote">Keine Parameter.</p>}
      {rows.map((r) => (
        <Row key={r.name} row={r} module={module} />
      ))}
    </aside>
  );
}

export function SettingsButton() {
  const [open, setOpen] = useState(false);
  const settings = useCad((s) => s.settings);
  const set = (patch: Partial<Settings>) => {
    const s = { ...store.getState().settings, ...patch };
    saveSettings(s);
    store.setState({ settings: s });
  };
  const field = (key: keyof Settings, label: string, unit: string) => (
    <label className="setrow">
      <span>{label}</span>
      <input
        type="number"
        min={0}
        step="any"
        value={settings[key]}
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
    <span className="settings-wrap">
      <button onClick={() => setOpen(!open)} aria-expanded={open} data-testid="settings" title="Einstellungen">
        Einstellungen
      </button>
      {open && (
        <div className="settings-pop" role="dialog" aria-label="Einstellungen">
          <b>Ziehen von Griffen</b>
          {field("stepMm", "Schrittweite Länge", "mm")}
          {field("stepDeg", "Schrittweite Winkel", "°")}
          {field("snapPx", "Fangradius", "px")}
          <p className="pbnote">Mit Alt wird frei gezogen (ohne Snap); Zahlen werden auf die Schrittweite gerundet.</p>
          <button onClick={() => set({ ...DEFAULT_SETTINGS })}>Standard</button>
        </div>
      )}
    </span>
  );
}
