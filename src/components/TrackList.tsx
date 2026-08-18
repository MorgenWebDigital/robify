import { useState, type ReactNode } from "react";
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
  /** Spielt die gesamte Liste ab diesem Index. */
  onPlay?: (index: number) => void;
  showCover?: boolean;
  showAlbum?: boolean;
  showArtist?: boolean;
  /**
   * Albumnummern statt fortlaufender Position anzeigen.
   *
   * Nur auf einer Albumseite sinnvoll: In Playlists und Mixen ist der erste
   * Titel die Nummer 1, egal an welcher Stelle er auf seinem Album stand.
   */
  albumNumbering?: boolean;
  onRemove?: (track: Track) => void;
  removeLabel?: string;
  onChanged?: () => void;
  emptyMessage?: ReactNode;
  /** Diesen Titel hervorheben und in den sichtbaren Bereich holen. */
  highlightTrackId?: number | null;
  /**
   * Reihenfolge per Ziehen änderbar. Bekommt die neue Reihenfolge als Liste
   * von Kennungen. Ohne diesen Rückruf bleibt die Liste unverschiebbar.
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
  /** Im Dialog angehakt: künftig ohne Rückfrage löschen. */
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

  const toggleFavorite = async (track: Track) => {
    try {
      await api.setFavorite(track.id, !track.favorite);
      onChanged?.();
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  /**
   * Entfernt Eintrag und Datei. Die Datei wandert in den Papierkorb der App,
   * darum lässt sich der Griff eine Weile zurücknehmen. Gefragt wird trotzdem
   * vorher, und der Pfad genannt, damit klar ist, was verschwindet.
   */
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
    // Reiner Text bekommt den gestrichelten Rahmen; ein ganzes Bauteil bringt
    // seine eigene Fläche mit und würde darin doppelt gerahmt.
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
              className={`group relative hover:z-20 grid grid-cols-[2.25rem_1fr_auto] items-center gap-3 rounded-lg px-2 py-1.5 transition sm:grid-cols-[2.25rem_minmax(0,3fr)_minmax(0,2fr)_auto] ${
                isCurrent ? "raised-row" : "hover:bg-ink-800"
              } ${isHighlighted ? "bg-ink-800 ring-2 ring-[var(--accent)]" : ""} ${
                zieht === index ? "opacity-40" : ""
              }`}
            >
              {/* Zeigt die Lücke, in die der Titel fällt. Ein Strich sagt das
                  genauer als ein Rahmen um eine Zeile: Der Rahmen ließ offen,
                  ob es davor oder dahinter wird. */}
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
                  {/* Der Titel startet ihn.
                      Der Knopf links tut das auch, zeigt sein Play-Symbol aber
                      erst beim Überfahren — auf einem Telefon also nie: Dort
                      stand nur die Nummer, und dass sie tippbar ist, sah
                      niemand. Den Titel anzutippen ist die Geste, die man
                      ohnehin versucht. Er steht neben den Künstlerlinks, nicht
                      um sie herum, sonst läge ein Knopf über einem Link. */}
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

              {/* Laufzeit, dann die Handgriffe: Zu einer Playlist hinzufügen ist
                der häufigere als das Favorisieren, das bleibt im Menü. Beide
                Knöpfe stehen erst beim Überfahren der Zeile heraus. */}
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
                      // Der Hinweis nennt beim Namen, was geschah: Die
                      // beiden Einträge sehen einander ähnlich, und ohne
                      // Rückmeldung ist am Fenster nichts zu sehen, wenn die
                      // Warteschlange gerade nicht ausgefahren ist.
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
                        // Wer die Rückfrage abgestellt hat, will sie auch nicht
                        // beim nächsten Mal sehen. Rückgängig geht trotzdem.
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
                      /* Solange die Liste offen steht, bleibt der Knopf stehen
                         und leuchtet. Vorher verblasste er, sobald der Zeiger
                         die Zeile verließ, die Liste hing dann ohne sichtbaren
                         Auslöser in der Luft und sah aus wie ein Fehler. */
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

/**
 * Zeigt alle Beteiligten eines Titels. Gastkünstler stehen hinter „feat.“,
 * jeder Name führt auf seine Künstlerseite.
 */
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
          // Die Lyrics-Ansicht legt sich über Seitenleiste und Inhalt. Bliebe
          // sie offen, liefe die Navigation ins Leere: Die Künstlerseite baut
          // sich dahinter auf, zu sehen wäre weiterhin das Cover. In einer
          // Titelliste ist die Ansicht ohnehin zu, der Aufruf also folgenlos.
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
