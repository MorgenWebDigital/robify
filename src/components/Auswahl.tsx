import { useEffect, useMemo, useRef, useState } from "react";
import { CheckIcon, ChevronDownIcon } from "./Icons";
import { t } from "../lib/i18n";

export interface AuswahlOption {
  id: string;
  label: string;
  /** Kleines Zeichen links vom Namen, etwa eine Flagge. */
  symbol?: string;
  /** Zusätzliche Wörter, auf die die Suche anspringt, ohne sichtbar zu sein. */
  suchtext?: string;
}

/**
 * Auswahlfeld mit eigener Liste.
 *
 * Ein natives `select` gibt seinen aufgeklappten Teil an das Betriebssystem
 * ab, unter Linux zeichnet ihn GTK, und dorthin reicht kein CSS. Übrig blieb
 * ein Feld im Stil der App, aus dem eine fremd aussehende Liste fuhr.
 *
 * Darum hier ein eigener Aufbau: Knopf plus Liste, beide in derselben
 * Bauweise wie die Menüs. Die Tastatur bedient ihn wie ein echtes
 * Auswahlfeld, und für Vorleseprogramme meldet er sich als solches.
 */
export function Auswahl({
  value,
  options,
  onChange,
  label,
  className = "",
}: {
  value: string;
  options: AuswahlOption[];
  onChange: (value: string) => void;
  /** Beschriftung für Vorleseprogramme. */
  label: string;
  className?: string;
}) {
  const [offen, setOffen] = useState(false);
  const [suche, setSuche] = useState("");
  /** Eintrag unter der Tastatur-Markierung, unabhängig vom gewählten. */
  const [markiert, setMarkiert] = useState(0);
  /** Nach oben ausfahren, wenn unten kein Platz mehr ist. */
  const [nachOben, setNachOben] = useState(false);
  const huelle = useRef<HTMLDivElement>(null);
  const knopf = useRef<HTMLButtonElement>(null);

  const gewaehlt = options.find((option) => option.id === value);

  /**
   * Ab einer Handvoll Einträgen eine Suchleiste. Bei den Sprachen zahlt sie
   * sich schon bei acht aus: Wer sein 中文 sucht, tippt lieber „chin“, als
   * eine Schrift zu suchen, die er nicht kennt.
   */
  const mitSuche = options.length > 5;
  const gefiltert = useMemo(() => {
    const begriff = suche.trim().toLowerCase();
    if (!begriff) return options;
    return options.filter((option) =>
      `${option.label} ${option.suchtext ?? ""}`
        .toLowerCase()
        .includes(begriff),
    );
  }, [options, suche]);

  useEffect(() => {
    if (!offen) return;
    const draussenGeklickt = (event: PointerEvent) => {
      if (!huelle.current?.contains(event.target as Node)) setOffen(false);
    };
    document.addEventListener("pointerdown", draussenGeklickt);
    return () => document.removeEventListener("pointerdown", draussenGeklickt);
  }, [offen]);

  const oeffnen = () => {
    const kasten = knopf.current?.getBoundingClientRect();
    // Grob geschätzte Listenhöhe genügt: Es geht nur um oben oder unten.
    if (kasten)
      setNachOben(
        window.innerHeight - kasten.bottom < Math.min(options.length * 38, 240),
      );
    setSuche("");
    setMarkiert(
      Math.max(
        0,
        options.findIndex((option) => option.id === value),
      ),
    );
    setOffen(true);
  };

  const waehlen = (id: string) => {
    onChange(id);
    setOffen(false);
    knopf.current?.focus();
  };

  const aufTaste = (event: React.KeyboardEvent) => {
    if (!offen) {
      if (
        event.key === "Enter" ||
        event.key === " " ||
        event.key === "ArrowDown"
      ) {
        event.preventDefault();
        oeffnen();
      }
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setOffen(false);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const schritt = event.key === "ArrowDown" ? 1 : -1;
      // Am Ende umbrechen, wie es ein natives Auswahlfeld auch tut.
      setMarkiert(
        (alt) =>
          (alt + schritt + gefiltert.length) % Math.max(gefiltert.length, 1),
      );
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      const ziel = gefiltert[markiert];
      if (ziel) waehlen(ziel.id);
    }
  };

  return (
    <div
      ref={huelle}
      className={`relative ${offen ? "z-50" : ""} ${className}`}
    >
      <button
        ref={knopf}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={offen}
        aria-label={label}
        onClick={() => (offen ? setOffen(false) : oeffnen())}
        onKeyDown={aufTaste}
        className="pill-select w-full text-start"
      >
        {gewaehlt?.symbol && <Zeichen wert={gewaehlt.symbol} />}
        {gewaehlt?.label ?? ""}
      </button>

      {/* Der Pfeil liegt über dem Knopf und nimmt keine Klicks an. */}
      <ChevronDownIcon
        size={16}
        className={`pointer-events-none absolute top-1/2 end-3 text-mute transition-transform ${
          offen
            ? "translate-y-[calc(-50%-2px)] rotate-180"
            : "translate-y-[calc(-50%-2px)]"
        }`}
      />

      {offen && (
        // Der Abstand zum Knopf liegt als Polsterung innerhalb der Hülle,
        // damit auf dem Weg zur Liste keine tote Fläche entsteht.
        <div
          className={`absolute inset-x-0 z-50 ${nachOben ? "bottom-full pb-1" : "top-full pt-1"}`}
        >
          <div className="animate-rise overflow-hidden rounded-xl border border-ink-600 bg-ink-800 shadow-2xl">
            {mitSuche && (
              <div className="border-b border-ink-700 p-2">
                <input
                  autoFocus
                  value={suche}
                  onChange={(event) => {
                    setSuche(event.target.value);
                    setMarkiert(0);
                  }}
                  onKeyDown={aufTaste}
                  placeholder={t("Sprache suchen")}
                  className="search-field"
                />
              </div>
            )}

            <ul
              role="listbox"
              aria-label={label}
              className="max-h-60 overflow-y-auto py-1"
            >
              {gefiltert.length === 0 && (
                <li className="px-3.5 py-3 text-center text-sm text-mute">
                  {t("Nichts gefunden")}
                </li>
              )}
              {gefiltert.map((option, index) => {
                const istGewaehlt = option.id === value;
                return (
                  <li key={option.id}>
                    <button
                      type="button"
                      role="option"
                      aria-selected={istGewaehlt}
                      onClick={() => waehlen(option.id)}
                      onPointerEnter={() => setMarkiert(index)}
                      className={`flex w-full items-center gap-2.5 px-3.5 py-2 text-start text-sm whitespace-nowrap transition ${
                        index === markiert ? "bg-ink-700 text-fg" : "text-fg/90"
                      }`}
                    >
                      <span className="w-4 shrink-0">
                        {istGewaehlt && (
                          <CheckIcon
                            size={14}
                            style={{ color: "var(--accent)" }}
                          />
                        )}
                      </span>
                      {/* Der Platz bleibt auch ohne Zeichen stehen: „System“
                          trägt keine Flagge, und ohne den leeren Kasten
                          begänne es weiter links als die Sprachen darunter.
                          Nur die Liste braucht das; auf dem Knopf steht
                          jeweils ein einziger Eintrag. */}
                      {options.some((eintrag) => eintrag.symbol) && (
                        <Zeichen wert={option.symbol ?? ""} />
                      )}
                      {option.label}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Kleines Zeichen vor dem Namen, etwa eine Flagge.
 *
 * Der abgerundete Rahmen mit Überlauf-Beschnitt macht aus dem rechteckigen
 * Flaggen-Emoji ein Feld mit weichen Ecken, dieselbe Formensprache wie die
 * Knöpfe daneben. Ohne feste Größe ständen die Namen je nach Flagge
 * unterschiedlich weit eingerückt.
 */
function Zeichen({ wert }: { wert: string }) {
  return (
    <span
      aria-hidden="true"
      className="grid h-4 w-6 shrink-0 place-items-center overflow-hidden rounded-[3px] text-base leading-none"
    >
      {wert}
    </span>
  );
}
