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

// a function and not a fixed list: a list at module level comes into being
// once at load time. does the user switch language afterwards, the app
// rebuilds itself but the module does not, and the labels would stay in the
// starting language
function perioden(): { id: Period; label: string }[] {
  return [
    { id: "month", label: t("Monat") },
    { id: "year", label: t("Jahr") },
    { id: "all", label: t("Gesamt") },
  ];
}

// the heading of the period, "August 2026" for instance.
//
// grows here and not on the rust side: month names belong to the language of
// the ui, and generated there "August 2026" still stood above the russian
// review. the starting point is enough, what kind of period it is stands next
// to it
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
  /** with "month" or "year" the period is fixed and the choice is dropped. */
  const fest = modus === "month" || modus === "year";

  const [period, setPeriod] = useState<Period>("month");
  const [offset, setOffset] = useState(0);
  const [data, setData] = useState<Wrapped | null>(null);
  const [loading, setLoading] = useState(true);

  // where the period is fixed, the completed one before it applies: in
  // february that is january, in 2026 the year 2025. a running month is no
  // review, its numbers still change daily
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
  // all-time and year count in months, month counts in days. across several
  // years a month needs its year, otherwise "Aug." stands at both ends of the
  // history meaning two different ones
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
            {/* not `hidden`: the display type of the pill bar outvotes the
                attribute and the switch would stay visible. */}
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
          {/* two side by side on a phone already. under each other the four
              numbers took 388 of 914 pixels, half the screen for four rows,
              and the review itself only started below that. */}
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
              {/* the gap stands in pixels and not as a class because it has
                  to give way with many bars: a gap in a flex row does not
                  shrink along. with 300 bars the gaps ate the whole width,
                  every bar was zero pixels wide and the card stayed
                  empty. */}
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
              {/* start and end on the outside, the strongest day below. as
                  three in one row they stood against each other without a gap
                  on a hand's width, and in a language with longer month names
                  they overlapped. */}
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
                {/* an action row as on every other page: does it get tight,
                    the button collapses onto its icon instead of pushing the
                    heading into a second line.

                    `grow` is no decoration here. an action row is a size
                    container, and one of those does not count its content
                    towards its own width, so as a flex child it collapsed to
                    zero and the number stood at the edge as a narrow column.
                    growing lets it fill the rest of the row. */}
                <div className="aktionsreihe grow justify-end gap-3">
                  <span className="min-w-0 truncate text-sm text-mute">
                    {t("Zusammen {0}", formatDuration(data.topTracksTotalMs))}
                  </span>
                  {/* removed tracks have no file any more, they would only
                      break the playback off. the loop variable is not called
                      `t`, that is the name of the translation in this
                      component. */}
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
                        {/* as with tracks and artists: time listened, not
                            running time. "57:08" next to "1 Std. 42 Min." read
                            like the length of the release. */}
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
  // one step smaller on a phone. in two columns 144 pixels are left for the
  // number, and "234 Std. 12 Min." needs 168 of them at 1.5rem and would be
  // cut off, while at 1.25rem it is 140
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
