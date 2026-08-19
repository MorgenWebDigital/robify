import { useEffect, useMemo, useState } from "react";
import { t } from "../lib/i18n";
import { version } from "../../package.json";
import { Button, Modal } from "./Modal";
import { SearchIcon } from "./Icons";

interface Paket {
  name: string;
  version: string;
  lizenz: string;
  quelle: string | null;
  herkunft: "Rust" | "npm";
  textIds: number[];
}

interface Lizenzdaten {
  erzeugt: string;
  pakete: Paket[];
  texte: string[];
}

/**
 * a notice about foreign content, for the downloader page.
 *
 * it stands there and not in the readme alone: a warning is read where the
 * action happens. whoever downloads is to know that the responsibility lies
 * with them.
 */
export function DownloadHinweis() {
  return (
    <p className="mt-6 text-xs leading-relaxed text-mute">
      {t(
        "Robify ist ein Werkzeug ohne eigene Inhalte. Ob du eine bestimmte Aufnahme herunterladen darfst, richtet sich nach dem Urheberrecht und den Bedingungen der jeweiligen Plattform. Das liegt in deiner Verantwortung. Die Metadatensuche liest öffentlich erreichbare Seiten von Spotify, Genius und anderen aus; deren Nutzungsbedingungen erlauben das in der Regel nicht.",
      )}
    </p>
  );
}

// the footer of the settings page.
//
// deliberately no card like the sections above it but a calm row under a
// hairline, like the footer of a web page. the legal part is to be findable
// without competing with the switches one actually comes here for
export function RechtlichesFuss() {
  const [offen, setOffen] = useState(false);

  return (
    <footer className="mt-10 border-t border-ink-700 pt-6 pb-4 text-center">
      <p className="text-xs text-mute">
        <span className="font-medium text-fg/70">Robify {version}</span>
        <Punkt />
        {/* the name of the licence, not translated: it is a name, as
            "Robify" is. what it means stands in the licence and the
            readme. */}
        PolyForm Noncommercial
        <Punkt />
        <button
          type="button"
          onClick={() => setOffen(true)}
          className="underline decoration-mute/40 underline-offset-2 transition hover:text-fg"
        >
          {t("Verwendete Bibliotheken")}
        </button>
      </p>

      <p className="mx-auto mt-3 max-w-xl text-[11px] leading-relaxed text-mute/70">
        {t(
          "Robify liefert keine Inhalte mit. Ob du eine bestimmte Aufnahme herunterladen darfst, richtet sich nach dem Urheberrecht und den Bedingungen der Plattform. Es werden keine Nutzungsdaten erhoben und nichts an Dritte gesendet außer den Abfragen, die du selbst auslöst.",
        )}
      </p>

      <LizenzenDialog open={offen} onClose={() => setOffen(false)} />
    </footer>
  );
}

/** the separator dot between the entries in the footer. */
function Punkt() {
  return <span className="mx-2 text-mute/40">·</span>;
}

function LizenzenDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [daten, setDaten] = useState<Lizenzdaten | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [suche, setSuche] = useState("");
  const [ausgeklappt, setAusgeklappt] = useState<string | null>(null);

  // loaded on opening only: the file is close to a megabyte and is never
  // needed in the vast majority of sessions
  useEffect(() => {
    if (!open || daten || fehler) return;
    let abgebrochen = false;
    void fetch("/lizenzen.json")
      .then((antwort) => {
        if (!antwort.ok)
          throw new Error(`${antwort.status} ${antwort.statusText}`);
        return antwort.json() as Promise<Lizenzdaten>;
      })
      .then((wert) => {
        if (!abgebrochen) setDaten(wert);
      })
      .catch((ursache) => {
        if (!abgebrochen) setFehler(String(ursache));
      });
    return () => {
      abgebrochen = true;
    };
  }, [open, daten, fehler]);

  const gefiltert = useMemo(() => {
    if (!daten) return [];
    const begriff = suche.trim().toLowerCase();
    if (!begriff) return daten.pakete;
    return daten.pakete.filter(
      (p) =>
        p.name.toLowerCase().includes(begriff) ||
        p.lizenz.toLowerCase().includes(begriff),
    );
  }, [daten, suche]);

  const ohneText =
    daten?.pakete.filter((p) => p.textIds.length === 0).length ?? 0;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("Verwendete Bibliotheken")}
      subtitle={
        daten
          ? `${daten.pakete.length} Pakete · Stand ${daten.erzeugt}`
          : fehler
            ? undefined
            : t("wird geladen…")
      }
      width="max-w-3xl"
      footer={
        <Button onClick={onClose} variant="ghost">
          {t("Schließen")}
        </Button>
      }
    >
      {fehler ? (
        <p className="text-sm text-mute">
          {t(
            "Die Lizenzliste konnte nicht geladen werden ({0}). Sie entsteht beim Bauen über npm run lizenzen.",
            fehler,
          )}
        </p>
      ) : !daten ? (
        <p className="text-sm text-mute">{t("Einen Moment…")}</p>
      ) : (
        <div className="space-y-4">
          <p className="text-xs leading-relaxed text-mute">
            {t(
              "Robify wird mit diesen Bibliotheken ausgeliefert. Ihre Lizenzen, ganz überwiegend MIT und Apache-2.0, verlangen, dass Urheberrechtsvermerk und Lizenztext mitgeliefert werden; das geschieht hier.",
            )}
            {ohneText > 0 && (
              <>
                {" "}
                {t(
                  "Bei {0} Paketen liegt dem veröffentlichten Archiv kein Lizenztext bei; dort stehen die Lizenzangabe und der Verweis auf die Quelle.",
                  ohneText,
                )}
              </>
            )}
          </p>

          <div className="relative">
            <SearchIcon
              size={16}
              className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
            />
            <input
              value={suche}
              onChange={(event) => setSuche(event.target.value)}
              placeholder={t("Paket oder Lizenz suchen")}
              className="search-field ps-10"
            />
          </div>

          {gefiltert.length === 0 ? (
            <p className="py-6 text-center text-sm text-mute">
              {t("Nichts passt zu „{0}“.", suche)}
            </p>
          ) : (
            <ul className="space-y-1">
              {gefiltert.map((paket) => {
                const schluessel = `${paket.herkunft}:${paket.name}@${paket.version}`;
                const offen = ausgeklappt === schluessel;
                return (
                  <li
                    key={schluessel}
                    className="rounded-lg border border-ink-700"
                  >
                    <button
                      type="button"
                      onClick={() => setAusgeklappt(offen ? null : schluessel)}
                      className="flex w-full items-center gap-3 px-3 py-2 text-start"
                    >
                      <span className="min-w-0 flex-1 truncate text-sm">
                        {paket.name}
                        <span className="text-mute"> {paket.version}</span>
                      </span>
                      <span className="shrink-0 text-xs text-mute">
                        {paket.lizenz}
                      </span>
                      <span className="shrink-0 text-[10px] tracking-wider text-mute/60 uppercase">
                        {paket.herkunft}
                      </span>
                    </button>

                    {offen && (
                      <div className="border-t border-ink-700 px-3 py-3">
                        {paket.quelle && (
                          <p
                            className="mb-2 truncate text-xs text-mute"
                            title={paket.quelle}
                          >
                            {paket.quelle}
                          </p>
                        )}
                        {paket.textIds.length > 0 ? (
                          paket.textIds.map((id) => (
                            <pre
                              key={id}
                              className="max-h-72 overflow-auto rounded bg-ink-900 p-3 text-[11px] leading-relaxed whitespace-pre-wrap text-mute"
                            >
                              {daten.texte[id]}
                            </pre>
                          ))
                        ) : (
                          <p className="text-xs text-mute">
                            {t(
                              "Dem veröffentlichten Paket liegt kein Lizenztext bei. Es gilt {0}; der Wortlaut steht in der oben genannten Quelle.",
                              paket.lizenz,
                            )}
                          </p>
                        )}
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </Modal>
  );
}
