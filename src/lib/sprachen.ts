// the selectable languages.
//
// each one stands in its own spelling: whoever wants the ui in japanese looks
// for 日本語 and not for "Japanisch". for the same reason the search in
// `Auswahl` runs on several tracks, it takes the endonym, the german name and
// the code.
//
// the flag is an emoji made of two regional indicators. that saves 30 image
// files and follows the system font, and under linux noto color emoji draws
// them in colour. a country is always a simplification, arabic is not spoken
// in saudi arabia alone, but as a recognition aid in a long list it beats any
// code
export interface SprachEintrag {
  /** iso 639-1, the same code as in the translation tables */
  id: string;
  /** endonym, the way speakers of this language write it */
  name: string;
  /** german name, so the search fires on it as well */
  deutsch: string;
  flagge: string;
}

/**
 * the selectable languages, sorted by their endonym.
 *
 * seven languages rather than every conceivable one: a language in this list
 * is a promise that the ui really exists in it. thirty entries of which most
 * fall back to english would be a disappointment with an extra step.
 */
export const SPRACHEN: SprachEintrag[] = [
  { id: "de", name: "Deutsch", deutsch: "Deutsch", flagge: "🇩🇪" },
  { id: "en", name: "English", deutsch: "Englisch", flagge: "🇬🇧" },
  { id: "es", name: "Español", deutsch: "Spanisch", flagge: "🇪🇸" },
  { id: "fr", name: "Français", deutsch: "Französisch", flagge: "🇫🇷" },
  { id: "ru", name: "Русский", deutsch: "Russisch", flagge: "🇷🇺" },
  { id: "ar", name: "العربية", deutsch: "Arabisch", flagge: "🇸🇦" },
  { id: "zh", name: "中文", deutsch: "Chinesisch", flagge: "🇨🇳" },
].sort((a, b) => a.name.localeCompare(b.name, "de"));

/** languages written from right to left */
export const RECHTS_NACH_LINKS = new Set(["ar"]);

/** the entry for a language code, `undefined` where it is unknown */
export function sprachEintrag(id: string): SprachEintrag | undefined {
  return SPRACHEN.find((eintrag) => eintrag.id === id);
}
