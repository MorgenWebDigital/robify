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
import {
  formatDuration,
  formatNumber,
  formatTime,
  plural,
} from "../lib/format";
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
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
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
              <div className="flex h-32 items-end gap-1">
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
                    title={`${ausSchluessel(bucket.label)}: ${formatDuration(bucket.msPlayed)}`}
                  />
                ))}
              </div>
              <div className="mt-2 flex justify-between text-xs text-mute">
                <span>{ausSchluessel(data.buckets[0]?.label ?? "")}</span>
                {data.busiestDay && (
                  <span>
                    {t(
                      "Stärkster Tag: {0} ({1})",
                      ausSchluessel(data.busiestDay.label),
                      formatDuration(data.busiestDay.msPlayed),
                    )}
                  </span>
                )}
                <span>
                  {ausSchluessel(
                    data.buckets[data.buckets.length - 1]?.label ?? "",
                  )}
                </span>
              </div>
            </section>
          )}

          {data.topTracks.length > 0 && (
            <section>
              <div className="mb-3 flex flex-wrap items-baseline justify-between gap-3">
                <h2 className="text-xl font-semibold tracking-tight">
                  {t("Deine Top 5 Titel")}
                </h2>
                <div className="flex items-center gap-3">
                  <span className="text-sm text-mute">
                    {t("Zusammen {0}", formatDuration(data.topTracksTotalMs))}
                  </span>
                  {/* Entfernte Titel haben keine Datei mehr, sie würden die
                      Wiedergabe nur abbrechen. */}
                  <button
                    type="button"
                    onClick={() =>
                      void api.playTracks(
                        data.topTracks
                          .filter((t) => !t.track.deleted)
                          .map((t) => t.track.id),
                        0,
                      )
                    }
                    disabled={data.topTracks.every((t) => t.track.deleted)}
                    className="pill-btn is-raised is-accent h-9 px-4 text-sm font-semibold"
                  >
                    <PlayIcon size={16} />
                    {t("Abspielen")}
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
                        <span className="shrink-0 text-sm text-mute tabular-nums">
                          {formatTime(album.msPlayed)}
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
  return (
    <div className="surface px-5 py-4">
      <p className="eyebrow">{label}</p>
      <p
        className="mt-1.5 truncate text-2xl font-bold tracking-tight"
        style={accent ? { color: "var(--accent)" } : undefined}
        title={value}
      >
        {value}
      </p>
    </div>
  );
}
