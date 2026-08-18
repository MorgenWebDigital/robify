/**
 * Rechnerei rund um die Akzentfarbe.
 *
 * Steht als eigenes Modul da, weil es reine Funktionen sind: Sie brauchen
 * weder React noch das Backend und lassen sich darum unmittelbar prüfen.
 */

/** Schrift auf hellem Akzent. Derselbe Ton wie der Fließtext. */
const DUNKLE_SCHRIFT = "#16161a";
/** Schrift auf dunklem Akzent. */
const HELLE_SCHRIFT = "#ffffff";

/**
 * Bringt eine Eingabe auf die Form `#rrggbb`, oder gibt `null` zurück.
 *
 * Erlaubt ist mit und ohne Doppelkreuz sowie die Kurzform mit drei Stellen,
 * denn genau so hat man einen Farbwert im Kopf oder in der Zwischenablage.
 */
export function normalisiereHex(eingabe: string): string | null {
  const roh = eingabe.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(roh)) {
    return `#${roh[0]}${roh[0]}${roh[1]}${roh[1]}${roh[2]}${roh[2]}`;
  }
  if (/^[0-9a-f]{6}$/.test(roh)) return `#${roh}`;
  return null;
}

/** Zerlegt die gespeicherte Liste; unbrauchbare Einträge fallen weg. */
export function eigeneFarben(gespeichert: string): string[] {
  return gespeichert
    .split(",")
    .map((teil) => normalisiereHex(teil))
    .filter((farbe): farbe is string => farbe !== null);
}

/**
 * Relative Helligkeit nach WCAG, zwischen 0 (schwarz) und 1 (weiß).
 *
 * Die Kanäle werden vorher entzerrt: Ein Bildschirm gibt Werte nicht linear
 * aus, und ohne diesen Schritt gälte Grün als zu dunkel und Blau als zu hell.
 */
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
  // Das Auge sieht Grün am stärksten, Blau am schwächsten.
  return 0.2126 * kanaele[0] + 0.7152 * kanaele[1] + 0.0722 * kanaele[2];
}

/** Kontrastverhältnis zweier Helligkeiten nach WCAG (1 bis 21). */
function kontrast(a: number, b: number): number {
  const [hell, dunkel] = a > b ? [a, b] : [b, a];
  return (hell + 0.05) / (dunkel + 0.05);
}

/**
 * Wählt die Schriftfarbe, die auf dem Akzent besser lesbar ist.
 *
 * Nötig, seit die Vorgaben von hellem Grau bis Indigo reichen: Ein fester
 * dunkler Ton verschwand auf DarkRed, ein fester heller auf Khaki. Gerechnet
 * statt je Farbe hinterlegt, damit es auch für selbst gemischte Farben gilt.
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
