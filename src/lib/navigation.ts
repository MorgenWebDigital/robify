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
   * Nachschlagebegriff für die Texttabelle, nicht die fertige Beschriftung.
   *
   * Übersetzt wird erst beim Zeichnen: Eine Liste auf Modulebene entsteht
   * einmal beim Laden und bliebe nach einem Sprachwechsel in der
   * Anfangssprache stehen.
   */
  schluessel: string;
  icon: ComponentType<{ size?: number; className?: string }>;
  /** Weitere Adressen, die zu diesem Abschnitt gehören. */
  auch: string[];
}

/**
 * Die Navigationspunkte, gemeinsam für Seitenleiste und Unterleiste.
 *
 * Steht hier und nicht in einem der beiden Bauteile: Am Rechner führt eine
 * Leiste an der Seite durch die App, am Telefon eine am unteren Rand. Zwei
 * Listen liefen über kurz oder lang auseinander, und ein Abschnitt, den es
 * nur auf einem der beiden Geräte gibt, wäre ein Fehler, den niemand sucht.
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

/**
 * Was am Telefon unten steht.
 *
 * Vier Punkte plus „Mehr“: Mehr als fünf Ziele nebeneinander werden auf einer
 * Handbreite zu schmal zum Treffen. Die übrigen liegen hinter „Mehr“, damit
 * kein Abschnitt unerreichbar wird.
 */
export const UNTEN = NAV.slice(0, 4);
export const UNTER_MEHR: NavEintrag[] = [
  ...NAV.slice(4),
  {
    to: "/settings",
    schluessel: "Einstellungen",
    icon: SettingsIcon,
    auch: [],
  },
];

/**
 * Gehört die geöffnete Seite zu diesem Navigationspunkt?
 *
 * `NavLink` vergleicht nur seine eigene Adresse, und die Detailseiten heißen
 * anders als ihr Abschnitt: Ein Künstler steht unter `/artist/7`, der
 * Abschnitt unter `/artists`. Wer von einem Titel aus zum Künstler ging, sah
 * darum eine Leiste ohne jede Markierung, obwohl er mitten in einem Abschnitt
 * stand. Die Zweitadressen stehen in `auch`.
 *
 * Der Schrägstrich beim Vergleich ist wichtig: Ohne ihn hielte `/playlist`
 * auch `/playlists` für seine eigene Seite.
 */
export function istHier(pfad: string, eintrag: NavEintrag): boolean {
  if (pfad === eintrag.to) return true;
  if (eintrag.to !== "/" && pfad.startsWith(`${eintrag.to}/`)) return true;
  return eintrag.auch.some(
    (zweit) => pfad === zweit || pfad.startsWith(`${zweit}/`),
  );
}
