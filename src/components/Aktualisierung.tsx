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

/** what is to be had, and what is to be done about it. */
export function Aktualisierungsfenster({
  offen,
  schliessen,
}: {
  offen: boolean;
  schliessen: () => void;
}) {
  const notify = useUi((s) => s.notify);
  const was = useAktualisierungen();
  const [holt, setHolt] = useState(false);

  const ytdlpHolen = async () => {
    setHolt(true);
    try {
      const fassung = await api.ytdlpAktualisieren();
      notify(t("yt-dlp steht jetzt auf {0}.", fassung), "success");
      // gone from the list, and with it possibly the way in: what has been
      // fetched is not to keep offering itself
      if (stand) setzen({ ...stand, ytdlp: null });
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setHolt(false);
    }
  };

  const seiteOeffnen = async () => {
    try {
      await api.releaseSeiteOeffnen();
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  return (
    <Modal
      open={offen}
      title={t("Aktualisierung verfügbar")}
      onClose={schliessen}
      width="max-w-md"
      footer={
        <Button onClick={schliessen} variant="ghost">
          {t("Schließen")}
        </Button>
      }
    >
      <div className="space-y-4">
        {was?.app && (
          <Zeile
            name={t("Robify")}
            jetzt={was.app.jetzt}
            neu={was.app.neu}
            hinweis={t(
              "Robify kann sich nicht selbst austauschen. Die neue Fassung liegt zum Herunterladen bereit.",
            )}
            knopf={
              <Button onClick={() => void seiteOeffnen()} variant="primary">
                {t("Zur Veröffentlichung")}
              </Button>
            }
          />
        )}
        {was?.ytdlp && (
          <Zeile
            name={t("yt-dlp")}
            jetzt={was.ytdlp.jetzt}
            neu={was.ytdlp.neu}
            hinweis={t(
              "YouTube weist alte Fassungen mit „403“ ab. Hilft eine Aktualisierung nicht, liegt es an der Quelle.",
            )}
            knopf={
              <Button
                onClick={() => void ytdlpHolen()}
                variant="primary"
                disabled={holt}
              >
                {holt ? t("Holt…") : t("Jetzt erneuern")}
              </Button>
            }
          />
        )}
      </div>
    </Modal>
  );
}

/** one of the two, with both versions and what is to be done about it. */
function Zeile({
  name,
  jetzt,
  neu,
  hinweis,
  knopf,
}: {
  name: string;
  jetzt: string;
  neu: string;
  hinweis: string;
  knopf: React.ReactNode;
}) {
  return (
    <div className="space-y-2 rounded-xl bg-ink-800 p-4">
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span className="font-semibold">{name}</span>
        <span className="text-sm text-mute">
          {t("{0} statt {1}", neu, jetzt)}
        </span>
      </div>
      <p className="text-sm text-mute">{hinweis}</p>
      <div className="flex">{knopf}</div>
    </div>
  );
}
