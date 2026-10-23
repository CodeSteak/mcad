import { describe, expect, it } from "vitest";
import { history, redo, undo, undoDepth } from "@codemirror/commands";
import { EditorState, type Transaction } from "@codemirror/state";
import { EditGroup, type EditTarget } from "./editGroup";
import { mapEdits } from "./applyEdits";
import { VersionMapper } from "../logic/positions";
import { literalAt } from "./literalControls";
import { Text } from "@codemirror/state";

/** Minimal editor: state + dispatch (what the undo commands and EditGroup need). */
function editor(doc: string) {
  const ed: EditTarget & { state: EditorState } = {
    state: EditorState.create({ doc, extensions: [history()] }),
    dispatch(tr: Transaction) {
      ed.state = tr.state;
    },
  };
  return ed;
}

const type = (ed: ReturnType<typeof editor>, at: number, text: string) => ed.dispatch(ed.state.update({ changes: { from: at, insert: text }, userEvent: "input.type" }));

describe("Ein Ziehvorgang ist ein Undo-Schritt", () => {
  it("viele Live-Änderungen → ein Undo stellt den Ausgangstext her", () => {
    const ed = editor("cube([20, 20, 20]);\n");
    const g = new EditGroup(() => ed);
    g.begin();
    for (const v of ["21", "23", "27", "26"]) {
      const from = 6;
      const to = ed.state.doc.toString().indexOf(",");
      g.apply({ from, to, insert: v });
    }
    expect(ed.state.doc.toString()).toBe("cube([26, 20, 20]);\n");
    // Live edits are not in the history yet.
    expect(undoDepth(ed.state)).toBe(0);
    g.end();
    expect(ed.state.doc.toString()).toBe("cube([26, 20, 20]);\n");
    expect(undoDepth(ed.state)).toBe(1);
    expect(undo(ed)).toBe(true);
    expect(ed.state.doc.toString()).toBe("cube([20, 20, 20]);\n");
    expect(redo(ed)).toBe(true);
    expect(ed.state.doc.toString()).toBe("cube([26, 20, 20]);\n");
  });

  it("früheres Tippen bleibt ein eigener Schritt", () => {
    const ed = editor("a = 1;\n");
    type(ed, 6, "\nb = 2;");
    const g = new EditGroup(() => ed);
    g.begin();
    g.apply({ from: 4, to: 5, insert: "5" });
    g.apply({ from: 4, to: 5, insert: "7" });
    g.end();
    expect(ed.state.doc.toString()).toBe("a = 7;\nb = 2;\n");
    undo(ed);
    expect(ed.state.doc.toString()).toBe("a = 1;\nb = 2;\n");
    undo(ed);
    expect(ed.state.doc.toString()).toBe("a = 1;\n");
  });

  it("Tippen direkt danach wird nicht angehängt", () => {
    const ed = editor("x = 1;");
    const g = new EditGroup(() => ed);
    g.begin();
    g.apply({ from: 4, to: 5, insert: "9" });
    g.end();
    type(ed, 6, " ");
    undo(ed);
    expect(ed.state.doc.toString()).toBe("x = 9;");
    undo(ed);
    expect(ed.state.doc.toString()).toBe("x = 1;");
  });

  it("ohne Änderung kein Undo-Schritt, Abbrechen stellt her", () => {
    const ed = editor("x = 1;");
    const g = new EditGroup(() => ed);
    g.begin();
    g.end();
    expect(undoDepth(ed.state)).toBe(0);
    g.begin();
    g.apply({ from: 4, to: 5, insert: "3" });
    g.cancel();
    expect(ed.state.doc.toString()).toBe("x = 1;");
    expect(undoDepth(ed.state)).toBe(0);
  });
});

describe("Engine-Edits auf das aktuelle Dokument abbilden", () => {
  it("über Änderungen seit der Version", () => {
    const m = new VersionMapper(17);
    m.mark(1, 17);
    const ed = editor("translate([5, 5, ");
    expect(ed.state.doc.length).toBe(17);
    // 2 characters inserted at the start since version 1.
    const cs = ed.state.update({ changes: { from: 0, insert: "  " } }).changes;
    m.apply(cs);
    const mapped = mapEdits(m, 1, [{ from: 11, to: 12, text: "9" }], 19);
    expect(mapped).toEqual([{ from: 13, to: 14, insert: "9" }]);
    expect(mapEdits(m, 7, [{ from: 0, to: 1, text: "a" }], 19)).toBeNull();
  });

  it("überlappende Edits werden verworfen", () => {
    const m = new VersionMapper(10);
    m.mark(3, 10);
    expect(mapEdits(m, 3, [{ from: 0, to: 4, text: "a" }, { from: 2, to: 6, text: "b" }], 10)).toBeNull();
    expect(mapEdits(m, 3, [{ from: 5, to: 6, text: "b" }, { from: 0, to: 1, text: "a" }], 10)).toEqual([
      { from: 0, to: 1, insert: "a" },
      { from: 5, to: 6, insert: "b" },
    ]);
  });
});

describe("Inline-Regler: Literal noch an der Stelle?", () => {
  const doc = Text.of(["t = 3;", "hohl = true;", "x = -2.5;"]);
  it("prüft Zahl und Bool", () => {
    expect(literalAt(doc, { from: 4, to: 5, kind: "number", value: 3 })).toBe(true);
    expect(literalAt(doc, { from: 3, to: 5, kind: "number", value: 3 })).toBe(true);
    expect(literalAt(doc, { from: 0, to: 1, kind: "number", value: 3 })).toBe(false);
    expect(literalAt(doc, { from: 14, to: 18, kind: "bool", value: true })).toBe(true);
    expect(literalAt(doc, { from: 24, to: 28, kind: "number", value: -2.5 })).toBe(true);
    expect(literalAt(doc, { from: 5, to: 5, kind: "number", value: 3 })).toBe(false);
  });
});
