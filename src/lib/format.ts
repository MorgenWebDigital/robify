import { spracheJetzt, t } from "./i18n";
import { mehrzahl } from "./mehrzahl";

/** mm:ss bzw. h:mm:ss, für Laufzeiten im Player und in Listen. */
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
 * Lesbare Gesamtdauer, z. B. „12 Std. 4 Min.“
 *
 * Die Einheiten laufen durch `t`, weil sie in jeder Sprache anders abgekürzt
 * werden, „Std.“ heißt auf Russisch „ч“, auf Chinesisch „小时“. Vorher stand
 * hier fester deutscher Text, und dadurch endete jede Zeitangabe der App auf
 * „Min.“, gleich welche Sprache eingestellt war.
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
 * Zahl mit den Trennzeichen der eingestellten Sprache.
 *
 * Deutsch trennt Tausender mit Punkt, Englisch mit Komma, Französisch mit
 * schmalem Leerzeichen. Fest auf „de-DE“ gestellt, las sich „1.234“ für einen
 * englischen Leser wie eine Kommazahl.
 */
export function formatNumber(value: number): string {
  return new Intl.NumberFormat(spracheJetzt()).format(value);
}

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
 * Art eines Releases als Wort.
 *
 * Als Funktion und nicht als feste Tabelle: Eine Tabelle auf Modulebene
 * entstünde einmal beim Laden und bliebe nach einem Sprachwechsel in der
 * Anfangssprache stehen. „EP“ heißt überall gleich und braucht kein `t`.
 */
export function releaseLabel(type: string): string {
  if (type === "single") return t("Single");
  if (type === "ep") return "EP";
  return t("Album");
}

/**
 * Anzahl samt gebeugtem Hauptwort, z. B. „7 Titel“.
 *
 * Das Stichwort ist das deutsche Wort in der Einzahl; die Beugung besorgt
 * `mehrzahl`. Früher nahm diese Funktion zwei fertige Wörter entgegen, das
 * reicht für Deutsch und Englisch, aber nicht für Russisch mit seinen vier
 * Formen, und in der App stand deshalb „7 Треки“ statt „7 треков“.
 */
export function plural(count: number, stichwort: string): string {
  // Chinesisch setzt kein Leerzeichen zwischen Zahl und Zählwort: „7首曲目“,
  // nicht „7 首曲目“. Ein Leerzeichen sähe dort aus wie ein Tippfehler.
  const fuge = spracheJetzt() === "zh" ? "" : " ";
  return `${formatNumber(count)}${fuge}${mehrzahl(count, stichwort)}`;
}
