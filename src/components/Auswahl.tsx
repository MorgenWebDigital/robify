import { useEffect, useMemo, useRef, useState } from "react";
import { CheckIcon, ChevronDownIcon } from "./Icons";
import { t } from "../lib/i18n";

export interface AuswahlOption {
  id: string;
  label: string;
  /** a small icon to the left of the name, a flag for instance */
  symbol?: string;
  /** extra words the search fires on without being visible */
  suchtext?: string;
}

// a select field with a list of its own.
//
// a native `select` hands its unfolded part to the operating system, under
// linux gtk draws it, and no css reaches in there. what was left was a field
// in the style of the app out of which a foreign-looking list slid.
//
// hence a build of its own here: button plus list, both in the same build as
// the menus. the keyboard operates it like a real select field, and to screen
// readers it reports itself as one
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
  /** label for screen readers */
  label: string;
  className?: string;
}) {
  const [offen, setOffen] = useState(false);
  const [suche, setSuche] = useState("");
  /** the entry under the keyboard cursor, independent of the selected one */
  const [markiert, setMarkiert] = useState(0);
  /** open upwards where there is no room left below */
  const [nachOben, setNachOben] = useState(false);
  const huelle = useRef<HTMLDivElement>(null);
  const knopf = useRef<HTMLButtonElement>(null);

  const gewaehlt = options.find((option) => option.id === value);

  /**
   * a search bar from a handful of entries on. with the languages it pays off
   * at eight already: whoever looks for their 中文 would rather type "chin"
   * than search for a script they cannot read.
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
    // a roughly estimated list height does, it is only about above or below
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
      // wrap at the end, as a native select field does too
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

      {/* the arrow lies over the button and takes no clicks. */}
      <ChevronDownIcon
        size={16}
        className={`pointer-events-none absolute top-1/2 end-3 text-mute transition-transform ${
          offen
            ? "translate-y-[calc(-50%-2px)] rotate-180"
            : "translate-y-[calc(-50%-2px)]"
        }`}
      />

      {offen && (
        // the distance to the button lies as padding inside the wrapper, so
        // no dead area appears on the way to the list
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
                      {/* the room stays even without an icon: "system"
                          carries no flag, and without the empty box it would
                          start further left than the languages below. only
                          the list needs that, the button carries a single
                          entry at a time. */}
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

// a small icon before the name, a flag for instance.
//
// the rounded frame with overflow clipping turns the rectangular flag emoji
// into a field with soft corners, the same formal language as the buttons
// next to it. without a fixed size the names would be indented differently
// depending on the flag
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
