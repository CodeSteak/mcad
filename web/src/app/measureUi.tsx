// Measure mode (Plan 18): mode "Messen" of the view toolbar, the context bar (kind, "Im Schnitt",
// copy/delete the selected or last measurement, "Fertig"), the status line text and the list of
// measurements in the tab "Messen" of the side panel (docs/konzepte/ui-ueberarbeitung).

import { MEASURE_KINDS, accuracyText, kindAvailable, sectionText } from "../logic/measure";
import type { MeasureKind, MeasurePick } from "../engine/protocol";
import { useCad } from "../state/store";
import { measure } from "./runtime";
import { setSectionOn } from "./section";
import {
  Button,
  IconAngle,
  IconArea,
  IconBody,
  IconButton,
  IconCheck,
  IconCopy,
  IconDelete,
  IconDistance,
  IconLength,
  IconRadius,
  type IconType,
  IconWall,
  Segmented,
  Toolbar,
  ToolSep,
} from "./ui";

const NO_PICKS: MeasurePick[] = [];

const KIND_ICONS: Record<MeasureKind, IconType> = {
  distance: IconDistance,
  radius: IconRadius,
  angle: IconAngle,
  length: IconLength,
  area: IconArea,
  body: IconBody,
  wall: IconWall,
};

export function MeasureBar() {
  const m = useCad((s) => s.measure);
  const sectionOn = useCad((s) => s.section.on);
  const target = useCad((s) => s.measureSelected ?? s.measurements[s.measurements.length - 1]?.id ?? null);
  const isSel = useCad((s) => s.measureSelected !== null);
  if (!m) return null;
  return (
    <Toolbar label="Messen" float className="contextbar measurebar" data-testid="measurebar">
      <Segmented
        label="Messart"
        className="measure-kinds"
        value={m.kind}
        onChange={(k) => measure.setKind(k)}
        options={MEASURE_KINDS.map((k) => {
          const ok = kindAvailable(k.kind, m.inSection);
          return { value: k.kind, label: k.label, icon: KIND_ICONS[k.kind], shortcut: k.key, disabled: !ok, hint: `${k.title}${ok ? "" : k.where === "section" ? " – nur im Schnitt" : " – nicht im Schnitt"}` };
        })}
      />
      <ToolSep />
      <Button
        variant="quiet"
        pressed={m.inSection}
        hint={sectionOn ? "In der Schnittebene messen: Konturen, Kreise, Wandstärke" : "Schaltet die Schnittansicht ein und misst in der Schnittebene"}
        onClick={() => measure.setInSection(!m.inSection, () => setSectionOn(true))}
      >
        Im Schnitt
      </Button>
      <IconButton icon={IconCopy} label="Messung kopieren" hint={isSel ? "Gewählte Messung als Text kopieren" : "Letzte Messung als Text kopieren"} disabled={target === null} onClick={() => target !== null && void measure.copy(target, "text")} />
      <IconButton icon={IconDelete} label="Messung löschen" shortcut="Entf" hint={isSel ? "Gewählte Messung löschen" : "Letzte Messung löschen"} disabled={target === null} onClick={() => target !== null && measure.remove(target)} />
      <ToolSep />
      <Button variant="primary" icon={IconCheck} shortcut="Esc" hint="Messen beenden" onClick={() => measure.exit()}>
        Fertig
      </Button>
    </Toolbar>
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
  const list = useCad((s) => s.measurements);
  const sel = useCad((s) => s.measureSelected);
  const version = useCad((s) => s.complete?.version ?? null);
  const picks = useCad((s) => s.measure?.picks) ?? NO_PICKS;
  return (
    <div className="measurepanel" role="region" aria-label="Messungen" data-testid="measure-panel">
      <div className="mptitle">
        <span>
          Messungen <span className="mpcount">{list.length}</span>
        </span>
        <span className="mpactions">
          <IconButton icon={IconCopy} label="Alle kopieren" hint="Alle Messungen als Text kopieren" disabled={!list.length} onClick={() => void measure.copyAll()} />
          <IconButton icon={IconDelete} label="Alle löschen" hint="Alle Messungen löschen" disabled={!list.length} onClick={() => measure.clear()} />
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
              tabIndex={0}
              aria-current={sel === e.id ? "true" : undefined}
              aria-label={`${m.title} ${m.label}`}
              title="Klick: in der Ansicht hervorheben"
              onClick={() => measure.select(sel === e.id ? null : e.id)}
              onKeyDown={(ev) => {
                if (ev.target !== ev.currentTarget || (ev.key !== "Enter" && ev.key !== " ")) return;
                ev.preventDefault();
                measure.select(sel === e.id ? null : e.id);
              }}
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
                <Button className="ui-btn--small" aria-label="Als Text kopieren" onClick={() => void measure.copy(e.id, "text")} title="Als Text kopieren">
                  Kopieren
                </Button>
                <Button className="ui-btn--small" aria-label="Als Kommentar kopieren" onClick={() => void measure.copy(e.id, "comment")} title="Als Code-Kommentar kopieren (// Messung: …)">
                  // Kommentar
                </Button>
                <Button className="ui-btn--small" aria-label="Als mcad measure kopieren" onClick={() => void measure.copy(e.id, "cli")} title="Gleiche Messung in der CLI: mcad measure …">
                  <code>mcad measure</code>
                </Button>
                <IconButton className="ui-btn--small mpdel" icon={IconDelete} label="Messung löschen" shortcut="Entf" onClick={() => measure.remove(e.id)} />
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
