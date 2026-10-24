// Menus (docs/konzepte/ui-ueberarbeitung: "Selten Genutztes in Menüs"): a button that opens a
// menu (`MenuButton`), a split button (main action + menu, `SplitButton`) and a context menu
// (`ContextMenu`). The popups render into <body> with fixed position (never cut off by a
// scrolling bar), follow the WAI-ARIA menu pattern (roles menu/menuitem…, arrow keys, Home/End,
// Enter/Space, Escape returns the focus to the button, submenus with →/←) and close on a click
// outside.

import { forwardRef, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ariaShortcut, Button, type ButtonVariant, CaretIcon, Icon, IconButton, type IconType, tooltip } from "./Button";
import { IconCheck, IconChevronDown, IconChevronRight } from "./icons";

export type MenuEntry =
  | {
      kind?: "item";
      label: string;
      onSelect: () => void;
      shortcut?: string;
      disabled?: boolean;
      icon?: IconType;
      /** Checkable item: `radio` (one of several) or `checkbox`. */
      check?: "radio" | "checkbox";
      checked?: boolean;
      /** Tooltip. */
      hint?: string;
      /** Shown next to the name (e.g. the folder of a recent file). */
      detail?: string;
    }
  | { kind: "sep" }
  | { kind: "heading"; label: string }
  | { kind: "sub"; label: string; items: MenuEntry[]; disabled?: boolean; icon?: IconType; hint?: string };

type Item = Exclude<MenuEntry, { kind: "sep" } | { kind: "heading" }>;

const isItem = (e: MenuEntry): e is Item => e.kind === undefined || e.kind === "item" || e.kind === "sub";

/** Where the popup goes relative to its anchor. */
type Placement = "below-start" | "below-end" | "right" | "point";

interface Anchor {
  rect: { left: number; top: number; right: number; bottom: number };
  placement: Placement;
}

/** All open popups (for "click outside"). */
const openPopups = new Set<HTMLElement>();

function MenuPopup({
  items,
  label,
  anchor,
  onClose,
  focusFirst,
  level = 0,
}: {
  items: MenuEntry[];
  label: string;
  anchor: Anchor;
  /** `escape`/`left`: this level only (focus back to the opener); `done`: everything. */
  onClose: (reason: "escape" | "left" | "done" | "outside" | "tab") => void;
  focusFirst: "first" | "last" | "none";
  level?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number; maxHeight: number } | null>(null);
  const [sub, setSub] = useState<{ index: number; focus: "first" | "none" } | null>(null);
  const itemRefs = useRef<Array<HTMLElement | null>>([]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const r = anchor.rect;
    let left: number;
    let top: number;
    if (anchor.placement === "right") {
      left = r.right + 2;
      if (left + w > vw - 4) left = r.left - w - 2;
      top = r.top - 5;
    } else if (anchor.placement === "point") {
      left = r.left;
      top = r.top;
      if (left + w > vw - 4) left = r.left - w;
    } else {
      left = anchor.placement === "below-end" ? r.right - w : r.left;
      top = r.bottom + 4;
      if (top + h > vh - 4 && r.top - h - 4 > 4) top = r.top - h - 4;
    }
    left = Math.max(4, Math.min(left, vw - w - 4));
    const maxHeight = vh - 8;
    if (top + Math.min(h, maxHeight) > vh - 4) top = Math.max(4, vh - 4 - Math.min(h, maxHeight));
    setPos({ left, top, maxHeight });
  }, [anchor.rect.left, anchor.rect.top, anchor.rect.right, anchor.rect.bottom, anchor.placement, items.length]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    openPopups.add(el);
    return () => {
      openPopups.delete(el);
    };
  }, []);

  const enabled = items.map((e, i) => (isItem(e) && !e.disabled ? i : -1)).filter((i) => i >= 0);
  useEffect(() => {
    if (!pos || focusFirst === "none") return;
    const i = focusFirst === "first" ? enabled[0] : enabled[enabled.length - 1];
    if (i !== undefined) itemRefs.current[i]?.focus();
    else ref.current?.focus();
    // only when the popup appears
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pos !== null]);

  const focusAt = (i: number) => itemRefs.current[i]?.focus();
  const current = () => itemRefs.current.findIndex((x) => x === document.activeElement);

  const activate = (i: number, how: "key" | "mouse") => {
    const e = items[i];
    if (!e || !isItem(e) || e.disabled) return;
    if (e.kind === "sub") {
      setSub({ index: i, focus: how === "key" ? "first" : "none" });
      return;
    }
    onClose("done");
    e.onSelect();
  };

  const onKey = (ev: React.KeyboardEvent) => {
    // keys of a submenu are handled there
    if (ev.target instanceof Node && !ref.current?.contains(ev.target)) return;
    const cur = current();
    const pos = enabled.indexOf(cur);
    switch (ev.key) {
      case "ArrowDown":
        ev.preventDefault();
        focusAt(enabled[(pos + 1 + enabled.length) % enabled.length]);
        break;
      case "ArrowUp":
        ev.preventDefault();
        focusAt(enabled[(pos - 1 + enabled.length * 2) % enabled.length]);
        break;
      case "Home":
        ev.preventDefault();
        focusAt(enabled[0]);
        break;
      case "End":
        ev.preventDefault();
        focusAt(enabled[enabled.length - 1]);
        break;
      case "ArrowRight": {
        const e = items[cur];
        if (e && e.kind === "sub" && !e.disabled) {
          ev.preventDefault();
          setSub({ index: cur, focus: "first" });
        }
        break;
      }
      case "ArrowLeft":
        if (level > 0) {
          ev.preventDefault();
          ev.stopPropagation();
          onClose("left");
        }
        break;
      case "Enter":
      case " ":
        ev.preventDefault();
        ev.stopPropagation();
        if (cur >= 0) activate(cur, "key");
        break;
      case "Escape":
        ev.preventDefault();
        ev.stopPropagation();
        onClose("escape");
        break;
      case "Tab":
        ev.preventDefault();
        onClose("tab");
        break;
    }
  };

  const subEntry = sub !== null ? items[sub.index] : null;
  const subRect = sub !== null ? itemRefs.current[sub.index]?.getBoundingClientRect() : null;

  return createPortal(
    <div
      ref={ref}
      className="ui-menu"
      role="menu"
      aria-label={label}
      tabIndex={-1}
      onKeyDown={onKey}
      style={pos ? { left: pos.left, top: pos.top, maxHeight: pos.maxHeight } : { left: -9999, top: 0, visibility: "hidden" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((e, i) => {
        if (e.kind === "sep") return <div key={i} className="ui-menu-sep" role="separator" />;
        if (e.kind === "heading")
          return (
            <div key={i} className="ui-menu-heading" role="presentation">
              {e.label}
            </div>
          );
        const isSub = e.kind === "sub";
        const item = e as Item & { kind?: "item" };
        const role = !isSub && item.check ? (item.check === "radio" ? "menuitemradio" : "menuitemcheckbox") : "menuitem";
        const open = sub?.index === i;
        return (
          <div
            key={i}
            ref={(el) => {
              itemRefs.current[i] = el;
            }}
            role={role}
            tabIndex={-1}
            aria-label={e.label}
            aria-disabled={e.disabled || undefined}
            aria-checked={role !== "menuitem" ? !!item.checked : undefined}
            aria-haspopup={isSub ? "menu" : undefined}
            aria-expanded={isSub ? open : undefined}
            aria-keyshortcuts={!isSub ? ariaShortcut(item.shortcut) : undefined}
            className={`ui-menu-item${e.disabled ? " is-disabled" : ""}${open ? " is-open" : ""}`}
            title={e.hint}
            onMouseEnter={(ev) => {
              (ev.currentTarget as HTMLElement).focus();
              if (isSub && !e.disabled) setSub({ index: i, focus: "none" });
              else if (sub !== null) setSub(null);
            }}
            onClick={(ev) => {
              ev.stopPropagation();
              activate(i, "mouse");
            }}
          >
            <span className="ui-menu-check" aria-hidden="true">
              {role !== "menuitem" && item.checked ? <Icon icon={IconCheck} /> : e.icon ? <Icon icon={e.icon} /> : null}
            </span>
            <span className="ui-menu-label">{e.label}</span>
            {!isSub && item.detail && <span className="ui-menu-detail">{item.detail}</span>}
            {!isSub && item.shortcut && (
              <kbd className="ui-kbd" aria-hidden="true">
                {item.shortcut}
              </kbd>
            )}
            {isSub && <Icon icon={IconChevronRight} className="ui-menu-subarrow" />}
          </div>
        );
      })}
      {subEntry && subEntry.kind === "sub" && subRect && (
        <MenuPopup
          items={subEntry.items}
          label={subEntry.label}
          level={level + 1}
          anchor={{ rect: { left: subRect.left, top: subRect.top, right: subRect.right, bottom: subRect.bottom }, placement: "right" }}
          focusFirst={sub!.focus}
          onClose={(reason) => {
            const i = sub!.index;
            setSub(null);
            if (reason === "escape" || reason === "left") focusAt(i);
            else onClose(reason);
          }}
        />
      )}
    </div>,
    document.body,
  );
}

/** Closes a popup on a pointer press outside of it, its submenus and its opener. */
function useOutside(open: boolean, opener: React.RefObject<HTMLElement | null>, close: () => void): void {
  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => {
      const t = e.target as Node | null;
      if (!t) return;
      if (opener.current?.contains(t)) return;
      for (const p of openPopups) if (p.contains(t)) return;
      close();
    };
    const blur = () => close();
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("blur", blur);
    };
  }, [open, opener, close]);
}

export interface MenuButtonProps {
  /** Accessible name of the button (and of the menu). */
  label: string;
  /** Visible text (default: `label`; `null`: symbol only). */
  text?: ReactNode | null;
  icon?: IconType;
  variant?: ButtonVariant;
  items: MenuEntry[];
  align?: "start" | "end";
  shortcut?: string;
  hint?: string;
  disabled?: boolean;
  /** Visible caret (default for text buttons). */
  caret?: boolean;
  pressed?: boolean;
  className?: string;
  "data-testid"?: string;
}

export const MenuButton = forwardRef<HTMLButtonElement, MenuButtonProps>(function MenuButton(
  { label, text, icon, variant = "quiet", items, align = "start", shortcut, hint, disabled, caret, pressed, className, ...rest },
  fwd,
) {
  const btn = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState<null | "first" | "last" | "none">(null);
  const close = useCallback(() => setOpen(null), []);
  useOutside(open !== null, btn, close);
  const setRef = (el: HTMLButtonElement | null) => {
    btn.current = el;
    if (typeof fwd === "function") fwd(el);
    else if (fwd) fwd.current = el;
  };
  const r = btn.current?.getBoundingClientRect();
  const shown = text === undefined ? label : text;
  const common = {
    ref: setRef,
    "aria-haspopup": "menu" as const,
    "aria-expanded": open !== null,
    disabled,
    pressed,
    className,
    onClick: () => setOpen(open === null ? "first" : null),
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setOpen("first");
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setOpen("last");
      }
    },
    ...rest,
  };
  return (
    <>
      {shown === null ? (
        <IconButton icon={icon!} label={label} variant={variant} shortcut={shortcut} hint={hint} {...common} />
      ) : (
        <Button variant={variant} icon={icon} aria-label={label} shortcut={shortcut} title={tooltip(label, shortcut, hint)} caret={caret ?? true} {...common}>
          {shown}
        </Button>
      )}
      {open !== null && r && (
        <MenuPopup
          items={items}
          label={label}
          anchor={{ rect: { left: r.left, top: r.top, right: r.right, bottom: r.bottom }, placement: align === "end" ? "below-end" : "below-start" }}
          focusFirst={open}
          onClose={(reason) => {
            setOpen(null);
            if (reason === "escape" || reason === "done" || reason === "tab") btn.current?.focus();
          }}
        />
      )}
    </>
  );
});

export interface SplitButtonProps {
  /** Name of the main action (e.g. "Exportieren: 3MF mehrfarbig"). */
  label: string;
  text: ReactNode;
  icon?: IconType;
  variant?: ButtonVariant;
  onClick: () => void;
  shortcut?: string;
  hint?: string;
  /** Name of the menu button (e.g. "Exportformat"). */
  menuLabel: string;
  items: MenuEntry[];
  disabled?: boolean;
  "data-testid"?: string;
}

/** Main action plus a menu of variants (the export button keeps the last format). */
export function SplitButton({ label, text, icon, variant = "primary", onClick, shortcut, hint, menuLabel, items, disabled, ...rest }: SplitButtonProps) {
  return (
    <span className={`ui-split ui-split--${variant}`} {...rest}>
      <Button variant={variant} icon={icon} aria-label={label} shortcut={shortcut} hint={hint} onClick={onClick} disabled={disabled} className="ui-split-main">
        {text}
      </Button>
      <MenuButton label={menuLabel} text={null} icon={IconChevronDown} variant={variant} items={items} align="end" className="ui-split-menu" disabled={disabled} />
    </span>
  );
}

/** Context menu at a point (right click or the context menu key). */
export function ContextMenu({ at, label, items, onClose }: { at: { x: number; y: number; opener?: HTMLElement | null } | null; label: string; items: MenuEntry[]; onClose: () => void }) {
  const opener = useRef<HTMLElement | null>(null);
  opener.current = at?.opener ?? null;
  useOutside(at !== null, opener, onClose);
  if (!at) return null;
  return (
    <MenuPopup
      items={items}
      label={label}
      anchor={{ rect: { left: at.x, top: at.y, right: at.x, bottom: at.y }, placement: "point" }}
      focusFirst="first"
      onClose={(reason) => {
        onClose();
        if (reason !== "outside") at.opener?.focus();
      }}
    />
  );
}

/** Caret for custom triggers. */
export { CaretIcon };
