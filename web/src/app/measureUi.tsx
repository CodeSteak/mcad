// Measure mode (Plan 18): toggle in the view tools, the context bar (kind, "Im Schnitt"),
// the status line text and the list of measurements ("Messungen").
//
// UI concept (docs/konzepte/ui-ueberarbeitung): the context bar belongs top left under the
// toolbar and the list into a tab "Messen" of the side panel. Both components are
// self-contained (`MeasureBar`, `MeasurePanel`), so the overhaul only has to place them.

import { MEASURE_KINDS, accuracyText, kindAvailable, sectionText } from "../logic/measure";
import type { MeasurePick } from "../engine/protocol";
import { useCad } from "../state/store";

const NO_PICKS: MeasurePick[] = [];
import { measure } from "./runtime";
import { setSectionOn } from "./section";

export function MeasureToggle() {
  const on = useCad((s) => s.measure !== null);
  return (
    <button
      aria-pressed={on}
      className={on ? "on" : ""}
      onClick={() => (on ? measure.exit() : measure.enter())}
      title="Messen: Abstand, Radius, Winkel, Länge, Fläche, Körper – auch im Schnitt (Esc beendet)"
      data-testid="measure-toggle"
    >
      Messen
    </button>
  );
}

export function MeasureBar() {
  const m = useCad((s) => s.measure);
  const sectionOn = useCad((s) => s.section.on);
  if (!m) return null;
  return (
    <div className="measurebar contextbar" role="toolbar" aria-label="Messen" data-testid="measurebar">
      <span className="stitle">Messen</span>
      <span className="seg" role="radiogroup" aria-label="Messart">
        {MEASURE_KINDS.map((k) => {
          const ok = kindAvailable(k.kind, m.inSection);
          return (
            <button
              key={k.kind}
              role="radio"
              aria-checked={m.kind === k.kind}
              aria-label={k.label}
              className={m.kind === k.kind ? "on" : ""}
              disabled={!ok}
              title={`${k.title} (${k.key})${ok ? "" : k.where === "section" ? " – nur im Schnitt" : " – nicht im Schnitt"}`}
              onClick={() => measure.setKind(k.kind)}
              data-testid={`measure-kind-${k.kind}`}
            >
              {k.label}
            </button>
          );
        })}
      </span>
      <span className="sep" />
      <label className="check" title={sectionOn ? "In der Schnittebene messen: Konturen, Kreise, Wandstärke" : "Schaltet die Schnittansicht ein und misst in der Schnittebene"}>
        <input type="checkbox" checked={m.inSection} onChange={(e) => measure.setInSection(e.target.checked, () => setSectionOn(true))} data-testid="measure-in-section" />
        Im Schnitt
      </label>
      <button onClick={() => measure.exit()} title="Messen beenden (Esc)" data-testid="measure-exit">
        Fertig
      </button>
    </div>
  );
}

export function MeasureStatus() {
  const m = useCad((s) => s.measure);
  if (!m) return null;
  const text = m.message || m.prompt || "Messen";
  const sec = m.inSection && m.section ? (m.section.message ? ` · ${m.section.message}` : ` · Schnitt: ${m.section.regions} Bereich(e), ${Number(m.section.area.toFixed(3))} mm²${m.section.exact ? ", exakt" : ""}`) : "";
  return (
    <span className={`st-note measure${m.message ? " warn" : ""}`} data-testid="status-measure" title="Alt: ohne Punktfang · Esc: Auswahl verwerfen, nochmals: beenden · Entf: gewählte Messung löschen">
      {text}
      {m.hover ? ` · ${m.hover.label}` : ""}
      {sec}
    </span>
  );
}

export function MeasurePanel() {
  const active = useCad((s) => s.measure !== null);
  const list = useCad((s) => s.measurements);
  const sel = useCad((s) => s.measureSelected);
  const version = useCad((s) => s.complete?.version ?? null);
  const picks = useCad((s) => s.measure?.picks) ?? NO_PICKS;
  if (!active && !list.length) return null;
  return (
    <aside className="measurepanel" aria-label="Messungen" data-testid="measure-panel">
      <div className="mptitle">
        <span>
          Messungen <span className="mpcount">{list.length}</span>
        </span>
        <span className="mpactions">
          <button disabled={!list.length} onClick={() => void measure.copyAll()} title="Alle Messungen als Text kopieren" data-testid="measure-copy-all">
            Alle kopieren
          </button>
          <button disabled={!list.length} onClick={() => measure.clear()} title="Alle Messungen löschen" data-testid="measure-clear">
            Alle löschen
          </button>
        </span>
      </div>
      {picks.length > 0 && (
        <div className="mppicks" data-testid="measure-picks">
          Gewählt: {picks.map((p) => p.label).join(" · ")}
        </div>
      )}
      {!list.length && <p className="mpempty">Noch keine Messung – Messart wählen und Elemente in der Ansicht anklicken.</p>}
      <ol className="mplist">
        {list.map((e) => {
          const m = e.m;
          const stale = version !== null && e.version !== version;
          const sec = sectionText(m);
          return (
            <li
              key={e.id}
              className={`mpitem${sel === e.id ? " selected" : ""}${stale ? " stale" : ""}`}
              data-testid={`measure-item-${e.id}`}
              onClick={() => measure.select(sel === e.id ? null : e.id)}
            >
              <div className="mphead">
                <span className="mpkind">{m.title}</span>
                <span className="mpvalue" data-testid={`measure-value-${e.id}`}>
                  {m.label}
                </span>
                <span className={`badge${m.exact ? "" : " warn"}`} title={m.exact ? "aus der exakten Geometrie (B-Rep)" : "genähert"}>
                  {accuracyText(m)}
                </span>
                {stale && (
                  <span className="badge warn" title="Das Modell hat sich seit der Messung geändert">
                    veraltet
                  </span>
                )}
              </div>
              <div className="mppicks">{m.picks.map((p) => p.label).join(" – ")}</div>
              {sec && <div className="mpdetail">im Schnitt {sec}</div>}
              {m.details.map((d, i) => (
                <div key={i} className="mpdetail">
                  {d}
                </div>
              ))}
              {m.picks.some((p) => p.origin.length) && (
                <ul className="mporigin" data-testid={`measure-origin-${e.id}`}>
                  {[...new Set(m.picks.flatMap((p) => p.origin))].map((o) => (
                    <li key={o}>{o}</li>
                  ))}
                </ul>
              )}
              <div className="mpbuttons" onClick={(ev) => ev.stopPropagation()}>
                <button onClick={() => void measure.copy(e.id, "text")} title="Als Text kopieren" data-testid={`measure-copy-${e.id}`}>
                  Kopieren
                </button>
                <button onClick={() => void measure.copy(e.id, "comment")} title="Als Code-Kommentar kopieren (// Messung: …)" data-testid={`measure-comment-${e.id}`}>
                  // Kommentar
                </button>
                <button onClick={() => void measure.copy(e.id, "cli")} title="Gleiche Messung in der CLI: cad measure …" data-testid={`measure-cli-${e.id}`}>
                  Als <code>cad measure</code>
                </button>
                <button className="mpdel" aria-label="Messung löschen" onClick={() => measure.remove(e.id)} title="Löschen (Entf)" data-testid={`measure-delete-${e.id}`}>
                  ×
                </button>
              </div>
            </li>
          );
        })}
      </ol>
    </aside>
  );
}
