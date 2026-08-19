/**
 * Wohin Rückmeldungen gehen und wo gespendet werden kann.
 *
 * Steht an einer Stelle und nicht verstreut in der Oberfläche, weil beides von
 * außen kommt: eine Adresse im Netz, die es erst gibt, wenn das Vorhaben
 * veröffentlicht ist, und Zahlungsadressen, die sich niemand ausdenken kann.
 *
 * Was hier leer bleibt, zeigt die App nicht an. Ein Knopf, der ins Leere
 * führt, ist schlechter als kein Knopf.
 */

/**
 * Öffentliche Stelle für Fehler und Vorschläge, etwa ein Fehlermelder.
 *
 * Bewusst kein Briefkasten: Eine E-Mail verrät die Adresse des Absenders,
 * bleibt unter zweien hängen und ist für den nächsten mit demselben Problem
 * nicht auffindbar. Eine öffentliche Stelle kommt ohne Namen und ohne Adresse
 * aus, und wer dasselbe erlebt, findet den Eintrag wieder.
 */
export const MELDESTELLE = "https://github.com/MorgenWebDigital/robify/issues";

/** Eine Spendenadresse, wie sie in den Einstellungen erscheint. */
export interface Spendenweg {
  /** Was dort ankommt, z. B. „Bitcoin“ oder „Monero“. */
  name: string;
  /** Die vollständige Adresse. Angezeigt wird sie gekürzt. */
  adresse: string;
}

/**
 * Spendenwege, in der Reihenfolge, in der sie erscheinen sollen.
 *
 * Kryptowährungen und nicht ein Bezahldienst: Für eine Überweisung braucht es
 * weder ein Konto bei einem Dritten noch den Namen des Gebenden, und Robify
 * selbst bekommt von einer Spende nichts mit — es zeigt nur eine Zeichenkette
 * an, die man kopieren kann.
 */
export const SPENDEN: Spendenweg[] = [];
