import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { t } from "../lib/i18n";
import { useAusblenden } from "../lib/ausblenden";
import { CloseIcon } from "./Icons";

interface ModalProps {
  open: boolean;
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}

/** what can be reached with the tab key */
const ANSTEUERBAR =
  'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

// the reachable elements of a dialog, without the hidden ones.
//
// `offsetParent === null` means not rendered. an `input[type=file]` sitting
// behind a styled button would count otherwise, and the focus landed on
// something nobody sees
function sichtbareFelder(wurzel: HTMLElement | null): HTMLElement[] {
  if (!wurzel) return [];
  return Array.from(wurzel.querySelectorAll<HTMLElement>(ANSTEUERBAR)).filter(
    (element) => element.offsetParent !== null,
  );
}

/** how long the fade-out takes, has to match `.animate-out` in the css */
const AUSBLENDEN_MS = 160;

export function Modal({
  open,
  title,
  subtitle,
  onClose,
  children,
  footer,
  width = "max-w-2xl",
}: ModalProps) {
  const dialog = useRef<HTMLDivElement>(null);
  // stays standing briefly after closing so the fade-out can be seen
  const { sichtbar, schliesst } = useAusblenden(open, AUSBLENDEN_MS);

  // `onClose` lies in a ref instead of hanging off the effect.
  //
  // the callers almost always hand a freshly built function over there, and
  // that is a different one at every render. standing in the dependency list,
  // the effect would run again after every keystroke and set the focus anew:
  // one typed a letter into the playlist name and had to click back into the
  // field afterwards
  const schliessRef = useRef(onClose);
  schliessRef.current = onClose;

  // the focus belongs in the dialog while it is open.
  //
  // without that the caret stays on the page behind: whoever works by
  // keyboard tabs through a list they cannot see at all and ends up behind
  // the window at some point. on closing the focus goes back where it came
  // from, otherwise one starts over after every dialog
  useEffect(() => {
    if (!open) return;
    const vorher = document.activeElement as HTMLElement | null;

    // only what can be seen: the cover picker brings an invisible file input
    // along, and in the playlist dialog it stood exactly where the focus
    // aims. the same check as when wrapping with the tab key further down
    const felder = sichtbareFelder(dialog.current);
    // the first control that is not the close cross: the dialog is to offer
    // its actual action, not the exit
    const erstes = felder.length > 1 ? felder[1] : felder[0];
    // `preventScroll`: focusing scrolls the element into view, and in a dialog
    // whose body overflows that pushed the top of the form up under the
    // header. the release editor opened with its first button half hidden.
    erstes?.focus({ preventScroll: true });

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        schliessRef.current();
        return;
      }
      if (event.key !== "Tab" || !dialog.current) return;

      const liste = sichtbareFelder(dialog.current);
      if (liste.length === 0) return;

      const erst = liste[0];
      const letzt = liste[liste.length - 1];
      const aktiv = document.activeElement;

      // wrap at the edge instead of falling out
      if (
        event.shiftKey &&
        (aktiv === erst || !dialog.current.contains(aktiv))
      ) {
        event.preventDefault();
        letzt.focus();
      } else if (
        !event.shiftKey &&
        (aktiv === letzt || !dialog.current.contains(aktiv))
      ) {
        event.preventDefault();
        erst.focus();
      }
    };

    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      vorher?.focus?.();
    };
  }, [open]);

  if (!sichtbar) return null;

  // drawn onto the body, not where the dialog stands in the tree.
  //
  // `fixed` refers to the window only as long as no ancestor carries a
  // `transform`. the page area has one: its slide-in runs with `both` and
  // therefore keeps its end position after it has finished. that made it the
  // reference frame, and the dialogs stood centred over the page content
  // instead of over the window, so too far right and too high
  return createPortal(
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center p-4 ${
        schliesst ? "animate-out" : ""
      }`}
    >
      <div
        className={`absolute inset-0 backdrop-blur-sm ${schliesst ? "" : "animate-scrim"}`}
        style={{ background: "var(--scrim)" }}
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={`surface relative flex max-h-[90vh] w-full ${width} flex-col overflow-hidden shadow-2xl ${
          schliesst ? "" : "animate-rise"
        }`}
      >
        <header className="animate-content flex items-start justify-between gap-4 border-b border-ink-700 px-6 py-4">
          <div>
            <h2 className="text-lg font-semibold">{title}</h2>
            {subtitle && <p className="mt-0.5 text-sm text-mute">{subtitle}</p>}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("Schließen")}
            className="pill-btn h-8 w-8"
          >
            <CloseIcon />
          </button>
        </header>

        <div className="animate-content min-h-0 flex-1 overflow-y-auto px-6 py-5">
          {children}
        </div>

        {/* `flex-wrap`: where something other than the buttons stands on the
            left of the footer, a checkbox with a label for instance, all
            three squeezed into one line on a hand's width and the text broke
            mid-word. wrapped it stands in a line of its own. */}
        {footer && (
          <footer className="animate-content flex flex-wrap items-center justify-end gap-3 border-t border-ink-700 px-6 py-4">
            {footer}
          </footer>
        )}
      </div>
    </div>,
    document.body,
  );
}

export function Button({
  variant = "ghost",
  className = "",
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "ghost" | "outline";
}) {
  // the same build as everywhere else: a pill with a hard bottom edge that
  // sinks onto it when pressed. `ghost` stays deliberately flat, a cancel is
  // not to look as if it wanted to be pressed
  const styles = {
    primary: "is-raised is-accent font-semibold",
    ghost: "font-medium",
    outline: "is-raised font-semibold",
  }[variant];

  return (
    <button
      type="button"
      {...props}
      className={`pill-btn h-9 px-4 text-sm disabled:cursor-not-allowed ${styles} ${className}`}
    />
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block eyebrow">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-xs text-mute">{hint}</span>}
    </label>
  );
}

/**
 * a single-line field in a dialog.
 *
 * the same groove the search bars are built from, not a flat bordered box of
 * its own. a dialog stood next to a select of the app's own make, and the two
 * did not look like one family: the field flat and squared, the select a
 * raised pill.
 */
export const inputClass = "search-field";

/** the same for several lines: same groove, a rounded rectangle instead of a pill */
export const textareaClass = "text-field";
