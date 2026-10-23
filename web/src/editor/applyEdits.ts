// Engine text edits → editor changes. Edits are valid for the source version they were
// computed for; positions are mapped into the current document through the edits made since
// (VersionMapper). Edits of an unknown (pruned) version are dropped.

import type { ChangeSpec } from "@codemirror/state";
import type { Edit } from "../engine/protocol";
import type { VersionMapper } from "../logic/positions";

/**
 * Changes for a document that has not changed since the edits' version (a project file that
 * is not in the editor): positions as they are, clamped.
 */
export function mapEditsPlain(edits: Edit[], docLength: number): ChangeSpec[] | null {
  const out: Array<{ from: number; to: number; insert: string }> = [];
  for (const e of [...edits].sort((a, b) => a.from - b.from)) {
    const from = Math.min(e.from, docLength);
    const to = Math.max(from, Math.min(e.to, docLength));
    if (out.length && from < out[out.length - 1].to) return null;
    out.push({ from, to, insert: e.text });
  }
  return out;
}

/** Changes for the current document, or `null` if the version is unknown. */

export function mapEdits(mapper: VersionMapper, version: number, edits: Edit[], docLength: number): ChangeSpec[] | null {
  if (!mapper.has(version)) return null;
  const m = mapper.mapper(version);
  const out: Array<{ from: number; to: number; insert: string }> = [];
  for (const e of [...edits].sort((a, b) => a.from - b.from)) {
    const from = Math.min(m(e.from, -1), docLength);
    const to = Math.max(from, Math.min(m(e.to, 1), docLength));
    // Overlapping after mapping: the document changed under the edit; drop it.
    if (out.length && from < out[out.length - 1].to) return null;
    out.push({ from, to, insert: e.text });
  }
  return out;
}
