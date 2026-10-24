// Tool bars: groups (2 px apart) separated by a line and 12 px (docs/konzepte/ui-ueberarbeitung,
// "Gruppen"). `Toolbar` is a WAI-ARIA toolbar with a name; floating bars in the 3D view get the
// `float` look (panel background, border, shadow).

import type { HTMLAttributes, ReactNode } from "react";

export function Toolbar({ label, children, float, className, ...rest }: { label: string; children: ReactNode; float?: boolean } & HTMLAttributes<HTMLDivElement>) {
  return (
    <div role="toolbar" aria-label={label} className={`ui-toolbar${float ? " ui-toolbar--float" : ""}${className ? ` ${className}` : ""}`} {...rest}>
      {children}
    </div>
  );
}

export function ToolGroup({ children, label, className }: { children: ReactNode; label?: string; className?: string }) {
  return (
    <div className={`ui-group${className ? ` ${className}` : ""}`} role={label ? "group" : undefined} aria-label={label}>
      {children}
    </div>
  );
}

export function ToolSep() {
  return <span className="ui-sep" role="separator" aria-orientation="vertical" />;
}

/** Short label inside a bar ("z =", "sketch · Zeile 5"). */
export function ToolLabel({ children, title, className, ...rest }: { children: ReactNode; title?: string; className?: string } & HTMLAttributes<HTMLSpanElement>) {
  return (
    <span className={`ui-label${className ? ` ${className}` : ""}`} title={title} {...rest}>
      {children}
    </span>
  );
}
