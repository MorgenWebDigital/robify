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
   * Ausfahrrichtung erzwingen.
   *
   * Ohne Angabe entscheidet das Menü selbst nach dem Platz um den Knopf.
   */
  side?: "bottom" | "top";
  /**
   * Öffnet beim Überfahren und schließt beim Verlassen, statt auf Klick.
   *
   * Für Knöpfe, die ohnehin nur beim Überfahren einer Zeile erscheinen: Dort
   * ist der Klick ein zusätzlicher Schritt für etwas, das man nur ansehen
   * will. Der Klick funktioniert weiterhin, für Zeigegeräte ohne Schweben.
   */
  hover?: boolean;
}

/** Wo das Menü steht, in Fensterkoordinaten. */
interface Lage {
  obenAus: boolean;
  /** Abstand zur oberen bzw. unteren Fensterkante, je nach `obenAus`. */
  y: number;
  /** Abstand zur linken bzw. rechten Fensterkante, je nach `align`. */
  x: number;
  maxHoehe: number;
}

/** Luft zu den Fensterkanten. */
const RAND = 8;
/** Luft zwischen Knopf und Liste. */
const LUFT = 4;

/**
 * Kleines Kontextmenü; schließt bei Klick nach außen und mit Escape.
 *
 * Die Liste hängt am Fenster (`position: fixed`) und wird durch ein Portal
 * unmittelbar an den Körper gezeichnet, nicht neben den Knopf. Der Grund ist
 * die Bildlauffläche: Ein absolut gesetztes Element zählt zum scrollbaren
 * Inhalt seines Behälters. Beim letzten Titel einer Playlist wuchs die Seite
 * dadurch um die Höhe des Menüs, und zwar auch dann, wenn im Fenster darunter
 * noch Platz war, denn der Inhalt endete ja mit dieser Zeile. Am Fenster
 * hängend nimmt die Liste keinen Platz mehr ein und kann sich zugleich an
 * dessen Kanten halten.
 */
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
   * Zwischen Knopf und Liste liegen vier Pixel Luft. Ohne diese Verzögerung
   * fiele das Menü genau beim Überqueren dieser Lücke wieder zu, und man käme
   * nie an einen Eintrag heran. Sie trägt jetzt doppelt: Die Liste liegt in
   * einem Portal, der Zeiger verlässt den Knopf also in jedem Fall, bevor er
   * die Liste erreicht.
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
   * Wohin die Liste gehört.
   *
   * Die Höhe wird geschätzt, nicht gemessen: Messen ließe sie sich erst, wenn
   * die Liste schon steht. Für die Frage „oben oder unten“ genügt die
   * Schätzung; die genaue Begrenzung übernimmt danach `maxHoehe`, und was
   * nicht hineinpasst, wird in der Liste selbst scrollbar.
   */
  const messen = (): Lage | null => {
    const kasten = container.current?.getBoundingClientRect();
    if (!kasten) return null;

    const platzUnten = window.innerHeight - kasten.bottom - LUFT - RAND;
    const platzOben = kasten.top - LUFT - RAND;
    const geschaetzt = Math.min(items.length * 38 + 12, 320);
    // Nur wechseln, wenn oben tatsächlich mehr Platz ist: Bei einem Fenster,
    // das für beides zu klein ist, bliebe die Liste sonst genauso knapp, nur
    // an der anderen Kante.
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
    // Am Fenster hängend wandert die Liste beim Blättern nicht mit dem Knopf.
    // `capture`, weil gescrollt wird, was unter dem Zeiger liegt, und das
    // Ereignis dort nicht nach oben steigt.
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
        // Berührung meldet sich ebenfalls als „enter“; dort soll erst der
        // Klick öffnen, sonst klappt beim Tippen zweimal etwas auf.
        if (event.pointerType === "touch") return;
        nichtSchliessen();
        oeffnen();
      }
    : undefined;

  /**
   * Beim Verlassen zufallen — außer der Finger war es.
   *
   * Auf einem Telefon endet der Zeiger mit der Berührung: Gleich nach dem
   * Tippen meldet sich „leave“, und das eben erst geöffnete Menü fiel nach
   * 160 ms wieder zu, ohne dass etwas zu sehen war. Das Öffnen nimmt
   * Berührungen längst aus, das Schließen tat es nicht. Dort schließt der
   * Druck daneben, den `pointerdown` am Dokument schon abfängt.
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
            // `w-max`: Die Breite richtet sich nach dem längsten Eintrag, nicht
            // nach dem schmalen Knopf darüber. `overflow-y-auto` fängt Listen,
            // die auch in der besseren Richtung nicht ganz hineinpassen.
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
