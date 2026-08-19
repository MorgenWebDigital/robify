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
 * Wie viele Kacheln eine Vorschau auf der Startseite zeigt.
 *
 * Die Startseite ist ein Überblick, keine Liste. Drei Kacheln passen auf einem
 * Telefon nebeneinander, ohne dass etwas abgeschnitten wirkt, und darunter
 * beginnt sofort der nächste Abschnitt statt der fünften Reihe Alben.
 */
const VORSCHAU = 3;

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
      setAlbums(albumValue);
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

          {/* Drei Kacheln, der Rest hinter „Alle ansehen“. Vorher lag hier
              eine waagerechte Reihe mit bis zu zwölf Wochen zum Schieben; auf
              einem Telefon sah man davon zweieinhalb und ahnte nicht, dass es
              weiterging. */}
          <SectionTitle
            action={
              mixes.length > VORSCHAU ? (
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
              {mixes.slice(0, VORSCHAU).map((mix) => (
                <MixKachel key={mix.weekKey} mix={mix} ohneAbspielen />
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
                  albums.length > VORSCHAU ? (
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
                {albums.slice(0, VORSCHAU).map((album) => (
                  <AlbumCard key={album.id} album={album} ohneAbspielen />
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
