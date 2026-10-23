// Source positions of older scenes (Plan 7.4): every scene and diagnostic list carries the
// version of the source it was computed from. Until a newer one arrives, the UI maps its
// positions through the editor changes made since that version (`ChangeSet.mapPos`), so
// highlights do not jump while typing.

import { ChangeSet } from "@codemirror/state";

export type MapPos = (pos: number, assoc?: -1 | 1) => number;

export const identityMap: MapPos = (pos) => pos;

export class VersionMapper {
  /** version → changes from that version's document to the current document. */
  private entries = new Map<number, ChangeSet>();
  private len: number;

  constructor(docLength: number) {
    this.len = docLength;
  }

  /** The current document is `version` (called when the version is sent to the engine). */
  mark(version: number, docLength = this.len): void {
    this.len = docLength;
    this.entries.set(version, ChangeSet.empty(docLength));
  }

  /** The editor applied `changes` to the current document. */
  apply(changes: ChangeSet): void {
    for (const [v, c] of this.entries) this.entries.set(v, c.compose(changes));
    this.len = changes.newLength;
  }

  has(version: number): boolean {
    return this.entries.has(version);
  }

  /** Changes since `version` (`null` if the version is unknown). */
  changesSince(version: number): ChangeSet | null {
    return this.entries.get(version) ?? null;
  }

  /** Maps a position of the document at `version` into the current document. */
  map(version: number, pos: number, assoc: -1 | 1 = 1): number {
    const c = this.entries.get(version);
    if (!c) return Math.min(pos, this.len);
    return c.mapPos(Math.min(pos, c.length), assoc);
  }

  /** Mapping function for one version. */
  mapper(version: number): MapPos {
    const c = this.entries.get(version);
    const len = this.len;
    if (!c) return (pos) => Math.min(pos, len);
    if (c.empty) return (pos) => Math.min(pos, len);
    return (pos, assoc = 1) => c.mapPos(Math.min(pos, c.length), assoc);
  }

  /** Drops versions older than `version`. */
  prune(version: number): void {
    for (const v of [...this.entries.keys()]) if (v < version) this.entries.delete(v);
  }

  versions(): number[] {
    return [...this.entries.keys()].sort((a, b) => a - b);
  }
}

/** Maps a range; an empty result keeps its start. */
export function mapRange(map: MapPos, from: number, to: number): { from: number; to: number } {
  const f = map(from, 1);
  const t = Math.max(f, map(to, -1));
  return { from: f, to: t };
}
