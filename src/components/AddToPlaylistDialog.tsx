import { useEffect, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage, fallback } from "../lib/api";
import { plural } from "../lib/format";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import { PlaylistMosaic } from "./Cards";
import { playlistCover } from "../lib/cover";
import { PlusIcon } from "./Icons";
import { Button, Modal } from "./Modal";
import { PlaylistCreateDialog } from "./PlaylistCreateDialog";

export function AddToPlaylistDialog() {
  const { addToPlaylistIds, openAddToPlaylist, notify } = useUi();
  const playlists = useLibrary((s) => s.playlists);
  const reloadPlaylists = useLibrary((s) => s.reloadPlaylists);
  const [anlegen, setAnlegen] = useState(false);
  const [busy, setBusy] = useState(false);
  /** playlist id to how many of the chosen tracks already lie there */
  const [enthalten, setEnthalten] = useState<Map<number, number>>(new Map());

  const trackIds = addToPlaylistIds ?? [];
  const close = () => openAddToPlaylist(null);

  // duplicate entries are skipped silently on insert, and without a hint one
  // wonders why nothing happens
  useEffect(() => {
    if (addToPlaylistIds === null) return;
    let cancelled = false;
    void api
      .playlistsContaining(addToPlaylistIds)
      .then((paare) => {
        if (!cancelled) setEnthalten(new Map(paare));
      })
      .catch((error) => {
        if (!cancelled)
          setEnthalten(fallback(new Map<number, number>(), "Playlists")(error));
      });
    return () => {
      cancelled = true;
    };
  }, [addToPlaylistIds]);

  const addTo = async (playlistId: number, playlistName: string) => {
    setBusy(true);
    try {
      await api.addToPlaylist(playlistId, trackIds);
      await reloadPlaylists();
      notify(
        t(
          "{0} zu „{1}“ hinzugefügt",
          plural(trackIds.length, "Titel"),
          playlistName,
        ),
        "success",
      );
      close();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  };

  /** after creating it, the chosen tracks travel straight into it */
  const einfuellen = async (playlist: { id: number; name: string }) => {
    try {
      await api.addToPlaylist(playlist.id, trackIds);
      await reloadPlaylists();
      notify(
        t(
          "{0} zu „{1}“ hinzugefügt",
          plural(trackIds.length, "Titel"),
          playlist.name,
        ),
        "success",
      );
      close();
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  return (
    <>
      <Modal
        open={addToPlaylistIds !== null}
        title={t("Zu Playlist hinzufügen")}
        subtitle={plural(trackIds.length, "Titel ausgewählt")}
        onClose={close}
        width="max-w-md"
      >
        <div className="space-y-4">
          <Button
            onClick={() => setAnlegen(true)}
            variant="primary"
            className="w-full"
          >
            <PlusIcon size={16} />
            {t("Neue Playlist")}
          </Button>

          {playlists.length === 0 ? (
            <p className="py-6 text-center text-sm text-mute">
              {t("Noch keine Playlists vorhanden.")}
            </p>
          ) : (
            <ul className="max-h-72 space-y-1 overflow-y-auto">
              {playlists.map((playlist) => {
                const schonDrin = enthalten.get(playlist.id) ?? 0;
                const alleDrin = schonDrin >= trackIds.length;

                return (
                  <li key={playlist.id}>
                    <button
                      type="button"
                      disabled={busy || alleDrin}
                      onClick={() => void addTo(playlist.id, playlist.name)}
                      className="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-start transition hover:bg-ink-700 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-transparent"
                    >
                      {/* the same image as in the sidebar: a playlist is
                          recognised faster by its cover than by its name. */}
                      <PlaylistMosaic
                        albumIds={playlist.coverAlbumIds}
                        name={playlist.name}
                        size="h-10 w-10"
                        coverSrc={
                          playlist.hasCover ? playlistCover(playlist.id) : null
                        }
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm font-medium">
                          {playlist.name}
                        </span>
                        {schonDrin > 0 && (
                          <span className="block text-xs text-warning">
                            {alleDrin
                              ? trackIds.length === 1
                                ? t("Bereits in dieser Playlist")
                                : t("Alle bereits enthalten")
                              : t(
                                  "{0} von {1} bereits enthalten",
                                  schonDrin,
                                  trackIds.length,
                                )}
                          </span>
                        )}
                      </span>
                      <span className="shrink-0 text-xs text-mute">
                        {plural(playlist.trackCount, "Titel")}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </Modal>

      {/* stands next to the dialog, not inside it: that keeps it visible
          while the add dialog is open in the background. */}
      <PlaylistCreateDialog
        open={anlegen}
        onClose={() => setAnlegen(false)}
        onCreated={einfuellen}
      />
    </>
  );
}
