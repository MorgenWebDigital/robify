import { spracheJetzt, t } from "./i18n";

// dates in the selected language.
//
// `Intl.DateTimeFormat` brings month and weekday names for every language,
// and their order along with them: in german the day stands before the month,
// in english behind it, in chinese the year comes first. "de-DE" used to
// stand fixed everywhere in the app, and the weekday on the home page stayed
// german even with a russian interface.
//
// every function creates its formatter on demand rather than at module level:
// a formatter remembers its language, and one created once would keep the old
// one after a language switch

/** weekday with day and month, "Montag, 17. August" for instance */
export function wochentagUndTag(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    weekday: "long",
    day: "numeric",
    month: "long",
  }).format(datum);
}

/** month and year, "August 2026" for instance */
export function monatUndJahr(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    month: "long",
    year: "numeric",
  }).format(datum);
}

/** the day written out, "17. August 2026" for instance */
export function vollesDatum(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(datum);
}

/** short day without a year, for the bars in the review */
export function kurzerTag(datum: Date): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    day: "numeric",
    month: "short",
  }).format(datum);
}

/**
 * short month without a day, for the yearly review.
 *
 * with the year across several years: in the review of everything, "Aug."
 * would otherwise stand at both ends of the history meaning two different
 * ones.
 */
export function kurzerMonat(datum: Date, mitJahr = false): string {
  return new Intl.DateTimeFormat(spracheJetzt(), {
    month: "short",
    ...(mitJahr ? { year: "numeric" } : {}),
  }).format(datum);
}

/**
 * a span from day to day, "17. bis 23. August 2026" for instance.
 *
 * `formatRange` folds what repeats itself: where both days lie in the same
 * month it names it once, and it picks the separator of the language.
 * assembled by hand a dash stood there which neither english nor arabic
 * spelling sets that way.
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
 * a date from the database into readable form.
 *
 * the evaluation delivers keys such as `2026-08-17` or `2026-08` because
 * sqlite groups by them. they are meant for sorting, not for reading.
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
 * a greeting by time of day.
 *
 * the bounds are deliberately rough: whoever listens to music at four in the
 * morning is more likely still awake than already up.
 */
export function begruessung(): string {
  const stunde = new Date().getHours();
  if (stunde < 5) return t("Gute Nacht");
  if (stunde < 11) return t("Guten Morgen");
  if (stunde < 18) return t("Hallo");
  return t("Guten Abend");
}
