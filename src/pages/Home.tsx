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
 * Die Startseite ist ein Überblick, keine Liste. Fünf passen am Rechner in
 * eine Reihe, auf einem Telefon nur drei; die beiden hinteren bleiben dort
 * verborgen, statt allein eine zweite Reihe anzufangen. Darunter beginnt so
 * oder so gleich der nächste Abschnitt.
 */
const VORSCHAU = 5;

/** So viele davon sind auf einem Telefon zu sehen. */
const VORSCHAU_SCHMAL = 3;

/** Blendet die hinteren Kacheln aus, solange die Reihe nur drei fasst. */
function nurBreit(stelle: number): string {
  return stelle >= VORSCHAU_SCHMAL ? "max-lg:hidden" : "";
}

/**
 * Wie viele Zeilen eine Vorschau als Liste zeigt.
 *
 * Mehr als bei den Kacheln, weil eine Zeile flacher ist: Fünf davon nehmen
 * etwa so viel Platz wie eine Reihe Kacheln, und die Startseite bleibt in
 * beiden Fällen ein Überblick. Vorher standen hier acht, und die zwei
 * Abschnitte zusammen füllten mehr als einen Bildschirm.
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
