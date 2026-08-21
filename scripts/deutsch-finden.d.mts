// type declarations for the finder of unwrapped german text.
//
// the tool itself is javascript so it can run without a compile step. the
// coverage guard imports it from typescript though, and a description is
// needed for that

/** every finding under `wurzel`, as `file:line  text` */
export function ungehuellteStellen(wurzel?: string): string[];

/** every `.ts` and `.tsx` file under a directory */
export function dateien(verzeichnis: string): string[];

/** the findings of a single file, as `[line, text]` */
export function pruefen(datei: string): [number, string][];
