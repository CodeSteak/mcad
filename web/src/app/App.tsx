import { useEffect, useRef, useState } from "react";
import { createEditor } from "../editor/setup";
import { EXAMPLE_PROJECTS, GROUP_LABELS, SINGLE_EXAMPLES } from "../files/files";
import { pathSegments } from "../logic/context";
import { diagnosticPath } from "../logic/files";
import { fmtNum } from "../logic/format";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { store, useCad } from "../state/store";
import { Viewer, type StandardView } from "../view/viewer";
import {
  addProjectFile,
  downloadProjectZip,
  exportModel,
  exportParts,
  initialProject,
  jumpToLevel,
  meshes,
  newFile,
  onDocChange,
  openExample,
  openExampleProject,
  openFolderOrZip,
  openFromDisk,
  openZip,
  removeProjectFile,
  saveToDisk,
  sendNow,
  client,
  setMainFile,
  setViewer,
  startProject,
  switchFile,
  togglePart,
} from "./runtime";
import { ParamBar, SettingsButton } from "./panels";
import { SectionBar, SectionStatus, SectionToggle } from "./section";
import { DiffDialog, ExternalBar, RecentMenu } from "./desktopUi";

function Toolbar() {
  const theme = useCad((s) => s.theme);
  const fileName = useCad((s) => s.fileName);
  const groups = [...new Set(SINGLE_EXAMPLES.map((e) => e.group))];
  return (
    <header className="toolbar" role="toolbar" aria-label="Werkzeugleiste">
      <span className="brand">Parametrisches CAD</span>
      <span className="sep" />
      <button onClick={newFile} title="Neue Datei">Neu</button>
      <button onClick={() => void openFromDisk()} title="Datei öffnen">Öffnen…</button>
      <button onClick={() => void saveToDisk()} title="Speichern (Strg+S)">Speichern</button>
      <button onClick={() => void saveToDisk(true)} title="Speichern unter">Speichern unter…</button>
      <RecentMenu />
      <label className="examples">
        <span>Beispiele</span>
        <select
          aria-label="Beispiel öffnen"
          data-testid="examples"
          value=""
          onChange={(e) => {
            const v = e.target.value;
            const pr = EXAMPLE_PROJECTS.find((x) => `projekt:${x.name}` === v);
            if (pr) return openExampleProject(pr);
            const ex = SINGLE_EXAMPLES.find((x) => x.path === v);
            if (ex) openExample(ex);
          }}
        >
          <option value="">Beispiel wählen …</option>
          {groups.map((g) => (
            <optgroup key={g} label={GROUP_LABELS[g] ?? g}>
              {SINGLE_EXAMPLES.filter((e) => e.group === g).map((e) => (
                <option key={e.path} value={e.path}>
                  {e.name}
                </option>
              ))}
            </optgroup>
          ))}
          {EXAMPLE_PROJECTS.length > 0 && (
            <optgroup label="Projekte (mehrere Dateien)">
              {EXAMPLE_PROJECTS.map((p) => (
                <option key={p.name} value={`projekt:${p.name}`}>
                  {p.name}/
                </option>
              ))}
            </optgroup>
          )}
        </select>
      </label>
      <span className="sep" />
      <button onClick={() => void exportModel("stl")} title="Als STL exportieren">Export STL</button>
      <button onClick={() => void exportModel("3mf")} title="Als 3MF exportieren">Export 3MF</button>
      <button onClick={() => void exportModel("step")} title="Als STEP exportieren (über den Kernel)">Export STEP</button>
      <span className="grow" />
      <SettingsButton />
      <span className="filename" title="Aktuelle Datei">{fileName}</span>
      <button
        onClick={() => {
          const t = theme === "dark" ? "light" : "dark";
          try {
            localStorage.setItem("cad-theme", t);
          } catch {
            /* ignore */
          }
          store.setState({ theme: t });
        }}
        title="Hell/Dunkel umschalten"
        data-testid="theme"
      >
        {theme === "dark" ? "Hell" : "Dunkel"}
      </button>
    </header>
  );
}

/** File tabs of the project: switch, main file, add/remove files, folders and ZIPs. */
function FileBar() {
  const p = useCad((s) => s.project);
  const diags = useCad((s) => s.diagnostics);
  const dirty = useCad((s) => s.dirty);
  const errorsIn = new Set(diags.items.filter((d) => d.severity === "error").map((d) => diagnosticPath(d, p.main)));
  return (
    <div className="filebar" role="tablist" aria-label="Dateien des Projekts" data-testid="filebar">
      <div className="tabs">
        {p.paths.map((path) => (
          <button
            key={path}
            role="tab"
            aria-selected={path === p.active}
            className={`tab${path === p.active ? " active" : ""}${path === p.main ? " is-main" : ""}`}
            data-testid={`tab-${path}`}
            title={path === p.main ? `${path} (Hauptdatei)` : path}
            onClick={() => switchFile(path)}
          >
            {path === p.main && <span className="star" aria-label="Hauptdatei">★</span>}
            {path}
            {dirty.includes(path) && (
              <span className="dirty" title="Ungespeicherte Änderungen" data-testid={`dirty-${path}`}>
                ●
              </span>
            )}
            {errorsIn.has(path) && <span className="dot" title="Fehler in dieser Datei" />}
          </button>
        ))}
      </div>
      <div className="fileactions">
        <button
          title="Neue Datei im Projekt"
          data-testid="file-add"
          onClick={() => {
            const name = window.prompt("Name der neuen Datei (z. B. lib/teile.scad)", "neu.scad");
            if (name) addProjectFile(name);
          }}
        >
          + Datei
        </button>
        <button title="Angezeigte Datei als Hauptdatei auswerten" data-testid="file-main" disabled={p.active === p.main} onClick={() => setMainFile(p.active)}>
          ★ Hauptdatei
        </button>
        <button
          title="Angezeigte Datei aus dem Projekt entfernen"
          data-testid="file-remove"
          disabled={p.active === p.main}
          onClick={() => {
            if (window.confirm(`${p.active} aus dem Projekt entfernen?`)) removeProjectFile(p.active);
          }}
        >
          ×
        </button>
        <span className="sep" />
        <button title="Ordner als Projekt öffnen (sonst ZIP)" data-testid="open-folder" onClick={() => void openFolderOrZip()}>
          Ordner…
        </button>
        <button title="Projekt aus ZIP-Datei öffnen" data-testid="open-zip" onClick={() => void openZip()}>
          ZIP laden…
        </button>
        <button title="Projekt als ZIP herunterladen" data-testid="download-zip" onClick={() => downloadProjectZip()}>
          ZIP ↓
        </button>
      </div>
    </div>
  );
}

function vol(v: number | null | undefined): string {
  return v === null || v === undefined ? "–" : `${fmtNum(Math.round(v * 10) / 10)} mm³`;
}

/** Parts (named instances at the top level): visibility and export per part. */
function PartsBar() {
  const shown = useCad((s) => s.complete ?? s.shown);
  const hidden = useCad((s) => s.hiddenParts);
  const fileContext = !shown || shown.context.module === null;
  const parts = fileContext ? (shown?.parts ?? []) : [];
  const names = parts.map((p) => p.name);
  // Always one row (the 3D view keeps its size when parts come and go).
  if (!parts.length) {
    return (
      <div className="partsbar empty" data-testid="partsbar" aria-label="Bauteile">
        <span className="ptitle">Bauteile</span>
        <span className="pinfo">{fileContext ? "keine – benannte Instanzen auf oberster Ebene (deckel: deckel();) sind Bauteile" : "nur im Dateikontext"}</span>
      </div>
    );
  }
  return (
    <div className="partsbar" data-testid="partsbar" aria-label="Bauteile">
      <span className="ptitle">Bauteile</span>
      {parts.map((pt) => {
        const b = pt.bounds;
        const size = b ? b.max.map((x, i) => fmtNum(Math.round((x - b.min[i]) * 10) / 10)).join(" × ") : "";
        return (
          <span key={pt.name} className={`part${hidden.includes(pt.name) ? " off" : ""}`} data-testid={`part-${pt.name}`}>
            <label title={`Bauteil ${pt.name} (Zeile ${pt.range.line}) ein-/ausblenden`}>
              <input type="checkbox" checked={!hidden.includes(pt.name)} onChange={(e) => togglePart(pt.name, e.target.checked)} data-testid={`part-visible-${pt.name}`} />
              <span className="pname">{pt.name}</span>
            </label>
            <span className="pinfo" title={size ? `Hülle ${size} mm` : undefined}>
              {vol(pt.volume)}
            </span>
            <button onClick={() => void exportParts("stl", [pt.name], false)} title={`${pt.name} als STL`} data-testid={`part-stl-${pt.name}`}>
              STL
            </button>
            <button onClick={() => void exportParts("3mf", [pt.name], false)} title={`${pt.name} als 3MF`} data-testid={`part-3mf-${pt.name}`}>
              3MF
            </button>
            <button onClick={() => void exportParts("step", [pt.name], false)} title={`${pt.name} als STEP`} data-testid={`part-step-${pt.name}`}>
              STEP
            </button>
          </span>
        );
      })}
      <span className="pall">
        Alle:
        <button onClick={() => void exportParts("stl", names, true)} title="Jedes Bauteil als STL, zusammen als ZIP" data-testid="parts-zip">
          ZIP (STL)
        </button>
        <button onClick={() => void exportParts("step", names, true)} title="Jedes Bauteil als STEP, zusammen als ZIP" data-testid="parts-zip-step">
          ZIP (STEP)
        </button>
        <button onClick={() => void exportParts("3mf", names, false)} title="Ein 3MF mit einem Objekt je Bauteil" data-testid="parts-3mf">
          3MF
        </button>
      </span>
    </div>
  );
}

function PathBar() {
  const shown = useCad((s) => s.shown);
  const fileName = useCad((s) => s.fileName);
  const segs = pathSegments(shown, fileName);
  return (
    <nav className="pathbar" aria-label="Pfadleiste" data-testid="pathbar">
      {segs.map((s, i) => (
        <span key={i} className="seg-wrap">
          {i > 0 && <span className="chev">›</span>}
          {i < segs.length - 1 ? (
            <button className="seg link" onClick={() => jumpToLevel(i)} title={i === 0 ? "Zur Datei" : `Zu ${s}`}>
              {i === 0 ? "Datei" : s}
            </button>
          ) : (
            <span className="seg current">{i === 0 ? "Datei" : s}</span>
          )}
        </span>
      ))}
      {segs.length === 1 && <span className="pathfile">{fileName}</span>}
    </nav>
  );
}

function ViewPane() {
  const ref = useRef<HTMLDivElement>(null);
  const [viewer, setV] = useState<Viewer | null>(null);
  const allGrids = useCad((s) => s.allGrids);
  const refitHint = useCad((s) => s.refitHint);
  useEffect(() => {
    if (!ref.current) return;
    const v = new Viewer(ref.current, client, meshes);
    setViewer(v);
    setV(v);
    return () => {
      setViewer(null);
      v.dispose();
    };
  }, []);
  const view = (v: StandardView) => viewer?.setView(v);
  return (
    <div className="viewpane">
      <div className="viewport" ref={ref} data-testid="viewport" />
      <div className="viewtools" role="toolbar" aria-label="Ansicht">
        <button onClick={() => viewer?.fitAll()} title="Alles zeigen">Alles zeigen</button>
        {refitHint && (
          <button className="refit" onClick={() => viewer?.fitAll()} title="Das Modell hat seine Größe deutlich geändert: Ansicht neu einpassen" data-testid="refit">
            Neu einpassen
          </button>
        )}
        <button onClick={() => view("iso")}>Iso</button>
        <button onClick={() => view("top")}>Oben</button>
        <button onClick={() => view("front")}>Vorne</button>
        <button onClick={() => view("right")}>Rechts</button>
        <label className="check">
          <input type="checkbox" checked={allGrids} onChange={(e) => store.setState({ allGrids: e.target.checked })} data-testid="all-grids" />
          alle Raster
        </label>
        <SectionToggle />
      </div>
      <SectionBar />
    </div>
  );
}

function EditorPane() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let disposed = false;
    let view: ReturnType<typeof createEditor> | null = null;
    void initialProject().then((d) => {
      if (disposed || !ref.current) return;
      startProject(d);
      view = createEditor(ref.current, project.texts.get(project.active) ?? "", {
        onChange: onDocChange,
        onSave: () => void saveToDisk(),
        onOpenFile: (path) => {
          if (!switchFile(path)) store.setState({ notice: `Datei ${path} ist nicht im Projekt` });
        },
      });
      sendNow();
      view.focus();
    });
    return () => {
      disposed = true;
      view?.destroy();
      bridge.view = null;
    };
  }, []);
  return <div className="editor" ref={ref} data-testid="editor" />;
}

function StatusBar() {
  const progress = useCad((s) => s.progress);
  const diags = useCad((s) => s.diagnostics);
  const shown = useCad((s) => s.shown);
  const engineError = useCad((s) => s.engineError);
  const ready = useCad((s) => s.engineReady);
  const notice = useCad((s) => s.notice);
  const complete = useCad((s) => s.complete);
  const hoverOther = useCad((s) => s.viewHoverOther);
  const errors = diags.items.filter((d) => d.severity === "error").length;
  const warnings = diags.items.filter((d) => d.severity === "warning").length;
  const syntax = diags.items.some((d) => d.source === "syntax" && d.severity === "error");
  const bang = shown && (shown.root_is_subtree || shown.context.root_modifier !== null);
  const bangLine = shown && shown.context.root_modifier !== null ? shown.nodes[shown.context.root_modifier]?.range.line : null;
  const st = complete?.stats;
  const drag = useCad((s) => s.drag);
  // Draft mode while dragging: blends that take too long are left out until the drag ends.
  const suspended = shown ? [...new Set(shown.nodes.filter((n) => n.blend_suspended).map((n) => n.range.line))] : [];
  return (
    <footer className="statusbar" data-testid="statusbar">
      <span className="st-progress" data-testid="status-progress">
        {!ready ? "Engine lädt …" : progress && progress.total > 0 ? `Auswertung ${progress.done}/${progress.total}${progress.line ? ` · Zeile ${progress.line}` : ""}` : "Bereit"}
      </span>
      <span className={`st-diag${errors ? " has-errors" : warnings ? " has-warnings" : ""}`} data-testid="status-errors">
        {errors} Fehler · {warnings} Warnungen
      </span>
      {syntax && <span className="st-note warn">Syntaxfehler – letzter gültiger Stand wird angezeigt</span>}
      {bang && (
        <span className="st-note bang" data-testid="status-bang">
          ! aktiv: nur Teilbaum{bangLine ? ` aus Zeile ${bangLine}` : ""} sichtbar
        </span>
      )}
      {engineError && (
        <span className="st-note error" data-testid="status-engine-error" title={engineError.message}>
          {engineError.message}
        </span>
      )}
      {drag && (
        <span className="st-note drag" data-testid="status-drag">
          {drag.axis !== null ? `Achse ${"xyz"[drag.axis]} · ` : ""}
          {drag.alt ? "Alt: frei ziehen (Zahlen)" : drag.snap ? `Snap: ${drag.snap}` : drag.axis !== null ? "Umschalt/X/Y/Z: Achse" : "Ziehen – Alt: ohne Snap, Umschalt: Achse, Esc: abbrechen"}
        </span>
      )}
      {suspended.length > 0 && (
        <span className="st-note draft" data-testid="status-draft" title="Entwurfsmodus: Verrundungen, die länger als 25 ms brauchen, werden beim Ziehen ausgesetzt und danach berechnet">
          Verrundung beim Ziehen ausgesetzt (Zeile {suspended.slice(0, 4).join(", ")}
          {suspended.length > 4 ? ", …" : ""})
        </span>
      )}
      {hoverOther && (
        <span className="st-note" data-testid="status-hover-file" title="Klick öffnet die Datei an dieser Stelle">
          Fläche aus {hoverOther.path}, Zeile {hoverOther.line}
        </span>
      )}
      <SectionStatus />
      {notice && <span className="st-note" data-testid="status-notice">{notice}</span>}
      <span className="grow" />
      {st && (
        <span className="st-stats" title="Sprachauswertung · Kernel · Knoten (aus Cache)">
          Sprache {st.eval_ms.toFixed(0)} ms · Kernel {st.kernel_ms.toFixed(0)} ms · {st.nodes} Knoten ({st.cache_hits} aus Cache)
        </span>
      )}
      <span className="st-version">v{shown?.version ?? "–"}</span>
    </footer>
  );
}

function Splitter() {
  const onDown = (e: React.PointerEvent) => {
    const main = (e.currentTarget as HTMLElement).parentElement!;
    const move = (ev: PointerEvent) => {
      const r = main.getBoundingClientRect();
      const f = Math.min(0.75, Math.max(0.2, (ev.clientX - r.left) / r.width));
      main.style.setProperty("--split", `${(f * 100).toFixed(1)}%`);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  return <div className="splitter" onPointerDown={onDown} role="separator" aria-orientation="vertical" />;
}

export function App() {
  const theme = useCad((s) => s.theme);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  return (
    <div className="app">
      <Toolbar />
      <main className="main">
        <section className="editorpane" aria-label="Editor">
          <FileBar />
          <ExternalBar />
          <EditorPane />
        </section>
        <Splitter />
        <section className="viewcol" aria-label="3D-Ansicht">
          <PathBar />
          <PartsBar />

          <div className="viewbody">
            <ViewPane />
            <ParamBar />
          </div>
        </section>
      </main>
      <StatusBar />
      <DiffDialog />
    </div>
  );
}
