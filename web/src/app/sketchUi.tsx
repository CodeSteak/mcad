// Sketch editor (Plan 17.12): mode "Skizze" of the view toolbar, the context bar while drawing
// (docs/concepts/ui-redesign, Abschnitt 5: symbol groups, arc kinds in a menu, "Fertig"
// as the primary button, shortcuts in the tooltips) and the status line text (what the tool asks
// for, where the mouse snapped, why an action failed).

import type { SketchTool } from "../engine/protocol";
import { DICTS, t } from "../i18n";
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
  const ti = info(tool);
  return <IconButton icon={ICONS[tool]!} label={SKETCH_TOOL_NAMES[tool]} shortcut={SKETCH_TOOL_KEYS[tool]} hint={ti.title} pressed={active === tool} onClick={() => sketch.setTool(tool)} />;
}

/** Arc button: the last arc kind; the menu chooses the kind (3 points, centre, tangential). */
function ArcButton({ active }: { active: SketchTool }) {
  const last = useCad((s) => s.sketchArc);
  const cur = ARCS.includes(active) ? active : last;
  const ti = info(cur);
  return (
    <span className="ui-split ui-split--quiet">
      <IconButton icon={IconArc} label={SKETCH_TOOL_NAMES[cur]} shortcut={SKETCH_TOOL_KEYS[cur]} hint={ti.title} pressed={ARCS.includes(active)} onClick={() => sketch.setTool(cur)} className="ui-split-main" />
      <MenuButton
        label={t("sketch.arcKind")}
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
  const target = a?.sketch ? `sketch · ${t("status.line", { line: a.sketch.line })}` : t("sketch.new");
  return (
    <Toolbar label={t("mode.sketch")} float className="contextbar sketchbar" data-testid="sketchbar">
      <ToolLabel className="ctx-title" title={a?.sketch ? t("sketch.targetHint", { line: a.sketch.line }) : t("sketch.newHint")} data-testid="sketch-target">
        {target}
      </ToolLabel>
      <ToolGroup label={t("sketch.groupSelect")}>
        <ToolButton tool="select" active={s.tool} />
      </ToolGroup>
      <ToolSep />
      <ToolGroup label={t("sketch.groupDraw")}>
        <ToolButton tool="line" active={s.tool} />
        <ArcButton active={s.tool} />
        <ToolButton tool="bezier" active={s.tool} />
        <ToolButton tool="rect" active={s.tool} />
        <ToolButton tool="circle" active={s.tool} />
      </ToolGroup>
      <ToolSep />
      <ToolGroup label={t("sketch.groupEdit")}>
        <ToolButton tool="corner" active={s.tool} />
        <IconButton icon={IconCloseContour} label={t("sketch.tool.close")} shortcut="Enter" hint={t("sketch.closeHint")} onClick={() => sketch.closeContour()} />
        <IconButton icon={IconDelete} label={t("sketch.tool.delete")} shortcut="Entf" hint={t("sketch.deleteHint")} onClick={() => sketch.deleteSegment()} />
      </ToolGroup>
      <ToolSep />
      <IconButton icon={IconPerpendicular} label={t("sketch.align")} shortcut="N" hint={t("sketch.alignHint")} onClick={() => sketch.align()} />
      <Button variant="primary" icon={IconCheck} shortcut="Esc" hint={t("sketch.doneHint")} onClick={() => sketch.exit()}>
        {t("common.done")}
      </Button>
    </Toolbar>
  );
}

export function SketchStatus() {
  const s = useCad((st) => st.sketch);
  if (!s) return null;
  const a = s.answer;
  const text = s.message || a?.prompt || SKETCH_TOOL_NAMES[s.tool];
  // the prompts come from the engine (German): no tool prefix when they start with the tool's name
  const prefixes = [SKETCH_TOOL_NAMES[s.tool], DICTS.de[`sketch.tool.${s.tool}`] as string];
  return (
    <span className={`st-note sketch${s.message ? " warn" : ""}`} data-testid="status-sketch" title={t("sketch.statusHint")}>
      {!prefixes.some((p) => text.startsWith(p.split(" ")[0])) && <span className="st-tool">{SKETCH_TOOL_NAMES[s.tool]}: </span>}
      {text}
      {a?.snap ? ` · ${a.snap.label}` : ""}
    </span>
  );
}
