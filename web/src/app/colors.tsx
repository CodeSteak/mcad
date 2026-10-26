// "Farben & Filamente" (Plan 16.3): colours of the model with their filament slot, volume and
// estimated mass; the slot is written into the code (`filament = n`), exports for Bambu
// Studio / OrcaSlicer, "Farbregionen zeigen".

import { t } from "../i18n";
import { colorRows, MAX_FILAMENT, sharedSlots } from "../logic/colors";
import { fmtNum } from "../logic/format";
import { bridge } from "../state/bridge";
import { store, useCad } from "../state/store";
import { client, getViewer, openLocation, runExport } from "./runtime";
import { Button } from "./ui";

function vol(v: number | null): string {
  return v === null ? "…" : `${fmtNum(Math.round(v * 10) / 10)} mm³`;
}

function mass(g: number | null): string {
  return g === null ? "" : `${fmtNum(Math.round(g * 100) / 100)} g`;
}

export function ColorsPanel() {
  const scene = useCad((s) => s.complete);
  const regions = useCad((s) => s.regions);
  const regionsError = useCad((s) => s.regionsError);
  const theme = useCad((s) => s.theme);
  const showRegions = useCad((s) => s.showRegions);
  if (!scene || !(scene.colors ?? []).length)
    return (
      <div className="colorspanel" role="region" aria-label={t("colors.region")} data-testid="colors-panel">
        <p className="pbnote">{t("colors.none")}</p>
      </div>
    );
  const model = getViewer()?.modelColor() ?? "#c3cad4";
  const rows = colorRows(scene, regions, theme, model);
  const shared = sharedSlots(rows);
  const total = rows.every((r) => r.volume !== null) ? rows.reduce((a, r) => a + (r.volume ?? 0), 0) : null;
  const warnings = regions && regions.version === scene.version ? regions.items.map((s) => s.warning).filter((w): w is string => !!w) : [];
  const parts = scene.context.module === null ? scene.parts.length : 0;
  return (
    <div className="colorspanel" role="region" aria-label={t("colors.region")} data-testid="colors-panel">
      <div className="cptitle">
        <span>
          {t("colors.title")} <span className="cpcount">{rows.length}</span>
        </span>
      </div>
      <label className="check cpregions" title={t("colors.regionsHint")}>
        <input type="checkbox" checked={showRegions} onChange={(e) => store.setState({ showRegions: e.target.checked })} data-testid="colors-regions" />
        {t("view.showRegions")}
      </label>
      <table className="cptable">
        <thead>
          <tr>
            <th>{t("colors.color")}</th>
            <th title={t("colors.slotHint")}>{t("colors.slot")}</th>
            <th className="n">{t("colors.volume")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            // `label` is a fixed value (test ids); material without color() has the label "default", shown translated
            const name = r.source === "default" ? t("colors.defaultLabel") : r.label;
            return (
              <tr key={r.key} data-testid={`color-row-${r.label}`} className={shared.includes(r.filament) ? "shared" : ""}>
                <td className="cpcolor">
                  <span className="swatch" style={{ background: r.swatch }} data-testid={`color-swatch-${r.label}`} title={t("colors.swatch", { hex: r.hex, swatch: r.swatch })} />
                  <span className="cplabel">
                    {name}
                    {r.source === "random" && (
                      <span className="badge warn" title={t("colors.randomHint")}>
                        {t("colors.random")}
                      </span>
                    )}
                    {r.source === "default" && <span className="badge">{t("colors.default")}</span>}
                    <span className="cplines">
                      {r.places.map((p) => (
                        <button
                          key={p.label}
                          className="link"
                          title={p.file ? t("colors.showFileLine", { file: p.file, line: p.line }) : t("colors.showLine", { line: p.line })}
                          onClick={() => (p.file ? openLocation(p.file, p.from) : bridge.setCursorLine(p.line))}
                        >
                          {p.label}
                        </button>
                      ))}
                    </span>
                  </span>
                </td>
                <td>
                  <select
                    aria-label={t("colors.filamentFor", { label: name })}
                    data-testid={`color-slot-${r.label}`}
                    value={r.explicit ? String(r.filament) : "auto"}
                    disabled={r.source === "default"}
                    title={
                      r.source === "default"
                        ? t("colors.defaultSlotHint")
                        : t("colors.slotSelectHint")
                    }
                    onChange={(e) => client.setFilament(r.key, e.target.value === "auto" ? null : Number(e.target.value))}
                  >
                    <option value="auto">{r.explicit ? t("colors.auto") : `${r.filament} (auto)`}</option>
                    {Array.from({ length: MAX_FILAMENT }, (_, i) => i + 1).map((s) => (
                      <option key={s} value={String(s)}>
                        {s}
                      </option>
                    ))}
                  </select>
                </td>
                <td className="n" data-testid={`color-volume-${r.label}`}>
                  {vol(r.volume)}
                  <span className="cpmass">{mass(r.grams)}</span>
                </td>
              </tr>
            );
          })}
        </tbody>
        <tfoot>
          <tr>
            <td colSpan={2}>
              {t("common.sum")}
              {parts > 0 ? ` (${t("parts.count", { n: parts })})` : ""}
            </td>
            <td className="n" data-testid="colors-total">
              {vol(total)}
              <span className="cpmass">{total !== null ? `${mass(total * 1.24e-3)} PLA` : ""}</span>
            </td>
          </tr>
        </tfoot>
      </table>
      {shared.length > 0 && <p className="pbnote warn">{t("colors.shared", { slots: shared.join(", ") })}</p>}
      {regionsError && <p className="pbnote warn">{regionsError}</p>}
      {warnings.map((w) => (
        <p key={w} className="pbnote warn">
          {w}
        </p>
      ))}
      <div className="cpexport">
        <b>{t("colors.exportTitle")}</b>
        <div className="cpexport-buttons">
          <Button onClick={() => void runExport("3mf")} title={t("export.hint.3mf")}>
            {t("export.label.3mf")}
          </Button>
          <Button onClick={() => void runExport("stl-colors")} title={t("export.hint.stlColors")}>
            {t("export.label.stlColors")}
          </Button>
          <Button onClick={() => void runExport("3mf-single")} title={t("export.hint.3mfSingle")}>
            {t("export.label.3mfSingle")}
          </Button>
        </div>
      </div>
      <p
        className="pbnote"
        title={t("colors.noteHint")}
      >
        {t("colors.note1")} <code>filament = n</code> {t("colors.note2")}
      </p>
    </div>
  );
}
