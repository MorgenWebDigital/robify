import { PlaylistMosaic, ZurueckKnopf } from "../components/Cards";
import { t } from "../lib/i18n";
import { EmptyState } from "../components/EmptyState";
import { CoverPicker, type CoverChoice } from "../components/CoverPicker";
import { bustCoverCache, dataUrl, playlistCover } from "../lib/cover";
import { useEffect, useState } from "react";
import {
  Link,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import {
  PencilIcon,
  PlaylistIcon,
  PlayIcon,
  ShuffleIcon,
  TrashIcon,
} from "../components/Icons";
import { Button, Field, inputClass, Modal } from "../components/Modal";
import { TrackList } from "../components/TrackList";
import { api, errorMessage, fallback } from "../lib/api";
import { formatDuration, plural } from "../lib/format";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type { Playlist, Track } from "../types";

export function PlaylistDetail() {
  const { id } = useParams();
  // „Zum Song“ aus dem Hinzufügen-Dialog landet mit ?track=… hier.
  const [suchparameter] = useSearchParams();
  const hervorheben = Number(suchparameter.get("track")) || null;
  const playlistId = Number(id);
  const navigate = useNavigate();
  const revision = useLibrary((s) => s.revision);
  const reloadPlaylists = useLibrary((s) => s.reloadPlaylists);
  const notify = useUi((s) => s.notify);
  const notifyUndo = useUi((s) => s.notifyUndo);

  const [playlist, setPlaylist] = useState<Playlist | null>(null);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [editing, setEditing] = useState(false);
  const [loeschen, setLoeschen] = useState(false);
  const [nichtMehrFragen, setNichtMehrFragen] = useState(false);
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [cover, setCover] = useState<CoverChoice | null>(null);
  const [coverEntfernt, setCoverEntfernt] = useState(false);

  const load = async () => {
    const [playlistValue, trackValue] = await Promise.all([
      api.getPlaylist(playlistId).catch(fallback(null, t("Playlist"))),
      api
        .playlistTracks(playlistId)
        .catch(fallback([], t("Titel der Playlist"))),
    ]);
    setPlaylist(playlistValue);
    setTracks(trackValue);
    if (playlistValue) {
      setName(playlistValue.name);
      setDescription(playlistValue.description ?? "");
      setCover(null);
      setCoverEntfernt(false);
    }
  };

  useEffect(() => {
    if (!Number.isFinite(playlistId)) return;
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playlistId, revision]);

  if (!playlist) {
    return (
      <p className="py-20 text-center text-mute">
        {t("Playlist wird geladen…")}
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

  /** Neue Reihenfolge nach dem Ziehen sichern. */
  const neuOrdnen = async (ids: number[]) => {
    // Sofort anzeigen, damit die Zeile nicht zurückspringt, während die
    // Datenbank noch schreibt.
    setTracks(
      ids.map((id) => tracks.find((t) => t.id === id)!).filter(Boolean),
    );
    try {
      await api.reorderPlaylist(playlist.id, ids);
    } catch (error) {
      notify(errorMessage(error), "error");
      await load();
    }
  };

  const removeTrack = async (track: Track) => {
    // Die alte Reihenfolge merken, sonst landet der Titel beim Zurücklegen
    // hinten statt an seiner Stelle.
    const vorher = tracks.map((t) => t.id);
    try {
      await api.removeFromPlaylist(playlist.id, track.id);
      await Promise.all([load(), reloadPlaylists()]);
      notifyUndo(t("„{0}“ entfernt", track.title), async () => {
        try {
          await api.addToPlaylist(playlist.id, [track.id]);
          await api.reorderPlaylist(playlist.id, vorher);
          await Promise.all([load(), reloadPlaylists()]);
          notify(t("„{0}“ zurückgelegt", track.title), "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        }
      });
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  const save = async () => {
    try {
      await api.updatePlaylist(
        playlist.id,
        name.trim(),
        description.trim() || null,
        cover,
        coverEntfernt,
      );
      // Sonst zeigt der Zwischenspeicher weiter das alte Bild.
      bustCoverCache();
      await Promise.all([load(), reloadPlaylists()]);
      setEditing(false);
      notify(t("Playlist aktualisiert"), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  const remove = async () => {
    setLoeschen(false);
    if (nichtMehrFragen) {
      setNichtMehrFragen(false);
      void saveSetting("confirmDelete", false);
    }
    try {
      await api.deletePlaylist(playlist.id);
      await reloadPlaylists();
      navigate("/playlists");
      notifyUndo(t("„{0}“ gelöscht", playlist.name), async () => {
        try {
          await api.restorePlaylist(playlist.id);
          await reloadPlaylists();
          navigate(`/playlist/${playlist.id}`);
          notify(t("„{0}“ wiederhergestellt", playlist.name), "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        }
      });
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  return (
    <div>
      <ZurueckKnopf ziel="/playlists" />
      <div className="mb-8 flex flex-wrap items-end gap-6">
        <PlaylistMosaic
          albumIds={playlist.coverAlbumIds}
          name={playlist.name}
          size="h-44 w-44"
          coverSrc={playlist.hasCover ? playlistCover(playlist.id) : null}
        />
        <div className="min-w-0 flex-1">
          <p className="eyebrow">{t("Playlist")}</p>
          <h1 className="mt-1 text-3xl font-bold tracking-tight">
            {playlist.name}
          </h1>
          {playlist.description && (
            <p className="mt-2 max-w-xl text-sm text-mute">
              {playlist.description}
            </p>
          )}
          <p className="mt-2 text-sm text-mute">
            {plural(playlist.trackCount, "Titel")}
            {playlist.durationMs > 0 &&
              ` · ${formatDuration(playlist.durationMs)}`}
          </p>

          {/* Erst was die Playlist verwaltet, dann was sie hört, alle vier
              dicht beieinander. Ohne Lücke vor dem Abspielen: Sie schob es
              zwar an die Kante, riss die Reihe dabei aber auseinander.

              Alle klein: Bei Enge fallen sie auf ihr Zeichen zusammen, wie in
              der Bibliothek. Vier beschriftete Knöpfe nebeneinander passen auf
              eine Handbreite ohnehin nicht. */}
          <div className="aktionsreihe mt-4 gap-2">
            <button
              type="button"
              onClick={() => setEditing(true)}
              title={t("Bearbeiten")}
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <PencilIcon size={16} />
              <span className="beschriftung truncate">{t("Bearbeiten")}</span>
            </button>
            <button
              type="button"
              onClick={() => {
                if (settings && !settings.confirmDelete) void remove();
                else setLoeschen(true);
              }}
              title={t("Löschen")}
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <TrashIcon size={16} />
              <span className="beschriftung truncate">{t("Löschen")}</span>
            </button>
            <button
              type="button"
              onClick={() => play(true)}
              disabled={!tracks.length}
              title={t("Zufällig")}
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <ShuffleIcon size={16} />
              <span className="beschriftung truncate">{t("Zufällig")}</span>
            </button>
            <button
              type="button"
              onClick={() => play(false)}
              disabled={!tracks.length}
              title={t("Abspielen")}
              className="pill-btn is-raised is-accent aktionsknopf aktionsknopf-kurz"
            >
              <PlayIcon size={16} />
              <span className="beschriftung truncate">{t("Abspielen")}</span>
            </button>
          </div>
        </div>
      </div>

      <TrackList
        tracks={tracks}
        onReorder={(ids) => void neuOrdnen(ids)}
        highlightTrackId={hervorheben}
        onRemove={(track) => void removeTrack(track)}
        removeLabel={t("Aus Playlist entfernen")}
        onChanged={() => void load()}
        emptyMessage={
          <EmptyState
            icon={PlaylistIcon}
            title={t("Diese Playlist ist noch leer")}
            text={t(
              "Titel kommen über das Plus in jeder Titelliste hinein, in der Bibliothek, auf einer Albumseite oder bei einem Künstler.",
            )}
            actions={
              <Link
                to="/library"
                className="pill-btn is-raised is-accent aktionsknopf shrink-0"
              >
                {t("Zur Bibliothek")}
              </Link>
            }
          />
        }
      />

      <Modal
        open={editing}
        title={t("Playlist bearbeiten")}
        onClose={() => setEditing(false)}
        width="max-w-md"
        footer={
          <>
            <Button onClick={() => setEditing(false)} variant="ghost">
              {t("Abbrechen")}
            </Button>
            <Button
              onClick={() => void save()}
              variant="primary"
              disabled={!name.trim()}
            >
              {t("Speichern")}
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Field label={t("Cover")}>
            <CoverPicker
              preview={
                coverEntfernt
                  ? null
                  : (dataUrl(cover?.base64 ?? null, cover?.mime ?? null) ??
                    (playlist.hasCover ? playlistCover(playlist.id) : null))
              }
              onPick={(wahl) => {
                setCover(wahl);
                setCoverEntfernt(false);
              }}
              onRemove={() => {
                setCover(null);
                setCoverEntfernt(true);
              }}
            />
          </Field>
          <Field label={t("Name")}>
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label={t("Beschreibung")}>
            <textarea
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              rows={3}
              className={inputClass}
            />
          </Field>
        </div>
      </Modal>

      <Modal
        open={loeschen}
        title={t("Playlist löschen?")}
        subtitle={playlist.name}
        onClose={() => setLoeschen(false)}
        width="max-w-md"
        footer={
          <>
            <Button onClick={() => setLoeschen(false)} variant="ghost">
              {t("Abbrechen")}
            </Button>
            <Button onClick={() => void remove()} variant="outline">
              {t("Löschen")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-mute">
          {t(
            "Die Playlist verschwindet aus deiner Sammlung. Die Titel bleiben in der Bibliothek. Kurz danach lässt sich der Griff über die Meldung noch zurücknehmen.",
          )}
        </p>
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
    </div>
  );
}
