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
  /**
   * what else arrives at the same address, by its ticker.
   *
   * a token does not live on a chain of its own but on the one of its coin,
   * and it therefore needs no address of its own. whoever wants to give
   * dollars rather than a rate of exchange can already do so; without this
   * line nobody would know it.
   */
  dazu?: string[];
}

/**
 * donation routes, in the order they are to appear in.
 *
 * cryptocurrencies rather than a payment service: a transfer needs neither an
 * account with a third party nor the name of the giver, and robify itself
 * learns nothing of a donation, it only shows a string that can be copied.
 *
 * the seven cover the field without a further entry. the address of ethereum
 * also receives on arbitrum, base, optimism and polygon and every token of
 * that chain, the one of solana likewise, and tron is where most of the
 * stablecoin traffic runs because a transfer there costs cents. bitcoin is
 * the one everybody has, monero the one for whoever minds who can read along.
 *
 * named under `dazu` are only the stablecoins. a wrapped bitcoin and a
 * bridged dogecoin arrive at those addresses as well, but naming them would
 * invite a detour: bitcoin has an entry of its own here, and whoever holds
 * dogecoin holds it on its own chain, which this list does not take. an
 * offer nobody can sensibly accept is worse than none.
 *
 * bitcoin deliberately as a plain bech32 address and not as a silent payment:
 * that would hide the sum received and every giver, but hardly any exchange
 * can pay to one, and that is where a donation usually comes from.
 */
export const SPENDEN: Spendenweg[] = [
  { name: "Bitcoin", adresse: "bc1q2p7srml7dww00g23nj5ln5mskusymm4x4nwvk4" },
  {
    name: "Monero",
    adresse:
      "43oCMfjUrkVAUhNAL9PhJm7ujPmEvpfERe7LQc58SirtX9NJWpMXSLdGLa2c55veSj1ovh4PRreaadTmaCb86krwBAY8z8D",
  },
  {
    name: "Ethereum",
    adresse: "0x6cBc187FbCc8aE8bD85cAade7b701f2936a14204",
    dazu: ["USDT", "USDC", "DAI"],
  },
  {
    name: "Solana",
    adresse: "9oPPuDzgzEJ1NGsaQEJyDYdin2QHs4382FydZBrjqE5Q",
    dazu: ["USDT", "USDC"],
  },
  { name: "Litecoin", adresse: "ltc1q8ldj4jy4nkjf0usec8u56z0894dlplfcp2a4dz" },
  {
    name: "Bitcoin Cash",
    adresse: "bitcoincash:qqm3wga0uyxnh8ux74m4yqtcs6fzfkq4mgv80h9ky0",
  },
  {
    name: "Tron",
    adresse: "TWYGPLvr6tkz9FBbF5Q8HtULafcLu4RfAc",
    dazu: ["USDT", "USDC"],
  },
];
