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

/**
 * Alle mit dem Herz markierten Titel.
 *
 * Keine Playlist in der Datenbank, sondern eine Sicht auf die Markierung
 * selbst: So gibt es nur eine Wahrheit, und ein entferntes Herz verschwindet
 * hier sofort mit.
 */
export function FavoritesPage() {
  const revision = useLibrary((s) => s.revision);
  const refresh = useLibrary((s) => s.refresh);
  const notify = useUi((s) => s.notify);
  /** `null`, solange noch nicht geladen, sonst blitzt der Leerzustand auf. */
  const [tracks, setTracks] = useState<Track[] | null>(null);

  useEffect(() => {
    void api
      .favoriteTracks()
      .then(setTracks)
      .catch((error) => setTracks(fallback([], t("Favoriten"))(error)));
  }, [revision]);

  /**
   * Neue Reihenfolge nach dem Ziehen sichern.
   *
   * Erst anzeigen, dann schreiben: Sonst springt die Zeile an ihren alten
   * Platz zurück, während die Datenbank noch arbeitet. Geht das Schreiben
   * schief, holt der Neuaufbau den wahren Stand zurück.
   */
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
