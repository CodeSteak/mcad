// Undo grouping for live edits (handle drags, inline sliders): one drag = one undo step
// (Spec: Griffe, Regel 6).
//
// While the drag runs, edits are applied without entering the history. At the end the
// accumulated change is reverted (outside the history) and re-applied as a single isolated
// history event. The document is the same before and after the end; undo then restores the
// text from before the drag in one step, independent of how long the drag took.

import { ChangeSet, type ChangeSpec, type EditorState, type Text, Transaction, type TransactionSpec } from "@codemirror/state";
import { isolateHistory } from "@codemirror/commands";

export interface EditTarget {
  readonly state: EditorState;
  dispatch(tr: Transaction): void;
}

export class EditGroup {
  private start: Text | null = null;
  private changes: ChangeSet | null = null;
  private count = 0;

  constructor(
    private target: () => EditTarget | null,
    private userEvent = "input.drag",
  ) {}

  get active(): boolean {
    return this.start !== null;
  }

  /** Number of live edits applied in this group. */
  get edits(): number {
    return this.count;
  }

  begin(): void {
    const t = this.target();
    if (!t) return;
    this.start = t.state.doc;
    this.changes = ChangeSet.empty(t.state.doc.length);
    this.count = 0;
  }

  /** Applies a live edit (not recorded in the history yet). Returns the transaction. */
  apply(changes: ChangeSpec, extra: Partial<TransactionSpec> = {}): Transaction | null {
    const t = this.target();
    if (!t) return null;
    if (!this.start) this.begin();
    const tr = t.state.update({
      ...extra,
      changes,
      annotations: [Transaction.addToHistory.of(false)],
      userEvent: `${this.userEvent}.live`,
    });
    if (tr.changes.empty) return null;
    t.dispatch(tr);
    this.changes = this.changes!.compose(tr.changes);
    this.count++;
    return tr;
  }

  /** Ends the group: one history event from the text before the drag to the current text. */
  end(): void {
    const t = this.target();
    const start = this.start;
    const changes = this.changes;
    this.start = null;
    this.changes = null;
    if (!t || !start || !changes || changes.empty) return;
    // Someone else changed the document meanwhile: keep it as it is.
    if (changes.length !== start.length || changes.newLength !== t.state.doc.length) return;
    const revert = t.state.update({
      changes: changes.invert(start),
      annotations: [Transaction.addToHistory.of(false)],
      userEvent: `${this.userEvent}.revert`,
    });
    t.dispatch(revert);
    const redo = t.state.update({
      changes,
      annotations: [isolateHistory.of("full")],
      userEvent: `${this.userEvent}.end`,
    });
    t.dispatch(redo);
  }

  /** Drops the group, reverting its live edits (Escape). */
  cancel(): void {
    const t = this.target();
    const start = this.start;
    const changes = this.changes;
    this.start = null;
    this.changes = null;
    if (!t || !start || !changes || changes.empty) return;
    if (changes.newLength !== t.state.doc.length) return;
    t.dispatch(t.state.update({ changes: changes.invert(start), annotations: [Transaction.addToHistory.of(false)], userEvent: `${this.userEvent}.cancel` }));
  }
}
