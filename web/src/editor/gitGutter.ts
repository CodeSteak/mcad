// Git line markers in the editor gutter (desktop app): new lines green, changed lines blue
// (a narrow, faint bar), deleted lines a small red triangle between the lines. The hunks come
// from the app (buffer against HEAD, crates/cad-desktop/src/git.rs); between two updates the
// markers move with the edits (RangeSet mapping). Hovering a marker shows the old lines.

import { type EditorState, RangeSet, RangeSetBuilder, StateEffect, StateField, type Text } from "@codemirror/state";
import { GutterMarker, gutter } from "@codemirror/view";
import { t } from "../i18n";
import type { Hunk } from "../platform/desktop";

class GitMarker extends GutterMarker {
  constructor(
    readonly kind: "added" | "modified" | "deleted" | "deleted-end",
    readonly title: string,
  ) {
    super();
  }
  eq(o: GitMarker): boolean {
    return o.kind === this.kind && o.title === this.title;
  }
  toDOM(): Node {
    const el = document.createElement("div");
    el.className = `cm-git-mark cm-git-${this.kind}`;
    if (this.title) el.title = this.title;
    return el;
  }
}

/** Sets the hunks of the shown document (`null`: no markers). */
export const setHunks = StateEffect.define<Hunk[] | null>();

function title(h: Hunk): string {
  const what = h.kind === "added" ? t("git.added") : h.kind === "modified" ? t("git.modified") : t("git.deleted");
  if (!h.old.length) return t("git.vsHead", { what });
  const shown = h.old.slice(0, 12).join("\n");
  return `${t("git.before", { what })}\n${shown}${h.old.length > 12 ? `\n${t("git.more", { n: h.old.length - 12 })}` : ""}`;
}

/** Marker set for hunks on document `doc`. */
export function markers(doc: Text, hunks: Hunk[]): RangeSet<GitMarker> {
  const b = new RangeSetBuilder<GitMarker>();
  const items: Array<{ pos: number; m: GitMarker }> = [];
  for (const h of hunks) {
    if (h.kind === "deleted") {
      if (h.start > doc.lines) items.push({ pos: doc.line(doc.lines).from, m: new GitMarker("deleted-end", title(h)) });
      else items.push({ pos: doc.line(Math.max(1, h.start)).from, m: new GitMarker("deleted", title(h)) });
      continue;
    }
    const t = title(h);
    for (let l = h.start; l <= Math.min(h.end, doc.lines); l++) items.push({ pos: doc.line(l).from, m: new GitMarker(h.kind, t) });
  }
  items.sort((x, y) => x.pos - y.pos);
  for (const it of items) b.add(it.pos, it.pos, it.m);
  return b.finish();
}

export const gitField = StateField.define<{ set: RangeSet<GitMarker>; hunks: Hunk[] | null }>({
  create: () => ({ set: RangeSet.empty, hunks: null }),
  update(v, tr) {
    for (const e of tr.effects) {
      if (e.is(setHunks)) return { set: e.value ? markers(tr.state.doc, e.value) : RangeSet.empty, hunks: e.value };
    }
    return tr.docChanged ? { set: v.set.map(tr.changes), hunks: v.hunks } : v;
  },
});

/** Hunks and markers as drawn (test hook). */
export function gitMarks(state: EditorState): { hunks: Hunk[] | null; lines: Array<{ line: number; kind: string }> } {
  const f = state.field(gitField, false);
  if (!f) return { hunks: null, lines: [] };
  const out: Array<{ line: number; kind: string }> = [];
  f.set.between(0, state.doc.length, (from, _to, m) => {
    out.push({ line: state.doc.lineAt(from).number, kind: m.kind });
  });
  return { hunks: f.hunks, lines: out };
}

export function gitGutter() {
  return [
    gitField,
    gutter({
      class: "cm-git-gutter",
      markers: (v) => v.state.field(gitField).set,
    }),
  ];
}
