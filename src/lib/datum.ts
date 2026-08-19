import { spracheJetzt, t } from "./i18n";

/**
 * Datumsangaben in der eingestellten Sprache.
 *
 * `Intl.DateTimeFormat` bringt Monats- und Wochentagsnamen für jede Sprache
 * mit, und dazu deren Reihenfolge: Im Deutschen steht der Tag vor dem Monat,
 * im Englischen dahinter, im Chinesischen das Jahr zuerst. Vorher stand in der
 * App überall fest „de-DE“, und der Wochentag auf der Startseite blieb
 * deutsch, auch wenn die Oberfläche russisch war.
 *
 * Jede Funktion legt ihren Formatierer bei Bedarf an statt auf Modulebene:
 * Ein Formatierer merkt sich seine Sprache, und ein einmal angelegter bliebe
 * nach einem Sprachwechsel bei der alten.
 */

/** Wochentag mit Tag und Monat, z. B. „Montag, 17. August“. */
export function wochentagUndTag(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    weekday: "long",
    day: "numeric",
    month: "long",
  }).format(datum);
}

/** Monat und Jahr, z. B. „August 2026“. */
export function monatUndJahr(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    month: "long",
    year: "numeric",
  }).format(datum);
}

/** Tag ausgeschrieben, z. B. „17. August 2026“. */
export function vollesDatum(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(datum);
}

/** Kurzer Tag ohne Jahr, für die Balken im Rückblick. */
export function kurzerTag(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    day: "numeric",
    month: "short",
  }).format(datum);
}

/**
 * Kurzer Monat ohne Tag, für den Jahresrückblick.
 *
 * Über mehrere Jahre hinweg mit Jahreszahl: Im Rückblick auf alles stehen
 * sonst an beiden Enden des Verlaufs „Aug.“ und meinen zwei verschiedene.
 */
export function kurzerMonat(datum: Date, mitJahr = false): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    month: "short",
    ...(mitJahr ? { year: "numeric" } : {}),
  }).format(datum);
}

/**
 * Zeitspanne von Tag zu Tag, z. B. „17. bis 23. August 2026“.
 *
 * `formatRange` setzt selbst zusammen, was sich wiederholt: Liegen beide Tage
 * im selben Monat, nennt es ihn nur einmal, und es wählt das Trennzeichen der
 * Sprache. Von Hand zusammengesetzt stand dort ein Gedankenstrich, den weder
 * die englische noch die arabische Schreibweise so setzt.
 */
export function zeitraum(von: Date, bis: Date): string {
  const formatierer = new Intl.DateTimeFormat(spracheJetzt(), {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
  return formatierer.formatRange(von, bis);
}

/**
 * Ein Datum aus der Datenbank in lesbare Form.
 *
 * Die Auswertung liefert Schlüssel wie `2026-08-17` oder `2026-08`, weil
 * SQLite danach gruppiert. Sie sind zum Sortieren gedacht, nicht zum Lesen.
 */
export function ausSchluessel(schluessel: string, mitJahr = false): string {
  const teile = schluessel.split("-").map(Number);
  if (teile.length === 3 && teile.every(Number.isFinite)) {
    return kurzerTag(new Date(teile[0], teile[1] - 1, teile[2]));
  }
  if (teile.length === 2 && teile.every(Number.isFinite)) {
    return kurzerMonat(new Date(teile[0], teile[1] - 1, 1), mitJahr);
  }
  return schluessel;
}

/**
 * Begrüßung nach Tageszeit.
 *
 * Die Grenzen sind bewusst grob: Wer um vier Uhr morgens Musik hört, ist eher
 * noch wach als schon auf.
 */
export function begruessung(): string {
  const stunde = new Date().getHours();
  if (stunde < 5) return t("Gute Nacht");
  if (stunde < 11) return t("Guten Morgen");
  if (stunde < 18) return t("Hallo");
  return t("Guten Abend");
}
