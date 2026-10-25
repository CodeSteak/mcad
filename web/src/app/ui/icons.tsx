// Symbols of the UI: simple stroke icons, 15 px. Most come from Lucide (lucide-react, ISC
// licence, https://lucide.dev); the few CAD-specific ones (line with end points, arc,
// "perpendicular to the plane") are drawn here in the same style (24 × 24 grid, stroke 2,
// round caps), so they sit next to the Lucide ones without standing out.

import type { SVGProps } from "react";

export {
  Angle as IconAngle,
  BetweenVerticalStart as IconWall,
  Box as IconModel,
  Cuboid as IconBody,
  MoveHorizontal as IconDistance,
  Radius as IconRadius,
  Ruler as IconLength,
  SquareDashed as IconArea,
  Check as IconCheck,
  ChevronDown as IconChevronDown,
  ChevronRight as IconChevronRight,
  Copy as IconCopy,
  Download as IconDownload,
  Ellipsis as IconMore,
  Eye as IconEye,
  FlipVertical2 as IconFlip,
  Grid3x3 as IconGrid,
  Monitor as IconSystem,
  Moon as IconMoon,
  MousePointer2 as IconSelect,
  PanelRightClose as IconPanelClose,
  PanelRightOpen as IconPanelOpen,
  PencilLine as IconSketch,
  Plus as IconPlus,
  RotateCcw as IconReset,
  RulerDimensionLine as IconMeasure,
  Save as IconSave,
  Scan as IconFit,
  ScissorsLineDashed as IconSection,
  SlidersHorizontal as IconSettings,
  Spline as IconBezier,
  Square as IconRect,
  SquareRoundCorner as IconCorner,
  Star as IconStar,
  Sun as IconSun,
  Circle as IconCircle,
  Trash2 as IconDelete,
  X as IconClose,
  Lasso as IconCloseContour,
  ShieldBan as IconKeepout,
  TriangleAlert as IconAlert,
  type LucideIcon,
} from "lucide-react";

/** Props shared by the own icons (subset of the Lucide props). */
export interface OwnIconProps extends SVGProps<SVGSVGElement> {
  size?: number;
  strokeWidth?: number;
}

function own(paths: React.ReactNode) {
  return function Icon({ size = 24, strokeWidth = 2, ...rest }: OwnIconProps) {
    return (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
        {...rest}
      >
        {paths}
      </svg>
    );
  };
}

/** Line tool: a segment with its two end points. */
export const IconLine = own(
  <>
    <path d="M6.5 17.5 17.5 6.5" />
    <circle cx="5" cy="19" r="2" />
    <circle cx="19" cy="5" r="2" />
  </>,
);

/** Arc tools: a circular arc with its end points. */
export const IconArc = own(
  <>
    <path d="M5 17A12 12 0 0 1 17 5" />
    <circle cx="5" cy="19" r="2" />
    <circle cx="19" cy="5" r="2" />
  </>,
);

/** "⊥ Ansicht": look perpendicular onto the sketch plane. */
export const IconPerpendicular = own(
  <>
    <path d="M12 4v14" />
    <path d="M4 18h16" />
    <path d="M12 14h4v4" />
  </>,
);
