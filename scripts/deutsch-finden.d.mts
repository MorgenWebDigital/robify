/**
 * Typangaben für den Sucher nach ungehülltem deutschem Text.
 *
 * Das Werkzeug selbst ist JavaScript, damit es ohne Übersetzungsschritt
 * laufen kann. Die Deckungswache bindet es aber aus TypeScript ein, und
 * dorthin braucht es eine Beschreibung.
 */

/** Alle Fundstellen unter `wurzel`, als `datei:zeile  text`. */
export function ungehuellteStellen(wurzel?: string): string[];

/** Alle `.ts`- und `.tsx`-Dateien unter einem Verzeichnis. */
export function dateien(verzeichnis: string): string[];

/** Fundstellen einer einzelnen Datei, als `[zeile, text]`. */
export function pruefen(datei: string): [number, string][];
