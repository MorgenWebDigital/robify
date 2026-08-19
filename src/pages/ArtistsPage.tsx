import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArtistCard, Grid, PageHeader } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { ArtistIcon, SearchIcon } from "../components/Icons";
import { api, fallback } from "../lib/api";
import { plural } from "../lib/format";
import { t } from "../lib/i18n";
import { useLibrary } from "../store/library";
import type { Artist } from "../types";

export function ArtistsPage() {
  const revision = useLibrary((s) => s.revision);
  // `null` means not looked yet. only afterwards may the page claim there
  // are no artists
  const [artists, setArtists] = useState<Artist[] | null>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    let cancelled = false;
    api
      .listArtists(search || undefined)
      .then((value) => {
        if (!cancelled) setArtists(value);
      })
      .catch((error) => {
        if (!cancelled) setArtists(fallback([], t("Künstler"))(error));
      });
    return () => {
      cancelled = true;
    };
  }, [search, revision]);

  return (
    <div>
      <PageHeader
        eyebrow={t("Sammlung")}
        title={t("Künstler")}
        subtitle={plural(artists?.length ?? 0, "Künstler in deiner Bibliothek")}
      />

      {/* across the whole width, unlike in the library. there the search
          stands between buttons and has to fold into a circle so the row does
          not overflow. here it stands alone: folded it would hand the room it
          saves to nobody, and a circle in an empty row would be left. */}
      <div className="aktionsreihe mb-5 gap-3">
        <div className="aktionsfeld relative flex-1">
          <SearchIcon
            size={16}
            className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
          />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("Künstler suchen")}
            aria-label={t("Künstler suchen")}
            className="search-field ps-10"
          />
        </div>
      </div>

      {artists === null ? null : artists.length === 0 ? (
        search ? (
          <p className="rounded-xl border border-dashed border-ink-700 px-5 py-12 text-center text-sm text-mute">
            {t("Kein Künstler passt zu „{0}“.", search)}
          </p>
        ) : (
          <EmptyState
            icon={ArtistIcon}
            title={t("Noch keine Künstler")}
            text={t(
              "Künstler entstehen aus deiner Musik. Importiere vorhandene Dateien oder lade Titel über den Downloader, dann erscheinen sie hier samt Bild und Beschreibung.",
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
        )
      ) : (
        <Grid>
          {artists.map((artist) => (
            <ArtistCard key={artist.id} artist={artist} />
          ))}
        </Grid>
      )}
    </div>
  );
}
