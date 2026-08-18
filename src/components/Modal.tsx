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

/** Was sich mit der Tabulatortaste ansteuern lässt. */
const ANSTEUERBAR =
  'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

/**
 * Die ansteuerbaren Elemente eines Dialogs, ohne die verborgenen.
 *
 * `offsetParent === null` heißt: nicht dargestellt. Ein `input[type=file]`,
 * das hinter einem gestalteten Knopf steckt, zählt sonst mit, und der Fokus
 * landete auf etwas, das niemand sieht.
 */
function sichtbareFelder(wurzel: HTMLElement | null): HTMLElement[] {
  if (!wurzel) return [];
  return Array.from(wurzel.querySelectorAll<HTMLElement>(ANSTEUERBAR)).filter(
    (element) => element.offsetParent !== null,
  );
}

/** Wie lange das Ausblenden dauert; muss zu `.animate-out` im CSS passen. */
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
  // Bleibt nach dem Schließen kurz stehen, damit das Ausblenden zu sehen ist.
  const { sichtbar, schliesst } = useAusblenden(open, AUSBLENDEN_MS);

  /*
   * `onClose` liegt in einer Truhe, statt am Effekt zu hängen.
   *
   * Die Aufrufer geben dort fast immer eine frisch gebaute Funktion mit, und
   * die ist bei jedem Zeichnen eine andere. Stünde sie in der Abhängigkeits-
   * liste, liefe der Effekt nach jedem Tastendruck erneut und setzte den Fokus
   * neu: Man tippte einen Buchstaben in den Playlist-Namen und musste danach
   * wieder ins Feld klicken.
   */
  const schliessRef = useRef(onClose);
  schliessRef.current = onClose;

  /**
   * Der Fokus gehört in den Dialog, solange er offen ist.
   *
   * Ohne das steht die Schreibmarke weiter auf der Seite dahinter: Wer mit der
   * Tastatur arbeitet, tabbt durch eine Liste, die er gar nicht sieht, und
   * landet irgendwann hinter dem Fenster. Beim Schließen geht der Fokus dorthin
   * zurück, wo er herkam, sonst beginnt man nach jedem Dialog wieder von vorn.
   */
  useEffect(() => {
    if (!open) return;
    const vorher = document.activeElement as HTMLElement | null;

    // Nur was man auch sieht: Der Cover-Wähler bringt eine unsichtbare
    // Dateiauswahl mit, und die stand im Playlist-Dialog an genau der Stelle,
    // auf die der Fokus zielt. Dieselbe Prüfung wie beim Umbrechen mit der
    // Tabulatortaste weiter unten.
    const felder = sichtbareFelder(dialog.current);
    // Das erste Bedienelement, das kein Schließkreuz ist: Der Dialog soll seine
    // eigentliche Handlung anbieten, nicht den Ausgang.
    const erstes = felder.length > 1 ? felder[1] : felder[0];
    erstes?.focus();

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

      // Am Rand umbrechen statt hinausfallen.
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

  /*
   * Am Körper gezeichnet, nicht dort, wo der Dialog im Baum steht.
   *
   * `fixed` bezieht sich auf das Fenster nur so lange, wie kein Vorfahr eine
   * `transform` trägt. Der Seitenbereich hat eine: Sein Einfahren läuft mit
   * `both` und behält darum seine Endstellung auch nach dem Ablauf. Damit
   * wurde er zum Bezugsrahmen, und die Dialoge standen mittig über dem
   * Seiteninhalt statt über dem Fenster, also zu weit rechts und zu hoch.
   */
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

        {footer && (
          <footer className="animate-content flex items-center justify-end gap-3 border-t border-ink-700 px-6 py-4">
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
  // Dieselbe Bauweise wie überall sonst: Pille mit harter Unterkante, die
  // beim Drücken darauf absinkt. `ghost` bleibt bewusst flach, ein Abbrechen
  // soll nicht so aussehen, als wolle es gedrückt werden.
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

export const inputClass =
  "w-full rounded-lg border border-ink-600 bg-ink-900 px-3 py-2 text-sm text-fg placeholder:text-mute/60 transition focus:border-transparent";
