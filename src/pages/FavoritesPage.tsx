import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { PageHeader } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { HeartIcon, PlayIcon } from "../components/Icons";
import { TrackList } from "../components/TrackList";
import { api, errorMessage, fallback } from "../lib/api";
import { formatDuration, plural } from "../lib/format";
import { t } from "../lib/i18n";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type { Track } from "../types";

// every track marked with the heart.
//
// no playlist in the database but a view onto the marking itself: that way
// there is only one truth, and a heart taken away disappears from here at
// once
export function FavoritesPage() {
  const revision = useLibrary((s) => s.revision);
  const refresh = useLibrary((s) => s.refresh);
  const notify = useUi((s) => s.notify);
  /** `null` while not loaded yet, otherwise the empty state flashes up */
  const [tracks, setTracks] = useState<Track[] | null>(null);

  useEffect(() => {
    void api
      .favoriteTracks()
      .then(setTracks)
      .catch((error) => setTracks(fallback([], t("Favoriten"))(error)));
  }, [revision]);

  // stores the new order after dragging.
  //
  // show first, write afterwards: otherwise the row jumps back to its old
  // place while the database is still working. does the write fail, the
  // rebuild brings the true state back
  const neuOrdnen = async (ids: number[]) => {
    const vorher = tracks ?? [];
    setTracks(
      ids.map((id) => vorher.find((t) => t.id === id)!).filter(Boolean),
    );
    try {
      await api.reorderFavorites(ids);
    } catch (error) {
      notify(errorMessage(error), "error");
      setTracks(await api.favoriteTracks().catch(fallback([], t("Favoriten"))));
    }
  };

  const gesamt = (tracks ?? []).reduce(
    (summe, track) => summe + track.durationMs,
    0,
  );

  return (
    <div>
      <PageHeader
        actionsRechts
        eyebrow={t("Sammlung")}
        title={t("Favoriten")}
        subtitle={
          tracks && tracks.length > 0
            ? `${plural(tracks.length, "Titel")} · ${formatDuration(gesamt)}`
            : undefined
        }
        actions={
          tracks && tracks.length > 0 ? (
            <button
              type="button"
              onClick={() =>
                void api.playTracks(
                  tracks.map((t) => t.id),
                  0,
                )
              }
              className="pill-btn is-raised is-accent h-9 min-w-40 px-4 text-sm font-semibold"
            >
              <PlayIcon size={16} />
              {t("Abspielen")}
            </button>
          ) : undefined
        }
      />

      {tracks !== null && (
        <TrackList
          tracks={tracks}
          onReorder={(ids) => void neuOrdnen(ids)}
          onChanged={() => void refresh()}
          emptyMessage={
            <EmptyState
              icon={HeartIcon}
              title={t("Noch keine Favoriten")}
              text={t(
                "Markiere Titel über das Menü hinter den drei Punkten. Sie sammeln sich hier, ohne dass du eine Playlist anlegen musst.",
              )}
              actions={
                <Link
                  to="/library"
                  className="pill-btn is-raised is-accent h-9 px-4 text-sm font-semibold"
                >
                  {t("Zur Bibliothek")}
                </Link>
              }
            />
          }
        />
      )}
    </div>
  );
}
