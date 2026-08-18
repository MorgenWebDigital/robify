import { t } from "./i18n";
import type { Track } from "../types";

/**
 * Auswahl für die Bibliotheksseite, in der Reihenfolge des Aufklappmenüs.
 *
 * Als Funktion, nicht als feste Liste: Eine Liste auf Modulebene entsteht
 * einmal beim Laden. Wechselt der Nutzer danach die Sprache, baut die App sich
 * zwar neu auf, das Modul aber nicht, und die Beschriftungen blieben in der
 * Anfangssprache stehen.
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
 * Vergleicht zwei Titel nach der gewählten Ordnung.
 *
 * Jede Ordnung hat Folgeschlüssel, sonst stünden die Titel eines Künstlers in
 * zufälliger Reihenfolge untereinander: nach Künstler kommt das Album, darin
 * die Titelnummer. Verglichen wird mit deutschen Regeln, damit Umlaute richtig
 * einsortiert werden und nicht hinter „Z“ landen.
 *
 * Steht hier statt in der Seite, damit sich die Ordnung ohne Browser prüfen
 * lässt, sie ist reine Rechnung und braucht kein React.
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

  /**
   * Releases eines Künstlers nach Erscheinen, neueste oben, nicht
   * alphabetisch: Wer die Titel eines Künstlers durchgeht, sucht nach Zeit,
   * nicht nach Anfangsbuchstaben. Ohne Jahresangabe ans Ende, danach der
   * Albumname als fester Anker, sonst zerfielen gleichjährige Releases
   * ineinander. Innerhalb eines Releases zählt die Titelnummer.
   */
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
      // Neueste zuerst; Titel ohne Jahr ans Ende.
      return (
        (b.year ?? 0) - (a.year ?? 0) ||
        text(a.artistName, b.artistName) ||
        nachAlbum()
      );
    default:
      return b.addedAt - a.addedAt;
  }
}
