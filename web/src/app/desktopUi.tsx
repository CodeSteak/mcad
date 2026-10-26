// Desktop app UI parts: bar for external changes, diff view ("Vergleichen"). The recent files
// are a submenu of "Datei" (header.tsx).

import { t } from "../i18n";
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
              <b>{path}</b> {t("external.changed")}
            </span>
            <Button onClick={() => reloadFile(path, e.text ?? "")} data-testid="external-reload" title={t("external.reloadHint")}>
              {t("external.reload")}
            </Button>
            <Button onClick={() => keepMine(path)} data-testid="external-keep" title={t("external.keepHint")}>
              {t("external.keep")}
            </Button>
            <Button onClick={() => compare(path)} data-testid="external-compare">
              {t("external.compare")}
            </Button>
          </div>
        ) : (
          <div key={path} className="extbar removed" role="alert" data-testid="external-bar" data-path={path}>
            <span className="msg">
              <b>{path}</b> {t("external.removed")}
            </span>
            <Button onClick={() => void saveProject()} data-testid="external-restore">
              {t("external.restore")}
            </Button>
            {path !== project.main && (
              <Button onClick={() => closeRemoved(path)} data-testid="external-close">
                {t("common.close")}
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
      <div className="diffview" role="dialog" aria-label={t("diff.label", { path: d.path })} data-testid="diff-view" onClick={(e) => e.stopPropagation()}>
        <header>
          <b>{d.path}</b>: <span className="del">− {t("diff.disk")}</span> · <span className="add">+ {t("diff.editor")}</span>
          <span className="grow" />
          <Button onClick={() => reloadFile(d.path, d.disk)} data-testid="diff-reload">
            {t("diff.takeDisk")}
          </Button>
          <Button
            onClick={() => {
              keepMine(d.path);
            }}
            data-testid="diff-keep"
          >
            {t("external.keep")}
          </Button>
          <Button onClick={close}>{t("common.close")}</Button>
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
