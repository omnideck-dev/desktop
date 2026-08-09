import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { KebabIcon } from "./icons";

export interface DropdownMenuItem {
  label: string;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
}

const VIEWPORT_MARGIN = 8;

/** Generic kebab (⋮) trigger + popover menu. Closes on outside click or
 * Escape. Positioned with `position: fixed` and measured coordinates rather
 * than plain CSS so it can flip to open upward — for a row near the bottom
 * of the table/window, opening straight down would push the menu past the
 * viewport edge and force a scroll. The menu's right edge always aligns
 * with the trigger's right edge. */
export default function DropdownMenu({ items, label }: { items: DropdownMenuItem[]; label: string }) {
  const [open, setOpen] = useState(false);
  const [style, setStyle] = useState<{ top?: number; bottom?: number; right: number } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const trigger = triggerRef.current.getBoundingClientRect();
    const menuWidth = listRef.current?.offsetWidth ?? 0;
    const menuHeight = listRef.current?.offsetHeight ?? 0;
    // Clamp so the menu's left edge can't go past the viewport even if the
    // trigger sits close to the left edge (not a case the actions column
    // hits today, but cheap to guard against).
    const right = Math.min(
      window.innerWidth - trigger.right,
      window.innerWidth - menuWidth - VIEWPORT_MARGIN,
    );

    const spaceBelow = window.innerHeight - trigger.bottom;
    const fitsBelow = spaceBelow >= menuHeight + VIEWPORT_MARGIN;

    if (fitsBelow) {
      setStyle({ top: trigger.top, right });
    } else {
      // Open upward, anchored so the menu's bottom sits at the trigger's
      // top — and never pokes past the top of the viewport either.
      const top = Math.max(VIEWPORT_MARGIN, trigger.top - menuHeight);
      setStyle({ top, right });
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className="dropdown-menu" ref={rootRef}>
      <button
        ref={triggerRef}
        className="icon-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <KebabIcon />
      </button>
      {open && (
        <div
          ref={listRef}
          className="dropdown-menu__list"
          role="menu"
          style={
            style
              ? { top: style.top, right: style.right, visibility: "visible" }
              : { visibility: "hidden" }
          }
        >
          {items.map((item) => (
            <button
              key={item.label}
              role="menuitem"
              className={`dropdown-menu__item ${item.danger ? "dropdown-menu__item--danger" : ""}`}
              disabled={item.disabled}
              onClick={() => {
                setOpen(false);
                item.onClick();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
