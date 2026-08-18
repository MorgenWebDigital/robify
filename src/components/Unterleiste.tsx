import { useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { t } from "../lib/i18n";
import { UNTEN, UNTER_MEHR, istHier } from "../lib/navigation";
import { useAusblenden } from "../lib/ausblenden";
import { DotsIcon } from "./Icons";

/** Muss zur Dauer von `.animate-out` im Stylesheet passen. */
const AUSBLENDEN_MS = 160;

/**
 * Navigation am unteren Rand, für das Telefon.
 *
 * Eine Seitenleiste von 240 Punkten nimmt auf einer Handbreite von etwa 410
 * mehr als die Hälfte ein; für den Inhalt blieb ein Streifen, in dem jede
 * Überschrift nach zwei Wörtern umbrach. Unten kostet dieselbe Navigation
 * nichts von der Breite.
 *
 * Vier Ziele plus „Mehr“: Fünf Flächen nebeneinander sind auf dieser Breite
 * gerade noch sicher zu treffen. Was nicht hineinpasst, liegt hinter „Mehr“,
 * damit kein Abschnitt unerreichbar wird.
 */
export function Unterleiste() {
  const { pathname: pfad } = useLocation();
  const [mehrOffen, setMehrOffen] = useState(false);
  const { sichtbar, schliesst } = useAusblenden(mehrOffen, AUSBLENDEN_MS);

  const mehrAktiv = UNTER_MEHR.some((eintrag) => istHier(pfad, eintrag));

  return (
    <>
      {sichtbar && (
        <>
          {/* Die Abdunklung fängt den Klick daneben ab. */}
          <button
            type="button"
            aria-label={t("Schließen")}
            onClick={() => setMehrOffen(false)}
            className={`fixed inset-0 z-40 backdrop-blur-sm ${schliesst ? "" : "animate-scrim"}`}
            style={{ background: "var(--scrim)" }}
          />
          {/* Steigt von unten auf, also von dort, wo der Knopf steht. */}
          <div
            className={`unterleiste-blatt ${schliesst ? "animate-out" : "animate-rise"}`}
          >
            <ul className="flex flex-col gap-0.5 p-2">
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
          <DotsIcon size={22} />
          <span className="truncate">{t("Mehr")}</span>
        </button>
      </nav>
    </>
  );
}
