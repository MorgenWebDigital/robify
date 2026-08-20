import { useEffect, useState, useSyncExternalStore } from "react";
import { api, errorMessage } from "../lib/api";
import { t } from "../lib/i18n";
import { useUi } from "../store/ui";
import type { Aktualisierungen } from "../types";
import { Button, Modal } from "./Modal";
import { DownloadIcon } from "./Icons";

// tells that there is something newer, and nothing else.
//
// nothing is visible while everything is up to date. that is the whole point:
// a place that always shows something has to be read every time, one that is
// empty most of the time is understood at a glance.
//
// two things are watched over, and they are renewed in different ways. yt-dlp
// renews itself, robify does not: an installed app cannot exchange its own
// files without going behind the back of a package manager, and the honest
// way is therefore the page the new version lies on.

// --- the one answer for the whole app ---
//
// the sidebar and the sheet of the bottom bar both want it, and both exist at
// the same time: the one is hidden on a phone, the other on a desktop, but
// hidden means invisible and not absent. without a shared place each of them
// would ask on its own, and a renewal in the one would not be noticed by the
// other.
let stand: Aktualisierungen | null = null;
let gefragt = false;
const horcher = new Set<() => void>();

function setzen(neu: Aktualisierungen | null) {
  stand = neu;
  for (const melden of horcher) melden();
}

function abonnieren(melden: () => void) {
  horcher.add(melden);
  return () => {
    horcher.delete(melden);
  };
}

function lesen(): Aktualisierungen | null {
  return stand;
}

/**
 * what is to be had, asked once for the whole run.
 *
 * a failure stays silent, unlike everywhere else in the app: nobody asked for
 * this check, and a network that is down would otherwise complain at every
 * start about something no one can do anything about. it is asked again at
 * the next start.
 */
export function useAktualisierungen(): Aktualisierungen | null {
  const wert = useSyncExternalStore(abonnieren, lesen, lesen);

  useEffect(() => {
    if (gefragt) return;
    gefragt = true;
    api
      .aktualisierungenPruefen()
      .then(setzen)
      .catch(() => {
        gefragt = false;
      });
  }, []);

  return wert;
}

/**
 * the colour of the marking, in every appearance.
 *
 * deliberately not the accent colour. that one is freely chosen, and the
 * default is a grey that vanishes on a dark surface — the marking would then
 * be invisible for exactly those who never touched the setting. a signal has
 * to be a signal whatever else is set.
 *
 * it stands in the stylesheet with the other colours and not as a number
 * here: whoever wants to change it looks there, where every colour of the
 * app is.
 */
export const SIGNAL = "var(--hinweis)";

/** whether anything at all is to be had. */
export function etwasNeues(was: Aktualisierungen | null): boolean {
  return Boolean(was?.app || was?.ytdlp);
}

// --- the two ways in ---

/** the key in the foot of the sidebar, on a desktop. */
export function Aktualisierungsknopf() {
  const was = useAktualisierungen();
  const [offen, setOffen] = useState(false);

  if (!etwasNeues(was)) return null;

  return (
    <>
      <button
        type="button"
        onClick={() => setOffen(true)}
        title={t("Aktualisierung verfügbar")}
        aria-label={t("Aktualisierung verfügbar")}
        // the same build as the gear beside it: the rounded square of the
        // navigation, and `raised-row` holds it in the raised state the gear
        // only takes on the page it belongs to. a whole surface in a signal
        // colour would shout louder than the play button, and nothing here is
        // more urgent than that one — the colour therefore sits on the sign
        className="nav-item raised-row justify-center p-2"
        style={{ color: SIGNAL }}
      >
        <DownloadIcon size={18} />
      </button>
      <Aktualisierungsfenster
        offen={offen}
        schliessen={() => setOffen(false)}
      />
    </>
  );
}

/**
 * the row in the sheet behind "more", on a phone.
 *
 * there is no sidebar there, and the settings sit in that sheet. the row
 * stands above them, in the same build as every entry of the sheet, only with
 * the accent on the sign.
 *
 * the window is not built here but by the bottom bar. tapping the row closes
 * the sheet, the sheet unmounts, and a window standing inside it would be
 * taken along before it could be seen.
 */
export function Aktualisierungszeile({ oeffnen }: { oeffnen: () => void }) {
  const was = useAktualisierungen();

  if (!etwasNeues(was)) return null;

  return (
    <li>
      <button
        type="button"
        onClick={oeffnen}
        className="nav-item w-full gap-3 px-3 py-3 text-sm font-medium"
        style={{ color: SIGNAL }}
      >
        <DownloadIcon size={20} />
        {t("Aktualisierung verfügbar")}
      </button>
    </li>
  );
}

// --- what stands in the window ---

export function Aktualisierungsfenster({
  offen,
  schliessen,
}: {
  offen: boolean;
  schliessen: () => void;
}) {
  const notify = useUi((s) => s.notify);
  const was = useAktualisierungen();
  const [laeuft, setLaeuft] = useState(false);
  const [mehr, setMehr] = useState(false);
  const [notizen, setNotizen] = useState<string | null>(null);

  // the notes are fetched at the click on "show more" and not before: the
  // check runs at every start and is to stay cheap, this is one question over
  // the api of github and it is asked rarely
  useEffect(() => {
    if (!mehr || !was?.app || notizen !== null) return;
    api
      .aktualisierungsnotizen()
      .then((text) => setNotizen(text ?? ""))
      .catch(() => setNotizen(""));
  }, [mehr, was?.app, notizen]);

  // one key for everything that is pending.
  //
  // the two are renewed in different ways — yt-dlp exchanges itself, robify
  // cannot and points at its page — but that is our business and not the
  // user's. they want it up to date, and one key says so.
  const aktualisieren = async () => {
    setLaeuft(true);
    try {
      if (was?.ytdlp) {
        const erneuert = await api.ytdlpAktualisieren();
        notify(
          erneuert.eigeneKopie
            ? // the found yt-dlp belonged to pip or to a package manager and
              // would not renew itself. saying so matters: from now on robify
              // works with a different file than before
              t(
                "yt-dlp gehörte einer fremden Verwaltung und erneuerte sich nicht selbst. Robify benutzt ab jetzt eine eigene Kopie, Fassung {0}.",
                erneuert.fassung,
              )
            : t("yt-dlp steht jetzt auf {0}.", erneuert.fassung),
          "success",
        );
        if (stand) setzen({ ...stand, ytdlp: null });
      }

      if (was?.app) {
        // robify cannot exchange its own files without going behind the back
        // of the manager that put them there. the honest step is the page the
        // new version lies on
        await api.releaseSeiteOeffnen();
      }

      schliessen();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setLaeuft(false);
    }
  };

  return (
    <Modal
      open={offen}
      title={t("Aktualisierung verfügbar")}
      onClose={schliessen}
      width="max-w-md"
      footer={
        <>
          <Button onClick={schliessen} variant="ghost">
            {t("Schließen")}
          </Button>
          <Button
            onClick={() => void aktualisieren()}
            variant="primary"
            disabled={laeuft}
          >
            {laeuft ? t("Holt…") : t("Jetzt aktualisieren")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        {/* short and nothing else: what is renewed, and to which version. */}
        <ul className="space-y-1.5">
          {was?.app && <Zeile name={t("Robify")} neu={was.app.neu} />}
          {was?.ytdlp && <Zeile name={t("yt-dlp")} neu={was.ytdlp.neu} />}
        </ul>

        <button
          type="button"
          onClick={() => setMehr((offen) => !offen)}
          className="text-sm text-mute underline decoration-mute/40 underline-offset-2 transition hover:text-fg"
        >
          {mehr ? t("Weniger anzeigen") : t("Mehr anzeigen")}
        </button>

        {mehr && (
          <div className="space-y-3 rounded-xl bg-ink-800 p-4 text-sm text-mute">
            {was?.app && (
              <div className="space-y-2">
                <p>
                  {t(
                    "Robify kann sich nicht selbst austauschen. Die neue Fassung liegt zum Herunterladen bereit.",
                  )}
                </p>
                {notizen === null ? (
                  <p>{t("Holt…")}</p>
                ) : notizen ? (
                  // as it stands in the changelog: line breaks kept, nothing
                  // interpreted. a text from the net is not to become markup
                  // here
                  <p className="whitespace-pre-wrap">{notizen}</p>
                ) : (
                  <p>{t("Zu dieser Fassung liegen keine Angaben vor.")}</p>
                )}
              </div>
            )}
            {was?.ytdlp && (
              <p>
                {t(
                  "YouTube weist alte Fassungen mit „403“ ab. Hilft eine Aktualisierung nicht, liegt es an der Quelle.",
                )}
              </p>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}

/** one line: what is renewed, and to which version. */
function Zeile({ name, neu }: { name: string; neu: string }) {
  return (
    <li className="flex items-baseline gap-2">
      <span className="font-semibold">{name}</span>
      <span className="text-sm text-mute">{neu}</span>
    </li>
  );
}
