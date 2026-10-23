// Inline controls for literal values of assignments (Spec: Werttypen): a number literal gets
// a small slider, a bool literal a switch, at the end of its line. They change the literal in
// the code; one slider drag is one undo step.

import type { Text } from "@codemirror/state";
import { type EditorView, WidgetType } from "@codemirror/view";
import { fmtNum } from "../logic/format";
import { sliderRange } from "../logic/params";
import { EditGroup } from "./editGroup";

export interface LiteralControl {
  /** Literal range in the current document. */
  from: number;
  to: number;
  kind: "number" | "bool";
  value: number | boolean;
}

/** Whether the text at the range still is the literal (positions may be stale). */
export function literalAt(doc: Text, c: LiteralControl): boolean {
  if (c.from < 0 || c.to > doc.length || c.from >= c.to) return false;
  const t = doc.sliceString(c.from, c.to).trim();
  if (c.kind === "bool") return t === "true" || t === "false";
  return /^-?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/.test(t);
}

interface DomState {
  c: LiteralControl;
  dragging: boolean;
  group: EditGroup | null;
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
    wrap.__lit = { c: this.c, dragging: false, group: null };
    if (this.c.kind === "bool") {
      const label = document.createElement("label");
      label.className = "cm-switch";
      const input = document.createElement("input");
      input.type = "checkbox";
      input.checked = this.c.value === true;
      input.setAttribute("aria-label", "Wert umschalten");
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
    } else {
      const input = document.createElement("input");
      input.type = "range";
      input.setAttribute("aria-label", "Wert ziehen");
      input.setAttribute("data-testid", "inline-slider");
      this.setRange(input, this.c.value as number);
      const begin = () => {
        const st = wrap.__lit!;
        if (st.dragging) return;
        st.dragging = true;
        st.group = new EditGroup(() => view, "input.slider");
        st.group.begin();
      };
      const end = () => {
        const st = wrap.__lit!;
        if (!st.dragging) return;
        st.dragging = false;
        st.group?.end();
        st.group = null;
      };
      input.addEventListener("pointerdown", begin);
      input.addEventListener("keydown", begin);
      input.addEventListener("input", () => {
        const st = wrap.__lit!;
        if (!literalAt(view.state.doc, st.c)) return;
        if (!st.dragging) begin();
        const text = fmtNum(Number(input.value));
        const tr = st.group!.apply({ from: st.c.from, to: st.c.to, insert: text });
        if (tr) st.c = { ...st.c, to: st.c.from + text.length, value: Number(input.value) };
      });
      input.addEventListener("change", end);
      input.addEventListener("pointerup", end);
      input.addEventListener("keyup", end);
      input.addEventListener("blur", end);
      wrap.appendChild(input);
    }
    return wrap;
  }

  private setRange(input: HTMLInputElement, v: number): void {
    const r = sliderRange(v);
    input.min = String(Math.min(r.min, v));
    input.max = String(Math.max(r.max, v));
    input.step = String(r.step);
    input.value = String(v);
  }

  updateDOM(dom: HTMLElement): boolean {
    const d = dom as ControlDom;
    const st = d.__lit;
    if (!st || !dom.classList.contains(`cm-litctl-${this.c.kind}`)) return false;
    if (st.dragging) return true;
    st.c = this.c;
    const input = dom.querySelector("input");
    if (!input) return false;
    if (this.c.kind === "bool") input.checked = this.c.value === true;
    else this.setRange(input, this.c.value as number);
    return true;
  }

  ignoreEvent(): boolean {
    return true;
  }
}
