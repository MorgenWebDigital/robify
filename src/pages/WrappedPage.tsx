import { useEffect, useState } from "react";
import { t } from "../lib/i18n";
import { Link } from "react-router-dom";
import { ArtistAvatar } from "../components/ArtistEditor";
import { PageHeader } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { Cover } from "../components/Cover";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  PlayIcon,
  SparkIcon,
} from "../components/Icons";
import { api, fallback } from "../lib/api";
import { albumCover } from "../lib/cover";
import { ausSchluessel, monatUndJahr } from "../lib/datum";
import { formatDuration, formatNumber, plural } from "../lib/format";
import { useLibrary } from "../store/library";
import type { Wrapped } from "../types";

type Period = "month" | "year" | "all";

/**
 * Als Funktion, nicht als feste Liste: Eine Liste auf Modulebene entsteht
 * einmal beim Laden. Wechselt der Nutzer danach die Sprache, baut die App sich
 * zwar neu auf, das Modul aber nicht, und die Beschriftungen blieben in der
 * Anfangssprache stehen.
 */
function perioden(): { id: Period; label: string }[] {
  return [
    { id: "month", label: t("Monat") },
    { id: "year", label: t("Jahr") },
    { id: "all", label: t("Gesamt") },
  ];
}

/**
 * Überschrift des Zeitraums, z. B. „August 2026“.
 *
 * Entsteht hier und nicht im Rust-Teil: Monatsnamen gehören zur Sprache der
 * Oberfläche, und dort erzeugt stand über dem russischen Rückblick weiterhin
 * „August 2026“. Der Anfangszeitpunkt reicht, welche Art Zeitraum es ist,
 * steht daneben.
 */
function zeitraumName(daten: Wrapped): string {
  const beginn = new Date(daten.start * 1000);
  if (daten.period === "month") return monatUndJahr(beginn);
  if (daten.period === "year") return String(beginn.getFullYear());
  return t("Gesamt");
}

export function WrappedPage() {
  const revision = useLibrary((s) => s.revision);
  const settings = useLibrary((s) => s.settings);
  const modus = settings?.wrappedMode ?? "all";
  /** Bei „month“ oder „year“ steht der Zeitraum fest, die Wahl entfällt. */
  const fest = modus === "month" || modus === "year";

  const [period, setPeriod] = useState<Period>("month");
  const [offset, setOffset] = useState(0);
  const [data, setData] = useState<Wrapped | null>(null);
  const [loading, setLoading] = useState(true);

  // Steht der Zeitraum fest, gilt der abgeschlossene davor: im Februar der
  // Januar, im Jahr 2026 das Jahr 2025. Ein laufender Monat ist kein
  // Rückblick, seine Zahlen ändern sich noch täglich.
  useEffect(() => {
    if (fest) {
      setPeriod(modus as Period);
      setOffset(-1);
    }
  }, [fest, modus]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .wrapped(period, offset)
      .then((value) => {
        if (!cancelled) setData(value);
      })
      .catch((error) => {
        if (!cancelled) setData(fallback(null, t("Rückblick"))(error));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [period, offset, revision]);

  const maxBucket = Math.max(
    1,
    ...(data?.buckets.map((b) => b.msPlayed) ?? [1]),
  );
  const empty = !loading && (data?.totalPlays ?? 0) === 0;
  /*
   * „Gesamt“ und „Jahr“ zählen in Monaten, „Monat“ in Tagen. Über mehrere
   * Jahre hinweg braucht ein Monat seine Jahreszahl, sonst steht an beiden
   * Enden des Verlaufs „Aug.“ und meint zwei verschiedene.
   */
  const monatsbalken = data?.period === "all";

  return (
    <div>
      <PageHeader
        actionsRechts
        eyebrow={t("Rückblick")}
        title={t("Wrapped")}
        subtitle={data ? zeitraumName(data) : undefined}
        actions={
          <div className="flex items-center gap-2">
            {/* Nicht `hidden`: Die Anzeigeart der Kapsel überstimmt das
                Merkmal, der Schalter bliebe sichtbar. */}
            {!fest && (
              <div className="pill-bar">
                {perioden().map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => {
                      setPeriod(item.id);
                      setOffset(0);
                    }}
                    aria-pressed={period === item.id}
                    className="pill-btn h-7 px-3.5 text-sm"
                  >
                    {item.label}
                  </button>
                ))}
              </div>
            )}

            {period !== "all" && (
              <div className="pill-bar">
                <button
                  type="button"
                  onClick={() => setOffset((value) => value - 1)}
                  aria-label={t("Vorheriger Zeitraum")}
                  className="pill-btn is-raised h-7 w-7"
                >
                  <ChevronLeftIcon size={16} />
                </button>
                <button
                  type="button"
                  onClick={() => setOffset((value) => Math.min(0, value + 1))}
                  disabled={offset >= 0}
                  aria-label={t("Nächster Zeitraum")}
                  className="pill-btn is-raised h-7 w-7"
                >
                  <ChevronRightIcon size={16} />
                </button>
              </div>
            )}
          </div>
        }
      />

      {loading && (
        <p className="py-16 text-center text-mute">{t("Wird ausgewertet…")}</p>
      )}

      {empty && (
        <EmptyState
          icon={SparkIcon}
          title={t("In diesem Zeitraum wurde noch nichts gehört")}
          text={
            <>
              {t(
                "Der Rückblick entsteht aus dem, was du hörst. Spiel ein paar Titel, dann füllt er sich von selbst. Welcher Zeitraum hier steht, legst du in den Einstellungen fest; dort lässt sich der Rückblick auch ganz ausblenden.",
              )}
            </>
          }
          actions={
            <>
              <Link
                to="/"
                className="pill-btn is-raised is-accent h-9 px-4 text-sm font-semibold"
              >
                {t("Zur Startseite")}
              </Link>
              <Link
                to="/settings"
                className="pill-btn is-raised h-9 px-4 text-sm font-semibold"
              >
                {t("Einstellungen")}
              </Link>
            </>
          }
        />
      )}

      {data && !loading && !empty && (
        <div className="space-y-8">
          {/* Zwei nebeneinander schon auf dem Handy. Untereinander nahmen
              die vier Zahlen 388 von 914 Bildpunkten ein — der halbe
              Bildschirm für vier Zeilen, und der Rückblick selbst begann
              erst darunter. */}
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <BigStat
              label={t("Hörzeit gesamt")}
              value={formatDuration(data.totalMs)}
              accent
            />
            <BigStat
              label={t("Wiedergaben")}
              value={formatNumber(data.totalPlays)}
            />
            <BigStat
              label={t("Verschiedene Titel")}
              value={formatNumber(data.distinctTracks)}
            />
            <BigStat
              label={t("Verschiedene Künstler")}
              value={formatNumber(data.distinctArtists)}
            />
          </div>

          {data.buckets.length > 1 && (
            <section className="surface p-5">
              <h2 className="mb-4 text-sm font-semibold">{t("Verlauf")}</h2>
              {/* Der Abstand steht in Bildpunkten und nicht als Klasse, weil
                  er bei vielen Balken weichen muss: Eine Lücke in einer
                  Flex-Reihe schrumpft nicht mit. Bei 300 Balken fraßen die
                  Lücken die ganze Breite auf, jeder Balken war null Bildpunkte
                  breit und die Karte blieb leer. */}
              <div
                className="flex h-32 items-end"
                style={{ gap: data.buckets.length > 31 ? 1 : 4 }}
              >
                {data.buckets.map((bucket) => (
                  <div
                    key={bucket.label}
                    className="group relative flex-1 rounded-t transition"
                    style={{
                      height: `${(bucket.msPlayed / maxBucket) * 100}%`,
                      minHeight: "2px",
                      background: "var(--accent)",
                      opacity: 0.75,
                    }}
                    title={`${ausSchluessel(bucket.label, monatsbalken)}: ${formatDuration(bucket.msPlayed)}`}
                  />
                ))}
              </div>
              {/* Anfang und Ende außen, der stärkste Tag darunter. Zu dritt in
                  einer Zeile standen sie auf einer Handbreite ohne Lücke
                  aneinander; in einer Sprache mit längeren Monatsnamen
                  überlappten sie. */}
              <div className="mt-2 flex justify-between text-xs text-mute">
                <span>
                  {ausSchluessel(data.buckets[0]?.label ?? "", monatsbalken)}
                </span>
                <span>
                  {ausSchluessel(
                    data.buckets[data.buckets.length - 1]?.label ?? "",
                    monatsbalken,
                  )}
                </span>
              </div>
              {data.busiestDay && (
                <p className="mt-1.5 text-center text-xs text-mute">
                  {t(
                    "Stärkster Tag: {0} ({1})",
                    ausSchluessel(data.busiestDay.label),
                    formatDuration(data.busiestDay.msPlayed),
                  )}
                </p>
              )}
            </section>
          )}

          {data.topTracks.length > 0 && (
            <section>
              <div className="mb-3 flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
                <h2 className="text-xl font-semibold tracking-tight">
                  {t("Deine Top 5 Titel")}
                </h2>
                {/* Als Aktionsreihe wie auf jeder anderen Seite: Wird es eng,
                    fällt der Knopf auf sein Zeichen zusammen, statt die
                    Überschrift in eine zweite Zeile zu drücken.

                    `grow` ist hier keine Zierde. Eine Aktionsreihe ist ein
                    Größenbehälter, und der zählt seinen Inhalt für die eigene
                    Breite nicht mit — als Flex-Kind fiel sie deshalb auf null
                    zusammen und die Zahl stand als schmale Säule am Rand.
                    Wachsen darf sie den Rest der Zeile ausfüllen. */}
                <div className="aktionsreihe grow justify-end gap-3">
                  <span className="min-w-0 truncate text-sm text-mute">
                    {t("Zusammen {0}", formatDuration(data.topTracksTotalMs))}
                  </span>
                  {/* Entfernte Titel haben keine Datei mehr, sie würden die
                      Wiedergabe nur abbrechen. Die Laufvariable heißt nicht
                      `t`: So hieße in diesem Baustein auch die Übersetzung. */}
                  <button
                    type="button"
                    onClick={() =>
                      void api.playTracks(
                        data.topTracks
                          .filter((eintrag) => !eintrag.track.deleted)
                          .map((eintrag) => eintrag.track.id),
                        0,
                      )
                    }
                    disabled={data.topTracks.every(
                      (eintrag) => eintrag.track.deleted,
                    )}
                    title={t("Abspielen")}
                    className="pill-btn is-raised is-accent aktionsknopf aktionsknopf-kurz"
                  >
                    <PlayIcon size={16} />
                    <span className="beschriftung">{t("Abspielen")}</span>
                  </button>
                </div>
              </div>

              <ol className="space-y-1">
                {data.topTracks.map((item, index) => (
                  <li
                    key={item.track.id}
                    className="flex items-center gap-4 rounded-xl p-2.5 transition hover:bg-ink-800"
                  >
                    <span className="w-6 text-center text-lg font-bold text-mute tabular-nums">
                      {index + 1}
                    </span>
                    <Cover
                      src={albumCover(item.track.albumId)}
                      alt={item.track.albumTitle}
                      seed={item.track.albumId}
                      className="h-12 w-12 shrink-0"
                      rounded="rounded-md"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-medium">{item.track.title}</p>
                      <span className="flex items-center gap-1.5 text-xs text-mute">
                        <Link
                          to={`/artist/${item.track.artistId}`}
                          className="truncate hover:text-fg hover:underline"
                        >
                          {item.track.artistName}
                        </Link>
                        {item.track.deleted && (
                          <span className="shrink-0 rounded-full border border-ink-700 px-1.5 py-0.5 text-[10px]">
                            {t("entfernt")}
                          </span>
                        )}
                      </span>
                    </div>
                    <div className="shrink-0 text-end">
                      <p className="text-sm font-medium tabular-nums">
                        {plural(item.playCount, "Mal")}
                      </p>
                      <p className="text-xs text-mute tabular-nums">
                        {formatDuration(item.msPlayed)}
                      </p>
                    </div>
                  </li>
                ))}
              </ol>
            </section>
          )}

          <div className="grid gap-6 lg:grid-cols-2">
            {data.topArtists.length > 0 && (
              <section>
                <h2 className="mb-3 text-xl font-semibold tracking-tight">
                  {t("Top-Künstler")}
                </h2>
                <ol className="space-y-1">
                  {data.topArtists.map((artist, index) => (
                    <li key={artist.artistId}>
                      <Link
                        to={`/artist/${artist.artistId}`}
                        className="flex items-center gap-4 rounded-xl p-2.5 transition hover:bg-ink-800"
                      >
                        <span className="w-6 text-center text-lg font-bold text-mute tabular-nums">
                          {index + 1}
                        </span>
                        <ArtistAvatar
                          artist={{
                            id: artist.artistId,
                            name: artist.name,
                            hasImage: artist.hasImage,
                          }}
                          className="h-11 w-11 shrink-0"
                        />
                        <div className="min-w-0 flex-1">
                          <p className="truncate font-medium">{artist.name}</p>
                          <p className="text-xs text-mute">
                            {plural(artist.trackCount, "Titel")} ·{" "}
                            {plural(artist.playCount, "Wiedergabe")}
                          </p>
                        </div>
                        <span className="shrink-0 text-sm text-mute tabular-nums">
                          {formatDuration(artist.msPlayed)}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ol>
              </section>
            )}

            {data.topAlbums.length > 0 && (
              <section>
                <h2 className="mb-3 text-xl font-semibold tracking-tight">
                  {t("Top-Releases")}
                </h2>
                <ol className="space-y-1">
                  {data.topAlbums.map((album, index) => (
                    <li key={album.albumId}>
                      <Link
                        to={`/album/${album.albumId}`}
                        className="flex items-center gap-4 rounded-xl p-2.5 transition hover:bg-ink-800"
                      >
                        <span className="w-6 text-center text-lg font-bold text-mute tabular-nums">
                          {index + 1}
                        </span>
                        <Cover
                          src={albumCover(album.albumId)}
                          alt={album.title}
                          seed={album.albumId}
                          className="h-11 w-11 shrink-0"
                          rounded="rounded-md"
                        />
                        <div className="min-w-0 flex-1">
                          <p className="truncate font-medium">{album.title}</p>
                          <p className="truncate text-xs text-mute">
                            {album.artistName}
                          </p>
                        </div>
                        {/* Wie bei Titeln und Künstlern: gehörte Zeit, nicht
                            Laufzeit. „57:08“ neben „1 Std. 42 Min.“ las sich
                            wie die Länge des Releases. */}
                        <span className="shrink-0 text-sm text-mute tabular-nums">
                          {formatDuration(album.msPlayed)}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ol>
              </section>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function BigStat({
  label,
  value,
  accent = false,
}: {
  label: string;
  value: string;
  accent?: boolean;
}) {
  /*
   * Auf dem Handy eine Stufe kleiner. In zwei Spalten bleiben 144 Bildpunkte
   * für die Zahl; „234 Std. 12 Min.“ braucht in 1.5rem deren 168 und wäre
   * abgeschnitten, in 1.25rem sind es 140.
   */
  return (
    <div className="surface px-4 py-3.5 sm:px-5 sm:py-4">
      <p className="eyebrow">{label}</p>
      <p
        className="mt-1.5 truncate text-xl font-bold tracking-tight sm:text-2xl"
        style={accent ? { color: "var(--accent)" } : undefined}
        title={value}
      >
        {value}
      </p>
    </div>
  );
}
