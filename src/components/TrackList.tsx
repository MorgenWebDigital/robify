import { useRef, useState, type ReactNode } from "react";
import { t } from "../lib/i18n";
import { Link } from "react-router-dom";
import { api, errorMessage } from "../lib/api";
import { albumCover } from "../lib/cover";
import { formatTime } from "../lib/format";
import { useZiehordnung } from "../lib/ziehordnung";
import { useLibrary } from "../store/library";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import {
  DotsIcon,
  HeartIcon,
  PencilIcon,
  PlayIcon,
  PlayingBars,
  PlaylistIcon,
  PlusIcon,
  QueueIcon,
  TrashIcon,
} from "./Icons";
import { Menu } from "./Menu";
import { Button, Modal } from "./Modal";
import type { Track } from "../types";

interface TrackListProps {
  tracks: Track[];
  /** plays the whole list from this index on. */
  onPlay?: (index: number) => void;
  showCover?: boolean;
  showAlbum?: boolean;
  showArtist?: boolean;
  /**
   * show album track numbers instead of the running position.
   *
   * only meaningful on an album page: in playlists and mixes the first track
   * is number 1, whatever place it held on its album.
   */
  albumNumbering?: boolean;
  onRemove?: (track: Track) => void;
  removeLabel?: string;
  onChanged?: () => void;
  emptyMessage?: ReactNode;
  /** highlight this track and bring it into view. */
  highlightTrackId?: number | null;
  /**
   * makes the order changeable by dragging. receives the new order as a list
   * of ids. without this callback the list stays unmovable.
   */
  onReorder?: (trackIds: number[]) => void;
}

export function TrackList({
  tracks,
  onPlay,
  showCover = true,
  showAlbum = true,
  showArtist = true,
  albumNumbering = false,
  onRemove,
  removeLabel = "Aus Liste entfernen",
  onChanged,
  emptyMessage = t("Keine Titel vorhanden."),
  highlightTrackId = null,
  onReorder,
}: TrackListProps) {
  const currentTrackId = usePlayer((s) => s.trackId);
  const playing = usePlayer((s) => s.playing);
  const toggle = usePlayer((s) => s.toggle);
  const { notify, notifyUndo, editTrack, openAddToPlaylist } = useUi();
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);
  /** ticked in the dialog: delete without asking from now on. */
  const [nichtMehrFragen, setNichtMehrFragen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<Track | null>(null);
  const { zieht, luecke, merkmale } = useZiehordnung(
    tracks.map((track) => track.id),
    onReorder,
  );

  const playAt = (index: number) => {
    if (tracks[index].id === currentTrackId) {
      void toggle();
      return;
    }
    if (onPlay) onPlay(index);
    else
      void api.playTracks(
        tracks.map((t) => t.id),
        index,
      );
  };

  // swiping on a row.
  //
  // only one row can be swiped at a time, so one state for the whole list
  // does instead of one per row.
  //
  // `gewischt` keeps the finger from starting the track after the swipe: the
  // browser sends a click event after a touch anyway, and it hits the same
  // row
  const [wisch, setWisch] = useState<{ index: number; dx: number } | null>(
    null,
  );
  const beginn = useRef<{ x: number; y: number; index: number } | null>(null);
  const gewischt = useRef(false);

  /** from here on it counts as a swipe and no longer as a tremor while tapping. */
  const SCHWELLE = 72;

  const wischStart = (index: number) => (event: React.TouchEvent) => {
    const finger = event.touches[0];
    beginn.current = { x: finger.clientX, y: finger.clientY, index };
    gewischt.current = false;
  };

  const wischZug = (event: React.TouchEvent) => {
    const start = beginn.current;
    if (!start) return;
    const finger = event.touches[0];
    const dx = finger.clientX - start.x;
    const dy = finger.clientY - start.y;
    // vertical wins: otherwise the list would catch while scrolling as soon
    // as the thumb wandered a little sideways
    if (Math.abs(dx) <= Math.abs(dy) || Math.abs(dx) < 8) return;
    gewischt.current = true;
    setWisch({ index: start.index, dx });
  };

  const wischEnde = (track: Track) => () => {
    const stand = wisch;
    beginn.current = null;
    setWisch(null);
    if (!stand || Math.abs(stand.dx) < SCHWELLE) return;

    if (stand.dx > 0) {
      void api.queueAdd([track.id]);
      notify(t("„{0}“ ans Ende der Warteschlange", track.title), "success");
    } else {
      openAddToPlaylist([track.id]);
    }
  };

  const toggleFavorite = async (track: Track) => {
    try {
      await api.setFavorite(track.id, !track.favorite);
      onChanged?.();
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  // removes row and file. the file travels into the app's trash, so the
  // action can be undone for a while. it still asks beforehand and names the
  // path, so it is clear what disappears
  const deleteTrack = async (track: Track) => {
    setPendingDelete(null);
    if (nichtMehrFragen) {
      setNichtMehrFragen(false);
      void saveSetting("confirmDelete", false);
    }
    try {
      await api.deleteTrack(track.id, true);
      onChanged?.();
      notifyUndo(t("„{0}“ gelöscht", track.title), async () => {
        try {
          await api.restoreTrack(track.id);
          onChanged?.();
          notify(t("„{0}“ wiederhergestellt", track.title), "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        }
      });
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  if (tracks.length === 0) {
    // plain text gets the dashed frame, a whole component brings a surface of
    // its own and would be framed twice inside it
    return typeof emptyMessage === "string" ? (
      <p className="rounded-xl border border-dashed border-ink-700 px-5 py-10 text-center text-sm text-mute">
        {emptyMessage}
      </p>
    ) : (
      <>{emptyMessage}</>
    );
  }

  return (
    <>
      <ul className="flex flex-col">
        {tracks.map((track, index) => {
          const isCurrent = track.id === currentTrackId;
          const isHighlighted = track.id === highlightTrackId;
          return (
            <li
              key={`${track.id}-${index}`}
              ref={
                isHighlighted
                  ? (element) =>
                      element?.scrollIntoView({
                        block: "center",
                        behavior: "smooth",
                      })
                  : undefined
              }
              {...merkmale(index)}
              // the whole row starts the track.
              //
              // on a phone one does not aim at a small button, one taps the
              // song. everything that does something itself keeps precedence:
              // the artist and album links, the two buttons on the right, the
              // title. without that exception a press on "Yeat" started the
              // track instead of leading to the artist.
              //
              // the row stays an `li` and does not become a button: it holds
              // buttons and links, and those must not stand inside a button.
              // by keyboard the title still leads in
              onTouchStart={wischStart(index)}
              onTouchMove={wischZug}
              onTouchEnd={wischEnde(track)}
              onClick={(event) => {
                if ((event.target as HTMLElement).closest("a, button")) return;
                // a click still comes after a swipe, and it must not start
                // the track as well
                if (gewischt.current) {
                  gewischt.current = false;
                  return;
                }
                playAt(index);
              }}
              className={`group relative hover:z-20 cursor-pointer overflow-hidden rounded-lg ${
                zieht === index ? "opacity-40" : ""
              }`}
            >
              {/* shows the gap the track falls into. a line says that more
                  precisely than a frame around a row: the frame left open
                  whether it lands before or after. */}
              {zieht !== null && luecke === index && (
                <span
                  aria-hidden="true"
                  className="pointer-events-none absolute -top-px inset-x-0 h-0.5 rounded-full"
                  style={{ background: "var(--accent)" }}
                />
              )}
              {zieht !== null &&
                luecke === index + 1 &&
                index === tracks.length - 1 && (
                  <span
                    aria-hidden="true"
                    className="pointer-events-none absolute -bottom-px inset-x-0 h-0.5 rounded-full"
                    style={{ background: "var(--accent)" }}
                  />
                )}

              {/* what the swipe brings about, while the finger still rests.
                  outside the travelling content, otherwise it would slide out
                  of view with it. */}
              {wisch?.index === index && (
                <span
                  aria-hidden="true"
                  className="pointer-events-none absolute inset-0 flex items-center justify-between px-4 text-mute"
                >
                  <QueueIcon
                    size={18}
                    className={wisch.dx > 0 ? "" : "opacity-0"}
                  />
                  <PlaylistIcon
                    size={18}
                    className={wisch.dx < 0 ? "" : "opacity-0"}
                  />
                </span>
              )}

              <div
                className={`grid items-center gap-3 rounded-lg px-2 py-1.5 grid-cols-[2.25rem_1fr_auto] sm:grid-cols-[2.25rem_minmax(0,3fr)_minmax(0,2fr)_auto] ${
                  isCurrent ? "raised-row" : "group-hover:bg-ink-800"
                } ${isHighlighted ? "bg-ink-800 ring-2 ring-[var(--accent)]" : ""}`}
                style={
                  wisch?.index === index
                    ? { transform: `translateX(${wisch.dx}px)` }
                    : // eased only while snapping back: while the finger
                      // rests, the row is to follow it without delay
                      { transition: "transform 0.18s ease" }
                }
              >
                <button
                  type="button"
                  onClick={() => playAt(index)}
                  aria-label={t("{0} abspielen", track.title)}
                  className="grid h-9 w-9 place-items-center rounded-md text-mute"
                >
                  {isCurrent && playing ? (
                    <PlayingBars />
                  ) : (
                    <>
                      <span className="text-xs tabular-nums group-hover:hidden">
                        {albumNumbering
                          ? (track.trackNo ?? index + 1)
                          : index + 1}
                      </span>
                      <PlayIcon
                        size={16}
                        className="hidden text-fg group-hover:block"
                      />
                    </>
                  )}
                </button>

                <div className="flex min-w-0 items-center gap-3">
                  {showCover && (
                    <Cover
                      src={albumCover(track.albumId)}
                      alt={track.albumTitle}
                      seed={track.albumId}
                      className="h-10 w-10 shrink-0"
                      rounded="rounded-md"
                    />
                  )}
                  <div className="min-w-0">
                    {/* the title starts it. the button on the left does that
                      too but shows its play icon on hover only, so on a phone
                      never: the number alone stood there, and that it can be
                      tapped was visible to nobody. tapping the title is the
                      gesture one tries anyway. it stands next to the artist
                      links and not around them, otherwise a button would lie
                      over a link. */}
                    <button
                      type="button"
                      onClick={() => playAt(index)}
                      className="block w-full truncate text-start text-sm font-medium"
                      style={isCurrent ? { color: "var(--accent)" } : undefined}
                      title={track.title}
                    >
                      {track.title}
                    </button>
                    {showArtist && <ArtistLinks track={track} />}
                  </div>
                </div>

                {showAlbum && (
                  <Link
                    draggable={false}
                    to={`/album/${track.albumId}`}
                    className="hidden truncate text-sm text-mute transition hover:text-fg hover:underline sm:block"
                    title={track.albumTitle}
                  >
                    {track.albumTitle}
                  </Link>
                )}

                {/* running time, then the handles: adding to a playlist is
                the more frequent one than marking a favourite, which stays in
                the menu. both buttons stand out on hovering the row only. */}
                <div className="flex items-center gap-1.5">
                  <span className="me-1 text-xs tabular-nums text-mute">
                    {formatTime(track.durationMs)}
                  </span>
                  <span className="hidden sm:block">
                    <button
                      type="button"
                      onClick={() => openAddToPlaylist([track.id])}
                      aria-label={t(
                        "{0} zu einer Playlist hinzufügen",
                        track.title,
                      )}
                      className="pill-btn is-lift h-8 w-8 opacity-0 transition-opacity group-hover:opacity-100"
                    >
                      <PlusIcon size={16} />
                    </button>
                  </span>
                  <Menu
                    hover
                    items={[
                      {
                        label: t("Als Nächstes spielen"),
                        icon: <QueueIcon size={16} />,
                        // the notice names what happened: the two entries
                        // look alike, and without feedback nothing is visible
                        // on screen while the queue is not slid out
                        onSelect: () => {
                          void api.queuePlayNext([track.id]);
                          notify(
                            t("„{0}“ läuft als Nächstes", track.title),
                            "success",
                          );
                        },
                      },
                      {
                        label: t("Zur Warteschlange"),
                        icon: <PlusIcon size={16} />,
                        onSelect: () => {
                          void api.queueAdd([track.id]);
                          notify(
                            t("„{0}“ ans Ende der Warteschlange", track.title),
                            "success",
                          );
                        },
                      },
                      {
                        label: t("Zu Playlist hinzufügen"),
                        icon: <PlaylistIcon size={16} />,
                        onSelect: () => openAddToPlaylist([track.id]),
                      },
                      {
                        label: track.favorite
                          ? t("Aus Favoriten entfernen")
                          : t("Zu Favoriten"),
                        icon: <HeartIcon size={16} filled={track.favorite} />,
                        onSelect: () => void toggleFavorite(track),
                      },
                      {
                        label: t("Metadaten bearbeiten"),
                        icon: <PencilIcon size={16} />,
                        onSelect: () => editTrack(track),
                      },
                      ...(onRemove
                        ? [
                            {
                              label: removeLabel,
                              icon: <TrashIcon size={16} />,
                              onSelect: () => onRemove(track),
                            },
                          ]
                        : []),
                      {
                        label: t("Löschen"),
                        icon: <TrashIcon size={16} />,
                        tone: "danger" as const,
                        onSelect: () => {
                          // whoever turned the confirmation off does not want
                          // to see it next time either. undoing still works
                          if (settings && !settings.confirmDelete)
                            void deleteTrack(track);
                          else setPendingDelete(track);
                        },
                      },
                    ]}
                    trigger={({ open: menuOffen, toggle: toggleMenu }) => (
                      <button
                        type="button"
                        onClick={toggleMenu}
                        aria-expanded={menuOffen}
                        aria-label={t("Weitere Aktionen")}
                        /* while the menu stands open the button stays and
                         lights up. before that it faded as soon as the pointer
                         left the row, and the menu then hung in the air
                         without a visible trigger and looked like a bug. */
                        className={`pill-btn is-lift h-8 w-8 transition-opacity ${
                          menuOffen
                            ? "is-on opacity-100"
                            : "opacity-0 group-hover:opacity-100"
                        }`}
                      >
                        <DotsIcon size={16} />
                      </button>
                    )}
                  />
                </div>
              </div>
            </li>
          );
        })}
      </ul>

      <Modal
        open={pendingDelete !== null}
        title={t("Titel löschen?")}
        subtitle={
          pendingDelete
            ? `${pendingDelete.artistName} · ${pendingDelete.title}`
            : undefined
        }
        onClose={() => setPendingDelete(null)}
        width="max-w-md"
        footer={
          <>
            <Button onClick={() => setPendingDelete(null)} variant="ghost">
              {t("Abbrechen")}
            </Button>
            <Button
              onClick={() => pendingDelete && void deleteTrack(pendingDelete)}
              variant="outline"
            >
              {t("Löschen")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-mute">
          {t(
            "Der Eintrag verschwindet aus der Bibliothek, die Datei wandert in den Papierkorb von Robify. Kurz danach lässt sich der Griff über die Meldung noch zurücknehmen.",
          )}
        </p>
        {pendingDelete && (
          <code
            className="mt-3 block truncate rounded bg-ink-800 px-2 py-1.5 text-xs"
            title={pendingDelete.path}
          >
            {pendingDelete.path}
          </code>
        )}
        <label className="mt-4 flex cursor-pointer items-center gap-2.5 text-sm text-mute">
          <input
            type="checkbox"
            checked={nichtMehrFragen}
            onChange={(event) => setNichtMehrFragen(event.target.checked)}
            className="check-box"
          />
          {t("Nicht mehr nachfragen")}
        </label>
      </Modal>
    </>
  );
}

// shows everyone involved in a track. guest artists stand behind "feat.",
// and every name leads to its artist page
export function ArtistLinks({
  track,
  className = "",
}: {
  track: Track;
  className?: string;
}) {
  const setNowPlayingOpen = useUi((s) => s.setNowPlayingOpen);

  const artists = track.artists?.length
    ? track.artists
    : [{ id: track.artistId, name: track.artistName, role: "main" }];
  const main = artists.filter((a) => a.role === "main");
  const featured = artists.filter((a) => a.role === "feature");

  const list = (group: typeof artists) =>
    group.map((artist, index) => (
      <span key={`${artist.id}-${artist.role}`}>
        {index > 0 && <span className="text-mute/60">, </span>}
        <Link
          draggable={false}
          to={`/artist/${artist.id}`}
          // the lyrics view lies over sidebar and content. left open, the
          // navigation would lead nowhere: the artist page builds up behind
          // it while the cover stays visible. in a track list the view is
          // closed anyway, so the call has no effect
          onClick={() => setNowPlayingOpen(false)}
          className="transition hover:text-fg hover:underline"
        >
          {artist.name}
        </Link>
      </span>
    ));

  return (
    <p className={`truncate text-xs text-mute ${className}`}>
      {list(main.length > 0 ? main : artists)}
      {featured.length > 0 && (
        <>
          <span className="text-mute/60"> feat. </span>
          {list(featured)}
        </>
      )}
    </p>
  );
}
