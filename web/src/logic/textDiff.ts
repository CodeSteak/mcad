// Line diff for the desktop app: reloading an externally changed file as small editor
// changes (cursor, scroll and the rest of the document stay), and the "Vergleichen" view.

export interface DiffLine {
  kind: "same" | "add" | "del";
  text: string;
  /** 1-based line in the old/new text. */
  a: number | null;
  b: number | null;
}

/** Lines with their start offsets (a final newline does not start another line). */
function lines(s: string): { text: string[]; starts: number[] } {
  const text: string[] = [];
  const starts: number[] = [];
  let at = 0;
  while (at <= s.length) {
    const nl = s.indexOf("\n", at);
    starts.push(at);
    if (nl < 0) {
      text.push(s.slice(at));
      break;
    }
    text.push(s.slice(at, nl + 1));
    at = nl + 1;
    if (at === s.length) break;
  }
  return { text, starts };
}

/** Edit script of lines: LCS on the part between common prefix and suffix. */
function script(a: string[], b: string[]): Array<{ kind: DiffLine["kind"]; ai: number; bi: number }> {
  let p = 0;
  while (p < a.length && p < b.length && a[p] === b[p]) p++;
  let s = 0;
  while (s < a.length - p && s < b.length - p && a[a.length - 1 - s] === b[b.length - 1 - s]) s++;
  const am = a.slice(p, a.length - s);
  const bm = b.slice(p, b.length - s);
  const out: Array<{ kind: DiffLine["kind"]; ai: number; bi: number }> = [];
  for (let i = 0; i < p; i++) out.push({ kind: "same", ai: i, bi: i });
  if (am.length * bm.length > 4_000_000) {
    // Too large for the table: the middle as one replacement.
    am.forEach((_, i) => out.push({ kind: "del", ai: p + i, bi: -1 }));
    bm.forEach((_, i) => out.push({ kind: "add", ai: -1, bi: p + i }));
  } else {
    const n = am.length;
    const m = bm.length;
    const t = new Uint32Array((n + 1) * (m + 1));
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        t[i * (m + 1) + j] = am[i] === bm[j] ? t[(i + 1) * (m + 1) + j + 1] + 1 : Math.max(t[(i + 1) * (m + 1) + j], t[i * (m + 1) + j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < n || j < m) {
      if (i < n && j < m && am[i] === bm[j]) {
        out.push({ kind: "same", ai: p + i, bi: p + j });
        i++;
        j++;
      } else if (i < n && (j >= m || t[(i + 1) * (m + 1) + j] >= t[i * (m + 1) + j + 1])) {
        // deletions before insertions (diff view: old line above the new one)
        out.push({ kind: "del", ai: p + i, bi: -1 });
        i++;
      } else {
        out.push({ kind: "add", ai: -1, bi: p + j });
        j++;
      }
    }
  }
  for (let k = 0; k < s; k++) out.push({ kind: "same", ai: a.length - s + k, bi: b.length - s + k });
  return out;
}

/** Changes (offsets into `oldText`) that turn `oldText` into `newText`, one per changed run. */
export function lineChanges(oldText: string, newText: string): Array<{ from: number; to: number; insert: string }> {
  if (oldText === newText) return [];
  const A = lines(oldText);
  const B = lines(newText);
  const ops = script(A.text, B.text);
  const changes: Array<{ from: number; to: number; insert: string }> = [];
  let k = 0;
  while (k < ops.length) {
    if (ops[k].kind === "same") {
      k++;
      continue;
    }
    // A run of deletions/insertions; its position in the old text is the next old line.
    let from = -1;
    let to = -1;
    let insert = "";
    const startK = k;
    while (k < ops.length && ops[k].kind !== "same") {
      const o = ops[k];
      if (o.kind === "del") {
        const s = A.starts[o.ai];
        if (from < 0) from = s;
        to = s + A.text[o.ai].length;
      } else insert += B.text[o.bi];
      k++;
    }
    if (from < 0) {
      // Pure insertion: before the next old line (or at the end).
      const next = ops.slice(k).find((o) => o.kind === "same");
      const prevSame = ops.slice(0, startK).reverse().find((o) => o.kind === "same");
      from = next ? A.starts[next.ai] : prevSame ? A.starts[prevSame.ai] + A.text[prevSame.ai].length : 0;
      to = from;
    }
    changes.push({ from, to, insert });
  }
  // Last lines without a final newline: fix up by comparing the result.
  let res = "";
  let at = 0;
  for (const c of changes) {
    res += oldText.slice(at, c.from) + c.insert;
    at = c.to;
  }
  res += oldText.slice(at);
  if (res !== newText) {
    let p = 0;
    while (p < oldText.length && p < newText.length && oldText[p] === newText[p]) p++;
    let s = 0;
    while (s < oldText.length - p && s < newText.length - p && oldText[oldText.length - 1 - s] === newText[newText.length - 1 - s]) s++;
    return [{ from: p, to: oldText.length - s, insert: newText.slice(p, newText.length - s) }];
  }
  return changes;
}

/** Line-by-line comparison (`a` old, `b` new) for the diff view. */
export function diffLines(a: string, b: string): DiffLine[] {
  const strip = (t: string) => t.replace(/\r?\n$/, "");
  const A = lines(a).text;
  const B = lines(b).text;
  return script(A, B).map((o) => ({
    kind: o.kind,
    text: strip(o.kind === "add" ? B[o.bi] : A[o.ai]),
    a: o.ai >= 0 ? o.ai + 1 : null,
    b: o.bi >= 0 ? o.bi + 1 : null,
  }));
}
