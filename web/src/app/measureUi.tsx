// Measure mode (Plan 18): mode "Messen" of the view toolbar, the context bar (kind, "Im Schnitt",
// copy/delete the selected or last measurement, "Fertig"), the status line text and the list of
// measurements in the tab "Messen" of the side panel (docs/konzepte/ui-ueberarbeitung).

import { t } from "../i18n";
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
    <Toolbar label={t("mode.measure")} float className="contextbar measurebar" data-testid="measurebar">
      <Segmented
        label={t("measure.kindLabel")}
        className="measure-kinds"
        value={m.kind}
        onChange={(k) => measure.setKind(k)}
        options={MEASURE_KINDS.map((k) => {
          const ok = kindAvailable(k.kind, m.inSection);
          return { value: k.kind, label: k.label, icon: KIND_ICONS[k.kind], shortcut: k.key, disabled: !ok, hint: `${k.title}${ok ? "" : ` – ${k.where === "section" ? t("measure.onlySection") : t("measure.notSection")}`}` };
        })}
      />
      <ToolSep />
      <Button
        variant="quiet"
        pressed={m.inSection}
        hint={sectionOn ? t("measure.inSectionHint") : t("measure.inSectionOffHint")}
        onClick={() => measure.setInSection(!m.inSection, () => setSectionOn(true))}
      >
        {t("measure.inSection")}
      </Button>
      <IconButton icon={IconCopy} label={t("measure.copy")} hint={isSel ? t("measure.copySelHint") : t("measure.copyLastHint")} disabled={target === null} onClick={() => target !== null && void measure.copy(target, "text")} />
      <IconButton icon={IconDelete} label={t("measure.delete")} shortcut="Entf" hint={isSel ? t("measure.deleteSelHint") : t("measure.deleteLastHint")} disabled={target === null} onClick={() => target !== null && measure.remove(target)} />
      <ToolSep />
      <Button variant="primary" icon={IconCheck} shortcut="Esc" hint={t("measure.doneHint")} onClick={() => measure.exit()}>
        {t("common.done")}
      </Button>
    </Toolbar>
  );
}

export function MeasureStatus() {
  const m = useCad((s) => s.measure);
  if (!m) return null;
  const text = m.message || m.prompt || t("mode.measure");
  const sec = m.inSection && m.section ? (m.section.message ? ` · ${m.section.message}` : ` · ${t("measure.sectionInfo", { regions: m.section.regions, area: Number(m.section.area.toFixed(3)) })}${m.section.exact ? `, ${t("measure.exact")}` : ""}`) : "";
  return (
    <span className={`st-note measure${m.message ? " warn" : ""}`} data-testid="status-measure" title={t("measure.statusHint")}>
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
    <div className="measurepanel" role="region" aria-label={t("measure.list")} data-testid="measure-panel">
      <div className="mptitle">
        <span>
          {t("measure.list")} <span className="mpcount">{list.length}</span>
        </span>
        <span className="mpactions">
          <IconButton icon={IconCopy} label={t("measure.copyAll")} hint={t("measure.copyAllHint")} disabled={!list.length} onClick={() => void measure.copyAll()} />
          <IconButton icon={IconDelete} label={t("measure.deleteAll")} hint={t("measure.deleteAllHint")} disabled={!list.length} onClick={() => measure.clear()} />
        </span>
      </div>
      {picks.length > 0 && (
        <div className="mppicks" data-testid="measure-picks">
          {t("measure.picked")}: {picks.map((p) => p.label).join(" · ")}
        </div>
      )}
      {!list.length && <p className="mpempty">{t("measure.empty")}</p>}
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
              title={t("measure.itemHint")}
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
                <span className={`badge${m.exact ? "" : " warn"}`} title={m.exact ? t("measure.exactHint") : t("measure.approx")}>
                  {accuracyText(m)}
                </span>
                {stale && (
                  <span className="badge warn" title={t("measure.staleHint")}>
                    {t("measure.stale")}
                  </span>
                )}
              </div>
              <div className="mppicks">{m.picks.map((p) => p.label).join(" – ")}</div>
              {sec && <div className="mpdetail">{t("measure.inSectionAt", { plane: sec })}</div>}
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
                <Button className="ui-btn--small" aria-label={t("measure.copyText")} onClick={() => void measure.copy(e.id, "text")} title={t("measure.copyText")}>
                  {t("measure.copyShort")}
                </Button>
                <Button className="ui-btn--small" aria-label={t("measure.copyComment")} onClick={() => void measure.copy(e.id, "comment")} title={t("measure.copyCommentHint")}>
                  {`// ${t("measure.commentShort")}`}
                </Button>
                <Button className="ui-btn--small" aria-label={t("measure.copyCli")} onClick={() => void measure.copy(e.id, "cli")} title={t("measure.copyCliHint")}>
                  <code>mcad measure</code>
                </Button>
                <IconButton className="ui-btn--small mpdel" icon={IconDelete} label={t("measure.delete")} shortcut="Entf" onClick={() => measure.remove(e.id)} />
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
