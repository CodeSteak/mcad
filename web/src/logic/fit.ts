// When the view should be refitted after the model changed (UI-Modell: the camera follows a
// clearly larger or smaller model as long as the user has not moved it).

import type { Vec3 } from "../engine/protocol";
import { type Box3, boxDiagonal, isEmpty } from "./math";

/** Size ratio from which a change counts as clear. */
export const REFIT_RATIO = 1.6;

function center(b: Box3): Vec3 {
  return [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2];
}

/**
 * The model box changed clearly against the box of the last fit: its diagonal grew or shrank
 * by `REFIT_RATIO`, or its centre moved by more than half the fitted diagonal.
 */
export function needsRefit(fitted: Box3, now: Box3): boolean {
  if (isEmpty(fitted) || isEmpty(now)) return false;
  const a = Math.max(boxDiagonal(fitted), 1e-6);
  const b = Math.max(boxDiagonal(now), 1e-6);
  if (b / a >= REFIT_RATIO || a / b >= REFIT_RATIO) return true;
  const c0 = center(fitted);
  const c1 = center(now);
  const d = Math.hypot(c1[0] - c0[0], c1[1] - c0[1], c1[2] - c0[2]);
  return d > 0.5 * Math.max(a, b);
}
