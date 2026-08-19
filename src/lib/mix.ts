import { t } from "./i18n";

/**
 * name of a weekly mix, "Wochenmix 7" for instance.
 *
 * grows here and not on the rust side: that one does not know the selected
 * interface language, and a name assembled there would arrive as "Wochenmix
 * 7" whatever the language. the number alone is enough for the display, it
 * stands in the data already.
 */
export function mixName(mix: { number: number }): string {
  return t("Wochenmix {0}", mix.number);
}
