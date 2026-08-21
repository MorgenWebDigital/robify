import { spracheJetzt, t } from "./i18n";
import { mehrzahl } from "./mehrzahl";

/** mm:ss or h:mm:ss, for running times in the player and in lists */
export function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) ms = 0;
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

/**
 * readable total duration, "12 Std. 4 Min." for instance.
 *
 * the units run through `t` because every language abbreviates them
 * differently, "Std." is "ч" in russian and "小时" in chinese. fixed german
 * text stood here before, which made every duration in the app end in "Min."
 * whatever the language was set to.
 */
export function formatDuration(ms: number): string {
  const totalMinutes = Math.round(ms / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours >= 24) {
    const days = Math.floor(hours / 24);
    return `${days} ${t("Tg.")} ${hours % 24} ${t("Std.")}`;
  }
  if (hours > 0) return `${hours} ${t("Std.")} ${minutes} ${t("Min.")}`;
  if (totalMinutes > 0) return `${totalMinutes} ${t("Min.")}`;
  return `${Math.round(ms / 1000)} ${t("Sek.")}`;
}

/**
 * a number with the separators of the selected language.
 *
 * german separates thousands with a dot, english with a comma, french with a
 * thin space. pinned to "de-DE", "1.234" read like a decimal to an english
 * reader.
 */
export function formatNumber(value: number): string {
  return new Intl.NumberFormat(spracheJetzt()).format(value);
}

/** a byte count as b, kb, mb or gb */
export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return t("unbekannt");
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

/**
 * the type of a release as a word.
 *
 * a function and not a fixed table: a table at module level would come into
 * being once at load time and stay in the starting language after a language
 * switch. "EP" is called the same everywhere and needs no `t`.
 */
export function releaseLabel(type: string): string {
  if (type === "single") return t("Single");
  if (type === "ep") return "EP";
  return t("Album");
}

/**
 * a count together with the inflected noun, "7 Titel" for instance.
 *
 * the keyword is the german word in the singular, and `mehrzahl` handles the
 * inflection. it used to take two finished words, which does for
 * german and english but not for russian with its four forms, and the app
 * therefore read "7 Треки" instead of "7 треков".
 */
export function plural(count: number, stichwort: string): string {
  // chinese sets no space between number and measure word: "7首曲目", not
  // "7 首曲目". a space would look like a typo there
  const fuge = spracheJetzt() === "zh" ? "" : " ";
  return `${formatNumber(count)}${fuge}${mehrzahl(count, stichwort)}`;
}
