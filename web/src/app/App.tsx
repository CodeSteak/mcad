import { useEffect, useRef, useState } from "react";
import { createEditor } from "../editor/setup";
import { EXAMPLES, GROUP_LABELS } from "../files/files";
import { pathSegments } from "../logic/context";
import { bridge } from "../state/bridge";
import { store, useCad } from "../state/store";
import { Viewer, type StandardView } from "../view/viewer";
import {
  exportModel,
  initialDocument,
  jumpToLevel,
  meshes,
  newFile,
  onDocChange,
  openExample,
  openFromDisk,
  saveToDisk,
  sendNow,
  client,
  setViewer,
} from "./runtime";
import { ParamBar, SettingsButton } from "./panels";

function Toolbar() {
  const theme = useCad((s) => s.theme);
  const fileName = useCad((s) => s.fileName);
  const groups = [...new Set(EXAMPLES.map((e) => e.group))];
  return (
    <header className="toolbar" role="toolbar" aria-label="Werkzeugleiste">
      <span className="brand">Parametrisches CAD</span>
      <span className="sep" />
      <button onClick={newFile} title="Neue Datei">Neu</button>
      <button onClick={() => void openFromDisk()} title="Datei öffnen">Öffnen…</button>
      <button onClick={() => void saveToDisk()} title="Speichern (Strg+S)">Speichern</button>
      <button onClick={() => void saveToDisk(true)} title="Speichern unter">Speichern unter…</button>
      <label className="examples">
        <span>Beispiele</span>
        <select
          aria-label="Beispiel öffnen"
          data-testid="examples"
          value=""
          onChange={(e) => {
            const ex = EXAMPLES.find((x) => x.path === e.target.value);
            if (ex) openExample(ex);
          }}
        >
          <option value="">Beispiel wählen …</option>
          {groups.map((g) => (
            <optgroup key={g} label={GROUP_LABELS[g] ?? g}>
              {EXAMPLES.filter((e) => e.group === g).map((e) => (
                <option key={e.path} value={e.path}>
                  {e.name}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </label>
      <span className="sep" />
      <button onClick={() => void exportModel("stl")} title="Als STL exportieren">Export STL</button>
      <button onClick={() => void exportModel("3mf")} title="Als 3MF exportieren">Export 3MF</button>
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
      </div>
    </div>
  );
}

function EditorPane() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let disposed = false;
    let view: ReturnType<typeof createEditor> | null = null;
    void initialDocument().then((d) => {
      if (disposed || !ref.current) return;
      store.setState({ fileName: d.name });
      view = createEditor(ref.current, d.text, { onChange: onDocChange, onSave: () => void saveToDisk() });
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
          <EditorPane />
        </section>
        <Splitter />
        <section className="viewcol" aria-label="3D-Ansicht">
          <PathBar />
          <div className="viewbody">
            <ViewPane />
            <ParamBar />
          </div>
        </section>
      </main>
      <StatusBar />
    </div>
  );
}
