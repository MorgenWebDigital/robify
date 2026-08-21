import { Link, useNavigate } from "react-router-dom";
import { t } from "../lib/i18n";
import { api } from "../lib/api";
import { albumCover, playlistCover } from "../lib/cover";
import { formatDuration, plural, releaseLabel } from "../lib/format";
import { mixName } from "../lib/mix";
import { Cover } from "./Cover";
import { ArtistAvatar } from "./ArtistEditor";
import { ChevronLeftIcon, PlaylistIcon, PlayIcon } from "./Icons";
import type { Album, Artist, Playlist, WeeklyMixSummary } from "../types";

// a release as a tile.
//
// `ohneAbspielen` takes the triangle off the cover. on a touch screen there
// is no hovering that could bring something out, so the triangle stands on
// the image permanently and covers exactly the corner where a cover usually
// still has something to show. on the home page, where three tiles are only
// an excerpt and one moves on anyway, that weighs more than the tap saved
export function AlbumCard({
  album,
  ohneAbspielen = false,
}: {
  album: Album;
  ohneAbspielen?: boolean;
}) {
  const play = async (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    const tracks = await api.albumTracks(album.id);
    if (tracks.length > 0)
      await api.playTracks(
        tracks.map((t) => t.id),
        0,
      );
  };

  return (
    <Link
      to={`/album/${album.id}`}
      // `block` explicitly: an `a` is inline out of the box. as a direct child
      // of the grid it is blockified automatically, but on the home page the
      // tile sits in a wrapper that hides the trailing ones on a narrow
      // window, and there it stayed inline: the padding did not take hold and
      // the hover background fell apart into line boxes, a grey strip beside
      // the cover instead of a surface behind it
      className="group block rounded-xl p-2 transition hover:bg-ink-800"
    >
      <div className="relative">
        <Cover
          src={albumCover(album.id)}
          alt={album.title}
          seed={album.id}
          className="aspect-square w-full"
          rounded="rounded-lg"
        />
        {!ohneAbspielen && (
          <button
            type="button"
            onClick={play}
            aria-label={t("{0} abspielen", album.title)}
            className="accent-bg absolute end-2 bottom-2 grid h-10 w-10 translate-y-2 place-items-center rounded-full opacity-0 shadow-xl transition group-hover:translate-y-0 group-hover:opacity-100"
          >
            <PlayIcon size={18} className="ml-0.5" />
          </button>
        )}
      </div>
      <p className="mt-3 truncate text-sm font-medium" title={album.title}>
        {album.title}
      </p>
      <p className="truncate text-xs text-mute">
        {[releaseLabel(album.releaseType), album.year, album.artistName]
          .filter(Boolean)
          .join(" · ")}
      </p>
    </Link>
  );
}

// a weekly mix as a tile, in the same shape as a release.
//
// lives in the cards and not on the home page because two pages show it: the
// home page as a preview, the overview as a complete list
export function MixKachel({
  mix,
  ohneAbspielen = false,
}: {
  mix: WeeklyMixSummary;
  ohneAbspielen?: boolean;
}) {
  // the mix is derived from the plays and is not stored, so the tile does not
  // carry its tracks. they are fetched on the press, the same call the detail
  // page makes.
  //
  // the triangle used to be a `span` without a handler: it appeared on hover,
  // looked like every other play button and did nothing but follow the link
  // underneath.
  const play = async (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    const daten = await api.weeklyMix(mix.offset);
    const ids = daten.items.map((item) => item.track.id);
    if (ids.length > 0) await api.playTracks(ids, 0);
  };

  return (
    <Link
      to={`/mix/${mix.offset}`}
      // `block` explicitly: an `a` is inline out of the box. as a direct child
      // of the grid it is blockified automatically, but on the home page the
      // tile sits in a wrapper that hides the trailing ones on a narrow
      // window, and there it stayed inline: the padding did not take hold and
      // the hover background fell apart into line boxes, a grey strip beside
      // the cover instead of a surface behind it
      className="group block rounded-xl p-2 transition hover:bg-ink-800"
    >
      <div className="relative">
        <PlaylistMosaic
          albumIds={mix.coverAlbumIds}
          name={mixName(mix)}
          size="aspect-square w-full"
        />
        {!ohneAbspielen && (
          <button
            type="button"
            onClick={play}
            aria-label={t("{0} abspielen", mixName(mix))}
            className="accent-bg absolute end-2 bottom-2 grid h-10 w-10 translate-y-2 place-items-center rounded-full opacity-0 shadow-xl transition group-hover:translate-y-0 group-hover:opacity-100"
          >
            <PlayIcon size={18} className="ml-0.5" />
          </button>
        )}
      </div>
      <p className="mt-3 truncate text-sm font-medium">{mixName(mix)}</p>
      <p className="truncate text-xs text-mute">
        {mix.offset === 0 ? t("Diese Woche") : plural(mix.trackCount, "Titel")}
      </p>
    </Link>
  );
}

// shows up to four covers as a tile, as is usual with playlist previews
export function PlaylistMosaic({
  albumIds,
  name,
  size = "h-16 w-16",
  coverSrc = null,
}: {
  albumIds: number[];
  name: string;
  size?: string;
  /** an image of the playlist's own. where there is one, the mosaic is dropped. */
  coverSrc?: string | null;
}) {
  if (coverSrc) {
    return <Cover src={coverSrc} alt={name} className={`shrink-0 ${size}`} />;
  }

  if (albumIds.length === 0) {
    return (
      <div
        className={`grid shrink-0 place-items-center rounded-lg bg-ink-700 ${size}`}
      >
        <PlaylistIcon className="h-1/2 w-1/2 text-mute" size={undefined} />
      </div>
    );
  }

  if (albumIds.length < 4) {
    return (
      <Cover
        src={albumCover(albumIds[0])}
        alt={name}
        seed={albumIds[0]}
        className={`shrink-0 ${size}`}
      />
    );
  }

  return (
    <div
      className={`grid shrink-0 grid-cols-2 overflow-hidden rounded-lg ${size}`}
    >
      {albumIds.slice(0, 4).map((albumId) => (
        <Cover
          key={albumId}
          src={albumCover(albumId)}
          alt={name}
          seed={albumId}
          className="h-full w-full"
          rounded="rounded-none"
        />
      ))}
    </div>
  );
}

// playlists appear like albums: the same tile, the same handling
export function PlaylistCard({
  playlist,
  size = "md",
}: {
  playlist: Playlist;
  size?: GridSize;
}) {
  return (
    <Link
      // without this the browser drags the link instead of the tile: a link
      // is draggable out of the box, and its drag outvotes the one of the
      // wrapper around it
      draggable={false}
      to={`/playlist/${playlist.id}`}
      // `block` explicitly: an `a` is inline out of the box. as a direct child
      // of the grid it became a block automatically, and since it sits in a
      // wrapper for dragging it no longer does. inline, the padding did not
      // take hold and the hover area fell apart into line boxes
      className="group block rounded-xl p-2 transition hover:bg-ink-800"
    >
      <div className="relative">
        <PlaylistMosaic
          albumIds={playlist.coverAlbumIds}
          name={playlist.name}
          size="aspect-square w-full"
          coverSrc={playlist.hasCover ? playlistCover(playlist.id) : null}
        />
        {/* no triangle on the image. without a pointing device it stands
            there permanently and covers a corner of the mosaic, and the tile
            leads into the playlist where playing has a button of its own. */}
      </div>
      <p
        className={`mt-3 truncate font-medium ${size === "sm" ? "text-xs" : "text-sm"}`}
        title={playlist.name}
      >
        {playlist.name}
      </p>
      <p className="truncate text-xs text-mute">
        {[
          plural(playlist.trackCount, "Titel"),
          playlist.durationMs > 0 ? formatDuration(playlist.durationMs) : null,
        ]
          .filter(Boolean)
          .join(" · ")}
      </p>
    </Link>
  );
}

export function ArtistCard({ artist }: { artist: Artist }) {
  return (
    <Link
      to={`/artist/${artist.id}`}
      className="group block rounded-xl p-2 text-center transition hover:bg-ink-800"
    >
      <ArtistAvatar artist={artist} className="mx-auto aspect-square w-full" />
      <p className="mt-3 truncate text-sm font-medium" title={artist.name}>
        {artist.name}
      </p>
      <p className="truncate text-xs text-mute">
        {plural(artist.trackCount, "Titel")}
      </p>
    </Link>
  );
}

export function PageHeader({
  eyebrow,
  title,
  subtitle,
  actions,
  actionsRechts = false,
  actionsReihe = false,
}: {
  eyebrow?: string;
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
  /** actions at the right edge instead of under the start of the heading */
  actionsRechts?: boolean;
  /**
   * grow narrower instead of wrapping.
   *
   * where the actions stand next to an image little width is left to them,
   * and three buttons slid under each other there: on the artist page that
   * was a column instead of a row. `aktionsreihe` makes the row itself the
   * yardstick, and the buttons then collapse onto their icon, as in the
   * library.
   *
   * deliberately an addition and not a new rule for everyone: without
   * `aktionsknopf-kurz` on the buttons they would run over the edge instead
   * of wrapping.
   */
  actionsReihe?: boolean;
}) {
  return (
    // a fixed build height for every page. eyebrow and subtitle are not
    // filled everywhere, the settings have none and the review gets its own
    // only after loading. were the lines missing then, the heading of
    // every page would sit at a different height, and search bar and content
    // below would jump along on a change.
    //
    // both lines are therefore always occupied, with a non-breaking space if
    // need be. to screen readers they stay silent that way, because they hold
    // no word then
    <header className="mb-6">
      {/* the heading block alone carries the fixed height. the actions below
          may wrap without shifting the headings of the pages against each
          other. */}
      <div className="min-h-20 min-w-0">
        <p className="eyebrow">{eyebrow || "\u00a0"}</p>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        <p className="mt-1 text-sm text-mute">{subtitle || "\u00a0"}</p>
      </div>
      {/* the actions stand under the heading, not next to it: that way they
          start at the same place on every page, and long titles no longer
          squeeze them together. the same spacing as in the search rows. */}
      {actions && (
        <div
          className={`mt-3 items-center gap-3 ${
            actionsReihe ? "aktionsreihe" : "flex flex-wrap"
          } ${actionsRechts ? "justify-end" : ""}`}
        >
          {actions}
        </div>
      )}
    </header>
  );
}

/**
 * one step back in the history.
 *
 * needed on sub-pages reachable from everywhere: artists and releases can be
 * clicked from the library, from the review, from the running playback and
 * from every track list. the sidebar leads back to none of them.
 *
 * where there is no history, because the page was opened first for instance,
 * the button leads to the target named. react router counts the steps in
 * `history.state`, and a zero there means this is the start.
 */
export function ZurueckKnopf({ ziel }: { ziel: string }) {
  const navigate = useNavigate();

  return (
    <button
      type="button"
      onClick={() => {
        const schritt = (window.history.state as { idx?: number } | null)?.idx;
        if (schritt && schritt > 0) navigate(-1);
        else navigate(ziel);
      }}
      aria-label={t("Zurück")}
      title={t("Zurück")}
      className="pill-btn is-raised mb-4 h-9 w-9 shrink-0"
    >
      <ChevronLeftIcon size={18} />
    </button>
  );
}

export function SectionTitle({
  children,
  action,
}: {
  children: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <div className="mt-8 mb-3 flex items-baseline justify-between gap-4">
      <h2 className="text-xl font-semibold tracking-tight">{children}</h2>
      {action}
    </div>
  );
}

/** how many tiles fit side by side, the larger the fewer */
const GRID_COLUMNS = {
  sm: "grid-cols-3 sm:grid-cols-4 lg:grid-cols-6 xl:grid-cols-8",
  md: "grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5",
  lg: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4",
} as const;

// layout for the preview row on the home page.
//
// three columns on a phone instead of the two `md` would give: two columns
// left the third tile standing alone in a second row, and that looks like a
// truncated list rather than a selection.
//
// six on a desktop, not five. the row fills the width either way, so the
// number of columns is what sets the size of a cover: at five they came to
// 184 points and stood as large as on a page of their own, at six they come
// to 161, which reads as an excerpt again.
//
// there is nothing between the two. what is left is the padding of a tile and
// the gap of the grid, and both stand at 8 points rather than 12 and 16.
//
// a value of its own and not a fourth size: `GridSize` also stands in the
// settings for the tile size of the playlists, and there are only three
// there
const VORSCHAU_SPALTEN = "grid-cols-3 lg:grid-cols-6";

export type GridSize = keyof typeof GRID_COLUMNS;

export function Grid({
  children,
  size = "md",
  vorschau = false,
}: {
  children: React.ReactNode;
  size?: GridSize;
  /** three tiles as an excerpt, the rest behind "view all" */
  vorschau?: boolean;
}) {
  const spalten = vorschau ? VORSCHAU_SPALTEN : GRID_COLUMNS[size];
  // six pixels between the tiles.
  //
  // every tile carries a hover background of its own, and at the four pixels
  // this once stood at those rectangles almost touched: moving across the grid
  // read as one continuous band with slits in it rather than as one tile
  // lighting up. six is the least that still keeps them apart, and with the
  // padding of a tile it leaves a good twenty points between one cover and the
  // next.
  return <div className={`grid gap-1.5 ${spalten}`}>{children}</div>;
}

// a playlist as a row, the same details, only saving space
export function PlaylistRow({
  playlist,
  size = "md",
}: {
  playlist: Playlist;
  size?: GridSize;
}) {
  const cover = { sm: "h-10 w-10", md: "h-14 w-14", lg: "h-20 w-20" }[size];

  return (
    <Link
      // without this the browser drags the link instead of the tile: a link
      // is draggable out of the box, and its drag outvotes the one of the
      // wrapper around it
      draggable={false}
      to={`/playlist/${playlist.id}`}
      className="flex items-center gap-4 rounded-xl p-2 transition hover:bg-ink-800"
    >
      <PlaylistMosaic
        albumIds={playlist.coverAlbumIds}
        name={playlist.name}
        size={cover}
        coverSrc={playlist.hasCover ? playlistCover(playlist.id) : null}
      />
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium">{playlist.name}</p>
        <p className="truncate text-xs text-mute">
          {[
            plural(playlist.trackCount, "Titel"),
            playlist.durationMs > 0
              ? formatDuration(playlist.durationMs)
              : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
        {playlist.description && (
          <p className="mt-0.5 truncate text-xs text-mute/80">
            {playlist.description}
          </p>
        )}
      </div>
    </Link>
  );
}
