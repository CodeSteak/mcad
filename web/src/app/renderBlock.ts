// Studio settings ↔ the file's `render { … }` block.

import type { StudioSettings } from "../logic/studio";
import { setStudio } from "../state/store";

/** Changes the studio settings from the panel. */
export function editStudio(patch: Partial<StudioSettings>): void {
  setStudio(patch);
}

/** Line of the main file's render block (`null`: the file has none). */
export function renderBlockLine(): number | null {
  return null;
}
