import { t } from "./i18n";
import type { Track } from "../types";

/**
 * the choices for the library page, in the order of the dropdown.
 *
 * a function rather than a fixed list: a list at module level comes into
 * being once at load time. does the user switch language afterwards, the app
 * rebuilds itself but the module does not, and the labels would stay in the
 * starting language.
 */
export function sortierungen() {
  return [
    { id: "added", label: t("Zuletzt hinzugefügt") },
    { id: "title", label: t("Titel") },
    { id: "artist", label: t("Künstler") },
    { id: "album", label: t("Album") },
    { id: "year", label: t("Jahr") },
  ];
}

/**
 * compares two tracks by the chosen order.
 *
 * every order carries follow-up keys, otherwise the tracks of one artist
 * would stand under each other at random: after the artist comes the album,
 * and inside it the track number. the comparison runs under german rules so
 * umlauts sort where they belong instead of landing behind "Z".
 *
 * lives here instead of in the page so the order can be checked without a
 * browser, it is pure calculation and needs no react.
 */
export function vergleicheTitel(a: Track, b: Track, ordnung: string): number {
  const text = (x: string, y: string) =>
    x.localeCompare(y, "de", { sensitivity: "base" });
  const zahl = (x: number | null, y: number | null) => (x ?? 0) - (y ?? 0);

  const nachAlbum = () =>
    text(a.albumTitle, b.albumTitle) ||
    zahl(a.discNo, b.discNo) ||
    zahl(a.trackNo, b.trackNo) ||
    text(a.title, b.title);

  // releases of an artist by release date, newest on top, not
  // alphabetically: whoever walks through the tracks of an artist is looking
  // by time, not by initial letter. without a year they go to the end, then
  // the album name as a fixed anchor, otherwise releases of the same year
  // would fall into each other. inside a release the track number counts
  const nachErscheinen = () =>
    (b.year ?? 0) - (a.year ?? 0) ||
    text(a.albumTitle, b.albumTitle) ||
    zahl(a.discNo, b.discNo) ||
    zahl(a.trackNo, b.trackNo) ||
    text(a.title, b.title);

  switch (ordnung) {
    case "title":
      return text(a.title, b.title) || text(a.artistName, b.artistName);
    case "artist":
      return text(a.artistName, b.artistName) || nachErscheinen();
    case "album":
      return nachAlbum() || text(a.artistName, b.artistName);
    case "year":
      // newest first, tracks without a year go to the end
      return (
        (b.year ?? 0) - (a.year ?? 0) ||
        text(a.artistName, b.artistName) ||
        nachAlbum()
      );
    default:
      return b.addedAt - a.addedAt;
  }
}
