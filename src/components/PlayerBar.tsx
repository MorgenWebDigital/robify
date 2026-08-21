// the bar at the lower edge: what is running, the buttons, the position.
// note: while dragging, the display follows the finger and not the backend,
// otherwise the handle jumps back at every progress event.

import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage } from "../lib/api";
import { albumCover } from "../lib/cover";
import { formatTime } from "../lib/format";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import {
  ChevronDownIcon,
  HeartIcon,
  LyricsIcon,
  MuteIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
  QueueIcon,
  RepeatIcon,
  RepeatOneIcon,
  ShuffleIcon,
  VolumeIcon,
} from "./Icons";
import { SleepTimerMenu } from "./SleepTimerMenu";
import { ArtistLinks } from "./TrackList";

export function PlayerBar() {
  const {
    playing,
    positionMs,
    durationMs,
    volume,
    muted,
    repeat,
    shuffle,
    currentTrack,
    toggle,
    next,
    previous,
    seek,
    setVolume,
    toggleMute,
    cycleRepeat,
    toggleShuffle,
  } = usePlayer();
  const { nowPlayingOpen, setNowPlayingOpen, queueOpen, setQueueOpen, notify } =
    useUi();

  // while dragging, the display is to follow the finger, not the backend
  const [scrubbing, setScrubbing] = useState<number | null>(null);
  const displayPosition = scrubbing ?? positionMs;
  const total = durationMs || currentTrack?.durationMs || 0;

  const [favorite, setFavorite] = useState(false);
  useEffect(() => setFavorite(currentTrack?.favorite ?? false), [currentTrack]);

  // space and the media keys control the playback
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
        return;
      if (event.key === " ") {
        event.preventDefault();
        void toggle();
      } else if (event.key === "ArrowRight" && event.ctrlKey) {
        void next();
      } else if (event.key === "ArrowLeft" && event.ctrlKey) {
        void previous();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle, next, previous]);

  const toggleFavorite = async () => {
    if (!currentTrack) return;
    try {
      const value = !favorite;
      setFavorite(value);
      await api.setFavorite(currentTrack.id, value);
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  const RepeatGlyph = repeat === "one" ? RepeatOneIcon : RepeatIcon;

  return (
    <>
      <MiniPlayer />

      <footer className="sunken-panel z-30 mx-3 mb-3 hidden h-[5.5rem] shrink-0 items-center gap-4 rounded-full bg-ink-950 px-7 md:flex">
        {/* track details */}
        <div className="flex w-[30%] min-w-0 items-center gap-3">
          {currentTrack ? (
            <>
              <button
                type="button"
                onClick={() => setNowPlayingOpen(!nowPlayingOpen)}
                aria-label={
                  nowPlayingOpen
                    ? t("Vollbild schließen")
                    : t("Vollbild öffnen")
                }
                className="relative shrink-0"
              >
                <Cover
                  src={albumCover(currentTrack.albumId)}
                  alt={currentTrack.albumTitle}
                  seed={currentTrack.albumId}
                  className="raised-cover h-14 w-14"
                  rounded="rounded-md"
                />
                <span className="raised-cover absolute inset-0 grid place-items-center rounded-md bg-black/55 opacity-0 transition hover:opacity-100">
                  <ChevronDownIcon
                    size={18}
                    className={nowPlayingOpen ? "" : "rotate-180"}
                  />
                </span>
              </button>
              <div className="min-w-0">
                {/* the title leads to the lyrics view, as the cover next to
                  it does. it is the largest area in the player and the place
                  one points at by oneself when wanting to know what is
                  running. the artist names below stay links of their own,
                  otherwise the artist page could not be reached from here. */}
                <button
                  type="button"
                  onClick={() => setNowPlayingOpen(!nowPlayingOpen)}
                  title={currentTrack.title}
                  aria-pressed={nowPlayingOpen}
                  className="block max-w-full truncate text-sm font-medium transition hover:underline"
                >
                  {currentTrack.title}
                </button>
                <ArtistLinks track={currentTrack} />
              </div>
              <span className="ms-1 hidden sm:block">
                <button
                  type="button"
                  onClick={() => void toggleFavorite()}
                  aria-label={
                    favorite ? t("Aus Favoriten entfernen") : t("Zu Favoriten")
                  }
                  aria-pressed={favorite}
                  className={`pill-btn is-raised h-9 w-9 ${favorite ? "is-on" : ""}`}
                >
                  <HeartIcon size={18} filled={favorite} />
                </button>
              </span>
            </>
          ) : (
            <p className="text-sm text-mute">{t("Kein Titel ausgewählt")}</p>
          )}
        </div>

        {/* controls */}
        <div className="flex flex-1 flex-col items-center gap-1.5">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void toggleShuffle()}
              aria-label={t("Zufallswiedergabe")}
              aria-pressed={shuffle}
              title={t("Zufallswiedergabe")}
              className={`pill-btn is-raised h-9 w-9 ${shuffle ? "is-on" : ""}`}
            >
              <ShuffleIcon size={18} />
            </button>
            <button
              type="button"
              onClick={() => void previous()}
              aria-label={t("Vorheriger Titel")}
              className="pill-btn is-raised h-9 w-9"
            >
              <PrevIcon size={20} />
            </button>
            <button
              type="button"
              onClick={() => void toggle()}
              aria-label={playing ? t("Pause") : t("Abspielen")}
              className="pill-btn is-raised is-accent h-11 w-11"
            >
              {playing ? (
                <PauseIcon size={18} />
              ) : (
                <PlayIcon size={18} className="ml-0.5" />
              )}
            </button>
            <button
              type="button"
              onClick={() => void next()}
              aria-label={t("Nächster Titel")}
              className="pill-btn is-raised h-9 w-9"
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
              title={
                repeat === "off"
                  ? t("Wiederholen: aus")
                  : repeat === "all"
                    ? t("Wiederholen: Warteschlange")
                    : t("Wiederholen: dieser Titel")
              }
              className={`pill-btn is-raised h-9 w-9 ${repeat !== "off" ? "is-on" : ""}`}
            >
              <RepeatGlyph size={18} />
            </button>
          </div>

          <div className="flex w-full max-w-2xl items-center gap-2">
            <span className="w-10 text-end text-[11px] tabular-nums text-mute">
              {formatTime(displayPosition)}
            </span>
            <input
              type="range"
              min={0}
              max={Math.max(total, 1)}
              step={250}
              value={Math.min(displayPosition, total || 1)}
              disabled={!currentTrack}
              aria-label={t("Wiedergabeposition")}
              onChange={(event) => setScrubbing(Number(event.target.value))}
              onPointerUp={() => {
                if (scrubbing !== null) void seek(scrubbing);
                setScrubbing(null);
              }}
              onKeyUp={() => {
                if (scrubbing !== null) void seek(scrubbing);
                setScrubbing(null);
              }}
              style={
                {
                  "--fill": `${total ? (Math.min(displayPosition, total) / total) * 100 : 0}%`,
                } as React.CSSProperties
              }
              className="h-1 flex-1"
            />
            <span className="w-10 text-[11px] tabular-nums text-mute">
              {formatTime(total)}
            </span>
          </div>
        </div>

        {/* extras */}
        <div className="flex w-[30%] items-center justify-end gap-1.5">
          <SleepTimerMenu />
          <button
            type="button"
            onClick={() => setNowPlayingOpen(!nowPlayingOpen)}
            aria-label={
              nowPlayingOpen ? t("Lyrics schließen") : t("Lyrics anzeigen")
            }
            title={t("Lyrics")}
            aria-pressed={nowPlayingOpen}
            className={`pill-btn is-raised h-9 w-9 ${nowPlayingOpen ? "is-on" : ""}`}
          >
            <LyricsIcon size={18} />
          </button>
          <button
            type="button"
            onClick={() => setQueueOpen(!queueOpen)}
            aria-label={t("Warteschlange")}
            title={t("Warteschlange")}
            aria-pressed={queueOpen}
            className={`pill-btn is-raised h-9 w-9 ${queueOpen ? "is-on" : ""}`}
          >
            <QueueIcon size={18} />
          </button>

          <div className="hidden items-center gap-1.5 md:flex">
            <button
              type="button"
              onClick={() => void toggleMute()}
              aria-label={muted ? t("Ton an") : t("Stumm")}
              aria-pressed={muted}
              className={`pill-btn is-raised h-9 w-9 ${muted ? "is-on" : ""}`}
            >
              {muted || volume === 0 ? (
                <MuteIcon size={18} />
              ) : (
                <VolumeIcon size={18} />
              )}
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={muted ? 0 : volume}
              aria-label={t("Lautstärke")}
              onChange={(event) => void setVolume(Number(event.target.value))}
              style={
                {
                  "--fill": `${(muted ? 0 : volume) * 100}%`,
                } as React.CSSProperties
              }
              className="h-1 w-24"
            />
          </div>
        </div>
      </footer>
    </>
  );
}

// the player on a phone.
//
// the desktop bar carries three columns and fourteen controls, and on a
// hand's width it ran out of the picture to the right with the volume not
// visible at all. what one needs in passing stays here: seeing what is
// running, and stopping it.
//
// everything else lies one touch away in the full screen view, which opens by
// tapping the cover or the title. on a phone that is the familiar reach
// anyway.
//
// a component of its own and not a rebuilt bar: the desktop one is finely
// balanced, and a dozen breakpoints inside it would have made both versions
// unreadable
function MiniPlayer() {
  const currentTrack = usePlayer((s) => s.currentTrack);
  const playing = usePlayer((s) => s.playing);
  const positionMs = usePlayer((s) => s.positionMs);
  const durationMs = usePlayer((s) => s.durationMs);
  const toggle = usePlayer((s) => s.toggle);
  const next = usePlayer((s) => s.next);
  const previous = usePlayer((s) => s.previous);
  const setNowPlayingOpen = useUi((s) => s.setNowPlayingOpen);
  const nowPlayingOpen = useUi((s) => s.nowPlayingOpen);

  const gesamt = durationMs || currentTrack?.durationMs || 0;
  const anteil = gesamt ? Math.min(positionMs / gesamt, 1) * 100 : 0;

  // swiping up opens the track.
  //
  // the bar is the lid over the full screen view, and pushing it up is the
  // movement one tries anyway. tapping does the same and stays, the gesture
  // is a shortcut and no replacement.
  //
  // upwards only and only clearly: the bar is two fingers high, and a tap
  // wobbles a few pixels inside it
  const hochBeginn = useRef<{ x: number; y: number } | null>(null);
  const HOCH_SCHWELLE = 40;

  const hochStart = (event: React.TouchEvent) => {
    hochBeginn.current = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY,
    };
  };

  const hochEnde = (event: React.TouchEvent) => {
    const start = hochBeginn.current;
    hochBeginn.current = null;
    if (!start || !currentTrack) return;
    const dx = event.changedTouches[0].clientX - start.x;
    const dy = event.changedTouches[0].clientY - start.y;
    if (dy < -HOCH_SCHWELLE && Math.abs(dy) > Math.abs(dx)) {
      setNowPlayingOpen(true);
    }
  };

  // gone as soon as the track itself is open.
  //
  // the bar is the way there, and standing inside already it shows the same
  // thing a second time and takes a row from the screen, right where cover,
  // controls and lyrics wrestle for room anyway. on a desktop the question
  // does not arise, it is `md:hidden` there and the full screen view leaves
  // it standing
  if (nowPlayingOpen) return null;

  return (
    <footer
      onTouchStart={hochStart}
      onTouchEnd={hochEnde}
      className="sunken-panel z-30 mx-2 mb-2 shrink-0 overflow-hidden rounded-2xl bg-ink-950 md:hidden"
    >
      {/* a line instead of a slider: the full screen view is there for
          seeking, here it is only about seeing how far the track has got. */}
      <div className="h-0.5 w-full bg-ink-700" aria-hidden="true">
        <div
          className="h-full transition-[width] duration-200"
          style={{ width: `${anteil}%`, background: "var(--accent)" }}
        />
      </div>

      <div className="flex items-center gap-3 p-2">
        {currentTrack ? (
          <button
            type="button"
            onClick={() => setNowPlayingOpen(true)}
            aria-label={t("Vollbild öffnen")}
            className="flex min-w-0 flex-1 items-center gap-3 text-start"
          >
            <Cover
              src={albumCover(currentTrack.albumId)}
              alt={currentTrack.albumTitle}
              seed={currentTrack.albumId}
              className="raised-cover h-11 w-11 shrink-0"
              rounded="rounded-lg"
            />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm font-medium">
                {currentTrack.title}
              </span>
              <span className="block truncate text-xs text-mute">
                {currentTrack.artistName}
              </span>
            </span>
          </button>
        ) : (
          <p className="min-w-0 flex-1 ps-1 text-sm text-mute">
            {t("Kein Titel ausgewählt")}
          </p>
        )}

        {/* back belongs with it: without the button the way to the previous
            track could only be found through the full screen view while the
            next one stood right beside it. */}
        <button
          type="button"
          onClick={() => void previous()}
          aria-label={t("Vorheriger Titel")}
          className="pill-btn is-raised h-11 w-11 shrink-0"
        >
          <PrevIcon size={20} />
        </button>
        <button
          type="button"
          onClick={() => void toggle()}
          aria-label={playing ? t("Pause") : t("Abspielen")}
          className="pill-btn is-raised is-accent h-11 w-11 shrink-0"
        >
          {playing ? (
            <PauseIcon size={18} />
          ) : (
            <PlayIcon size={18} className="ml-0.5" />
          )}
        </button>
        <button
          type="button"
          onClick={() => void next()}
          aria-label={t("Nächster Titel")}
          className="pill-btn is-raised h-11 w-11 shrink-0"
        >
          <NextIcon size={20} />
        </button>
      </div>
    </footer>
  );
}
