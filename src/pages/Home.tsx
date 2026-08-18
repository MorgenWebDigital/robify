import { useEffect, useState } from "react";
import { t } from "../lib/i18n";
import { Link } from "react-router-dom";
import {
  AlbumCard,
  Grid,
  PageHeader,
  PlaylistMosaic,
  SectionTitle,
} from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { PlayIcon, SparkIcon } from "../components/Icons";
import { TrackList } from "../components/TrackList";
import { api, fallback } from "../lib/api";
import { begruessung, wochentagUndTag } from "../lib/datum";
import { formatDuration, plural } from "../lib/format";
import { mixName } from "../lib/mix";
import { useLibrary } from "../store/library";
import type { Album, Track, WeeklyMixSummary } from "../types";

export function Home() {
  const revision = useLibrary((s) => s.revision);
  const stats = useLibrary((s) => s.stats);

  const [mixes, setMixes] = useState<WeeklyMixSummary[]>([]);
  const [played, setPlayed] = useState<Track[]>([]);
  const [recent, setRecent] = useState<Track[]>([]);
  const [albums, setAlbums] = useState<Album[]>([]);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      api.weeklyMixes(12).catch(fallback([], t("Wochenmixe"))),
      api.recentlyPlayed(8).catch(fallback([], t("Zuletzt gehört"))),
      api.listTracks(undefined, 8).catch(fallback([], t("Titel"))),
      api.listAlbums().catch(fallback([], t("Releases"))),
    ]).then(([mixValue, playedValue, recentValue, albumValue]) => {
      if (cancelled) return;
      setMixes(mixValue);
      setPlayed(playedValue);
      setRecent(recentValue);
      setAlbums(albumValue.slice(0, 10));
    });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  const geladen = useLibrary((s) => s.geladen);
  // Erst urteilen, wenn die Zahlen da sind: Vor dem ersten Abruf steht dort
  // eine Null, und die sähe aus wie eine leere Bibliothek.
  const empty = geladen && (stats?.trackCount ?? 0) === 0;

  return (
    <div>
      <PageHeader
        eyebrow={wochentagUndTag(new Date())}
        title={begruessung()}
        subtitle={
          stats
            ? `${plural(stats.trackCount, "Titel")} · ${plural(stats.artistCount, "Künstler")} · ${t("{0} Musik", formatDuration(stats.totalDurationMs))}`
            : undefined
        }
      />

      {!geladen ? null : empty ? (
        <EmptyState
          icon={SparkIcon}
          title={t("Deine Bibliothek ist noch leer")}
          text={t(
            "Importiere vorhandene Musik oder lade Titel über den Downloader. Wochenmix, Rückblick und Künstlerseiten entstehen dann von selbst.",
          )}
          actions={
            <>
              <Link
                to="/library"
                className="pill-btn is-raised is-accent h-9 px-4 text-sm font-semibold"
              >
                {t("Musik importieren")}
              </Link>
              <Link
                to="/downloader"
                className="pill-btn is-raised h-9 px-4 text-sm font-semibold"
              >
                {t("Zum Downloader")}
              </Link>
            </>
          }
        />
      ) : (
        <>
          {played.length > 0 && (
            <>
              <SectionTitle>{t("Zuletzt gespielt")}</SectionTitle>
              <TrackList tracks={played} />
            </>
          )}

          <SectionTitle>{t("Wochenmix")}</SectionTitle>

          {mixes.length > 0 ? (
            // Waagerechte Reihe: links die laufende Woche, rechts die älteren.
            <ul className="-mx-1 flex snap-x gap-3 overflow-x-auto px-1 pb-2">
              {mixes.map((mix) => (
                <li key={mix.weekKey} className="w-40 shrink-0 snap-start">
                  <Link
                    to={`/mix/${mix.offset}`}
                    className="group block rounded-xl p-2 transition hover:bg-ink-800"
                  >
                    <div className="relative">
                      <PlaylistMosaic
                        albumIds={mix.coverAlbumIds}
                        name={mixName(mix)}
                        size="h-36 w-36"
                      />
                      <span className="accent-bg absolute end-2 bottom-2 grid h-10 w-10 translate-y-2 place-items-center rounded-full opacity-0 shadow-xl transition group-hover:translate-y-0 group-hover:opacity-100">
                        <PlayIcon size={16} />
                      </span>
                    </div>
                    <p className="mt-2 truncate text-sm font-medium">
                      {mixName(mix)}
                    </p>
                    <p className="truncate text-xs text-mute">
                      {mix.offset === 0
                        ? t("Diese Woche")
                        : plural(mix.trackCount, "Titel")}
                    </p>
                  </Link>
                </li>
              ))}
            </ul>
          ) : (
            <p className="rounded-xl border border-dashed border-ink-700 px-5 py-8 text-center text-sm text-mute">
              {t("Der Wochenmix entsteht, sobald du etwas gehört hast.")}
            </p>
          )}

          {albums.length > 0 && (
            <>
              <SectionTitle
                action={
                  <Link
                    to="/library"
                    className="text-sm text-mute hover:text-fg hover:underline"
                  >
                    {t("Alle ansehen")}
                  </Link>
                }
              >
                {t("Aus deiner Bibliothek")}
              </SectionTitle>
              <Grid>
                {albums.map((album) => (
                  <AlbumCard key={album.id} album={album} />
                ))}
              </Grid>
            </>
          )}

          {recent.length > 0 && (
            <>
              <SectionTitle>{t("Zuletzt hinzugefügt")}</SectionTitle>
              <TrackList tracks={recent} />
            </>
          )}
        </>
      )}
    </div>
  );
}
