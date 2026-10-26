// Buttons of the UI (docs/konzepte/ui-ueberarbeitung): one height (26 px, `--ctl-h`),
// radius 6 px, four kinds:
//   primary  – filled with the accent, at most one per area ("Speichern", "Fertig")
//   standard – with a border
//   quiet    – without a border, for tool bars
//   toggle   – any kind with `pressed`: active = accent background
// Names: `aria-label` is the plain name (what tests and screen readers use), the tooltip
// (`title`) adds the shortcut, `aria-keyshortcuts` carries it for assistive technology.

import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { shortcutText } from "../../i18n";
import type { LucideIcon } from "./icons";

export type ButtonVariant = "primary" | "standard" | "quiet";

/** Icon component (Lucide or own) rendered at the UI size. */
export type IconType = LucideIcon | ((p: { size?: number; strokeWidth?: number; className?: string }) => ReactNode);

export function Icon({ icon: I, className }: { icon: IconType; className?: string }) {
  return <I size={15} strokeWidth={1.75} className={`ui-icon${className ? ` ${className}` : ""}`} aria-hidden="true" />;
}

/** "Strg+E" → tooltip text (shortcut in the UI language: "Ctrl+E"); `aria-keyshortcuts` wants "Control+E". */
export function tooltip(name: string, shortcut?: string, extra?: string): string {
  return `${name}${shortcut ? ` (${shortcutText(shortcut)})` : ""}${extra ? ` – ${extra}` : ""}`;
}

export function ariaShortcut(shortcut?: string): string | undefined {
  if (!shortcut) return undefined;
  return shortcut
    .split(" / ")[0]
    .replace(/Strg/g, "Control")
    .replace(/Umschalt|⇧/g, "Shift")
    .replace(/Entf/g, "Delete")
    .replace(/\s*\+\s*/g, "+");
}

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "title"> {
  variant?: ButtonVariant;
  icon?: IconType;
  /** Toggle state (`aria-pressed`). */
  pressed?: boolean;
  /** Shortcut shown in the tooltip, in the German notation ("Strg+S"; shown as "Ctrl+S" in English). */
  shortcut?: string;
  /** Tooltip; defaults to the text of the button with the shortcut. */
  title?: string;
  /** Additional explanation in the tooltip. */
  hint?: string;
  /** Show a caret (the button opens a menu). */
  caret?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "standard", icon, pressed, shortcut, title, hint, caret, className, children, type, ...rest },
  ref,
) {
  const name = rest["aria-label"] ?? (typeof children === "string" ? children : undefined);
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={`ui-btn ui-btn--${variant}${pressed ? " is-on" : ""}${className ? ` ${className}` : ""}`}
      aria-pressed={pressed}
      aria-keyshortcuts={ariaShortcut(shortcut)}
      title={title ?? (name ? tooltip(name, shortcut, hint) : undefined)}
      // the name stays the plain text (also for CSS/WebDriver selectors: [aria-label=…])
      aria-label={name}
      {...rest}
    >
      {icon && <Icon icon={icon} />}
      {children !== undefined && children !== null && children !== false && <span className="ui-btn-text">{children}</span>}
      {caret && <CaretIcon />}
    </button>
  );
});

export function CaretIcon() {
  return (
    <svg className="ui-caret" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <path d="M2.5 4 5 6.5 7.5 4" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export interface IconButtonProps extends Omit<ButtonProps, "children" | "icon" | "aria-label"> {
  icon: IconType;
  /** Accessible name and first part of the tooltip. */
  label: string;
}

/** Square symbol button (26 × 26) with tooltip; quiet unless stated otherwise. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton({ icon, label, variant = "quiet", shortcut, hint, title, className, ...rest }, ref) {
  return (
    <Button
      ref={ref}
      variant={variant}
      icon={icon}
      aria-label={label}
      shortcut={shortcut}
      title={title ?? tooltip(label, shortcut, hint)}
      className={`ui-btn--icon${className ? ` ${className}` : ""}`}
      {...rest}
    />
  );
});
