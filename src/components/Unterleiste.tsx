import { useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { t } from "../lib/i18n";
import { UNTEN, UNTER_MEHR, istHier } from "../lib/navigation";
import { useAusblenden } from "../lib/ausblenden";
import { DotsIcon } from "./Icons";
import {
  Aktualisierungsfenster,
  Aktualisierungszeile,
  etwasNeues,
  SIGNAL,
  useAktualisierungen,
} from "./Aktualisierung";

/** has to match the duration of `.animate-out` in the stylesheet */
const AUSBLENDEN_MS = 160;

// navigation at the bottom edge, for the phone.
//
// a sidebar of 240 points takes more than half of a hand's width of about
// 410, and a strip was left for the content in which every heading wrapped
// after two words. at the bottom the same navigation costs nothing of the
// width.
//
// four targets plus "more": five areas side by side are just about safe to
// hit at this width. what does not fit sits behind "more" so no section
// becomes unreachable
export function Unterleiste() {
  const { pathname: pfad } = useLocation();
  const [mehrOffen, setMehrOffen] = useState(false);
  const [neuesOffen, setNeuesOffen] = useState(false);
  const { sichtbar, schliesst } = useAusblenden(mehrOffen, AUSBLENDEN_MS);

  const mehrAktiv = UNTER_MEHR.some((eintrag) => istHier(pfad, eintrag));
  const neues = etwasNeues(useAktualisierungen());

  return (
    <>
      {sichtbar && (
        <>
          {/* the scrim catches the click next to it. */}
          <button
            type="button"
            aria-label={t("Schließen")}
            onClick={() => setMehrOffen(false)}
            className={`fixed inset-0 z-40 backdrop-blur-sm ${schliesst ? "" : "animate-scrim"}`}
            style={{ background: "var(--scrim)" }}
          />
          {/* rises from the bottom, from where the button stands. */}
          <div
            className={`unterleiste-blatt ${schliesst ? "animate-out" : "animate-rise"}`}
          >
            <ul className="flex flex-col gap-0.5 p-2">
              {/* above the sections, not among them: it is not a place one
                  goes to but something that wants doing, and it is gone again
                  once it is done */}
              <Aktualisierungszeile
                oeffnen={() => {
                  setMehrOffen(false);
                  setNeuesOffen(true);
                }}
              />
              {UNTER_MEHR.map((eintrag) => {
                const { to, schluessel, icon: Glyph } = eintrag;
                const hier = istHier(pfad, eintrag);
                return (
                  <li key={to}>
                    <Link
                      to={to}
                      onClick={() => setMehrOffen(false)}
                      aria-current={hier ? "page" : undefined}
                      className={`nav-item gap-3 px-3 py-3 text-sm font-medium ${
                        hier ? "is-active" : ""
                      }`}
                    >
                      <Glyph size={20} />
                      {t(schluessel)}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        </>
      )}

      <nav className="unterleiste md:hidden">
        {UNTEN.map((eintrag) => {
          const { to, schluessel, icon: Glyph } = eintrag;
          const hier = istHier(pfad, eintrag);
          return (
            <Link
              key={to}
              to={to}
              aria-current={hier ? "page" : undefined}
              className={`unterleiste-ziel ${hier ? "is-active" : ""}`}
            >
              <Glyph size={22} />
              <span className="truncate">{t(schluessel)}</span>
            </Link>
          );
        })}
        <button
          type="button"
          onClick={() => setMehrOffen((offen) => !offen)}
          aria-expanded={mehrOffen}
          className={`unterleiste-ziel ${mehrAktiv || mehrOffen ? "is-active" : ""}`}
        >
          {/* the dot says that something lies behind it. without it an update
              on a phone would be hidden behind a sheet nobody opens without a
              reason, and the whole point of the check would be lost */}
          <span className="relative">
            <DotsIcon size={22} />
            {neues && (
              // a ring in the colour of the bar around it: without it the dot
              // sits on the sign and reads as a part of it. with the ring it
              // floats above and is seen without being looked for
              <span
                aria-hidden="true"
                className="absolute -end-1 -top-1 h-2.5 w-2.5 rounded-full"
                style={{
                  background: SIGNAL,
                  boxShadow: "0 0 0 2px var(--canvas)",
                }}
              />
            )}
          </span>
          <span className="truncate">{t("Mehr")}</span>
        </button>
      </nav>

      {/* outside the sheet: tapping the row closes the sheet, and a window
          standing inside it would be taken along */}
      <Aktualisierungsfenster
        offen={neuesOffen}
        schliessen={() => setNeuesOffen(false)}
      />
    </>
  );
}
