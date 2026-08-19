import { useState } from "react";
import { version } from "../../package.json";
import { MELDESTELLE, SPENDEN } from "../lib/kontakt";
import { t } from "../lib/i18n";
import { useUi } from "../store/ui";
import { Button } from "./Modal";
import { CheckIcon, SparkIcon } from "./Icons";

/**
 * Legt Text in die Zwischenablage.
 *
 * Zwei Wege, weil der erste nicht überall gilt: `navigator.clipboard` verlangt
 * einen als sicher eingestuften Ursprung. Der ältere Weg über ein verstecktes
 * Feld ist abgekündigt, funktioniert aber noch dort, wo der neue fehlt.
 */
async function inDieAblage(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const feld = document.createElement("textarea");
    feld.value = text;
    feld.style.position = "fixed";
    feld.style.opacity = "0";
    document.body.appendChild(feld);
    feld.select();
    const gelungen = document.execCommand("copy");
    feld.remove();
    return gelungen;
  }
}

/**
 * Das System in einer lesbaren Zeile.
 *
 * `navigator.userAgent` steht voller Beiwerk, das seit dreißig Jahren
 * mitgeschleppt wird: „Mozilla/5.0 (Linux; Android 15; A063 Build/AQ3A…; wv)
 * AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/150.0.7871.181“.
 * In einem Fehlerbericht zählt davon dreierlei — welches Android, welches
 * Gerät, welcher Unterbau —, und der Rest verstellt den Blick darauf.
 *
 * Auf allen anderen Systemen steht in der ersten Klammer schon das Richtige:
 * „X11; Linux x86_64“, „Windows NT 10.0; Win64; x64“, „Macintosh; Intel Mac OS
 * X 10_15_7“.
 */
function systemZeile(): string {
  const kennung = navigator.userAgent;
  const android = /Android (\d+(?:\.\d+)*)[^)]*?; ([^;)]+?) Build\//.exec(
    kennung,
  );
  const unterbau = /Chrome\/(\d+)/.exec(kennung);

  if (android) {
    const teile = [`Android ${android[1]}`, android[2].trim()];
    if (unterbau) teile.push(`Chromium ${unterbau[1]}`);
    return teile.join(" · ");
  }

  const klammer = /\(([^)]+)\)/.exec(kennung);
  return klammer ? klammer[1] : kennung;
}

/** Adresse gekürzt, damit sie in eine Zeile passt: Anfang … Ende. */
function gekuerzt(adresse: string): string {
  if (adresse.length <= 24) return adresse;
  return `${adresse.slice(0, 10)}…${adresse.slice(-8)}`;
}

/**
 * Aufruf zum Mitmachen: Fehler melden, Vorschläge machen, spenden.
 *
 * Der Bericht wird hier zusammengestellt und in die Zwischenablage gelegt,
 * nicht verschickt. Das ist der ganze Unterschied zu einem Briefkasten: Robify
 * sendet von sich aus nichts, der Nutzer sieht, was dasteht, und entscheidet,
 * wohin er es gibt — oder ob überhaupt.
 */
export function Mitmachen({ ytdlp }: { ytdlp?: string | null }) {
  const notify = useUi((s) => s.notify);
  const [bericht, setBericht] = useState(false);

  const berichtKopieren = async () => {
    const text = t(
      "Robify {0}\nSystem: {1}\nyt-dlp: {2}\n\nWas wolltest du tun?\n\nWas ist stattdessen passiert?\n\nLässt es sich wiederholen?",
      version,
      systemZeile(),
      ytdlp || t("unbekannt"),
    );
    if (await inDieAblage(text)) {
      setBericht(true);
      notify(t("In die Zwischenablage kopiert"), "success");
      window.setTimeout(() => setBericht(false), 2000);
    } else {
      // Nicht stillschweigend nichts tun: Ein Knopf, der nichts sichtbar
      // bewirkt, sieht aus wie ein kaputter Knopf.
      notify(t("Die Zwischenablage ließ sich nicht beschreiben."), "error");
    }
  };

  const adresseKopieren = async (adresse: string) => {
    if (await inDieAblage(adresse)) {
      notify(t("In die Zwischenablage kopiert"), "success");
    } else {
      notify(t("Die Zwischenablage ließ sich nicht beschreiben."), "error");
    }
  };

  return (
    <div className="space-y-4">
      <p className="text-sm text-mute">
        {t(
          "Was fehlt, was stört, was nicht funktioniert: Rückmeldungen sind das, woraus die nächste Fassung entsteht. Robify schickt von sich aus nichts los. Der Bericht landet in der Zwischenablage, und du entscheidest, wohin er geht und was du vorher herausnimmst.",
        )}
      </p>

      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={() => void berichtKopieren()} variant="outline">
          {bericht ? <CheckIcon size={16} /> : <SparkIcon size={16} />}
          {t("Fehlerbericht vorbereiten")}
        </Button>
        {MELDESTELLE && (
          <a
            href={MELDESTELLE}
            target="_blank"
            rel="noreferrer"
            className="text-sm text-mute underline decoration-mute/40 underline-offset-2 transition hover:text-fg"
          >
            {t("Zur Meldestelle")}
          </a>
        )}
      </div>

      {SPENDEN.length > 0 && (
        <div className="space-y-2 border-t border-ink-700 pt-4">
          <p className="text-sm text-mute">
            {t(
              "Robify kostet nichts und wird es nicht. Wer trotzdem etwas dalassen möchte, kann das hier tun; die Adresse geht in die Zwischenablage, alles Weitere passiert in deiner Wallet.",
            )}
          </p>
          {SPENDEN.map((weg) => (
            <button
              key={weg.name}
              type="button"
              onClick={() => void adresseKopieren(weg.adresse)}
              title={weg.adresse}
              className="flex w-full items-center gap-3 rounded-xl p-2.5 text-start transition hover:bg-ink-800"
            >
              <span className="w-24 shrink-0 text-sm font-medium">
                {weg.name}
              </span>
              <code className="min-w-0 flex-1 truncate rounded bg-ink-900 px-2 py-1 text-xs">
                {gekuerzt(weg.adresse)}
              </code>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
