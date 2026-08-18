import { useEffect, useMemo, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { formatLrc } from "../lib/lrc";
import { api } from "../lib/api";
import { formatTime } from "../lib/format";
import { usePlayer } from "../store/player";
import { CloseIcon, PauseIcon, PlayIcon, PlusIcon, PrevIcon } from "./Icons";
import { Button } from "./Modal";
import type { Track } from "../types";

/** Übliche Reaktionszeit beim Mittippen. Wird von jeder Marke abgezogen. */
const STANDARD_VERSATZ = -250;

/**
 * Zeilen, die beim Kopieren von Lyrics-Seiten mitkommen, aber nicht gesungen
 * werden: Abschnittsmarken wie „[Refrain]“, Kopfzeilen mit dem Songnamen,
 * Beitragszähler und der Rest, den solche Seiten anhängen.
 *
 * Nur eine Vorauswahl, jede Zeile lässt sich von Hand wieder aufnehmen.
 */
function wirktWieBeiwerk(zeile: string): boolean {
  const t = zeile.trim();
  if (t.startsWith("[") && t.endsWith("]")) return true;
  if (/contributors?\b/i.test(t)) return true;
  if (/\blyrics$/i.test(t)) return true;
  if (/^\d*\s*embed$/i.test(t)) return true;
  if (/^(read more|see all|translations?)\b/i.test(t)) return true;
  return false;
}

/**
 * Macht aus reinem Text mitlaufende Lyrics, indem der Nutzer beim Hören
 * mittippt.
 *
 * Automatisch ginge das nur mit Spracherkennung. Tippen ist der ehrliche Weg:
 * Wer mitliest, weiß auf die Zehntelsekunde, wann eine Zeile beginnt.
 */
export function LyricsSync({
  track,
  plain,
  onDone,
  onCancel,
}: {
  track: Track;
  plain: string;
  /** Bekommt den LRC-Text und den um das Beiwerk bereinigten Fließtext. */
  onDone: (lrc: string, bereinigt: string) => void;
  onCancel: () => void;
}) {
  const positionMs = usePlayer((s) => s.positionMs);
  const durationMs = usePlayer((s) => s.durationMs);
  const playing = usePlayer((s) => s.playing);
  const currentTrack = usePlayer((s) => s.currentTrack);
  const toggle = usePlayer((s) => s.toggle);
  const seek = usePlayer((s) => s.seek);

  // Leerzeilen tragen keine Zeitmarke, sie trennen nur Strophen.
  const zeilen = useMemo(
    () =>
      plain
        .split(/\r?\n/)
        .map((t) => t.trim())
        .filter((t) => t.length > 0),
    [plain],
  );

  const [marken, setMarken] = useState<(number | null)[]>(() =>
    zeilen.map(() => null),
  );
  const [aus, setAus] = useState<boolean[]>(() => zeilen.map(wirktWieBeiwerk));
  const [versatz, setVersatz] = useState(STANDARD_VERSATZ);
  const aktiveZeile = useRef<HTMLLIElement>(null);

  const laeuftDieser = currentTrack?.id === track.id;

  /** Nächste Zeile, die eine Marke braucht: nicht abgewählt, noch ohne Zeit. */
  const dran = marken.findIndex((zeit, i) => zeit === null && !aus[i]);
  const fertig = dran === -1;
  const gesetzt = marken.filter((z, i) => z !== null && !aus[i]).length;
  const offen = aus.filter((a) => !a).length;

  // Die Zeile, die als Nächstes drankommt, bleibt in der Mitte.
  useEffect(() => {
    aktiveZeile.current?.scrollIntoView({
      block: "center",
      behavior: "smooth",
    });
  }, [dran]);

  const setzen = () => {
    if (fertig || !laeuftDieser) return;
    setMarken((bisher) =>
      bisher.map((zeit, i) => (i === dran ? positionMs : zeit)),
    );
  };

  const zurueck = () => {
    setMarken((bisher) => {
      // Die zuletzt gesetzte Zeile ist die letzte mit Zeit vor der aktuellen.
      const letzte = bisher.reduce<number>(
        (merker, zeit, i) => (zeit !== null && !aus[i] ? i : merker),
        -1,
      );
      if (letzte === -1) return bisher;
      const zeitDavor = bisher.reduce(
        (merker, zeit, i) =>
          i < letzte && zeit !== null && !aus[i] ? zeit : merker,
        null as number | null,
      );
      void seek(zeitDavor ?? 0);
      return bisher.map((zeit, i) => (i === letzte ? null : zeit));
    });
  };

  /** Zeile abwählen oder wieder aufnehmen. Abgewählte verlieren ihre Marke. */
  const umschalten = (index: number) => {
    setAus((bisher) => bisher.map((wert, i) => (i === index ? !wert : wert)));
    setMarken((bisher) => bisher.map((zeit, i) => (i === index ? null : zeit)));
  };

  const vonVorn = async () => {
    setMarken(zeilen.map(() => null));
    await seek(0);
    if (!playing) await toggle();
  };

  // Leertaste stempelt. Der Player hört sie sonst als Abspielen/Pause, deshalb
  // wird sie abgefangen, solange dieser Bereich offen ist.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === " " || event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        setzen();
      } else if (event.key === "Backspace") {
        event.preventDefault();
        event.stopPropagation();
        zurueck();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  const uebernehmen = () => {
    const behalten = zeilen
      .map((text, i) => ({ text, zeit: marken[i], aus: aus[i] }))
      .filter((z) => !z.aus);

    onDone(
      formatLrc(
        behalten
          .filter(
            (z): z is { text: string; zeit: number; aus: boolean } =>
              z.zeit !== null,
          )
          .map((z) => ({
            timeMs: Math.max(0, z.zeit + versatz),
            text: z.text,
          })),
      ),
      // Der Fließtext verliert das Beiwerk ebenfalls: Ohne Zeitmarken zeigt
      // der Player ihn, und dort hätte es genauso wenig verloren.
      behalten.map((z) => z.text).join("\n"),
    );
  };

  if (zeilen.length === 0) {
    return (
      <div className="space-y-4">
        <p className="text-sm text-mute">
          {t(
            "Für das Takten braucht es erst den Text. Trage ihn unter „Einfacher Text“ ein, dann kannst du ihn hier mit Zeitmarken versehen.",
          )}
        </p>
        <Button onClick={onCancel} variant="ghost">
          {t("Zurück")}
        </Button>
      </div>
    );
  }

  if (!laeuftDieser) {
    return (
      <div className="space-y-4">
        <p className="text-sm text-mute">
          {t(
            "Getaktet wird zur laufenden Wiedergabe: Du hörst den Titel und tippst bei jedem Zeilenanfang. Starte ihn dafür.",
          )}
        </p>
        <Button
          onClick={() => void api.playTracks([track.id], 0)}
          variant="primary"
        >
          <PlayIcon size={16} />
          {t("„{0}“ abspielen", track.title)}
        </Button>
        <Button onClick={onCancel} variant="ghost" className="ms-2">
          {t("Abbrechen")}
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <p className="text-sm text-mute">
        {t(
          "Abschnittsmarken und Beiwerk sind vorab abgewählt. Über das Kreuz rechts nimmst du weitere Zeilen heraus oder wieder auf. Abgewählte bekommen keine Zeitmarke und tauchen im Player nicht auf.",
        )}
      </p>

      {/* Der ganze Text, die nächste Zeile mittig. So sieht man, was kommt. */}
      <ol className="h-56 space-y-1 overflow-y-auto rounded-xl border border-ink-700 bg-ink-950 px-3 py-3 text-sm">
        {zeilen.map((text, index) => {
          const abgewaehlt = aus[index];
          const zeit = marken[index];
          const jetzt = index === dran;
          return (
            <li
              key={index}
              ref={jetzt ? aktiveZeile : undefined}
              className={`group flex items-center gap-3 rounded-lg px-1.5 py-0.5 ${
                abgewaehlt
                  ? "text-mute/40 line-through"
                  : jetzt
                    ? "text-base font-semibold"
                    : zeit !== null
                      ? "text-mute"
                      : "text-mute/70"
              }`}
              style={
                jetzt && !abgewaehlt ? { color: "var(--accent)" } : undefined
              }
            >
              <span className="w-14 shrink-0 text-xs tabular-nums text-mute">
                {zeit !== null && !abgewaehlt
                  ? formatTime(Math.max(0, zeit + versatz))
                  : ""}
              </span>
              <span className="min-w-0 flex-1 truncate" title={text}>
                {text}
              </span>
              <button
                type="button"
                onClick={() => umschalten(index)}
                aria-label={
                  abgewaehlt ? t("Zeile wieder aufnehmen") : t("Zeile abwählen")
                }
                title={abgewaehlt ? t("Wieder aufnehmen") : t("Abwählen")}
                className="icon-btn shrink-0 opacity-0 group-hover:opacity-100 aria-[pressed]:opacity-100"
                aria-pressed={abgewaehlt}
              >
                {abgewaehlt ? <PlusIcon size={14} /> : <CloseIcon size={14} />}
              </button>
            </li>
          );
        })}
      </ol>

      {/* Wo im Stück wir sind. Klicken springt, falls eine Stelle wiederholt
          werden soll. */}
      <div>
        <div
          className="h-1.5 w-full cursor-pointer overflow-hidden rounded-full bg-ink-700"
          onClick={(event) => {
            const kasten = event.currentTarget.getBoundingClientRect();
            const anteil = (event.clientX - kasten.left) / kasten.width;
            void seek(Math.round(anteil * durationMs));
          }}
        >
          <div
            className="h-full rounded-full"
            style={{
              width: `${durationMs ? (positionMs / durationMs) * 100 : 0}%`,
              background: "var(--accent)",
            }}
          />
        </div>
        <div className="mt-1 flex justify-between text-[11px] tabular-nums text-mute">
          <span>{formatTime(positionMs)}</span>
          <span>{t("{0} von {1} Zeilen", gesetzt, offen)}</span>
          <span>{formatTime(durationMs)}</span>
        </div>
      </div>

      {/* Die eine Taste, um die es geht. */}
      <button
        type="button"
        onClick={setzen}
        disabled={fertig}
        className="pill-btn is-raised is-accent h-14 w-full text-base font-semibold"
      >
        {fertig ? t("Alle Zeilen gesetzt") : t("Zeile setzen oder Leertaste")}
      </button>

      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => void toggle()} variant="outline">
          {playing ? <PauseIcon size={16} /> : <PlayIcon size={16} />}
          {playing ? t("Pause") : t("Weiter")}
        </Button>
        <Button
          onClick={() => void seek(Math.max(0, positionMs - 5000))}
          variant="outline"
        >
          <PrevIcon size={16} />5 s
        </Button>
        <Button onClick={zurueck} variant="ghost" disabled={gesetzt === 0}>
          {t("Rückgängig")}
        </Button>
        <Button
          onClick={() => void vonVorn()}
          variant="ghost"
          disabled={gesetzt === 0}
        >
          {t("Von vorn")}
        </Button>
      </div>

      {/* Getippt wird immer ein Stück zu spät. Statt jede Marke einzeln zu
          korrigieren, verschiebt dieser Wert alle zusammen. */}
      <div className="flex flex-wrap items-center gap-2 text-sm text-mute">
        <span>{t("Feinjustierung")}</span>
        <Button onClick={() => setVersatz((v) => v - 100)} variant="outline">
          {t("früher")}
        </Button>
        <span className="w-20 text-center tabular-nums">
          {(versatz / 1000).toFixed(1)} s
        </span>
        <Button onClick={() => setVersatz((v) => v + 100)} variant="outline">
          {t("später")}
        </Button>
        <span className="flex-1" />
        <Button onClick={onCancel} variant="ghost">
          {t("Abbrechen")}
        </Button>
        <Button
          onClick={uebernehmen}
          variant="primary"
          disabled={gesetzt === 0}
        >
          {fertig ? t("Übernehmen") : t("{0} übernehmen", gesetzt)}
        </Button>
      </div>
    </div>
  );
}
