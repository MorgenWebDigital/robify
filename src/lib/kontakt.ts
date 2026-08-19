// where feedback goes and where donations can be sent.
//
// kept in one place instead of scattered through the ui because both come
// from outside: an address on the net that exists only once the project is
// published, and payment addresses nobody can invent.
//
// note: whatever stays empty here is not shown by the app. a button leading
// nowhere is worse than no button.

/**
 * public place for bugs and suggestions, an issue tracker for instance.
 *
 * deliberately no mailbox: an e-mail gives the address of the sender away,
 * stays between two people and cannot be found by the next one with the same
 * problem. a public place needs neither a name nor an address, and whoever
 * runs into the same thing finds the entry again.
 */
export const MELDESTELLE = "https://github.com/MorgenWebDigital/robify/issues";

/** a donation address as it appears in the settings. */
export interface Spendenweg {
  /** what arrives there, "Bitcoin" or "Monero" for instance. */
  name: string;
  /** the complete address. it is displayed shortened. */
  adresse: string;
}

/**
 * donation routes, in the order they are to appear in.
 *
 * cryptocurrencies rather than a payment service: a transfer needs neither an
 * account with a third party nor the name of the giver, and robify itself
 * learns nothing of a donation, it only shows a string that can be copied.
 */
export const SPENDEN: Spendenweg[] = [];
