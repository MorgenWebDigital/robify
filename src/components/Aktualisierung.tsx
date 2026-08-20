import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { t } from "../lib/i18n";
import { useUi } from "../store/ui";
import type { Aktualisierungen } from "../types";
import { Button, Modal } from "./Modal";
import { DownloadIcon } from "./Icons";

// tells that there is something newer, and nothing else.
//
// the button is not there while everything is up to date. that is the whole
// point of it: a place that always shows something has to be read every time,
// one that is empty most of the time is understood at a glance.
//
// two things are watched over, and they are renewed in different ways. yt-dlp
// renews itself, robify does not: an installed app cannot exchange its own
// files without a package manager, and the honest way is therefore the page
// the new version lies on.
// what the last check found, for as long as the app is running.
//
// the effect below runs again at every fresh mount, and there are more of
// those than one thinks: switching the language rebuilds the whole app, and
// during development every saved file does. github does not mind being asked,
// but asking the same question three times in a minute is answered three
// times with the same thing.
let gemerkt: Promise<Aktualisierungen> | null = null;

function einmalPruefen(): Promise<Aktualisierungen> {
  gemerkt ??= api.aktualisierungenPruefen();
  return gemerkt;
}

export function Aktualisierungsknopf() {
  const notify = useUi((s) => s.notify);
  const [stand, setStand] = useState<Aktualisierungen | null>(null);
  const [offen, setOffen] = useState(false);
  const [holt, setHolt] = useState(false);

  // asked once at the start and never again.
  //
  // a failure stays silent here, unlike everywhere else in the app: nobody
  // asked for this check, and a network that is down would otherwise complain
  // at every start about something the user cannot do anything about anyway.
  useEffect(() => {
    let gilt = true;
    einmalPruefen()
      .then((was) => {
        if (gilt) setStand(was);
      })
      .catch(() => {
        // asked once more at the next start: a failure is not to stick to the
        // running app
        gemerkt = null;
      });
    return () => {
      gilt = false;
    };
  }, []);

  const ytdlpHolen = async () => {
    setHolt(true);
    try {
      const fassung = await api.ytdlpAktualisieren();
      notify(t("yt-dlp steht jetzt auf {0}.", fassung), "success");
      // gone from the list, and with it possibly the button: what has been
      // fetched is not to keep offering itself
      setStand((vorher) => {
        const neuer = vorher ? { ...vorher, ytdlp: null } : vorher;
        if (neuer) gemerkt = Promise.resolve(neuer);
        return neuer;
      });
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

  if (!stand?.app && !stand?.ytdlp) return null;

  return (
    <>
      <button
        type="button"
        onClick={() => setOffen(true)}
        title={t("Aktualisierung verfügbar")}
        aria-label={t("Aktualisierung verfügbar")}
        // the same build as the gear beside it: the rounded square of the
        // navigation, and `raised-row` holds it in the raised state the gear
        // only takes on the page it belongs to. a whole surface in the accent
        // colour would shout louder than the play button, and nothing here is
        // more urgent than that one — the accent therefore sits on the sign
        className="nav-item raised-row justify-center p-2"
        style={{ color: "var(--accent)" }}
      >
        <DownloadIcon size={18} />
      </button>

      <Modal
        open={offen}
        title={t("Aktualisierung verfügbar")}
        onClose={() => setOffen(false)}
        width="max-w-md"
        footer={
          <Button onClick={() => setOffen(false)} variant="ghost">
            {t("Schließen")}
          </Button>
        }
      >
        <div className="space-y-4">
          {stand.app && (
            <Zeile
              name={t("Robify")}
              jetzt={stand.app.jetzt}
              neu={stand.app.neu}
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
          {stand.ytdlp && (
            <Zeile
              name={t("yt-dlp")}
              jetzt={stand.ytdlp.jetzt}
              neu={stand.ytdlp.neu}
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
    </>
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
