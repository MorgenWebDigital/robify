/**
 * Die wählbaren Sprachen.
 *
 * Jede steht in ihrer eigenen Schreibweise da, wer die Oberfläche auf
 * Japanisch stellen will, sucht „日本語“ und nicht „Japanisch“. Aus demselben
 * Grund ist die Suche unten in `Auswahl` mehrgleisig: Sie greift auf den
 * Eigennamen, den deutschen Namen und das Kürzel.
 *
 * Die Flagge ist ein Emoji aus zwei Regionalzeichen. Das spart 30 Bilddateien
 * und folgt der Systemschrift; unter Linux zeichnet Noto Color Emoji sie
 * farbig. Ein Land ist dabei immer eine Vereinfachung, Arabisch wird nicht
 * nur in Saudi-Arabien gesprochen, aber als Wiedererkennungshilfe in einer
 * langen Liste schlägt es jedes Kürzel.
 */
export interface SprachEintrag {
  /** ISO-639-1, dasselbe Kürzel wie in den Übersetzungstabellen. */
  id: string;
  /** Eigenname, so wie ihn Sprecher dieser Sprache schreiben. */
  name: string;
  /** Deutscher Name, damit die Suche auch darauf anspringt. */
  deutsch: string;
  flagge: string;
}

/**
 * Die wählbaren Sprachen, nach ihrem Eigennamen sortiert.
 *
 * Sieben Sprachen statt aller denkbaren: Eine Sprache in der Liste ist ein
 * Versprechen, dass die Oberfläche darin auch wirklich vorliegt. Dreißig
 * Einträge, von denen die meisten auf Englisch zurückfallen, wären eine
 * Enttäuschung mit Extraschritt.
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

/** Sprachen, die von rechts nach links geschrieben werden. */
export const RECHTS_NACH_LINKS = new Set(["ar"]);

export function sprachEintrag(id: string): SprachEintrag | undefined {
  return SPRACHEN.find((eintrag) => eintrag.id === id);
}
