import type { ComponentType } from "react";
import {
  ArtistIcon,
  DownloadIcon,
  HeartIcon,
  HomeIcon,
  LibraryIcon,
  PlaylistIcon,
  SettingsIcon,
  SparkIcon,
} from "../components/Icons";

export interface NavEintrag {
  to: string;
  /**
   * lookup term for the text table, not the finished label.
   *
   * the translation happens at render time: a list at module level comes into
   * being once at load time and would stay in the starting language after a
   * language switch.
   */
  schluessel: string;
  icon: ComponentType<{ size?: number; className?: string }>;
  /** further addresses belonging to this section */
  auch: string[];
}

/**
 * the navigation entries, shared by the sidebar and the bottom bar.
 *
 * kept here and not in either component: on a desktop a bar at the side leads
 * through the app, on a phone one at the bottom edge. two lists would drift
 * apart sooner or later, and a section existing on only one of the two
 * devices would be a bug nobody looks for.
 */
export const NAV: NavEintrag[] = [
  { to: "/", schluessel: "Start", icon: HomeIcon, auch: ["/mix"] },
  {
    to: "/library",
    schluessel: "Bibliothek",
    icon: LibraryIcon,
    auch: ["/album"],
  },
  {
    to: "/artists",
    schluessel: "Künstler",
    icon: ArtistIcon,
    auch: ["/artist"],
  },
  {
    to: "/playlists",
    schluessel: "Playlists",
    icon: PlaylistIcon,
    auch: ["/playlist"],
  },
  { to: "/favorites", schluessel: "Favoriten", icon: HeartIcon, auch: [] },
  { to: "/wrapped", schluessel: "Wrapped", icon: SparkIcon, auch: [] },
  { to: "/downloader", schluessel: "Downloader", icon: DownloadIcon, auch: [] },
];

// what stands at the bottom on a phone, in this order.
//
// four entries plus "more": more than five targets side by side get too
// narrow to hit on a hand's width. the rest sit behind "more" so no section
// becomes unreachable.
//
// listed explicitly rather than the first four out of `NAV`: what belongs at
// the bottom is what one needs often on a phone, and that is not the same as
// the order in the sidebar. the downloader is a daily reach on a phone, while
// the artist overview is usually reached through a track
const UNTEN_PFADE = ["/", "/library", "/downloader", "/playlists"];

export const UNTEN: NavEintrag[] = UNTEN_PFADE.map((pfad) =>
  NAV.find((eintrag) => eintrag.to === pfad)!,
);

export const UNTER_MEHR: NavEintrag[] = [
  ...NAV.filter((eintrag) => !UNTEN_PFADE.includes(eintrag.to)),
  {
    to: "/settings",
    schluessel: "Einstellungen",
    icon: SettingsIcon,
    auch: [],
  },
];

/**
 * whether the open page belongs to this navigation entry.
 *
 * `NavLink` compares its own address alone, and the detail pages are called
 * differently from their section: an artist stands under `/artist/7`, the
 * section under `/artists`. whoever went from a track to the artist therefore
 * saw a bar without any marking although they stood in the middle of a
 * section. the secondary addresses stand in `auch`.
 *
 * the slash in the comparison matters: without it `/playlist` would take
 * `/playlists` for its own page as well.
 */
export function istHier(pfad: string, eintrag: NavEintrag): boolean {
  if (pfad === eintrag.to) return true;
  if (eintrag.to !== "/" && pfad.startsWith(`${eintrag.to}/`)) return true;
  return eintrag.auch.some(
    (zweit) => pfad === zweit || pfad.startsWith(`${zweit}/`),
  );
}
