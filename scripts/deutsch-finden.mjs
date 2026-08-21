#!/usr/bin/env node
// deutsch-finden.mjs — looks for display text that does not run through
// `t()`.
//
// the coverage guard in `src/lib/deckung.test.ts` checks the other direction:
// is every wrapped term in the table? text nobody wrapped is invisible to it.
//
// the first attempt here searched for german words from a list. that was the
// wrong grip: several phrases were not in it and stayed behind. a word list is
// never complete.
//
// hence the inversion now: every text at a place where it lands on the screen
// is reported, unless it runs through `t()`. what stays untranslated on
// purpose stands by name in `EIGENNAMEN`. that list stays short, and every
// entry in it is a decision instead of a gap.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/**
 * text that reads the same in every language.
 *
 * product names and file formats. everything else belongs through `t()`.
 */
const EIGENNAMEN = new Set([
  "Robify",
  "MP3",
  "FLAC",
  "M4A / AAC",
  "OGG Vorbis",
  "EP",
  "EPs",
  "Spotify",
  "YouTube",
  "SoundCloud",
  "Bandcamp",
  "Genius",
  "MusicBrainz",
  "yt-dlp",
  "ffmpeg",
  "LRC",
  // the name of a licence, as "MIT" would be one
  "PolyForm Noncommercial",
  // in music, "feat." stands that way on russian and chinese pages too
  "feat.",
]);

/** these files carry no display text */
const AUSGENOMMEN =
  /\.(test|d)\.tsx?$|types\.ts$|lib\/(i18n|mehrzahl|sprachen)\.ts$/;

/**
 * attributes and fields whose value lands on the screen.
 *
 * `className` and `to` are deliberately absent: they carry technology, not
 * text for humans.
 */
const ANZEIGEFELDER =
  "label|title|placeholder|aria-label|alt|subtitle|hint|text|message|tooltip|scope";

export function dateien(verzeichnis) {
  return readdirSync(verzeichnis, { withFileTypes: true }).flatMap((eintrag) =>
    eintrag.isDirectory()
      ? dateien(join(verzeichnis, eintrag.name))
      : /\.tsx?$/.test(eintrag.name)
        ? [join(verzeichnis, eintrag.name)]
        : [],
  );
}

/**
 * replaces the harmless parts with spaces of the same length.
 *
 * not with nothing: the line numbers are to stay right, otherwise a finding
 * points at the wrong place.
 */
function leeren(inhalt, muster) {
  return inhalt.replace(muster, (treffer) => treffer.replace(/[^\n]/g, " "));
}

/**
 * blanks out `className={…}` and `class="…"`.
 *
 * to the search, tailwind classes look like text: `text-mute/40 line-through`
 * has letters and spaces. they often stand in the branches of a condition, so
 * in exactly the shape real texts match too. the braces are counted along so
 * multi-line expressions disappear entirely.
 */
function klassenLeeren(inhalt) {
  let ergebnis = inhalt;
  let stelle;
  while ((stelle = ergebnis.indexOf("className={")) !== -1) {
    let tiefe = 0;
    let i = stelle + "className=".length;
    do {
      if (ergebnis[i] === "{") tiefe += 1;
      else if (ergebnis[i] === "}") tiefe -= 1;
      i += 1;
    } while (tiefe > 0 && i < ergebnis.length);
    const stueck = ergebnis.slice(stelle, i).replace(/[^\n]/g, " ");
    ergebnis = ergebnis.slice(0, stelle) + stueck + ergebnis.slice(i);
  }
  return leeren(ergebnis, /className="[^"]*"/g);
}

/**
 * whether this is text for humans at all.
 *
 * two adjacent letters suffice as evidence. separators, numbers, placeholders
 * such as `{0}` and single letters drop out with that, without each one having
 * to be listed separately.
 */
function istText(wert) {
  const blank = wert.replace(/\{\d+\}|\$\{[^}]*\}/g, "").trim();
  if (!/\p{L}{2}/u.test(blank)) return false;
  if (EIGENNAMEN.has(blank)) return false;
  // technical values: lowercase throughout, joined by a hyphen or a dot
  if (/^[a-z][a-z0-9]*([-.:][a-z0-9]+)+$/.test(blank)) return false;
  return true;
}

export function pruefen(datei) {
  let inhalt = readFileSync(datei, "utf8");
  inhalt = leeren(inhalt, /\/\*[\s\S]*?\*\//g); // block comments
  inhalt = leeren(inhalt, /\/\/[^\n]*/g); // line comments
  inhalt = leeren(inhalt, /^import[^;]*;/gm); // imports
  // the wrapped text together with its quotes. the further values of a call
  // stay standing and are checked on their own
  inhalt = leeren(
    inhalt,
    /(?<![A-Za-z0-9_$])t\(\s*(["'`])(?:(?!\1)[^\\]|\\.)*\1/g,
  );
  inhalt = klassenLeeren(inhalt);

  const funde = [];
  const istJsx = datei.endsWith(".tsx");

  inhalt.split("\n").forEach((zeile, nummer) => {
    const melden = (wert) => {
      if (istText(wert)) funde.push([nummer + 1, wert.trim()]);
    };

    // text between two jsx marks: `>Text<`.
    //
    // in `.tsx` only: in typescript `Promise<void>` looks exactly the same,
    // and without that restriction the finder reported every second type
    if (istJsx) {
      for (const treffer of zeile.matchAll(/(?<![=-])>([^<>{}"'`]+)</g))
        melden(treffer[1]);

      // lines consisting of text alone or running into an interpolation:
      // `Zusammen {formatDuration(…)}`.
      //
      // the capital first letter separates text from source: a sentence
      // starts in capitals, an identifier such as `readOnly` or
      // `actionsRechts` in lowercase. operators and brackets rule the rest
      // out
      const nurText = zeile.match(
        /^\s*([A-ZÄÖÜ][^<>{}"'`=;()[\]:,]*?)\s*(?:\{|$)/,
      );
      if (nurText) melden(nurText[1]);
    }

    // attributes in jsx: `label="…"`
    for (const treffer of zeile.matchAll(
      new RegExp(`\\b(?:${ANZEIGEFELDER})="([^"]*)"`, "g"),
    ))
      melden(treffer[1]);

    // fields in objects: `label: "…"` and `label: \`…\``
    for (const treffer of zeile.matchAll(
      new RegExp(
        `\\b(?:${ANZEIGEFELDER}):\\s*(["'\`])((?:(?!\\1)[^\\\\])*)\\1`,
        "g",
      ),
    ))
      melden(treffer[2]);

    // branches of a condition prettier puts on their own lines:
    // `? "Zeitsynchron hinterlegt"`. the start of the line is decisive,
    // otherwise every object field `key: "value"` would fire along
    const zweig = zeile.match(/^\s*[?:]\s*"([^"]*)"/);
    if (zweig) melden(zweig[1]);
  });

  return funde;
}

/**
 * every finding under a directory, as `file:line  text`.
 *
 * used by the test in `src/lib/deckung.test.ts` as well: the finder is to run
 * along with every test run, not only when somebody thinks of it.
 */
export function ungehuellteStellen(wurzel = "src") {
  const alle = [];
  for (const datei of dateien(wurzel)) {
    if (AUSGENOMMEN.test(datei)) continue;
    for (const [nummer, text] of pruefen(datei)) {
      alle.push(`${datei}:${nummer}  ${text.slice(0, 90)}`);
    }
  }
  return alle;
}

// report on a direct call only, not when imported from the test
if (
  process.argv[1] &&
  import.meta.url.endsWith(process.argv[1].split("/").pop())
) {
  const stellen = ungehuellteStellen();
  for (const stelle of stellen) console.log(stelle);
  console.log(
    stellen.length === 0
      ? "Nichts Ungehülltes gefunden."
      : `\n${stellen.length} Stellen.`,
  );
  process.exit(stellen.length === 0 ? 0 : 1);
}
