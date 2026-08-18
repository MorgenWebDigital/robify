import { ZurueckKnopf } from "../components/Cards";
import { t } from "../lib/i18n";
import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Cover } from "../components/Cover";
import { AlbumEditor } from "../components/AlbumEditor";
import {
  PencilIcon,
  PlayIcon,
  PlusIcon,
  ShuffleIcon,
} from "../components/Icons";
import { TrackList } from "../components/TrackList";
import { api, fallback } from "../lib/api";
import { albumCover } from "../lib/cover";
import { formatDuration, plural, releaseLabel } from "../lib/format";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type { Album, Track } from "../types";

export function AlbumDetail() {
  const { id } = useParams();
  const albumId = Number(id);
  const revision = useLibrary((s) => s.revision);
  const refresh = useLibrary((s) => s.refresh);
  const { openAddToPlaylist } = useUi();

  const [album, setAlbum] = useState<Album | null>(null);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    if (!Number.isFinite(albumId)) return;
    let cancelled = false;
    void Promise.all([
      api.getAlbum(albumId).catch(fallback(null, t("Release"))),
      api.albumTracks(albumId).catch(fallback([], t("Titel des Releases"))),
    ]).then(([albumValue, trackValue]) => {
      if (cancelled) return;
      setAlbum(albumValue);
      setTracks(trackValue);
    });
    return () => {
      cancelled = true;
    };
  }, [albumId, revision]);

  if (!album) {
    return (
      <p className="py-20 text-center text-mute">
        {t("Release wird geladen…")}
      </p>
    );
  }

  const play = (shuffle: boolean) => {
    if (tracks.length === 0) return;
    void api.setShuffle(shuffle);
    void api.playTracks(
      tracks.map((t) => t.id),
      0,
    );
  };

  return (
    <div>
      <ZurueckKnopf ziel="/library" />
      <div className="mb-8 flex flex-wrap items-end gap-6">
        <Cover
          src={albumCover(album.id)}
          alt={album.title}
          seed={album.id}
          className="h-44 w-44 shrink-0 shadow-2xl"
          rounded="rounded-xl"
        />
        <div className="min-w-0 flex-1">
          <p className="eyebrow">{releaseLabel(album.releaseType)}</p>
          <h1 className="mt-1 text-3xl font-bold tracking-tight">
            {album.title}
          </h1>
          <p className="mt-2 text-sm text-mute">
            <Link
              to={`/artist/${album.artistId}`}
              className="font-medium text-fg hover:underline"
            >
              {album.artistName}
            </Link>
            {album.year ? ` · ${album.year}` : ""} ·{" "}
            {plural(album.trackCount, "Titel")} ·{" "}
            {formatDuration(album.durationMs)}
          </p>

          <div className="aktionsreihe mt-4 gap-2">
            <button
              type="button"
              onClick={() => play(false)}
              disabled={!tracks.length}
              className="pill-btn is-raised is-accent aktionsknopf shrink-0"
            >
              <PlayIcon size={16} />
              {t("Abspielen")}
            </button>
            <button
              type="button"
              onClick={() => play(true)}
              disabled={!tracks.length}
              className="pill-btn is-raised aktionsknopf shrink-0"
            >
              <ShuffleIcon size={16} />
              {t("Zufällig")}
            </button>
            <button
              type="button"
              onClick={() => openAddToPlaylist(tracks.map((t) => t.id))}
              disabled={!tracks.length}
              className="pill-btn is-raised aktionsknopf aktionsknopf-weicht"
            >
              <PlusIcon size={16} />
              <span className="truncate">{t("Zu Playlist")}</span>
            </button>
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="pill-btn is-raised aktionsknopf shrink-0"
            >
              <PencilIcon size={16} />
              {t("Bearbeiten")}
            </button>
          </div>
        </div>
      </div>

      <TrackList
        tracks={tracks}
        albumNumbering
        showCover={false}
        showAlbum={false}
        showArtist={false}
        onChanged={() => void refresh()}
      />

      <AlbumEditor
        album={album}
        open={editing}
        onClose={() => setEditing(false)}
        onSaved={setAlbum}
      />
    </div>
  );
}
