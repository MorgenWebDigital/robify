import { Link } from "react-router-dom";
import { t } from "../lib/i18n";
import { albumCover } from "../lib/cover";
import { formatTime, releaseLabel } from "../lib/format";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import { ChevronDownIcon, QueueIcon } from "./Icons";
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
            <div
              className={`flex min-h-0 flex-col items-center justify-center gap-6 ${
                schliesst ? "" : "animate-content"
              }`}
            >
              <Cover
                src={albumCover(currentTrack.albumId)}
                alt={currentTrack.albumTitle}
                seed={currentTrack.albumId}
                className="raised-cover aspect-square w-full max-w-sm"
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
            </div>

            {/* Zuletzt: Die Lyrics laden oft noch nach, und sie sind das,
                worauf der Blick am Ende ruht. */}
            <div
              className={`surface min-h-0 overflow-hidden p-5 ${schliesst ? "" : "animate-content-late"}`}
            >
              <LyricsPanel track={currentTrack} />
            </div>
          </div>
        ) : (
          <div className="grid flex-1 place-items-center text-mute">
            {t("Starte einen Titel, um ihn hier zu sehen.")}
          </div>
        )}
      </div>
    </div>
  );
}
