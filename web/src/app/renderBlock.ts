// Studio settings ↔ the main file's `render { … }` block.
//
//   scene with a block → the studio settings follow it (the file is the source of truth; a file
//   whose block does not say `style = "standard"` switches the studio look on when opened)
//   panel change → the view at once, the block a moment later: `SetRender` (one in flight; the
//   engine answers `Edits`, which the drag controller applies as one undoable editor change and
//   sends at once, so the next request sees the new text). Without a block the first change
//   writes one with every value that differs from the defaults.

import type { EngineClient } from "../engine/client";
import type { Scene } from "../engine/protocol";
import { blockChanges, blockKey, settingsFromBlock } from "../logic/renderBlock";
import { sanitizeStudio, type StudioSettings } from "../logic/studio";
import { project } from "../state/project";
import { setStudio, store } from "../state/store";

/** Wait after the last change before writing (slider drags). */
const WRITE_DELAY_MS = 250;

/** Settings the file has (`null`: unknown or no block) and the last block applied. */
let fileSettings: StudioSettings | null = null;
let appliedKey: string | null = null;
/** File whose block was applied last (opening another file may switch the look on). */
let appliedFile: string | null = null;
/** Changes not written yet: the settings before the first of them. */
let pendingFrom: StudioSettings | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
/** A `SetRender` waits for its `Edits`. */
let inflight = false;
/** The engine (set by `initRenderBlock`; the runtime imports this module's users). */
let client: EngineClient | null = null;
/** A block was written in this session for the current main file (the scene may not show it yet). */
let wroteBlock: string | null = null;

/** The scene whose render block counts: the latest evaluation of the main file. */
function blockOf(s: Scene | null): Scene["render"] | null {
  return s?.render ?? null;
}

/** Line of the main file's render block (`null`: the file has none). */
export function renderBlockLine(st = store.getState()): number | null {
  return blockOf(st.scene)?.range.line ?? null;
}

/** Changes the studio settings from the panel: the view at once, the file's block after a
 *  short wait. Switching the look on or off is not written. */
export function editStudio(patch: Partial<StudioSettings>): void {
  const prev = store.getState().studio;
  setStudio(patch);
  const next = store.getState().studio;
  if (JSON.stringify({ ...prev, on: true }) === JSON.stringify({ ...next, on: true })) return;
  pendingFrom ??= prev;
  schedule();
}

function schedule(): void {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    flush();
  }, WRITE_DELAY_MS);
}

/** Sends the pending changes (after the request in flight). */
function flush(): void {
  if (inflight || !pendingFrom) return;
  const st = store.getState();
  const hasBlock = blockOf(st.scene) !== null || wroteBlock === project.main;
  // with a block, the change is relative to what the file has (the scene), else to the defaults
  const from = hasBlock ? (fileSettings ?? pendingFrom) : pendingFrom;
  const changes = blockChanges(from, st.studio, hasBlock);
  pendingFrom = null;
  if (!changes.length) return;
  if (!client) return;
  inflight = true;
  if (!hasBlock) wroteBlock = project.main;
  fileSettings = st.studio;
  client.post({ type: "SetRender", changes });
}

/** The settings follow the file's block when it changes (not while the panel is writing). */
function syncFromScene(): void {
  const st = store.getState();
  const b = blockOf(st.scene);
  const key = blockKey(b);
  if (pendingFrom || inflight || timer) return;
  if (!b) {
    appliedKey = null;
    fileSettings = null;
    if (st.scene && wroteBlock === project.main && !inflight) wroteBlock = null;
    return;
  }
  if (key === appliedKey && appliedFile === project.main) return;
  const opened = appliedFile !== project.main;
  appliedKey = key;
  appliedFile = project.main;
  const on = st.studio.on || (opened && b.style !== "standard");
  const s = sanitizeStudio(settingsFromBlock(b, on));
  fileSettings = s;
  // the file's look, not kept in localStorage (that holds the user's own settings)
  if (JSON.stringify(s) !== JSON.stringify(st.studio)) store.setState({ studio: s });
}

/** Connects the write-back to the engine (after the drag controller, which applies `Edits`). */
export function initRenderBlock(c: EngineClient): void {
  client = c;
  // `Edits` answer the request in flight (the drag controller, listening first, has applied
  // them and sent the new source); then the next changes may go.
  c.onEdits(() => {
    if (!inflight) return;
    inflight = false;
    if (pendingFrom) queueMicrotask(flush);
  });
  store.subscribe((s, p) => {
    if (s.scene !== p.scene) syncFromScene();
  });
}
