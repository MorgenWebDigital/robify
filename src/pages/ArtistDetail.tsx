import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { useParams } from "react-router-dom";
import {
  AlbumCard,
  Grid,
  PageHeader,
  SectionTitle,
  ZurueckKnopf,
} from "../components/Cards";
import { ArtistAvatar, ArtistEditor } from "../components/ArtistEditor";
import {
  DownloadIcon,
  PencilIcon,
  PlayIcon,
  ShuffleIcon,
} from "../components/Icons";
import { TrackList } from "../components/TrackList";
import { api, errorMessage, fallback } from "../lib/api";
import { bustCoverCache } from "../lib/cover";
import { useUi } from "../store/ui";
import { formatDuration, plural } from "../lib/format";
import { useLibrary } from "../store/library";
import type { Album, Artist, ReleaseType, Track } from "../types";

// the order of the sections on the artist page.
//
// a function and not a fixed list: a list at module level comes into being
// once at load time, and the headings would stay in the starting language
// after a language switch
function abschnitte(): { type: ReleaseType; title: string }[] {
  return [
    { type: "album", title: t("Alben") },
    { type: "ep", title: t("EPs") },
    { type: "single", title: t("Singles") },
  ];
}

/**
 * how many tiles one kind of release shows before a show-more button comes.
 *
 * the same number as on the home page: three fit side by side on a phone
 * without anything looking cut off.
 */
const VORSCHAU = 3;

/** how many tracks stand under the popular heading */
const VORSCHAU_TITEL = 5;

export function ArtistDetail() {
  const { id } = useParams();
  const artistId = Number(id);
  const revision = useLibrary((s) => s.revision);
  const refresh = useLibrary((s) => s.refresh);

  const [artist, setArtist] = useState<Artist | null>(null);
  const [releases, setReleases] = useState<Album[]>([]);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [features, setFeatures] = useState<Track[]>([]);
  const [editing, setEditing] = useState(false);
  const [fetching, setFetching] = useState(false);
  /** which release types show all of their tiles */
  const [entfaltet, setEntfaltet] = useState<string[]>([]);
  const notify = useUi((s) => s.notify);

  useEffect(() => {
    if (!Number.isFinite(artistId)) return;
    let cancelled = false;
    void Promise.all([
      api.getArtist(artistId).catch(fallback(null, t("Künstler"))),
      api.artistReleases(artistId).catch(fallback([], t("Releases"))),
      api.artistTracks(artistId).catch(fallback([], t("Titel"))),
      api.artistFeatures(artistId).catch(fallback([], t("Gastauftritte"))),
    ]).then(([artistValue, releaseValue, trackValue, featureValue]) => {
      if (cancelled) return;
      setArtist(artistValue);
      setReleases(releaseValue);
      setTracks(trackValue);
      setFeatures(featureValue);
    });
    return () => {
      cancelled = true;
    };
  }, [artistId, revision]);

  if (!artist) {
    return (
      <p className="py-20 text-center text-mute">
        {t("Künstler wird geladen…")}
      </p>
    );
  }

  const totalMs = tracks.reduce((sum, track) => sum + track.durationMs, 0);
  // they come sorted by plays from the rust side already, so the first five
  // are the most played
  const topTracks = tracks.slice(0, VORSCHAU_TITEL);

  /** fetches image and description in one step */
  const fetchMetadata = async () => {
    setFetching(true);
    try {
      const updated = await api.fetchArtistMetadata(artist.id);
      bustCoverCache();
      setArtist(updated);
      notify(
        updated.hasImage
          ? t("Profilbild und Beschreibung übernommen")
          : t("Beschreibung übernommen, kein Bild gefunden"),
        "success",
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setFetching(false);
    }
  };

  const playAll = (shuffle: boolean) => {
    if (tracks.length === 0) return;
    void api.setShuffle(shuffle);
    void api.playTracks(
      tracks.map((t) => t.id),
      0,
    );
  };

  return (
    <div>
      <ZurueckKnopf ziel="/artists" />
      <div className="mb-8 flex flex-wrap items-end gap-6">
        <ArtistAvatar
          artist={artist}
          className="h-36 w-36 shrink-0 shadow-xl"
        />
        <div className="min-w-0 flex-1">
          <PageHeader
            eyebrow={t("Künstler")}
            title={artist.name}
            subtitle={`${plural(artist.trackCount, "Titel")} · ${plural(releases.length, "Release")} · ${formatDuration(totalMs)}`}
            actionsReihe
            actions={
              <>
                {/* edit, shuffle, play, close together with play last. no gap
                    in front of it: it did push the accent button to the edge
                    but tore the row apart in doing so. */}
                <button
                  type="button"
                  onClick={() => setEditing(true)}
                  title={t("Bearbeiten")}
                  className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
                >
                  <PencilIcon size={16} />
                  <span className="beschriftung">{t("Bearbeiten")}</span>
                </button>
                {!artist.hasImage && !artist.bio && (
                  <button
                    type="button"
                    onClick={() => void fetchMetadata()}
                    disabled={fetching}
                    title={fetching ? t("Suche…") : t("Metadaten holen")}
                    className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
                  >
                    <DownloadIcon size={16} />
                    <span className="beschriftung">
                      {fetching ? t("Suche…") : t("Metadaten holen")}
                    </span>
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => playAll(true)}
                  disabled={!tracks.length}
                  title={t("Zufällig")}
                  className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
                >
                  <ShuffleIcon size={16} />
                  <span className="beschriftung">{t("Zufällig")}</span>
                </button>
                <button
                  type="button"
                  onClick={() => playAll(false)}
                  disabled={!tracks.length}
                  title={t("Abspielen")}
                  className="pill-btn is-raised is-accent aktionsknopf aktionsknopf-kurz"
                >
                  <PlayIcon size={16} />
                  <span className="beschriftung">{t("Abspielen")}</span>
                </button>
              </>
            }
          />
        </div>
      </div>

      {artist.bio && <Beschreibung text={artist.bio} />}

      {topTracks.length > 0 && (
        <>
          <h2 className="mb-3 text-xl font-semibold tracking-tight">
            {t("Beliebt")}
          </h2>
          <TrackList
            tracks={topTracks}
            showArtist={false}
            onPlay={(index) =>
              void api.playTracks(
                tracks.map((t) => t.id),
                index,
              )
            }
            onChanged={() => void refresh()}
          />
        </>
      )}

      {/* three tiles per kind, the rest on request. an artist with twenty
          singles pushed their albums far down otherwise, and the overview got
          lost between the sections.

          unfolded, the ordinary layout applies again: the three-column one is
          meant for exactly three tiles and would be too coarse for twenty. */}
      {abschnitte().map(({ type, title }) => {
        const items = releases.filter(
          (release) => release.releaseType === type,
        );
        if (items.length === 0) return null;
        const offen = entfaltet.includes(type);
        const sichtbar = offen ? items : items.slice(0, VORSCHAU);
        return (
          <section key={type}>
            <SectionTitle
              action={
                items.length > VORSCHAU ? (
                  <button
                    type="button"
                    onClick={() =>
                      setEntfaltet((vorher) =>
                        offen
                          ? vorher.filter((eintrag) => eintrag !== type)
                          : [...vorher, type],
                      )
                    }
                    className="text-sm text-mute transition hover:text-fg hover:underline"
                  >
                    {offen ? t("Weniger anzeigen") : t("Mehr anzeigen")}
                  </button>
                ) : undefined
              }
            >
              {title}
            </SectionTitle>
            <Grid vorschau={!offen}>
              {sichtbar.map((album) => (
                <AlbumCard key={album.id} album={album} ohneAbspielen />
              ))}
            </Grid>
          </section>
        );
      })}

      {features.length > 0 && (
        <section>
          <h2 className="mt-8 mb-3 text-xl font-semibold tracking-tight">
            {t("Als Gast dabei")}
          </h2>
          <TrackList tracks={features} onChanged={() => void refresh()} />
        </section>
      )}

      {releases.length === 0 && features.length === 0 && (
        <p className="mt-8 rounded-xl border border-dashed border-ink-700 px-5 py-10 text-center text-sm text-mute">
          {t("Für diesen Künstler sind keine Releases hinterlegt.")}
        </p>
      )}

      <ArtistEditor
        artist={artist}
        open={editing}
        onClose={() => setEditing(false)}
        onSaved={(updated) => {
          bustCoverCache();
          setArtist(updated);
        }}
      />
    </div>
  );
}

// long artist descriptions push everything else out of view, so the start
// only at first and the rest on request.
//
// it is shortened by line height, not by character count: where the text is
// cut depends on the window width, and a text shortened by characters wraps
// after two lines one time and after five the next
function Beschreibung({ text }: { text: string }) {
  const [offen, setOffen] = useState(false);
  const absatz = useRef<HTMLParagraphElement>(null);
  const [gekuerzt, setGekuerzt] = useState(false);

  // whether the button is needed at all is known only to the finished layout
  useEffect(() => {
    const element = absatz.current;
    if (!element) return;
    const pruefen = () =>
      setGekuerzt(element.scrollHeight > element.clientHeight + 4);
    pruefen();
    const beobachter = new ResizeObserver(pruefen);
    beobachter.observe(element);
    return () => beobachter.disconnect();
  }, [text]);

  return (
    <div className="relative mb-8 max-w-3xl">
      <p
        ref={absatz}
        className={`text-sm leading-relaxed whitespace-pre-line text-mute ${
          offen ? "" : "line-clamp-3"
        }`}
      >
        {text}
      </p>
      {(gekuerzt || offen) && (
        <button
          type="button"
          onClick={() => setOffen((wert) => !wert)}
          className={`text-sm font-medium text-fg underline-offset-2 hover:underline ${
            // unfolded there is no cut-off line left for the button to sit at
            // the end of, and it stands below again then
            offen ? "mt-1.5" : "mehr-anzeigen"
          }`}
        >
          {offen ? t("Weniger anzeigen") : t("Mehr anzeigen")}
        </button>
      )}
    </div>
  );
}
