import { useEffect, useState } from "react";
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

  // Beim Ziehen soll die Anzeige dem Finger folgen, nicht dem Backend.
  const [scrubbing, setScrubbing] = useState<number | null>(null);
  const displayPosition = scrubbing ?? positionMs;
  const total = durationMs || currentTrack?.durationMs || 0;

  const [favorite, setFavorite] = useState(false);
  useEffect(() => setFavorite(currentTrack?.favorite ?? false), [currentTrack]);

  // Leertaste und Medientasten steuern die Wiedergabe.
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
    <footer className="sunken-panel z-30 mx-3 mb-3 flex h-[5.5rem] shrink-0 items-center gap-4 rounded-full bg-ink-950 px-7">
      {/* Titelinformationen */}
      <div className="flex w-[30%] min-w-0 items-center gap-3">
        {currentTrack ? (
          <>
            <button
              type="button"
              onClick={() => setNowPlayingOpen(!nowPlayingOpen)}
              aria-label={
                nowPlayingOpen ? t("Vollbild schließen") : t("Vollbild öffnen")
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
              {/* Der Titel führt zur Lyrics-Ansicht, wie das Cover daneben.
                  Er ist die größte Fläche im Player und der Ort, auf den man
                  von selbst zeigt, wenn man wissen will, was da läuft. Die
                  Künstlernamen darunter bleiben eigene Verweise, sonst käme
                  man von hier nicht mehr zur Künstlerseite. */}
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

      {/* Steuerung */}
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

      {/* Zusatzfunktionen */}
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
  );
}
