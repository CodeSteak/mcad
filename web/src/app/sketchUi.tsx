// Sketch editor (Plan 17.12): mode "Skizze" of the view toolbar, the context bar while drawing
// (docs/konzepte/ui-ueberarbeitung, Abschnitt 5: symbol groups, arc kinds in a menu, "Fertig"
// as the primary button, shortcuts in the tooltips) and the status line text (what the tool asks
// for, where the mouse snapped, why an action failed).

import type { SketchTool } from "../engine/protocol";
import { SKETCH_TOOL_KEYS, SKETCH_TOOLS, SKETCH_TOOL_NAMES } from "../logic/sketchEditor";
import { useCad } from "../state/store";
import { sketch } from "./runtime";
import {
  Button,
  IconArc,
  IconBezier,
  IconButton,
  IconCheck,
  IconChevronDown,
  IconCircle,
  IconCloseContour,
  IconCorner,
  IconDelete,
  IconLine,
  IconPerpendicular,
  IconRect,
  IconSelect,
  type IconType,
  MenuButton,
  Toolbar,
  ToolGroup,
  ToolLabel,
  ToolSep,
} from "./ui";

const ICONS: Partial<Record<SketchTool, IconType>> = {
  select: IconSelect,
  line: IconLine,
  arc_through: IconArc,
  arc_center: IconArc,
  arc_tangent: IconArc,
  bezier: IconBezier,
  rect: IconRect,
  circle: IconCircle,
  corner: IconCorner,
};

const ARCS: SketchTool[] = ["arc_through", "arc_center", "arc_tangent"];

function info(tool: SketchTool) {
  return SKETCH_TOOLS.find((t) => t.tool === tool)!;
}

function ToolButton({ tool, active }: { tool: SketchTool; active: SketchTool }) {
  const t = info(tool);
  return <IconButton icon={ICONS[tool]!} label={SKETCH_TOOL_NAMES[tool]} shortcut={SKETCH_TOOL_KEYS[tool]} hint={t.title} pressed={active === tool} onClick={() => sketch.setTool(tool)} />;
}

/** Arc button: the last arc kind; the menu chooses the kind (3 points, centre, tangential). */
function ArcButton({ active }: { active: SketchTool }) {
  const last = useCad((s) => s.sketchArc);
  const cur = ARCS.includes(active) ? active : last;
  const t = info(cur);
  return (
    <span className="ui-split ui-split--quiet">
      <IconButton icon={IconArc} label={SKETCH_TOOL_NAMES[cur]} shortcut={SKETCH_TOOL_KEYS[cur]} hint={t.title} pressed={ARCS.includes(active)} onClick={() => sketch.setTool(cur)} className="ui-split-main" />
      <MenuButton
        label="Bogenart"
        text={null}
        icon={IconChevronDown}
        className="ui-split-menu ui-split-menu--narrow"
        items={ARCS.map((a) => ({ label: SKETCH_TOOL_NAMES[a], shortcut: SKETCH_TOOL_KEYS[a], hint: info(a).title, check: "radio" as const, checked: active === a, onSelect: () => sketch.setTool(a) }))}
      />
    </span>
  );
}

export function SketchBar() {
  const s = useCad((st) => st.sketch);
  if (!s) return null;
  const a = s.answer;
  const target = a?.sketch ? `sketch · Zeile ${a.sketch.line}` : "neue Skizze";
  return (
    <Toolbar label="Skizze" float className="contextbar sketchbar" data-testid="sketchbar">
      <ToolLabel className="ctx-title" title={a?.sketch ? `Der Code entsteht im sketch-Block in Zeile ${a.sketch.line} (Cursor)` : "Die erste Aktion legt einen sketch-Block an"} data-testid="sketch-target">
        {target}
      </ToolLabel>
      <ToolGroup label="Auswahl">
        <ToolButton tool="select" active={s.tool} />
      </ToolGroup>
      <ToolSep />
      <ToolGroup label="Zeichnen">
        <ToolButton tool="line" active={s.tool} />
        <ArcButton active={s.tool} />
        <ToolButton tool="bezier" active={s.tool} />
        <ToolButton tool="rect" active={s.tool} />
        <ToolButton tool="circle" active={s.tool} />
      </ToolGroup>
      <ToolSep />
      <ToolGroup label="Bearbeiten">
        <ToolButton tool="corner" active={s.tool} />
        <IconButton icon={IconCloseContour} label="Schließen" shortcut="Enter" hint="close(); für die offene Kontur am Cursor" onClick={() => sketch.closeContour()} />
        <IconButton icon={IconDelete} label="Löschen" shortcut="Entf" hint="Segment am Cursor löschen" onClick={() => sketch.deleteSegment()} />
      </ToolGroup>
      <ToolSep />
      <IconButton icon={IconPerpendicular} label="⊥ Ansicht" shortcut="N" hint="Ansicht senkrecht auf die Skizzenebene" onClick={() => sketch.align()} />
      <Button variant="primary" icon={IconCheck} shortcut="Esc" hint="Zeichenmodus beenden" onClick={() => sketch.exit()}>
        Fertig
      </Button>
    </Toolbar>
  );
}

export function SketchStatus() {
  const s = useCad((st) => st.sketch);
  if (!s) return null;
  const a = s.answer;
  const text = s.message || a?.prompt || SKETCH_TOOL_NAMES[s.tool];
  return (
    <span className={`st-note sketch${s.message ? " warn" : ""}`} data-testid="status-sketch" title="Alt: frei mit Schrittweite · Esc: Punkte verwerfen, nochmals: beenden">
      {!text.startsWith(SKETCH_TOOL_NAMES[s.tool].split(" ")[0]) && <span className="st-tool">{SKETCH_TOOL_NAMES[s.tool]}: </span>}
      {text}
      {a?.snap ? ` · ${a.snap.label}` : ""}
    </span>
  );
}
