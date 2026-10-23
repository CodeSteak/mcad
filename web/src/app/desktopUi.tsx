// Desktop app UI parts: bar for external changes, diff view ("Vergleichen"), recent files.

import { diffLines } from "../logic/textDiff";
import { IS_TAURI } from "../platform/desktop";
import { project } from "../state/project";
import { store, useCad } from "../state/store";
import { closeRemoved, compare, keepMine, openRecent, reloadFile, saveProject } from "./desktop";

/** "Datei wurde extern geändert: Neu laden / Meine behalten / Vergleichen" (above the editor). */
export function ExternalBar() {
  const ext = useCad((s) => s.external);
  const active = useCad((s) => s.project.active);
  const entries = Object.entries(ext).sort(([a], [b]) => (a === active ? -1 : b === active ? 1 : a.localeCompare(b)));
  if (!entries.length) return null;
  return (
    <div className="extbar-wrap">
      {entries.map(([path, e]) =>
        e.kind === "changed" ? (
          <div key={path} className="extbar" role="alert" data-testid="external-bar" data-path={path}>
            <span className="msg">
              <b>{path}</b> wurde extern geändert (ungespeicherte Änderungen im Editor).
            </span>
            <button onClick={() => reloadFile(path, e.text ?? "")} data-testid="external-reload" title="Stand der Festplatte übernehmen (rückgängig machbar)">
              Neu laden
            </button>
            <button onClick={() => keepMine(path)} data-testid="external-keep" title="Editor-Stand behalten; Speichern überschreibt die Datei">
              Meine behalten
            </button>
            <button onClick={() => compare(path)} data-testid="external-compare">
              Vergleichen
            </button>
          </div>
        ) : (
          <div key={path} className="extbar removed" role="alert" data-testid="external-bar" data-path={path}>
            <span className="msg">
              <b>{path}</b> wurde gelöscht oder umbenannt. Der Inhalt bleibt im Editor; Speichern legt die Datei wieder an.
            </span>
            <button onClick={() => void saveProject()} data-testid="external-restore">
              Wieder speichern
            </button>
            {path !== project.main && (
              <button onClick={() => closeRemoved(path)} data-testid="external-close">
                Schließen
              </button>
            )}
          </div>
        ),
      )}
    </div>
  );
}

/** Simple diff: disk (old) against the editor (new). */
export function DiffDialog() {
  const d = useCad((s) => s.diffView);
  if (!d) return null;
  const rows = diffLines(d.disk, d.mine);
  const close = () => store.setState({ diffView: null });
  return (
    <div className="modal-back" onClick={close}>
      <div className="diffview" role="dialog" aria-label={`Vergleich ${d.path}`} data-testid="diff-view" onClick={(e) => e.stopPropagation()}>
        <header>
          <b>{d.path}</b>: <span className="del">− auf der Festplatte</span> · <span className="add">+ im Editor</span>
          <span className="grow" />
          <button onClick={() => reloadFile(d.path, d.disk)} data-testid="diff-reload">
            Festplatte übernehmen
          </button>
          <button
            onClick={() => {
              keepMine(d.path);
            }}
            data-testid="diff-keep"
          >
            Meine behalten
          </button>
          <button onClick={close}>Schließen</button>
        </header>
        <pre>
          {rows.map((r, i) => (
            <div key={i} className={`dl ${r.kind}`}>
              <span className="ln">{r.a ?? ""}</span>
              <span className="ln">{r.b ?? ""}</span>
              <span className="mk">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : " "}</span>
              {r.text}
            </div>
          ))}
        </pre>
      </div>
    </div>
  );
}

/** "Zuletzt geöffnet" (desktop app). */
export function RecentMenu() {
  const recent = useCad((s) => s.recent);
  if (!IS_TAURI) return null;
  return (
    <label className="examples">
      <span>Zuletzt</span>
      <select
        aria-label="Zuletzt geöffnet"
        data-testid="recent"
        value=""
        onChange={(e) => {
          if (e.target.value) void openRecent(e.target.value);
        }}
      >
        <option value="">{recent.length ? "Zuletzt geöffnet …" : "(noch nichts)"}</option>
        {recent.map((r) => (
          <option key={r.path} value={r.path} title={r.path}>
            {(r.folder ? `${r.path}/` : r.path).split("/").slice(-3).join("/")}
          </option>
        ))}
      </select>
    </label>
  );
}
