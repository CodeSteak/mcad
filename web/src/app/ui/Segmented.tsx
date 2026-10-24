// Segmented control for an exclusive choice (mode, section axis, measure kind): a radio group
// with roving focus (arrow keys move and choose, Home/End), names as `aria-label`.

import { useRef, type KeyboardEvent } from "react";
import { ariaShortcut, Icon, type IconType, tooltip } from "./Button";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  icon?: IconType;
  /** Tooltip text after the name. */
  hint?: string;
  shortcut?: string;
  disabled?: boolean;
  /** Text shown in the segment (default: `label`; `null`: symbol only). */
  text?: string | null;
}

export interface SegmentedProps<T extends string> {
  /** Name of the group. */
  label: string;
  options: Array<SegmentOption<T>>;
  /** Chosen value (`null`: none). */
  value: T | null;
  onChange: (v: T) => void;
  className?: string;
  "data-testid"?: string;
}

export function Segmented<T extends string>({ label, options, value, onChange, className, ...rest }: SegmentedProps<T>) {
  const ref = useRef<HTMLDivElement>(null);
  const enabled = options.filter((o) => !o.disabled);
  const focusIndex = Math.max(0, enabled.findIndex((o) => o.value === value));
  const onKey = (e: KeyboardEvent) => {
    const keys: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };
    if (!(e.key in keys) && e.key !== "Home" && e.key !== "End") return;
    e.preventDefault();
    const cur = enabled.findIndex((o) => o.value === (document.activeElement as HTMLElement | null)?.dataset.value);
    const i = e.key === "Home" ? 0 : e.key === "End" ? enabled.length - 1 : (Math.max(0, cur) + keys[e.key] + enabled.length) % enabled.length;
    const o = enabled[i];
    if (!o) return;
    onChange(o.value);
    ref.current?.querySelector<HTMLButtonElement>(`[data-value="${o.value}"]`)?.focus();
  };
  return (
    <div ref={ref} className={`ui-seg${className ? ` ${className}` : ""}`} role="radiogroup" aria-label={label} onKeyDown={onKey} {...rest}>
      {options.map((o) => {
        const on = o.value === value;
        const text = o.text === undefined ? o.label : o.text;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={o.label}
            aria-keyshortcuts={ariaShortcut(o.shortcut)}
            data-value={o.value}
            className={`ui-seg-item${on ? " is-on" : ""}${text ? "" : " ui-seg-item--icon"}`}
            disabled={o.disabled}
            tabIndex={enabled[focusIndex]?.value === o.value ? 0 : -1}
            title={tooltip(o.label, o.shortcut, o.hint)}
            onClick={() => onChange(o.value)}
          >
            {o.icon && <Icon icon={o.icon} />}
            {text && <span className="ui-seg-text">{text}</span>}
          </button>
        );
      })}
    </div>
  );
}
