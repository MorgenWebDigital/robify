import { t } from "./i18n";

/**
 * Name eines Wochenmix, z. B. „Wochenmix 7“.
 *
 * Entsteht hier und nicht im Rust-Teil: Der kennt die eingestellte
 * Oberflächensprache nicht, und ein dort zusammengesetzter Name käme in jeder
 * Sprache als „Wochenmix 7“ an. Die Nummer allein reicht der Anzeige, sie
 * steht ohnehin schon in den Daten.
 */
export function mixName(mix: { number: number }): string {
  return t("Wochenmix {0}", mix.number);
}
