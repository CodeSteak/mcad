// Sketch editor (Plan 17.12): toggle in the view toolbar, the tool bar while drawing and the
// status line text (what the tool asks for, where the mouse snapped, why an action failed).

import { SKETCH_TOOLS, SKETCH_TOOL_NAMES } from "../logic/sketchEditor";
import { useCad } from "../state/store";
import { sketch } from "./runtime";

export function SketchToggle() {
  const on = useCad((s) => s.sketch !== null);
  return (
    <button
      aria-pressed={on}
      className={on ? "on" : ""}
      onClick={() => (on ? sketch.exit() : sketch.enter())}
      title="Skizze zeichnen: Werkzeuge in der Skizzenebene, Code entsteht im sketch-Block am Cursor (Esc beendet)"
      data-testid="sketch-toggle"
    >
      Skizze
    </button>
  );
}

export function SketchBar() {
  const s = useCad((st) => st.sketch);
  if (!s) return null;
  const a = s.answer;
  const target = a?.sketch ? `sketch in Zeile ${a.sketch.line}` : "neue Skizze";
  return (
    <div className="sketchbar" role="toolbar" aria-label="Skizze" data-testid="sketchbar">
      <span className="stitle" title={a?.sketch ? "Der Code entsteht in diesem sketch-Block (Cursor)" : "Die erste Aktion legt einen sketch-Block an"}>
        Skizze · <span data-testid="sketch-target">{target}</span>
      </span>
      {SKETCH_TOOLS.map((t) => (
        <button key={t.tool} aria-pressed={s.tool === t.tool} className={s.tool === t.tool ? "on" : ""} title={t.title} onClick={() => sketch.setTool(t.tool)} data-testid={`sketch-tool-${t.tool}`}>
          {t.label}
        </button>
      ))}
      <span className="sep" />
      <button onClick={() => sketch.closeContour()} title="close(); für die offene Kontur am Cursor" data-testid="sketch-close">
        Schließen
      </button>
      <button onClick={() => sketch.deleteSegment()} title="Segment am Cursor löschen (Entf)" data-testid="sketch-delete">
        Löschen
      </button>
      <button onClick={() => sketch.align()} title="Ansicht senkrecht auf die Skizzenebene" data-testid="sketch-align">
        ⊥ Ansicht
      </button>
      <button onClick={() => sketch.exit()} title="Zeichenmodus beenden (Esc)" data-testid="sketch-exit">
        Fertig
      </button>
    </div>
  );
}

export function SketchStatus() {
  const s = useCad((st) => st.sketch);
  if (!s) return null;
  const a = s.answer;
  const text = s.message || a?.prompt || SKETCH_TOOL_NAMES[s.tool];
  return (
    <span className={`st-note sketch${s.message ? " warn" : ""}`} data-testid="status-sketch" title="Alt: frei mit Schrittweite · Esc: Punkte verwerfen, nochmals: beenden">
      {text}
      {a?.snap ? ` · ${a.snap.label}` : ""}
    </span>
  );
}
