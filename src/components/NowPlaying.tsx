import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { t } from "../lib/i18n";
import { api, fallback } from "../lib/api";
import { albumCover, artistImage } from "../lib/cover";
import { formatTime, releaseLabel } from "../lib/format";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  LyricsIcon,
  QueueIcon,
} from "./Icons";
import { LyricsPanel } from "./LyricsPanel";
import { useAusblenden } from "../lib/ausblenden";

/** Muss zur Dauer von `.animate-stage-out` im Stylesheet passen. */
const ZU_MS = 280;

/** Vollbildansicht mit großem Cover und mitlaufenden Lyrics. */
export function NowPlaying() {
  const currentTrack = usePlayer((s) => s.currentTrack);
  const positionMs = usePlayer((s) => s.positionMs);
  const durationMs = usePlayer((s) => s.durationMs);
  const { nowPlayingOpen, setNowPlayingOpen, setQueueOpen } = useUi();
  const { sichtbar, schliesst } = useAusblenden(nowPlayingOpen, ZU_MS);
  /* Nur am Telefon: Dort steht der Text nicht daneben, sondern hinter einer
     Kachel. Am Rechner ist er ohnehin die ganze Zeit zu sehen. */
  const [textOffen, setTextOffen] = useState(false);
  const [anfang, setAnfang] = useState<string[]>([]);

  const trackId = currentTrack?.id ?? null;

  /*
   * Die ersten Zeilen für die Kachel.
   *
   * Ohne sie stünde dort nur „Songtext“, und man müsste tippen, um zu sehen,
   * ob überhaupt einer hinterlegt ist. Zwei Zeilen sagen das auf einen Blick.
   * Der volle Text wird davon nicht doppelt geladen: Die Vollbildansicht holt
   * ihn selbst, und der Befehl liest aus der Datenbank, nicht aus dem Netz.
   */
  useEffect(() => {
    if (!trackId) {
      setAnfang([]);
      return;
    }
    let gilt = true;
    void api
      .getLyrics(trackId)
      .catch(fallback(null, t("Lyrics")))
      .then((lyrics) => {
        if (!gilt) return;
        const roh = lyrics?.plain || lyrics?.synced || "";
        setAnfang(
          roh
            // Zeitmarken einer synchronen Fassung gehören nicht in die Vorschau.
            .replace(/\[\d{1,2}:\d{2}(?:[.:]\d{1,3})?\]/g, "")
            .split("\n")
            .map((zeile) => zeile.trim())
            .filter(Boolean)
            .slice(0, 2),
        );
      });
    return () => {
      gilt = false;
    };
  }, [trackId]);

  // Beim Titelwechsel schließt sich der Vollbildtext: Er gehörte zum vorigen.
  useEffect(() => setTextOffen(false), [trackId]);

  if (!sichtbar) return null;

  const progress = durationMs ? (positionMs / durationMs) * 100 : 0;

  return (
    <div
      className={`absolute inset-0 z-20 flex flex-col bg-frame p-3 ${
        schliesst ? "animate-stage-out" : "animate-stage"
      }`}
    >
      {/* Eine einzige Fläche hinter allem: Hintergrund und Schimmer wie bei der
          Player-Pille unten, Cover und Lyrics liegen gemeinsam darauf. */}
      <div className="sunken-deep flex min-h-0 flex-1 flex-col rounded-3xl bg-ink-950">
        {/* Ohne Beschriftung, dass gerade gespielt wird, sieht man. */}
        <div className="flex items-center justify-end gap-2 px-6 py-4">
          {/* Nur am Telefon: Dort ist der kompakte Player unten auf das
              Nötigste beschränkt, und ohne diesen Knopf käme man an die
              Warteschlange gar nicht mehr heran. Am Rechner steht sie in der
              Leiste unten, ein zweiter Knopf wäre dort doppelt. */}
          <button
            type="button"
            onClick={() => setQueueOpen(true)}
            aria-label={t("Warteschlange")}
            className="pill-btn is-raised h-9 w-9 md:hidden"
          >
            <QueueIcon size={18} />
          </button>
          <button
            type="button"
            onClick={() => setNowPlayingOpen(false)}
            aria-label={t("Vollbild schließen")}
            className="pill-btn is-raised h-9 w-9"
          >
            <ChevronDownIcon />
          </button>
        </div>

        {currentTrack ? (
          <div className="grid min-h-0 flex-1 gap-8 overflow-hidden px-6 pb-8 lg:grid-cols-2 lg:px-12">
            {/* Am Telefon die einzige Spalte, und sie rollt.
                Vorher teilte das Gitter die Höhe fest zwischen Cover und Text
                auf: 383 zu 178 Pixel. Das Cover bekam davon 224 statt seiner
                339 und stand als breiter Streifen da, der Text als leerer
                Kasten darunter. */}
            <div
              className={`flex min-h-0 flex-col items-center gap-6 overflow-y-auto lg:justify-center lg:overflow-visible ${
                schliesst ? "" : "animate-content"
              }`}
            >
              <Cover
                src={albumCover(currentTrack.albumId)}
                alt={currentTrack.albumTitle}
                seed={currentTrack.albumId}
                className="raised-cover aspect-square w-full max-w-sm shrink-0"
                rounded="rounded-2xl"
              />
              <div className="w-full max-w-sm text-center">
                <h2
                  className="truncate text-2xl font-bold"
                  title={currentTrack.title}
                >
                  {currentTrack.title}
                </h2>
                <Link
                  to={`/artist/${currentTrack.artistId}`}
                  onClick={() => setNowPlayingOpen(false)}
                  className="mt-1 inline-block text-mute transition hover:text-fg hover:underline"
                >
                  {currentTrack.artistName}
                </Link>
                <Link
                  to={`/album/${currentTrack.albumId}`}
                  onClick={() => setNowPlayingOpen(false)}
                  className="mt-2 block truncate text-sm text-mute/80 transition hover:text-fg"
                >
                  {releaseLabel(currentTrack.releaseType)} ·{" "}
                  {currentTrack.albumTitle}
                  {currentTrack.year ? ` · ${currentTrack.year}` : ""}
                </Link>

                <div className="mt-5">
                  <div className="h-1 w-full overflow-hidden rounded-full bg-ink-700">
                    <div
                      className="h-full rounded-full transition-[width] duration-200"
                      style={{
                        width: `${progress}%`,
                        background: "var(--accent)",
                      }}
                    />
                  </div>
                  <div className="mt-1.5 flex justify-between text-[11px] tabular-nums text-mute">
                    <span>{formatTime(positionMs)}</span>
                    <span>{formatTime(durationMs)}</span>
                  </div>
                </div>
              </div>

              {/* Zwei Kacheln, nur am Telefon: Dort ist neben dem Cover kein
                  Platz für den Text, und die Künstlerseite liegt sonst drei
                  Griffe entfernt. Am Rechner steht beides ohnehin da. */}
              <div className="w-full max-w-sm shrink-0 space-y-3 pb-2 lg:hidden">
                <button
                  type="button"
                  onClick={() => setTextOffen(true)}
                  className="kachel-akzent w-full p-4 text-start"
                >
                  <span className="flex items-center gap-2 text-xs font-semibold tracking-wide uppercase">
                    <LyricsIcon size={14} />
                    {t("Songtext")}
                  </span>
                  <span className="mt-2 block text-sm leading-snug">
                    {anfang.length > 0
                      ? anfang.join("\n")
                      : t("Noch keiner hinterlegt.")}
                  </span>
                </button>

                <Link
                  to={`/artist/${currentTrack.artistId}`}
                  onClick={() => setNowPlayingOpen(false)}
                  className="surface flex w-full items-center gap-3 p-3"
                >
                  <Cover
                    src={artistImage(currentTrack.artistId)}
                    alt={currentTrack.artistName}
                    seed={currentTrack.artistName}
                    className="h-11 w-11 shrink-0"
                    rounded="rounded-full"
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block eyebrow">{t("Künstler")}</span>
                    <span className="block truncate text-sm font-medium">
                      {currentTrack.artistName}
                    </span>
                  </span>
                  <ChevronRightIcon size={18} className="shrink-0 text-mute" />
                </Link>
              </div>
            </div>

            {/* Am Rechner steht der Text neben dem Cover; dort ist Platz, und
                er ist das, worauf der Blick am Ende ruht. Am Telefon führt der
                Weg über die Kachel ins Vollbild. */}
            <div
              className={`surface hidden min-h-0 overflow-hidden p-5 lg:block ${schliesst ? "" : "animate-content-late"}`}
            >
              <LyricsPanel track={currentTrack} />
            </div>
          </div>
        ) : (
          <div className="grid flex-1 place-items-center text-mute">
            {t("Starte einen Titel, um ihn hier zu sehen.")}
          </div>
        )}

        {/* Der Text im Vollbild, über allem.
            Innerhalb derselben Fläche, nicht als eigenes Fenster: So bleibt
            die Pille unten bedienbar, und der Weg zurück ist derselbe Griff
            wie überall — ein Pfeil oben rechts. */}
        {textOffen && currentTrack && (
          <div className="absolute inset-0 z-30 flex flex-col rounded-3xl bg-ink-950 p-5 animate-stage">
            <div className="mb-2 flex shrink-0 items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="eyebrow">{t("Songtext")}</p>
                <p className="truncate text-sm font-medium">
                  {currentTrack.title}
                </p>
              </div>
              <button
                type="button"
                onClick={() => setTextOffen(false)}
                aria-label={t("Songtext schließen")}
                className="pill-btn is-raised h-9 w-9 shrink-0"
              >
                <ChevronDownIcon />
              </button>
            </div>
            <div className="min-h-0 flex-1">
              <LyricsPanel track={currentTrack} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
