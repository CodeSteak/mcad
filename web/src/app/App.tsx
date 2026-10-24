import { useEffect, useRef, useState } from "react";
import { createEditor } from "../editor/setup";
import { diagnosticPath } from "../logic/files";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { setSideOpen, store, useCad } from "../state/store";
import { Viewer, type StandardView } from "../view/viewer";
import {
  initialProject,
  meshes,
  onDocChange,
  removeProjectFile,
  saveToDisk,
  sendNow,
  client,
  setMainFile,
  setViewer,
  setViewMode,
  startProject,
  switchFile,
  viewMode,
  type ViewMode,
} from "./runtime";
import { Header, UploadInput, chooseUpload, promptNewProjectFile, useGlobalShortcuts } from "./header";
import { SectionBar, SectionStatus, setSectionOn } from "./section";
import { DiffDialog, ExternalBar } from "./desktopUi";
import { SketchBar, SketchStatus } from "./sketchUi";
import { MeasureBar, MeasureStatus } from "./measureUi";
import { SidePanel } from "./sidePanel";
import { ViewCube } from "./viewCube";
import {
  Button,
  ContextMenu,
  IconButton,
  IconEye,
  IconFit,
  IconGrid,
  IconMeasure,
  IconModel,
  IconPanelOpen,
  IconPlus,
  IconSection,
  IconSketch,
  type IconType,
  type MenuEntry,
  MenuButton,
  Segmented,
  Toolbar,
  ToolGroup,
  ToolSep,
} from "./ui";

/** Actions of a file tab (context menu and the "+" menu). */
function tabItems(path: string, main: string): MenuEntry[] {
  return [
    { label: "Als Hauptdatei", hint: "Diese Datei als Hauptdatei auswerten", disabled: path === main, onSelect: () => setMainFile(path) },
    {
      label: "Schließen",
      hint: path === main ? "Die Hauptdatei bleibt im Projekt" : "Datei aus dem Projekt entfernen",
      disabled: path === main,
      onSelect: () => {
        if (window.confirm(`${path} aus dem Projekt entfernen?`)) removeProjectFile(path);
      },
    },
  ];
}

/**
 * File tabs of the project; "+" adds or loads files and holds the actions of the shown file;
 * right click (or the menu key, Umschalt+F10) on a tab: "Als Hauptdatei", "Schließen".
 */
function FileBar() {
  const p = useCad((s) => s.project);
  const diags = useCad((s) => s.diagnostics);
  const dirty = useCad((s) => s.dirty);
  const [menu, setMenu] = useState<{ x: number; y: number; path: string; opener: HTMLElement } | null>(null);
  const errorsIn = new Set(diags.items.filter((d) => d.severity === "error").map((d) => diagnosticPath(d, p.main)));
  const openMenu = (path: string, el: HTMLElement, x?: number, y?: number) => {
    const r = el.getBoundingClientRect();
    setMenu({ x: x ?? r.left, y: y ?? r.bottom + 2, path, opener: el });
  };
  return (
    <div className="filebar" data-testid="filebar">
      <div className="tabs" role="tablist" aria-label="Dateien des Projekts">
        {p.paths.map((path) => (
          <button
            key={path}
            type="button"
            role="tab"
            aria-label={path}
            aria-selected={path === p.active}
            aria-haspopup="menu"
            tabIndex={path === p.active ? 0 : -1}
            className={`tab${path === p.active ? " active" : ""}${path === p.main ? " is-main" : ""}`}
            data-testid={`tab-${path}`}
            title={`${path === p.main ? `${path} (Hauptdatei)` : path} – Rechtsklick: Als Hauptdatei, Schließen`}
            onClick={() => switchFile(path)}
            onContextMenu={(e) => {
              e.preventDefault();
              openMenu(path, e.currentTarget, e.clientX, e.clientY);
            }}
            onKeyDown={(e) => {
              if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
                e.preventDefault();
                openMenu(path, e.currentTarget);
                return;
              }
              const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
              if (!d) return;
              e.preventDefault();
              const i = (p.paths.indexOf(path) + d + p.paths.length) % p.paths.length;
              switchFile(p.paths[i]);
              requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(".filebar [role=tab][aria-selected=true]")?.focus());
            }}
          >
            {path === p.main && (
              <span className="star" aria-label="Hauptdatei" title="Hauptdatei">
                ★
              </span>
            )}
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
      <MenuButton
        label="Datei hinzufügen"
        text={null}
        icon={IconPlus}
        className="tab-add"
        hint="Neue Datei, Datei laden; Aktionen der angezeigten Datei"
        items={[
          { label: "Neue Datei …", hint: "Neue, leere Datei im Projekt", onSelect: promptNewProjectFile },
          { label: "Datei laden …", hint: "SVG (für import), Schrift .ttf/.otf (für text) oder .scad ins Projekt laden", onSelect: chooseUpload },
          { kind: "sep" },
          { kind: "heading", label: p.active },
          ...tabItems(p.active, p.main),
        ]}
      />
      <ContextMenu at={menu} label={`Datei ${menu?.path ?? ""}`} items={menu ? tabItems(menu.path, p.main) : []} onClose={() => setMenu(null)} />
    </div>
  );
}

const MODES: Array<{ value: ViewMode; label: string; icon: IconType; hint: string }> = [
  { value: "model", label: "Modell", icon: IconModel, hint: "Modell ansehen, Griffe ziehen" },
  { value: "section", label: "Schnitt", icon: IconSection, hint: "3D-Schnittansicht (Ebene am Griff ziehen, Alt: ohne Rasten)" },
  { value: "sketch", label: "Skizze", icon: IconSketch, hint: "Skizze zeichnen: Werkzeuge in der Skizzenebene, der Code entsteht im sketch-Block am Cursor (Esc beendet)" },
  { value: "measure", label: "Messen", icon: IconMeasure, hint: "Abstand, Radius, Winkel, Länge, Fläche, Körper – auch im Schnitt (Esc beendet)" },
];

const VIEWS: Array<{ v: StandardView; label: string }> = [
  { v: "iso", label: "Iso" },
  { v: "top", label: "Oben" },
  { v: "front", label: "Vorne" },
  { v: "right", label: "Rechts" },
  { v: "back", label: "Hinten" },
  { v: "left", label: "Links" },
  { v: "bottom", label: "Unten" },
];

/** Tool bar of the view: the mode on the left, camera and display on the right. */
function ViewToolbar({ viewer }: { viewer: Viewer | null }) {
  const mode = useCad((s) => viewMode(s));
  const allGrids = useCad((s) => s.allGrids);
  const sideOpen = useCad((s) => s.sideOpen);
  const showRegions = useCad((s) => s.showRegions);
  const [lastView, setLastView] = useState<StandardView>("iso");
  const view = (v: StandardView) => {
    setLastView(v);
    viewer?.setView(v);
  };
  return (
    <div className="vtoolbar">
      <Toolbar label="Modus" float>
        <Segmented label="Modus" value={mode} onChange={(m) => setViewMode(m, setSectionOn)} options={MODES} className="modes" data-testid="modes" />
      </Toolbar>
      <Toolbar label="Kamera und Anzeige" float>
        <ToolGroup>
          <IconButton icon={IconFit} label="Einpassen" hint="Alles zeigen" onClick={() => viewer?.fitAll()} />
          <MenuButton
            label="Ansicht"
            text={VIEWS.find((x) => x.v === lastView)?.label ?? "Iso"}
            hint="Standardansichten (auch über den Ansichtswürfel unten rechts)"
            items={VIEWS.map((x) => ({ label: x.label, onSelect: () => view(x.v) }))}
            align="end"
            className="view-menu"
          />
        </ToolGroup>
        <ToolSep />
        <ToolGroup>
          <IconButton icon={IconGrid} label="Alle Raster" hint="Alle Raster zeigen (sonst die am Cursor)" pressed={allGrids} onClick={() => store.setState({ allGrids: !allGrids })} />
          <MenuButton
            label="Anzeige"
            text={null}
            icon={IconEye}
            align="end"
            hint="Weitere Anzeigeoptionen"
            items={[
              { label: "Alle Raster", check: "checkbox", checked: allGrids, onSelect: () => store.setState({ allGrids: !allGrids }) },
              { label: "Farbregionen zeigen", check: "checkbox", checked: showRegions, hint: "Körper je Farbe, wie sie exportiert und gedruckt werden", onSelect: () => store.setState({ showRegions: !showRegions }) },
              { label: "Seitenpanel", check: "checkbox", checked: sideOpen, hint: "Bauteile, Farben, Messungen", onSelect: () => setSideOpen(!sideOpen) },
            ]}
          />
          {!sideOpen && <IconButton icon={IconPanelOpen} label="Seitenpanel einblenden" hint="Bauteile, Farben, Messungen" onClick={() => setSideOpen(true)} />}
        </ToolGroup>
      </Toolbar>
    </div>
  );
}

function ViewPane() {
  const ref = useRef<HTMLDivElement>(null);
  const [viewer, setV] = useState<Viewer | null>(null);
  const refitHint = useCad((s) => s.refitHint);
  const mode = useCad((s) => viewMode(s));
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
  return (
    <div className="viewpane">
      <div className="viewport" ref={ref} data-testid="viewport" />
      <div className="viewoverlay">
        <ViewToolbar viewer={viewer} />
        {/* the context bar of the mode (a section kept while drawing or measuring has none) */}
        <div className="ctxslot">
          {mode === "sketch" && <SketchBar />}
          {mode === "measure" && <MeasureBar />}
          {mode === "section" && <SectionBar />}
        </div>
      </div>
      {refitHint && (
        <Button variant="standard" className="refit" icon={IconFit} hint="Das Modell hat seine Größe deutlich geändert: Ansicht neu einpassen" onClick={() => viewer?.fitAll()} data-testid="refit">
          Neu einpassen
        </Button>
      )}
      <ViewCube viewer={viewer} />
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
      <SketchStatus />
      <MeasureStatus />
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
  useGlobalShortcuts();
  return (
    <div className="app">
      <Header />
      <main className="main">
        <section className="editorpane" aria-label="Editor">
          <FileBar />
          <ExternalBar />
          <EditorPane />
        </section>
        <Splitter />
        <section className="viewcol" aria-label="3D-Ansicht">
          <div className="viewbody">
            <ViewPane />
            <SidePanel />
          </div>
        </section>
      </main>
      <StatusBar />
      <DiffDialog />
      <UploadInput />
    </div>
  );
}
