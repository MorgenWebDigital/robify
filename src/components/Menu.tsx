import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  tone?: "default" | "danger";
  disabled?: boolean;
}

interface MenuProps {
  items: MenuItem[];
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  align?: "left" | "right";
  /**
   * forces the direction it opens in.
   *
   * without a value the menu decides by itself from the room around the
   * button.
   */
  side?: "bottom" | "top";
  /**
   * opens on hover and closes on leaving, instead of on a click.
   *
   * for buttons that appear on hovering a row anyway: a click is an extra
   * step there for something one only wants to look at. clicking still works,
   * for pointing devices without hovering.
   */
  hover?: boolean;
}

/** where the menu stands, in window coordinates */
interface Lage {
  obenAus: boolean;
  /** distance to the top or bottom window edge, depending on `obenAus` */
  y: number;
  /** distance to the left or right window edge, depending on `align` */
  x: number;
  maxHoehe: number;
}

/** air to the window edges */
const RAND = 8;
/** air between button and list */
const LUFT = 4;

// a small context menu, closes on a click outside and on escape.
//
// the list hangs off the window (`position: fixed`) and is drawn straight
// onto the body through a portal, not next to the button. the reason is the
// scrolling area: an absolutely positioned element counts towards the
// scrollable content of its container. at the last track of a playlist the
// page therefore grew by the height of the menu, even where there was room
// below in the window, since the content ended with that row. hanging off the
// window the list takes no room any more and can hold to its edges at the
// same time
export function Menu({
  items,
  trigger,
  align = "right",
  side,
  hover = false,
}: MenuProps) {
  const [open, setOpen] = useState(false);
  const [lage, setLage] = useState<Lage | null>(null);
  const container = useRef<HTMLDivElement>(null);
  const liste = useRef<HTMLDivElement>(null);
  const schliessUhr = useRef<number | null>(null);

  /**
   * four pixels of air lie between button and list. without this delay the
   * menu would fall shut exactly while crossing that gap, and one would never
   * reach an entry. it carries twice as much now: the list lies in a portal,
   * so the pointer leaves the button in any case before it reaches the list.
   */
  const spaeterSchliessen = () => {
    if (schliessUhr.current) window.clearTimeout(schliessUhr.current);
    schliessUhr.current = window.setTimeout(() => setOpen(false), 160);
  };
  const nichtSchliessen = () => {
    if (schliessUhr.current) window.clearTimeout(schliessUhr.current);
    schliessUhr.current = null;
  };

  useEffect(() => () => nichtSchliessen(), []);

  /**
   * where the list belongs.
   *
   * the height is estimated, not measured: measuring would only be possible
   * once the list already stands. for the question of above or below the
   * estimate does, the exact bound is taken over by `maxHoehe` afterwards,
   * and what does not fit becomes scrollable inside the list.
   */
  const messen = (): Lage | null => {
    const kasten = container.current?.getBoundingClientRect();
    if (!kasten) return null;

    const platzUnten = window.innerHeight - kasten.bottom - LUFT - RAND;
    const platzOben = kasten.top - LUFT - RAND;
    const geschaetzt = Math.min(items.length * 38 + 12, 320);
    // switch only where there is actually more room above: in a window too
    // small for either, the list would otherwise stay just as tight, only at
    // the other edge
    const obenAus = side
      ? side === "top"
      : platzUnten < geschaetzt && platzOben > platzUnten;

    return {
      obenAus,
      y: obenAus
        ? window.innerHeight - kasten.top + LUFT
        : kasten.bottom + LUFT,
      x:
        align === "right"
          ? Math.max(RAND, window.innerWidth - kasten.right)
          : Math.max(RAND, kasten.left),
      maxHoehe: Math.max(120, obenAus ? platzOben : platzUnten),
    };
  };

  const oeffnen = () => {
    setLage(messen());
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;

    const draussen = (ziel: Node) =>
      !container.current?.contains(ziel) && !liste.current?.contains(ziel);

    const onPointerDown = (event: PointerEvent) => {
      if (draussen(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    // hanging off the window, the list does not travel with the button while
    // scrolling. `capture`, because what is scrolled is whatever lies under
    // the pointer, and the event does not bubble up from there
    const nachfuehren = () => setLage(messen());

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    window.addEventListener("scroll", nachfuehren, true);
    window.addEventListener("resize", nachfuehren);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", nachfuehren, true);
      window.removeEventListener("resize", nachfuehren);
    };
  }, [open]);

  const beimUeberfahren = hover
    ? (event: React.PointerEvent) => {
        // a touch reports itself as an enter as well, and there the click is
        // to open it, otherwise something unfolds twice on a tap
        if (event.pointerType === "touch") return;
        nichtSchliessen();
        oeffnen();
      }
    : undefined;

  /**
   * falls shut on leaving, unless it was a finger.
   *
   * on a phone the pointer ends with the touch: right after the tap a leave
   * is reported, and the menu just opened fell shut after 160 ms without
   * anything having been visible. opening has excluded touches for a long
   * time, closing did not. there the press next to it closes, which
   * `pointerdown` on the document catches already.
   */
  const beimVerlassen = hover
    ? (event: React.PointerEvent) => {
        if (event.pointerType === "touch") return;
        spaeterSchliessen();
      }
    : undefined;

  return (
    <div
      ref={container}
      className="relative"
      onPointerEnter={beimUeberfahren}
      onPointerLeave={beimVerlassen}
    >
      {trigger({ open, toggle: () => (open ? setOpen(false) : oeffnen()) })}

      {open &&
        lage &&
        createPortal(
          <div
            ref={liste}
            role="menu"
            onPointerEnter={hover ? nichtSchliessen : undefined}
            onPointerLeave={beimVerlassen}
            style={{
              position: "fixed",
              [lage.obenAus ? "bottom" : "top"]: lage.y,
              [align === "right" ? "right" : "left"]: lage.x,
              maxHeight: lage.maxHoehe,
              maxWidth: `calc(100vw - ${RAND * 2}px)`,
            }}
            // `w-max`: the width follows the longest entry, not the narrow
            // button above it. `overflow-y-auto` catches lists that do not
            // quite fit even in the better direction
            className="animate-rise z-50 w-max min-w-56 overflow-x-hidden overflow-y-auto rounded-xl border border-ink-600 bg-ink-800 py-1 shadow-2xl"
          >
            {items.map((item) => (
              <button
                key={item.label}
                type="button"
                role="menuitem"
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
                className={`flex w-full items-center gap-2.5 px-3.5 py-2 text-start text-sm whitespace-nowrap transition disabled:opacity-40 ${
                  item.tone === "danger"
                    ? "text-danger hover:bg-danger-soft"
                    : "text-fg/90 hover:bg-ink-700"
                }`}
              >
                {item.icon && <span className="text-mute">{item.icon}</span>}
                {item.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}
