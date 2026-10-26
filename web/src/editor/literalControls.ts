// Inline control for literal values of assignments (Spec: Werttypen): a bool literal gets a
// switch at the end of its line that changes the literal in the code. Number literals have no
// inline slider (it got in the way).

import type { Text } from "@codemirror/state";
import { type EditorView, WidgetType } from "@codemirror/view";
import { t } from "../i18n";

export interface LiteralControl {
  /** Literal range in the current document. */
  from: number;
  to: number;
  kind: "bool";
  value: boolean;
}

/** Whether the text at the range still is the literal (positions may be stale). */
export function literalAt(doc: Text, c: LiteralControl): boolean {
  if (c.from < 0 || c.to > doc.length || c.from >= c.to) return false;
  const t = doc.sliceString(c.from, c.to).trim();
  return t === "true" || t === "false";
}

interface DomState {
  c: LiteralControl;
}

type ControlDom = HTMLElement & { __lit?: DomState };

export class LiteralControlWidget extends WidgetType {
  constructor(readonly c: LiteralControl) {
    super();
  }

  eq(o: LiteralControlWidget): boolean {
    return o.c.kind === this.c.kind && o.c.from === this.c.from && o.c.to === this.c.to && o.c.value === this.c.value;
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("span") as ControlDom;
    wrap.className = `cm-litctl cm-litctl-${this.c.kind}`;
    wrap.__lit = { c: this.c };
    const label = document.createElement("label");
    label.className = "cm-switch";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = this.c.value === true;
    input.setAttribute("aria-label", t("editor.toggle"));
    input.setAttribute("data-testid", "inline-switch");
    input.addEventListener("change", () => {
      const st = wrap.__lit!;
      if (!literalAt(view.state.doc, st.c)) return;
      const text = input.checked ? "true" : "false";
      view.dispatch({ changes: { from: st.c.from, to: st.c.to, insert: text }, userEvent: "input.switch" });
    });
    const knob = document.createElement("span");
    knob.className = "cm-switch-knob";
    label.append(input, knob);
    wrap.appendChild(label);
    return wrap;
  }

  updateDOM(dom: HTMLElement): boolean {
    const d = dom as ControlDom;
    const st = d.__lit;
    if (!st || !dom.classList.contains(`cm-litctl-${this.c.kind}`)) return false;
    st.c = this.c;
    const input = dom.querySelector("input");
    if (!input) return false;
    input.checked = this.c.value;
    return true;
  }

  ignoreEvent(): boolean {
    return true;
  }
}
