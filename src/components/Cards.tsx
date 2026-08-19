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

/**
 * Ein Release als Kachel.
 *
 * `ohneAbspielen` nimmt das Dreieck vom Cover. Auf einem Berührungsbildschirm
 * gibt es kein Zeigen, das etwas hervorholen könnte — dort steht das Dreieck
 * ständig auf dem Bild und verdeckt genau die Ecke, in der bei einem Cover
 * meist noch etwas zu sehen ist. Auf der Startseite, wo drei Kacheln nur ein
 * Ausschnitt sind und man ohnehin weitergeht, wiegt das schwerer als der
 * gesparte Antippen.
 */
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
      className="group rounded-xl p-3 transition hover:bg-ink-800"
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

/**
 * Ein Wochenmix als Kachel, in derselben Form wie ein Release.
 *
 * Steht in den Karten und nicht auf der Startseite, weil zwei Seiten sie
 * zeigen: die Startseite als Vorschau, die Übersicht als vollständige Liste.
 */
export function MixKachel({
  mix,
  ohneAbspielen = false,
}: {
  mix: WeeklyMixSummary;
  ohneAbspielen?: boolean;
}) {
  return (
    <Link
      to={`/mix/${mix.offset}`}
      className="group rounded-xl p-3 transition hover:bg-ink-800"
    >
      <div className="relative">
        <PlaylistMosaic
          albumIds={mix.coverAlbumIds}
          name={mixName(mix)}
          size="aspect-square w-full"
        />
        {!ohneAbspielen && (
          <span className="accent-bg absolute end-2 bottom-2 grid h-10 w-10 translate-y-2 place-items-center rounded-full opacity-0 shadow-xl transition group-hover:translate-y-0 group-hover:opacity-100">
            <PlayIcon size={18} className="ml-0.5" />
          </span>
        )}
      </div>
      <p className="mt-3 truncate text-sm font-medium">{mixName(mix)}</p>
      <p className="truncate text-xs text-mute">
        {mix.offset === 0 ? t("Diese Woche") : plural(mix.trackCount, "Titel")}
      </p>
    </Link>
  );
}

/** Zeigt bis zu vier Cover als Kachel, wie bei Playlist-Vorschauen üblich. */
export function PlaylistMosaic({
  albumIds,
  name,
  size = "h-16 w-16",
  coverSrc = null,
}: {
  albumIds: number[];
  name: string;
  size?: string;
  /** Eigenes Bild der Playlist. Ist eines da, entfällt das Mosaik. */
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

/** Playlists erscheinen wie Alben, gleiche Kachel, gleiche Bedienung. */
export function PlaylistCard({
  playlist,
  size = "md",
}: {
  playlist: Playlist;
  size?: GridSize;
}) {
  return (
    <Link
      // Ohne dies zieht der Browser den Verweis statt der Kachel: Ein
      // Link ist von Haus aus ziehbar, und sein Zug überstimmt den der
      // Hülle darum herum.
      draggable={false}
      to={`/playlist/${playlist.id}`}
      // `block` ausdrücklich: Ein `a` ist von Haus aus inline. Als direktes
      // Kind des Gitters wurde es automatisch zum Block, seit es in einer
      // Hülle zum Ziehen steckt, nicht mehr. Inline griff die Polsterung
      // nicht, und die Hoverfläche zerfiel in Zeilenkästen.
      className="group block rounded-xl p-3 transition hover:bg-ink-800"
    >
      <div className="relative">
        <PlaylistMosaic
          albumIds={playlist.coverAlbumIds}
          name={playlist.name}
          size="aspect-square w-full"
          coverSrc={playlist.hasCover ? playlistCover(playlist.id) : null}
        />
        {/* Kein Dreieck auf dem Bild. Ohne Zeigegerät steht es dort
            ständig und verdeckt eine Ecke des Mosaiks; die Kachel führt in
            die Playlist, wo das Abspielen als eigener Knopf steht. */}
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
      className="group rounded-xl p-3 text-center transition hover:bg-ink-800"
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
  /** Bedienung an den rechten Rand statt unter den Anfang der Überschrift. */
  actionsRechts?: boolean;
  /**
   * Enger werden statt umbrechen.
   *
   * Steht die Bedienung neben einem Bild, bleibt ihr wenig Breite, und drei
   * Knöpfe rutschten dort untereinander — auf der Künstlerseite war das eine
   * Spalte statt einer Reihe. `aktionsreihe` macht die Reihe selbst zum
   * Maßstab; die Knöpfe fallen dann auf ihr Zeichen zusammen, wie in der
   * Bibliothek.
   *
   * Bewusst als Zusatz und nicht als neue Regel für alle: Ohne
   * `aktionsknopf-kurz` an den Knöpfen liefen sie sonst über den Rand,
   * statt umzubrechen.
   */
  actionsReihe?: boolean;
}) {
  return (
    // Feste Bauhöhe für jede Seite. Rubrik und Untertitel sind nicht überall
    // gefüllt, die Einstellungen haben keinen, der Rückblick bekommt seinen
    // erst nach dem Laden. Fehlten die Zeilen dann einfach, säße die
    // Überschrift jeder Seite auf einer anderen Höhe, und beim Wechsel
    // sprängen Suchleiste und Inhalt darunter mit.
    //
    // Darum werden beide Zeilen immer belegt, notfalls mit einem
    // geschützten Leerzeichen. Für Vorleseprogramme bleiben sie dabei
    // stumm, weil sie dann kein Wort enthalten.
    <header className="mb-6">
      {/* Der Überschriftenblock allein trägt die feste Höhe. Die Bedienung
          darunter darf umbrechen, ohne die Überschriften der Seiten
          gegeneinander zu verschieben. */}
      <div className="min-h-20 min-w-0">
        <p className="eyebrow">{eyebrow || "\u00a0"}</p>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        <p className="mt-1 text-sm text-mute">{subtitle || "\u00a0"}</p>
      </div>
      {/* Bedienung steht unter der Überschrift, nicht daneben: So beginnt sie
          auf jeder Seite an derselben Stelle, und lange Titel drängen sie
          nicht mehr zusammen. Derselbe Abstand wie in den Suchzeilen. */}
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
 * Einen Schritt zurück im Verlauf.
 *
 * Nötig auf Unterseiten, die man von überall her erreicht: Künstler und
 * Releases sind aus der Bibliothek, aus dem Rückblick, aus der laufenden
 * Wiedergabe und aus jeder Titelliste heraus anklickbar. Die Seitenleiste
 * führt zu keiner davon zurück.
 *
 * Gibt es keinen Verlauf, etwa weil die Seite als erste geöffnet wurde,
 * führt der Knopf zum genannten Ziel. React Router zählt die Schritte in
 * `history.state` mit; steht dort die Null, ist dies der Anfang.
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

/** Wie viele Kacheln nebeneinander passen, je größer, desto weniger. */
const GRID_COLUMNS = {
  sm: "grid-cols-3 sm:grid-cols-4 lg:grid-cols-6 xl:grid-cols-8",
  md: "grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5",
  lg: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4",
} as const;

/*
 * Aufteilung für eine Vorschau von genau drei Kacheln neben „Alle ansehen“.
 *
 * Wie `md`, nur auf dem Telefon dreispaltig statt zweispaltig: Zwei Spalten
 * ließen die dritte Kachel allein in einer zweiten Reihe stehen, und das sieht
 * nach einer abgeschnittenen Liste aus, nicht nach einer Auswahl.
 *
 * Als eigene Angabe und nicht als vierte Größe: `GridSize` steht auch in den
 * Einstellungen für die Kachelgröße der Playlists, und dort gibt es nur drei.
 */
const VORSCHAU_SPALTEN = "grid-cols-3 lg:grid-cols-4 xl:grid-cols-5";

export type GridSize = keyof typeof GRID_COLUMNS;

export function Grid({
  children,
  size = "md",
  vorschau = false,
}: {
  children: React.ReactNode;
  size?: GridSize;
  /** Drei Kacheln als Ausschnitt, der Rest hinter „Alle ansehen“. */
  vorschau?: boolean;
}) {
  const spalten = vorschau ? VORSCHAU_SPALTEN : GRID_COLUMNS[size];
  return <div className={`grid gap-1 ${spalten}`}>{children}</div>;
}

/** Playlist als Zeile, dieselben Angaben, nur platzsparend. */
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
      // Ohne dies zieht der Browser den Verweis statt der Kachel: Ein
      // Link ist von Haus aus ziehbar, und sein Zug überstimmt den der
      // Hülle darum herum.
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
