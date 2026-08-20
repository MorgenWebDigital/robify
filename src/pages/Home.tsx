import { useEffect, useState } from "react";
import { t } from "../lib/i18n";
import { Link } from "react-router-dom";
import {
  AlbumCard,
  Grid,
  MixKachel,
  PageHeader,
  SectionTitle,
} from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { SparkIcon } from "../components/Icons";
import { TrackList } from "../components/TrackList";
import { api, fallback } from "../lib/api";
import { begruessung, wochentagUndTag } from "../lib/datum";
import { formatDuration, plural } from "../lib/format";
import { useLibrary } from "../store/library";
import type { Album, Track, WeeklyMixSummary } from "../types";

/**
 * how many tiles a preview on the home page shows.
 *
 * the home page is an overview, not a list. six fit in one row on a desktop
 * and only three on a phone, where the trailing ones stay hidden instead of
 * starting a second row on their own. below it the next section begins either
 * way.
 *
 * six and not five: the row fills the width whatever the count, so a tile
 * less means a cover larger. at five they stood as large as on a page of
 * their own.
 */
const VORSCHAU = 6;

/** this many of them are visible on a phone. */
const VORSCHAU_SCHMAL = 3;

/** hides the trailing tiles while the row holds three only. */
function nurBreit(stelle: number): string {
  return stelle >= VORSCHAU_SCHMAL ? "max-lg:hidden" : "";
}

/**
 * how many rows a preview shows as a list.
 *
 * more than with the tiles because a row is flatter: five of them take about
 * as much room as one row of tiles, and the home page stays an overview
 * either way. eight stood here before, and the two sections together filled
 * more than a screen.
 */
const VORSCHAU_ZEILEN = 5;

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
      api
        .recentlyPlayed(VORSCHAU_ZEILEN)
        .catch(fallback([], t("Zuletzt gehört"))),
      api
        .listTracks(undefined, VORSCHAU_ZEILEN)
        .catch(fallback([], t("Titel"))),
      api.listAlbums().catch(fallback([], t("Releases"))),
    ]).then(([mixValue, playedValue, recentValue, albumValue]) => {
      if (cancelled) return;
      setMixes(mixValue);
      setPlayed(playedValue);
      setRecent(recentValue);
      setAlbums(albumValue);
    });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  const geladen = useLibrary((s) => s.geladen);
  // judge only once the numbers are there: before the first fetch a zero
  // stands there, and that would look like an empty library
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

          {/* three tiles, the rest behind "view all". a horizontal row with
              up to twelve weeks to swipe lay here before, and on a phone one
              saw two and a half of them without suspecting that it went
              on. */}
          <SectionTitle
            action={
              mixes.length > VORSCHAU_SCHMAL ? (
                <Link
                  to="/mixes"
                  className="text-sm text-mute hover:text-fg hover:underline"
                >
                  {t("Alle ansehen")}
                </Link>
              ) : undefined
            }
          >
            {t("Wochenmix")}
          </SectionTitle>

          {mixes.length > 0 ? (
            <Grid vorschau>
              {mixes.slice(0, VORSCHAU).map((mix, stelle) => (
                <div key={mix.weekKey} className={nurBreit(stelle)}>
                  <MixKachel mix={mix} ohneAbspielen />
                </div>
              ))}
            </Grid>
          ) : (
            <p className="rounded-xl border border-dashed border-ink-700 px-5 py-8 text-center text-sm text-mute">
              {t("Der Wochenmix entsteht, sobald du etwas gehört hast.")}
            </p>
          )}

          {albums.length > 0 && (
            <>
              <SectionTitle
                action={
                  albums.length > VORSCHAU_SCHMAL ? (
                    <Link
                      to="/library"
                      className="text-sm text-mute hover:text-fg hover:underline"
                    >
                      {t("Alle ansehen")}
                    </Link>
                  ) : undefined
                }
              >
                {t("Aus deiner Bibliothek")}
              </SectionTitle>
              <Grid vorschau>
                {albums.slice(0, VORSCHAU).map((album, stelle) => (
                  <div key={album.id} className={nurBreit(stelle)}>
                    <AlbumCard album={album} ohneAbspielen />
                  </div>
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
