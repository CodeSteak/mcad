// "Farben & Filamente" (Plan 16.3): colours of the model with their filament slot, volume and
// estimated mass; the slot is written into the code (`filament = n`), exports for Bambu
// Studio / OrcaSlicer, "Farbregionen zeigen".

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
      <div className="colorspanel" role="region" aria-label="Farben und Filamente" data-testid="colors-panel">
        <p className="pbnote">Keine Farben – mit color("Name") oder color("#rrggbb") gefärbte Teile erscheinen hier mit Filament-Slot und Volumen.</p>
      </div>
    );
  const model = getViewer()?.modelColor() ?? "#c3cad4";
  const rows = colorRows(scene, regions, theme, model);
  const shared = sharedSlots(rows);
  const total = rows.every((r) => r.volume !== null) ? rows.reduce((a, r) => a + (r.volume ?? 0), 0) : null;
  const warnings = regions && regions.version === scene.version ? regions.items.map((s) => s.warning).filter((w): w is string => !!w) : [];
  const parts = scene.context.module === null ? scene.parts.length : 0;
  return (
    <div className="colorspanel" role="region" aria-label="Farben und Filamente" data-testid="colors-panel">
      <div className="cptitle">
        <span>
          Farben &amp; Filamente <span className="cpcount">{rows.length}</span>
        </span>
      </div>
      <label className="check cpregions" title="Körper je Farbe anzeigen, wie sie exportiert und gedruckt werden (aus: Farbe je Fläche, schneller, bei Überlappung und Verrundung ungenau)">
        <input type="checkbox" checked={showRegions} onChange={(e) => store.setState({ showRegions: e.target.checked })} data-testid="colors-regions" />
        Farbregionen zeigen
      </label>
      <table className="cptable">
        <thead>
          <tr>
            <th>Farbe</th>
            <th title="Filament-Slot (AMS)">Slot</th>
            <th className="n">Volumen</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.key} data-testid={`color-row-${r.label}`} className={shared.includes(r.filament) ? "shared" : ""}>
              <td className="cpcolor">
                <span className="swatch" style={{ background: r.swatch }} data-testid={`color-swatch-${r.label}`} title={`${r.hex} (Anzeige ${r.swatch})`} />
                <span className="cplabel">
                  {r.label}
                  {r.source === "random" && (
                    <span className="badge warn" title="Name nicht in der Palette: stabile Zufallsfarbe">
                      zufällig
                    </span>
                  )}
                  {r.source === "default" && <span className="badge">ohne color()</span>}
                  <span className="cplines">
                    {r.places.map((p) => (
                      <button
                        key={p.label}
                        className="link"
                        title={p.file ? `${p.file}, Zeile ${p.line} zeigen` : `Zeile ${p.line} zeigen`}
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
                  aria-label={`Filament für ${r.label}`}
                  data-testid={`color-slot-${r.label}`}
                  value={r.explicit ? String(r.filament) : "auto"}
                  disabled={r.source === "default"}
                  title={
                    r.source === "default"
                      ? "Material ohne color() druckt immer mit Filament 1"
                      : "Schreibt filament = n in den color()-Aufruf; kommt die Farbe aus einem Parameter oder einer Schleife, filament(\"Farbe\", n); am Anfang der Datei"
                  }
                  onChange={(e) => client.setFilament(r.key, e.target.value === "auto" ? null : Number(e.target.value))}
                >
                  <option value="auto">{r.explicit ? "automatisch" : `${r.filament} (auto)`}</option>
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
          ))}
        </tbody>
        <tfoot>
          <tr>
            <td colSpan={2}>Summe{parts > 0 ? ` (${parts} Bauteile)` : ""}</td>
            <td className="n" data-testid="colors-total">
              {vol(total)}
              <span className="cpmass">{total !== null ? `${mass(total * 1.24e-3)} PLA` : ""}</span>
            </td>
          </tr>
        </tfoot>
      </table>
      {shared.length > 0 && <p className="pbnote warn">Slot {shared.join(", ")}: mehrere Farben auf einem Filament.</p>}
      {regionsError && <p className="pbnote warn">{regionsError}</p>}
      {warnings.map((w) => (
        <p key={w} className="pbnote warn">
          {w}
        </p>
      ))}
      <div className="cpexport">
        <b>Export für den Druck</b>
        <div className="cpexport-buttons">
          <Button onClick={() => void runExport("3mf")} title="3MF: je Bauteil ein Objekt, ein Teil je Farbe mit Filament-Slot (Bambu Studio, OrcaSlicer)">
            3MF mehrfarbig
          </Button>
          <Button onClick={() => void runExport("stl-colors")} title="ZIP mit einem STL je Farbe (alle im selben Koordinatensystem)">
            STL je Farbe (ZIP)
          </Button>
          <Button onClick={() => void runExport("3mf-single")} title="3MF ohne Farben: ein Netz je Bauteil">
            3MF einfarbig
          </Button>
        </div>
      </div>
      <p
        className="pbnote"
        title="Kommt die Farbe aus einem Parameter oder einer Schleife, schreibt die Auswahl filament(&quot;Farbe&quot;, n); an den Anfang der Datei. 3MF: ein Objekt je Bauteil (Rest: „ohne Bauteil“), ohne Farbgruppe – Bambu Studio und OrcaSlicer übernehmen die Slots ohne Farbdialog."
      >
        Slot ändern schreibt <code>filament = n</code> in den Code. Masse: PLA 1,24 g/cm³.
      </p>
    </div>
  );
}
