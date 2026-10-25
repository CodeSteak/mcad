// Head bar (docs/konzepte/ui-ueberarbeitung, Abschnitt 3): app name, menus "Datei" and
// "Beispiele", the file name in the middle (dirty dot, path into the module context), "Speichern"
// as the primary button, the export split button (keeps the last format), theme and settings as
// symbol buttons. Global shortcuts: Strg+N/O/S/⇧S/E.

import { useEffect } from "react";
import { EXAMPLE_PROJECTS, GROUP_LABELS, SINGLE_EXAMPLES } from "../files/files";
import { pathSegments } from "../logic/context";
import { MODEL_ACCEPT, MODEL_EXT } from "../logic/files";
import { IS_TAURI } from "../platform/desktop";
import { type ExportChoice, type ThemePref, setThemePref, store, useCad } from "../state/store";
import { openRecent } from "./desktop";
import { SettingsButton } from "./panels";
import {
  EXPORT_LABELS,
  addProjectFile,
  downloadProjectZip,
  jumpToLevel,
  newFile,
  openExample,
  openExampleProject,
  openFolderOrZip,
  openFromDisk,
  openZip,
  partNames,
  runExport,
  saveToDisk,
  uploadProjectFiles,
} from "./runtime";
import { Button, IconDownload, IconMoon, IconSave, IconSun, IconSystem, type MenuEntry, MenuButton, SplitButton } from "./ui";

export const UPLOAD_INPUT_ID = "cad-upload-input";

/** Opens the file dialog of "Datei laden …" (SVG, fonts, .mcad/.scad into the project). */
export function chooseUpload(): void {
  (document.getElementById(UPLOAD_INPUT_ID) as HTMLInputElement | null)?.click();
}

/** Asks for a name and adds an empty file to the project. */
export function promptNewProjectFile(): void {
  const name = window.prompt("Name der neuen Datei (z. B. lib/teile.mcad)", `neu${MODEL_EXT}`);
  if (name) addProjectFile(name);
}

/** Hidden file input of "Datei laden …" (rendered once). */
export function UploadInput() {
  return (
    <input
      id={UPLOAD_INPUT_ID}
      type="file"
      multiple
      accept={`.svg,.stl,.ttf,.otf,${MODEL_ACCEPT}`}
      data-testid="file-upload-input"
      aria-hidden="true"
      tabIndex={-1}
      style={{ display: "none" }}
      onChange={(e) => {
        const files = e.currentTarget.files;
        if (files && files.length) void uploadProjectFiles(Array.from(files));
        e.currentTarget.value = "";
      }}
    />
  );
}

function recentItems(recent: Array<{ path: string; folder: boolean }>): MenuEntry[] {
  if (!recent.length) return [{ label: "(noch nichts)", onSelect: () => {}, disabled: true }];
  return recent.map((r) => {
    const parts = (r.folder ? `${r.path}/` : r.path).split("/").filter(Boolean);
    return { label: parts.slice(-2).join("/") + (r.folder ? "/" : ""), detail: parts.length > 2 ? `…/${parts.slice(-3, -2)[0]}` : undefined, hint: r.path, onSelect: () => void openRecent(r.path) };
  });
}

export function fileMenuItems(recent: Array<{ path: string; folder: boolean }>): MenuEntry[] {
  return [
    { label: "Neu", shortcut: IS_TAURI ? "Strg+N" : undefined, hint: "Neue Datei (das Projekt wird ersetzt)", onSelect: newFile },
    { label: "Öffnen …", shortcut: "Strg+O", hint: "Datei öffnen", onSelect: () => void openFromDisk() },
    { label: "Ordner öffnen …", hint: IS_TAURI ? "Ordner als Projekt öffnen" : "Ordner als Projekt öffnen (ohne Ordnerzugriff des Browsers: ZIP)", onSelect: () => void openFolderOrZip() },
    ...(IS_TAURI ? [{ kind: "sub" as const, label: "Zuletzt geöffnet", items: recentItems(recent) }] : []),
    { kind: "sep" },
    { label: "Speichern unter …", shortcut: "Strg+⇧+S", onSelect: () => void saveToDisk(true) },
    { kind: "sep" },
    { label: "Datei hinzufügen …", hint: "Neue, leere Datei im Projekt", onSelect: promptNewProjectFile },
    { label: "Datei laden …", hint: "SVG (für import), Schrift .ttf/.otf (für text) oder .mcad/.scad ins Projekt laden", onSelect: chooseUpload },
    { kind: "sep" },
    { label: "ZIP laden …", hint: "Projekt aus ZIP-Datei öffnen", onSelect: () => void openZip() },
    { label: "ZIP speichern", hint: "Projekt als ZIP herunterladen", onSelect: downloadProjectZip },
  ];
}

export function exampleItems(): MenuEntry[] {
  const groups = [...new Set(SINGLE_EXAMPLES.map((e) => e.group))];
  const out: MenuEntry[] = groups.map((g) => ({
    kind: "sub" as const,
    label: GROUP_LABELS[g] ?? g,
    items: SINGLE_EXAMPLES.filter((e) => e.group === g).map((e) => ({ label: e.name, hint: e.path, onSelect: () => openExample(e) })),
  }));
  if (EXAMPLE_PROJECTS.length)
    out.push({
      kind: "sub",
      label: "Projekte (mehrere Dateien)",
      items: EXAMPLE_PROJECTS.map((p) => ({ label: `${p.name}/`, onSelect: () => openExampleProject(p) })),
    });
  return out;
}

const EXPORT_ORDER: ExportChoice[] = ["3mf", "3mf-single", "stl", "stl-colors", "step"];

export function exportItems(current: ExportChoice, parts: number): MenuEntry[] {
  const item = (c: ExportChoice, hint: string): MenuEntry => ({
    label: EXPORT_LABELS[c].label,
    hint,
    check: "radio",
    checked: c === current,
    shortcut: c === current ? "Strg+E" : undefined,
    onSelect: () => void runExport(c),
  });
  const hints: Record<ExportChoice, string> = {
    "3mf": "3MF: je Bauteil ein Objekt, ein Teil je Farbe mit Filament-Slot (Bambu Studio, OrcaSlicer)",
    "3mf-single": "3MF ohne Farben: ein Netz je Bauteil",
    stl: "Als STL exportieren",
    "stl-colors": "ZIP mit einem STL je Farbe (alle im selben Koordinatensystem)",
    step: "Als STEP exportieren (über den Kernel)",
    "parts-stl": "Jedes Bauteil als STL, zusammen als ZIP",
    "parts-step": "Jedes Bauteil als STEP, zusammen als ZIP",
    "parts-3mf": "Ein 3MF mit einem Objekt je Bauteil",
  };
  const partItem = (c: ExportChoice): MenuEntry => ({ ...(item(c, hints[c]) as object), disabled: parts === 0 }) as MenuEntry;
  return [
    ...EXPORT_ORDER.map((c) => item(c, hints[c])),
    { kind: "sep" },
    {
      kind: "sub",
      label: "Je Bauteil",
      disabled: parts === 0,
      hint: parts === 0 ? "Keine Bauteile – benannte Instanzen auf oberster Ebene (deckel: deckel();) sind Bauteile" : `${parts} Bauteile`,
      items: [partItem("parts-stl"), partItem("parts-step"), partItem("parts-3mf")],
    },
  ];
}

function ThemeMenu() {
  const pref = useCad((s) => s.themePref);
  const theme = useCad((s) => s.theme);
  const opt = (p: ThemePref, label: string): MenuEntry => ({ label, check: "radio", checked: pref === p, onSelect: () => setThemePref(p) });
  return (
    <MenuButton
      label="Farbschema"
      text={null}
      icon={pref === "system" ? IconSystem : theme === "dark" ? IconMoon : IconSun}
      hint={`${pref === "system" ? "System" : pref === "dark" ? "Dunkel" : "Hell"}; System folgt der Einstellung des Betriebssystems`}
      align="end"
      items={[opt("light", "Hell"), opt("dark", "Dunkel"), opt("system", "System")]}
      data-testid="theme"
    />
  );
}

/** File name in the middle: dirty dot (desktop app) and the path into the module context. */
function Title() {
  const fileName = useCad((s) => s.fileName);
  const shown = useCad((s) => s.shown);
  const dirty = useCad((s) => s.dirty.length > 0);
  const segs = pathSegments(shown, fileName);
  return (
    <nav className="hd-title" aria-label="Pfadleiste" data-testid="pathbar">
      {segs.map((s, i) => (
        <span key={i} className="hd-seg-wrap">
          {i > 0 && (
            <span className="hd-chev" aria-hidden="true">
              ›
            </span>
          )}
          {i < segs.length - 1 ? (
            <button type="button" className="hd-seg link" onClick={() => jumpToLevel(i)} aria-label={i === 0 ? `Zur Datei ${s}` : `Zu ${s}`} title={i === 0 ? "Zur Datei (Dateikontext)" : `Zu ${s}`}>
              {s}
            </button>
          ) : (
            <span className={`hd-seg current${i === 0 ? " file" : ""}`} title={i === 0 ? "Aktuelle Datei" : `Kontext: Modul ${s}`} data-testid={i === 0 ? "filename" : undefined}>
              {s}
            </span>
          )}
          {i === 0 && dirty && (
            <span className="hd-dirty" title="Ungespeicherte Änderungen" aria-label="ungespeichert" data-testid="title-dirty">
              ●
            </span>
          )}
        </span>
      ))}
    </nav>
  );
}

function ExportButton() {
  const choice = useCad((s) => s.exportChoice);
  const parts = useCad((s) => {
    const sh = s.complete ?? s.shown;
    return sh && sh.context.module === null ? sh.parts.length : 0;
  });
  const l = EXPORT_LABELS[choice];
  return (
    <SplitButton
      label={`Exportieren: ${l.label}`}
      text={l.short}
      icon={IconDownload}
      variant="standard"
      shortcut="Strg+E"
      onClick={() => void runExport(choice)}
      menuLabel="Exportformat"
      items={exportItems(choice, parts)}
      data-testid="export"
    />
  );
}

export function Header() {
  const recent = useCad((s) => s.recent);
  return (
    <header className="hd" data-testid="header">
      <div className="hd-left" role="toolbar" aria-label="Datei und Beispiele">
        <span className="hd-brand" title="mcad – parametrisches CAD">mcad</span>
        <MenuButton label="Datei" items={fileMenuItems(recent)} data-testid="file-menu" />
        <MenuButton label="Beispiele" items={exampleItems()} data-testid="examples" />
      </div>
      <Title />
      <div className="hd-right" role="toolbar" aria-label="Speichern, Export, Einstellungen">
        <Button variant="primary" icon={IconSave} shortcut="Strg+S" hint={IS_TAURI ? undefined : "mehrere Dateien: ZIP"} onClick={() => void saveToDisk()} data-testid="save">
          Speichern
        </Button>
        <ExportButton />
        <span className="ui-sep" role="separator" aria-orientation="vertical" />
        <ThemeMenu />
        <SettingsButton />
      </div>
    </header>
  );
}

/** Strg+N, Strg+O, Strg+S, Strg+⇧+S, Strg+E anywhere in the window. */
export function useGlobalShortcuts(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.defaultPrevented) return;
      const k = e.key.toLowerCase();
      let f: (() => void) | null = null;
      if (k === "s") f = () => void saveToDisk(e.shiftKey);
      else if (k === "o" && !e.shiftKey) f = () => void openFromDisk();
      else if (k === "n" && !e.shiftKey) f = newFile;
      else if (k === "e" && !e.shiftKey) f = () => void runExport(store.getState().exportChoice);
      if (!f) return;
      e.preventDefault();
      f();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

export { partNames };
