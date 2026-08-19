import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { t } from "../lib/i18n";
import { api, errorMessage, fallback } from "../lib/api";
import { albumCover, artistImage } from "../lib/cover";
import { formatTime, releaseLabel } from "../lib/format";
import { useLibrary } from "../store/library";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import {
  ChevronDownIcon,
  HeartIcon,
  LyricsIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  PrevIcon,
  QueueIcon,
  RepeatIcon,
  RepeatOneIcon,
  ShuffleIcon,
} from "./Icons";
import type { Track } from "../types";
import { LyricsPanel } from "./LyricsPanel";
import { activeLineIndex, parseLrc, type LyricLine } from "../lib/lrc";
import { useAusblenden } from "../lib/ausblenden";
import { useSchliesstBeimSeitenwechsel } from "../lib/seitenwechsel";

/** has to match the duration of `.animate-stage-out` in the stylesheet. */
const ZU_MS = 280;

// the full screen view with a large cover and lyrics running along
export function NowPlaying() {
  const currentTrack = usePlayer((s) => s.currentTrack);
  const positionMs = usePlayer((s) => s.positionMs);
  const durationMs = usePlayer((s) => s.durationMs);
  const playing = usePlayer((s) => s.playing);
  const shuffle = usePlayer((s) => s.shuffle);
  const repeat = usePlayer((s) => s.repeat);
  const toggle = usePlayer((s) => s.toggle);
  const next = usePlayer((s) => s.next);
  const previous = usePlayer((s) => s.previous);
  const toggleShuffle = usePlayer((s) => s.toggleShuffle);
  const cycleRepeat = usePlayer((s) => s.cycleRepeat);
  const {
    nowPlayingOpen,
    setNowPlayingOpen,
    setQueueOpen,
    openAddToPlaylist,
    notify,
  } = useUi();
  const refreshLibrary = useLibrary((s) => s.refresh);
  const { sichtbar, schliesst } = useAusblenden(nowPlayingOpen, ZU_MS);
  useSchliesstBeimSeitenwechsel(setNowPlayingOpen);
  // on a phone only: the lyrics do not stand next to it there but behind a
  // tile. on a desktop they are visible the whole time anyway
  const [textOffen, setTextOffen] = useState(false);
  /** the preview as plain text where no time-synced version is on hand. */
  const [anfang, setAnfang] = useState<string[]>([]);
  /** the time-synced version, where there is one. */
  const [synchron, setSynchron] = useState<LyricLine[]>([]);

  const trackId = currentTrack?.id ?? null;

  // the first lines for the tile.
  //
  // without them only "Songtext" would stand there and one would have to tap
  // to see whether any is stored at all. eight lines are a piece of text
  // already and give the tile enough height to hold its own next to the
  // cover. the full text is not loaded twice over it: the full screen view
  // fetches it itself, and the command reads from the database, not from the
  // net
  useEffect(() => {
    if (!trackId) {
      setAnfang([]);
      setSynchron([]);
      return;
    }
    let gilt = true;
    void api
      .getLyrics(trackId)
      .catch(fallback(null, t("Lyrics")))
      .then((lyrics) => {
        if (!gilt) return;
        setSynchron(lyrics?.synced ? parseLrc(lyrics.synced) : []);
        const roh = lyrics?.plain || lyrics?.synced || "";
        setAnfang(
          roh
            // the timestamps of a synced version do not belong in the preview
            .replace(/\[\d{1,2}:\d{2}(?:[.:]\d{1,3})?\]/g, "")
            .split("\n")
            .map((zeile) => zeile.trim())
            .filter(Boolean)
            .slice(0, 8),
        );
      });
    return () => {
      gilt = false;
    };
  }, [trackId]);

  // the excerpt standing in the tile.
  //
  // where the text is time-synced it travels along: the line being sung
  // stands in the second of eight, two before it to read back and five behind
  // it to read ahead. the same eight lines from the start used to stand there
  // always, and by the third chorus that had nothing to do with what could be
  // heard any more.
  //
  // without timestamps it stays at the start: letting something run along
  // that does not know where it stands would be guesswork
  const SICHTBAR = 8;
  const VORLAUF = 2;
  const vorschau: { text: string; jetzt: boolean }[] = (() => {
    if (synchron.length === 0) {
      return anfang.map((text) => ({ text, jetzt: false }));
    }
    const laufend = Math.max(0, activeLineIndex(synchron, positionMs));
    const start = Math.min(
      Math.max(0, laufend - VORLAUF),
      Math.max(0, synchron.length - SICHTBAR),
    );
    return synchron
      .slice(start, start + SICHTBAR)
      .map((zeile, stelle) => ({
        text: zeile.text,
        jetzt: start + stelle === laufend,
      }))
      .filter((zeile) => zeile.text);
  })();

  // the full screen lyrics close on a track change, they belonged to the previous one
  useEffect(() => setTextOffen(false), [trackId]);

  // the favourite state here, not in the loaded track alone.
  //
  // `currentTrack` is fetched once on a track change and not again, so a
  // press on the heart changed the database while the heart itself stayed
  // empty. a state of its own flips at once and goes back should the saving
  // fail
  const [favorit, setFavorit] = useState(false);
  useEffect(
    () => setFavorit(Boolean(currentTrack?.favorite)),
    [currentTrack?.id, currentTrack?.favorite],
  );

  const favoritUmschalten = async () => {
    if (!currentTrack) return;
    const neu = !favorit;
    setFavorit(neu);
    try {
      await api.setFavorite(currentTrack.id, neu);
      void refreshLibrary();
    } catch (error) {
      setFavorit(!neu);
      notify(errorMessage(error), "error");
    }
  };

  // swiping on the track: sideways moves on, downwards closes.
  //
  // which of the two directions applies is decided by the longer distance.
  // without that separation every diagonal drag triggered both, and one
  // landed on the next track although one wanted to close.
  //
  // closing additionally only from the very top: further down the downward
  // drag is the scrolling through lyrics and artist. whether the column stood
  // at the top is therefore remembered when the finger lands. that does not
  // hold for moving sideways, nothing scrolls horizontally here
  const zugBeginn = useRef<{ x: number; y: number; oben: boolean } | null>(
    null,
  );
  /** from here on it is a gesture and no longer a slip. */
  const ZUG_SCHWELLE = 90;

  // where the move is going, for the brief animation along with it.
  //
  // the change itself comes from the rust side and takes a moment, and
  // without an animation the old track would simply stand there during that
  // time, making the swipe look as if it had gone nowhere
  const [blaettert, setBlaettert] = useState<"vor" | "zurueck" | null>(null);
  const blaetterUhr = useRef<number | null>(null);

  const blaettern = (richtung: "vor" | "zurueck") => {
    if (blaetterUhr.current) window.clearTimeout(blaetterUhr.current);
    setBlaettert(richtung);
    // has to match the duration of `blaettern-vor` in the stylesheet
    blaetterUhr.current = window.setTimeout(() => setBlaettert(null), 260);
  };

  useEffect(
    () => () => {
      if (blaetterUhr.current) window.clearTimeout(blaetterUhr.current);
    },
    [],
  );

  const zugStart = (event: React.TouchEvent<HTMLDivElement>) => {
    zugBeginn.current = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY,
      oben: event.currentTarget.scrollTop <= 0,
    };
  };

  const zugEnde = (event: React.TouchEvent<HTMLDivElement>) => {
    const start = zugBeginn.current;
    zugBeginn.current = null;
    if (!start) return;
    const dx = event.changedTouches[0].clientX - start.x;
    const dy = event.changedTouches[0].clientY - start.y;

    if (Math.abs(dx) > Math.abs(dy)) {
      if (Math.abs(dx) < ZUG_SCHWELLE) return;
      // to the left the next one comes into view, as when turning a page
      blaettern(dx < 0 ? "vor" : "zurueck");
      void (dx < 0 ? next() : previous());
      return;
    }

    if (start.oben && dy > ZUG_SCHWELLE) setNowPlayingOpen(false);
  };

  const artistId = currentTrack?.artistId ?? null;
  const [weitere, setWeitere] = useState<Track[]>([]);

  // more from the same artist, under the artist image.
  //
  // sorted by what has been heard, not by title: what one listens to often
  // anyway is the better suggestion than what happens to stand first in the
  // alphabet. the running track drops out, it stands above already, and five
  // are enough, more would be no suggestion but a second library
  useEffect(() => {
    if (!artistId) {
      setWeitere([]);
      return;
    }
    let gilt = true;
    void api
      .artistTracks(artistId)
      .catch(fallback([] as Track[], t("Künstler")))
      .then((titel) => {
        if (!gilt) return;
        setWeitere(
          titel
            .filter((s) => s.id !== trackId)
            .sort((a, b) => b.playCount - a.playCount)
            .slice(0, 5),
        );
      });
    return () => {
      gilt = false;
    };
  }, [artistId, trackId]);

  if (!sichtbar) return null;

  const progress = durationMs ? (positionMs / durationMs) * 100 : 0;

  return (
    <div
      className={`absolute inset-0 z-20 flex flex-col bg-frame p-3 ${
        schliesst ? "animate-stage-out" : "animate-stage"
      }`}
    >
      {/* one single surface behind everything: background and sheen as on
          the player pill below, cover and lyrics lie on it together. */}
      <div className="sunken-deep flex min-h-0 flex-1 flex-col rounded-3xl bg-ink-950">
        {/* no label saying something is playing, one can see that. */}
        {/* closing alone, and on the left: it is the way back, and that
            stands on the left everywhere else in the app too. the queue has
            travelled to the other handles under the timeline. */}
        <div className="flex items-center px-6 py-4">
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
            {/* on a phone the only column, and it scrolls. the grid used to
                split the height between cover and text at a fixed 383 to 178
                pixels. the cover got 224 of that instead of its 339 and stood
                there as a wide strip, the text as an empty box below. */}
            <div
              onTouchStart={zugStart}
              onTouchEnd={zugEnde}
              className={`flex min-h-0 flex-col items-center gap-6 overflow-y-auto lg:justify-center lg:overflow-visible ${
                schliesst ? "" : "animate-content"
              } ${
                blaettert === "vor"
                  ? "blaettert-vor"
                  : blaettert === "zurueck"
                    ? "blaettert-zurueck"
                    : ""
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

                {/* filing and queue: small and on the left, under the
                    timeline. deliberately beside and not in the button row
                    below, that one carries the playback, and what is to
                    happen to the track does not belong in the same row as
                    pause and next.

                    from `md` on both stand in the bar below, which is always
                    visible there, and here it would be twice. */}
                <div className="mt-4 flex items-center gap-2 md:hidden">
                  <button
                    type="button"
                    onClick={() => openAddToPlaylist([currentTrack.id])}
                    aria-label={t("Zu Playlist hinzufügen")}
                    title={t("Zu Playlist hinzufügen")}
                    className="pill-btn is-raised h-8 w-8"
                  >
                    <PlusIcon size={16} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void favoritUmschalten()}
                    aria-label={
                      favorit ? t("Aus Favoriten entfernen") : t("Zu Favoriten")
                    }
                    title={
                      favorit ? t("Aus Favoriten entfernen") : t("Zu Favoriten")
                    }
                    aria-pressed={favorit}
                    className={`pill-btn is-raised h-8 w-8 ${favorit ? "is-on" : ""}`}
                  >
                    <HeartIcon size={16} filled={favorit} />
                  </button>
                  <button
                    type="button"
                    onClick={() => setQueueOpen(true)}
                    aria-label={t("Warteschlange")}
                    title={t("Warteschlange")}
                    className="pill-btn is-raised h-8 w-8"
                  >
                    <QueueIcon size={16} />
                  </button>
                </div>

                {/* the controls, on a phone only: from `md` on they stand in
                    the bar below, which is always visible there, the same
                    breakpoint at which the bar appears. */}
                <div className="mt-3 flex items-center justify-center gap-2 md:hidden">
                  <button
                    type="button"
                    onClick={() => void toggleShuffle()}
                    aria-label={t("Zufallswiedergabe")}
                    aria-pressed={shuffle}
                    className={`pill-btn is-raised h-9 w-9 ${shuffle ? "is-on" : ""}`}
                  >
                    <ShuffleIcon size={18} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void previous()}
                    aria-label={t("Vorheriger Titel")}
                    className="pill-btn is-raised h-10 w-10"
                  >
                    <PrevIcon size={20} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void toggle()}
                    aria-label={playing ? t("Pause") : t("Abspielen")}
                    className="pill-btn is-raised is-accent h-14 w-14"
                  >
                    {playing ? (
                      <PauseIcon size={22} />
                    ) : (
                      <PlayIcon size={22} className="ml-0.5" />
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => void next()}
                    aria-label={t("Nächster Titel")}
                    className="pill-btn is-raised h-10 w-10"
                  >
                    <NextIcon size={20} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void cycleRepeat()}
                    aria-label={
                      repeat === "off"
                        ? t("Wiederholen aus")
                        : repeat === "all"
                          ? t("Alle wiederholen")
                          : t("Titel wiederholen")
                    }
                    aria-pressed={repeat !== "off"}
                    className={`pill-btn is-raised h-9 w-9 ${repeat !== "off" ? "is-on" : ""}`}
                  >
                    {repeat === "one" ? (
                      <RepeatOneIcon size={18} />
                    ) : (
                      <RepeatIcon size={18} />
                    )}
                  </button>
                </div>
              </div>

              {/* two tiles, on a phone only: there is no room for the lyrics
                  next to the cover there, and the artist page lies three
                  reaches away otherwise. on a desktop both stand there
                  anyway. */}
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
                  {/* every line as a piece of its own so the running one can
                      stand in the accent colour. the breaks stay characters
                      of their own: the tile bounds its height through
                      `-webkit-line-clamp`, and that counts lines in the text
                      flow, not blocks. */}
                  <span className="mt-2 block text-base leading-snug">
                    {vorschau.length > 0
                      ? vorschau.map((zeile, stelle) => (
                          <span
                            key={`${stelle}-${zeile.text}`}
                            className={zeile.jetzt ? "font-semibold" : ""}
                            style={
                              zeile.jetzt
                                ? { color: "var(--accent)" }
                                : undefined
                            }
                          >
                            {stelle > 0 ? "\n" : ""}
                            {zeile.text}
                          </span>
                        ))
                      : t("Noch keiner hinterlegt.")}
                  </span>
                </button>

                {/* the artist last, without a surface of their own. a box
                    made them an entry among others while they stand for
                    themselves, as cover and title do above. the image carries
                    the section, the frame would be trimming. */}
                <div className="w-full pt-2">
                  <Link
                    to={`/artist/${currentTrack.artistId}`}
                    onClick={() => setNowPlayingOpen(false)}
                    className="flex flex-col items-center gap-3"
                  >
                    <Cover
                      src={artistImage(currentTrack.artistId)}
                      alt={currentTrack.artistName}
                      seed={currentTrack.artistName}
                      className="raised-cover h-40 w-40 shrink-0"
                      rounded="rounded-full"
                    />
                    <span className="max-w-full truncate text-2xl font-bold">
                      {currentTrack.artistName}
                    </span>
                  </Link>

                  {weitere.length > 0 && (
                    <div className="mt-4">
                      <p className="mb-1 eyebrow">{t("Mehr davon")}</p>
                      <ul>
                        {weitere.map((titel) => (
                          <li key={titel.id}>
                            <button
                              type="button"
                              onClick={() => void api.playTracks([titel.id], 0)}
                              className="flex w-full items-center gap-3 rounded-lg py-1.5 text-start"
                            >
                              <Cover
                                src={albumCover(titel.albumId)}
                                alt={titel.albumTitle}
                                seed={titel.albumId}
                                className="h-10 w-10 shrink-0"
                                rounded="rounded-md"
                              />
                              <span className="min-w-0 flex-1 truncate text-sm">
                                {titel.title}
                              </span>
                              <span className="shrink-0 text-xs tabular-nums text-mute">
                                {formatTime(titel.durationMs)}
                              </span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              </div>
            </div>

            {/* on a desktop the lyrics stand next to the cover, there is room
                there and they are what the eye rests on in the end. on a
                phone the way leads through the tile into full screen. */}
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

        {/* the lyrics in full screen, over everything. inside the same
            surface and not as a window of its own: that keeps the pill below
            operable, and the way back is the same reach as everywhere, an
            arrow at the top right. */}
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
