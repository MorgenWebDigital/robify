// keeps note of backend calls that take unusually long — and above all of
// those that never come back.
//
// there was one such freeze, once, and it was never reproduced. that is the
// worst kind of fault: without a trace there is nothing to look at, and every
// attempt at a fix is guesswork. so no fix is attempted here; what is built is
// a memory. should it happen again, the report says which call is hanging and
// since when.
//
// deliberately without a timer. a `setTimeout` would have to be cancelled in
// every path, and it makes the whole thing awkward to test. instead the
// running calls are simply kept, and whoever asks is told which of them have
// been running too long by now.

/** from here on a call counts as conspicuous. */
export const GEDULD_MS = 30_000;

/** at most this many are kept; the oldest fall away. */
const HOECHSTENS = 20;

export type Auffaellig = {
  befehl: string;
  /** when it started, as `Date.now()`. */
  begonnen: number;
  /** how long it took, or `null` where it is still running. */
  dauerMs: number | null;
};

type Lauf = { befehl: string; begonnen: number };

const laufend = new Map<number, Lauf>();
const beendet: Auffaellig[] = [];
let zaehler = 0;

/** watches a call and answers exactly what it was given. */
export function beobachten<T>(
  befehl: string,
  starten: () => Promise<T>,
): Promise<T> {
  const id = ++zaehler;
  laufend.set(id, { befehl, begonnen: Date.now() });

  return starten().finally(() => {
    const lauf = laufend.get(id);
    laufend.delete(id);
    if (!lauf) return;

    const dauer = Date.now() - lauf.begonnen;
    if (dauer < GEDULD_MS) return;

    beendet.unshift({ befehl, begonnen: lauf.begonnen, dauerMs: dauer });
    beendet.length = Math.min(beendet.length, HOECHSTENS);
  });
}

/**
 * what was conspicuous, the still running ones first.
 *
 * a download or a scan over a large folder legitimately takes minutes and
 * shows up here as well. what a freeze looks like is different: an entry that
 * keeps standing without a running time.
 */
export function auffaelligkeiten(jetzt: number = Date.now()): Auffaellig[] {
  const haengend: Auffaellig[] = [...laufend.values()]
    .filter((lauf) => jetzt - lauf.begonnen >= GEDULD_MS)
    .sort((a, b) => a.begonnen - b.begonnen)
    .map((lauf) => ({
      befehl: lauf.befehl,
      begonnen: lauf.begonnen,
      dauerMs: null,
    }));

  return [...haengend, ...beendet].slice(0, HOECHSTENS);
}

/** the same as a few lines for the report, or an empty string where nothing stands out. */
export function alsText(jetzt: number = Date.now()): string {
  const liste = auffaelligkeiten(jetzt);
  if (liste.length === 0) return "";

  return liste
    .map((eintrag) => {
      const sekunden =
        eintrag.dauerMs === null
          ? Math.round((jetzt - eintrag.begonnen) / 1000)
          : Math.round(eintrag.dauerMs / 1000);
      const zustand = eintrag.dauerMs === null ? " (läuft noch)" : "";
      return `  ${eintrag.befehl}: ${sekunden} s${zustand}`;
    })
    .join("\n");
}

/** only for the tests. */
export function zuruecksetzen(): void {
  laufend.clear();
  beendet.length = 0;
  zaehler = 0;
}
