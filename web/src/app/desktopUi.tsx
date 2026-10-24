// Desktop app UI parts: bar for external changes, diff view ("Vergleichen"). The recent files
// are a submenu of "Datei" (header.tsx).

import { diffLines } from "../logic/textDiff";
import { project } from "../state/project";
import { store, useCad } from "../state/store";
import { closeRemoved, compare, keepMine, reloadFile, saveProject } from "./desktop";
import { Button } from "./ui";

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
            <Button onClick={() => reloadFile(path, e.text ?? "")} data-testid="external-reload" title="Stand der Festplatte übernehmen (rückgängig machbar)">
              Neu laden
            </Button>
            <Button onClick={() => keepMine(path)} data-testid="external-keep" title="Editor-Stand behalten; Speichern überschreibt die Datei">
              Meine behalten
            </Button>
            <Button onClick={() => compare(path)} data-testid="external-compare">
              Vergleichen
            </Button>
          </div>
        ) : (
          <div key={path} className="extbar removed" role="alert" data-testid="external-bar" data-path={path}>
            <span className="msg">
              <b>{path}</b> wurde gelöscht oder umbenannt. Der Inhalt bleibt im Editor; Speichern legt die Datei wieder an.
            </span>
            <Button onClick={() => void saveProject()} data-testid="external-restore">
              Wieder speichern
            </Button>
            {path !== project.main && (
              <Button onClick={() => closeRemoved(path)} data-testid="external-close">
                Schließen
              </Button>
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
          <Button onClick={() => reloadFile(d.path, d.disk)} data-testid="diff-reload">
            Festplatte übernehmen
          </Button>
          <Button
            onClick={() => {
              keepMine(d.path);
            }}
            data-testid="diff-keep"
          >
            Meine behalten
          </Button>
          <Button onClick={close}>Schließen</Button>
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
