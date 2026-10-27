import { useEffect, useRef, useState } from "react";
import { createEditor } from "../editor/setup";
import { t, useLang } from "../i18n";
import { diagnosticPath } from "../logic/files";
import { IS_TAURI } from "../platform/desktop";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { setShowKeepout, setSideOpen, store, useCad } from "../state/store";
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
  confirmExport,
  focusNextViolation,
} from "./runtime";
import { Header, UploadInput, chooseUpload, promptNewProjectFile, useGlobalShortcuts } from "./header";
import { SectionBar, SectionStatus, setSectionOn } from "./section";
import { StudioControls } from "./studio";
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
  IconKeepout,
  IconAlert,
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
    { label: t("tab.makeMain"), hint: t("tab.makeMainHint"), disabled: path === main, onSelect: () => setMainFile(path) },
    {
      label: t("tab.close"),
      hint: path === main ? t("tab.closeMainHint") : t("tab.closeHint"),
      disabled: path === main,
      onSelect: () => {
        if (window.confirm(t("tab.closeConfirm", { path }))) removeProjectFile(path);
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
      <div className="tabs" role="tablist" aria-label={t("tab.list")}>
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
            title={t("tab.title", { name: path === p.main ? t("tab.mainName", { path }) : path })}
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
              <span className="star" aria-label={t("tab.main")} title={t("tab.main")}>
                ★
              </span>
            )}
            {path}
            {dirty.includes(path) && (
              <span className="dirty" title={t("file.unsaved")} data-testid={`dirty-${path}`}>
                ●
              </span>
            )}
            {errorsIn.has(path) && <span className="dot" title={t("tab.errors")} />}
          </button>
        ))}
      </div>
      <MenuButton
        label={t("tab.add")}
        text={null}
        icon={IconPlus}
        className="tab-add"
        hint={t("tab.addHint")}
        items={[
          { label: t("tab.newFile"), hint: t("file.addHint"), onSelect: promptNewProjectFile },
          { label: t("file.upload"), hint: t("file.uploadHint"), onSelect: chooseUpload },
          { kind: "sep" },
          { kind: "heading", label: p.active },
          ...tabItems(p.active, p.main),
        ]}
      />
      <ContextMenu at={menu} label={t("tab.menu", { path: menu?.path ?? "" })} items={menu ? tabItems(menu.path, p.main) : []} onClose={() => setMenu(null)} />
    </div>
  );
}

function modes(): Array<{ value: ViewMode; label: string; icon: IconType; hint: string }> {
  return [
    { value: "model", label: t("mode.model"), icon: IconModel, hint: t("mode.modelHint") },
    { value: "section", label: t("mode.section"), icon: IconSection, hint: t("mode.sectionHint") },
    { value: "sketch", label: t("mode.sketch"), icon: IconSketch, hint: t("mode.sketchHint") },
    { value: "measure", label: t("mode.measure"), icon: IconMeasure, hint: t("mode.measureHint") },
  ];
}

const VIEWS: StandardView[] = ["iso", "top", "front", "right", "back", "left", "bottom"];

function viewLabel(v: StandardView): string {
  return t(`view.${v}`);
}

/** Tool bar of the view: the mode on the left, camera and display on the right. */
function ViewToolbar({ viewer }: { viewer: Viewer | null }) {
  const mode = useCad((s) => viewMode(s));
  const allGrids = useCad((s) => s.allGrids);
  const sideOpen = useCad((s) => s.sideOpen);
  const showRegions = useCad((s) => s.showRegions);
  const showKeepout = useCad((s) => s.showKeepout);
  const hasKeepout = useCad((s) => (s.shown?.nodes ?? []).some((n) => n.flags.keepout));
  const [lastView, setLastView] = useState<StandardView>("iso");
  const view = (v: StandardView) => {
    setLastView(v);
    viewer?.setView(v);
  };
  return (
    <div className="vtoolbar">
      <Toolbar label={t("mode.label")} float>
        <Segmented label={t("mode.label")} value={mode} onChange={(m) => setViewMode(m, setSectionOn)} options={modes()} className="modes" data-testid="modes" />
      </Toolbar>
      <div className="vt-end">
        <Toolbar label={t("view.toolbar")} float>
          <ToolGroup>
            <IconButton icon={IconFit} label={t("view.fit")} hint={t("view.fitHint")} onClick={() => viewer?.fitAll()} />
            <MenuButton
              label={t("view.menu")}
              text={viewLabel(lastView)}
              hint={t("view.menuHint")}
              items={VIEWS.map((v) => ({ label: viewLabel(v), onSelect: () => view(v) }))}
              align="end"
              className="view-menu"
            />
          </ToolGroup>
          <ToolSep />
          <StudioControls />
          <ToolSep />
          <ToolGroup>
            <IconButton icon={IconGrid} label={t("view.allGrids")} hint={t("view.allGridsHint")} pressed={allGrids} onClick={() => store.setState({ allGrids: !allGrids })} />
            {hasKeepout && (
              <IconButton
                icon={IconKeepout}
                label={t("view.keepout")}
                hint={t("view.keepoutHint")}
                pressed={showKeepout}
                onClick={() => setShowKeepout(!showKeepout)}
                data-testid="toggle-keepout"
              />
            )}
            <MenuButton
              label={t("view.display")}
              text={null}
              icon={IconEye}
              align="end"
              hint={t("view.displayHint")}
              items={[
                { label: t("view.allGrids"), check: "checkbox", checked: allGrids, onSelect: () => store.setState({ allGrids: !allGrids }) },
                { label: t("view.showKeepout"), check: "checkbox", checked: showKeepout, hint: t("view.showKeepoutHint"), onSelect: () => setShowKeepout(!showKeepout) },
                { label: t("view.showRegions"), check: "checkbox", checked: showRegions, hint: t("view.showRegionsHint"), onSelect: () => store.setState({ showRegions: !showRegions }) },
                { label: t("side.label"), check: "checkbox", checked: sideOpen, hint: t("side.hint"), onSelect: () => setSideOpen(!sideOpen) },
              ]}
            />
            {!sideOpen && <IconButton icon={IconPanelOpen} label={t("side.show")} hint={t("side.hint")} onClick={() => setSideOpen(true)} />}
          </ToolGroup>
        </Toolbar>
        <BusySpinner />
      </div>
    </div>
  );
}

/** Small spinner under the camera bar while the engine loads or evaluates (fades in after a moment). */
function BusySpinner() {
  const ready = useCad((s) => s.engineReady);
  const progress = useCad((s) => s.progress);
  if (ready && !progress) return null;
  const text = !ready
    ? t("status.engineLoading")
    : `${t("status.computing")}${progress && progress.total > 0 ? ` · ${progress.done}/${progress.total}` : ""}${progress?.line ? ` · ${t("status.line", { line: progress.line })}` : ""}`;
  return (
    <div className="view-busy" role="status" aria-label={text} title={text} data-testid="view-busy">
      <span className="view-busy-ring" aria-hidden="true" />
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
        <Button variant="standard" className="refit" icon={IconFit} hint={t("view.refitHint")} onClick={() => viewer?.fitAll()} data-testid="refit">
          {t("view.refit")}
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
          if (!switchFile(path)) store.setState({ notice: t("notice.fileNotInProject", { path }) });
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
  const asserts = useCad((s) => s.asserts);
  const violations = asserts && complete && asserts.version === complete.version ? asserts.items.reduce((n, r) => n + r.pairs.filter((p) => !p.ok).length, 0) : 0;
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
        {!ready
          ? t("status.engineLoading")
          : progress && progress.total > 0
            ? `${t("status.evaluating", { done: progress.done, total: progress.total })}${progress.line ? ` · ${t("status.line", { line: progress.line })}` : ""}`
            : t("status.ready")}
      </span>
      <span className={`st-diag${errors ? " has-errors" : warnings ? " has-warnings" : ""}`} data-testid="status-errors">
        {t("status.errors", { n: errors })} · {t("status.warnings", { n: warnings })}
      </span>
      {syntax && <span className="st-note warn">{t("status.syntax")}</span>}
      {violations > 0 && (
        <button type="button" className="st-note assert-viol" data-testid="status-asserts" title={t("status.assertsHint")} onClick={focusNextViolation}>
          {t("status.asserts", { n: violations })}
        </button>
      )}
      {bang && (
        <span className="st-note bang" data-testid="status-bang">
          {bangLine ? t("status.bangLine", { line: bangLine }) : t("status.bang")}
        </span>
      )}
      {engineError && (
        <span className="st-note error" data-testid="status-engine-error" title={engineError.message}>
          {engineError.message}
        </span>
      )}
      {drag && (
        <span className="st-note drag" data-testid="status-drag">
          {drag.axis !== null ? `${t("drag.axis", { axis: "xyz"[drag.axis] })} · ` : ""}
          {drag.alt ? t("drag.alt") : drag.snap ? `Snap: ${drag.snap}` : drag.axis !== null ? t("drag.axisKeys") : t("drag.hint")}
        </span>
      )}
      {suspended.length > 0 && (
        <span className="st-note draft" data-testid="status-draft" title={t("status.draftHint")}>
          {t("status.draft", { lines: suspended.slice(0, 4).join(", ") + (suspended.length > 4 ? ", …" : "") })}
        </span>
      )}
      {hoverOther && (
        <span className="st-note" data-testid="status-hover-file" title={t("status.hoverFileHint")}>
          {t("status.hoverFile", { path: hoverOther.path, line: hoverOther.line })}
        </span>
      )}
      <SketchStatus />
      <MeasureStatus />
      <SectionStatus />
      {notice && <span className="st-note" data-testid="status-notice">{notice}</span>}
      <span className="grow" />
      {st && (
        <span className="st-stats" title={t("status.statsHint")}>
          {t("status.stats", { lang: st.eval_ms.toFixed(0), kernel: st.kernel_ms.toFixed(0), nodes: st.nodes, cached: st.cache_hits })}
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

/** Export with violated assertions: "Trotzdem exportieren" (like `mcad export --ignore-asserts`). */
function ExportConfirm() {
  const c = useCad((s) => s.exportConfirm);
  const ok = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (c) ok.current?.focus();
  }, [c]);
  if (!c) return null;
  return (
    <div className="modal-back" onKeyDown={(e) => e.key === "Escape" && void confirmExport(false)}>
      <div className="dialog export-confirm" role="alertdialog" aria-modal="true" aria-labelledby="export-confirm-title" data-testid="export-confirm">
        <h2 id="export-confirm-title">
          <IconAlert size={16} aria-hidden="true" /> {t("exportConfirm.title")}
        </h2>
        <p>
          {c.what}: {t("exportConfirm.violated", { n: c.violations.length })} ({t("exportConfirm.like")} <code>mcad export</code>
          {t("exportConfirm.likeAfter")} <code>--ignore-asserts</code>
          {t("exportConfirm.likeEnd")}).
        </p>
        <ul className="export-confirm-list">
          {c.violations.slice(0, 8).map((v) => (
            <li key={v}>{v}</li>
          ))}
          {c.violations.length > 8 && <li>… {t("exportConfirm.more", { n: c.violations.length - 8 })}</li>}
        </ul>
        <div className="dialog-buttons">
          <Button variant="quiet" onClick={() => void confirmExport(false)} data-testid="export-cancel">
            {t("common.cancel")}
          </Button>
          <Button ref={ok} variant="primary" onClick={() => void confirmExport(true)} data-testid="export-anyway">
            {t("exportConfirm.anyway")}
          </Button>
        </div>
      </div>
    </div>
  );
}

export function App() {
  const theme = useCad((s) => s.theme);
  // the root follows the language: a switch re-renders the whole tree
  const lang = useLang();
  useEffect(() => {
    // browser tab (the desktop app sets its window title itself)
    if (!IS_TAURI) document.title = t("app.title");
  }, [lang]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  useGlobalShortcuts();
  return (
    <div className="app">
      <Header />
      <main className="main">
        <section className="editorpane" aria-label={t("app.editor")}>
          <FileBar />
          <ExternalBar />
          <EditorPane />
        </section>
        <Splitter />
        <section className="viewcol" aria-label={t("app.view")}>
          <div className="viewbody">
            <ViewPane />
            <SidePanel />
          </div>
        </section>
      </main>
      <StatusBar />
      <ExportConfirm />
      <DiffDialog />
      <UploadInput />
    </div>
  );
}
