// calculations around the accent colour.
//
// a module of its own because these are pure functions: they need neither
// react nor the backend and can therefore be checked directly

/** type on a light accent, the same tone as the body text. */
const DUNKLE_SCHRIFT = "#16161a";
/** type on a dark accent. */
const HELLE_SCHRIFT = "#ffffff";

/**
 * brings an input into the form `#rrggbb`, or returns `null`.
 *
 * allowed with and without the hash sign as well as the short form of three
 * digits, because that is exactly how a colour value sits in one's head or in
 * the clipboard.
 */
export function normalisiereHex(eingabe: string): string | null {
  const roh = eingabe.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(roh)) {
    return `#${roh[0]}${roh[0]}${roh[1]}${roh[1]}${roh[2]}${roh[2]}`;
  }
  if (/^[0-9a-f]{6}$/.test(roh)) return `#${roh}`;
  return null;
}

/** splits the stored list, unusable entries fall away. */
export function eigeneFarben(gespeichert: string): string[] {
  return gespeichert
    .split(",")
    .map((teil) => normalisiereHex(teil))
    .filter((farbe): farbe is string => farbe !== null);
}

// relative luminance per wcag, between 0 (black) and 1 (white).
//
// the channels are linearised first: a screen does not output values
// linearly, and without this step green would count as too dark and blue as
// too light
function helligkeit(hex: string): number {
  const zahl = Number.parseInt(hex.slice(1), 16);
  const kanaele = [(zahl >> 16) & 255, (zahl >> 8) & 255, zahl & 255].map(
    (wert) => {
      const anteil = wert / 255;
      return anteil <= 0.04045
        ? anteil / 12.92
        : ((anteil + 0.055) / 1.055) ** 2.4;
    },
  );
  // the eye sees green strongest and blue weakest
  return 0.2126 * kanaele[0] + 0.7152 * kanaele[1] + 0.0722 * kanaele[2];
}

// contrast ratio of two luminances per wcag, 1 to 21
function kontrast(a: number, b: number): number {
  const [hell, dunkel] = a > b ? [a, b] : [b, a];
  return (hell + 0.05) / (dunkel + 0.05);
}

/**
 * picks the type colour that reads better on the accent.
 *
 * needed since the presets range from light grey to indigo: a fixed dark tone
 * disappeared on darkred, a fixed light one on khaki. calculated instead of
 * stored per colour, so it holds for colours mixed by hand as well.
 */
export function akzentSchrift(akzent: string): string {
  const farbe = normalisiereHex(akzent);
  if (!farbe) return DUNKLE_SCHRIFT;

  const eigen = helligkeit(farbe);
  return kontrast(eigen, helligkeit(HELLE_SCHRIFT)) >
    kontrast(eigen, helligkeit(DUNKLE_SCHRIFT))
    ? HELLE_SCHRIFT
    : DUNKLE_SCHRIFT;
}
