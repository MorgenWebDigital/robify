#!/usr/bin/env node
/**
 * Sucht Anzeigetext, der *nicht* durch `t()` läuft.
 *
 * Die Deckungswache in `src/lib/deckung.test.ts` prüft die andere Richtung:
 * Steht jeder gehüllte Begriff in der Tabelle? Text, den niemand gehüllt hat,
 * sieht sie nicht.
 *
 * Der erste Anlauf hier suchte nach deutschen Wörtern aus einer Liste. Das
 * war der falsche Zugriff: „Einstellungen behalten“, „Zeitsynchron
 * hinterlegt“ und „Zusammen“ standen nicht darin und blieben liegen. Eine
 * Wortliste ist nie vollständig.
 *
 * Darum jetzt die Umkehrung: Gemeldet wird *jeder* Text an einer Stelle, an
 * der er auf dem Bildschirm landet, sofern er nicht durch `t()` läuft. Was
 * bewusst unübersetzt bleibt, steht namentlich in `EIGENNAMEN`. Diese Liste
 * bleibt kurz, und jeder Eintrag darin ist eine Entscheidung statt einer
 * Lücke.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/**
 * Text, der in jeder Sprache gleich lautet.
 *
 * Produktnamen und Dateiformate. Alles andere gehört durch `t()`.
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
  // In der Musik steht „feat.“ auch auf russischen und chinesischen Seiten so.
  "feat.",
]);

/** Diese Dateien tragen keinen Anzeigetext. */
const AUSGENOMMEN =
  /\.(test|d)\.tsx?$|types\.ts$|lib\/(i18n|mehrzahl|sprachen)\.ts$/;

/**
 * Merkmale und Felder, deren Wert auf dem Bildschirm landet.
 *
 * `className` und `to` stehen bewusst nicht dabei: Sie tragen Technik, keinen
 * Text für Menschen.
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
 * Ersetzt Unverdächtiges durch Leerzeichen gleicher Länge.
 *
 * Nicht durch nichts: Die Zeilennummern sollen stimmen, sonst zeigt der Fund
 * an eine falsche Stelle.
 */
function leeren(inhalt, muster) {
  return inhalt.replace(muster, (treffer) => treffer.replace(/[^\n]/g, " "));
}

/**
 * Blendet `className={…}` und `class="…"` aus.
 *
 * Tailwind-Klassen sehen für die Suche aus wie Text: `text-mute/40
 * line-through` hat Buchstaben und Leerzeichen. Sie stehen oft in Zweigen
 * einer Bedingung, also genau in der Form, auf die auch echte Texte passen.
 * Die geschweiften Klammern werden dabei mitgezählt, damit auch mehrzeilige
 * Ausdrücke ganz verschwinden.
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
 * Ist das überhaupt Text für Menschen?
 *
 * Zwei zusammenhängende Buchstaben genügen als Nachweis. Damit fallen
 * Trennzeichen, Zahlen, Einsetzstellen wie `{0}` und einzelne Buchstaben
 * heraus, ohne dass jedes einzeln aufgezählt werden müsste.
 */
function istText(wert) {
  const blank = wert.replace(/\{\d+\}|\$\{[^}]*\}/g, "").trim();
  if (!/\p{L}{2}/u.test(blank)) return false;
  if (EIGENNAMEN.has(blank)) return false;
  // Technische Werte: durchgehend klein, mit Bindestrich oder Punkt verbunden.
  if (/^[a-z][a-z0-9]*([-.:][a-z0-9]+)+$/.test(blank)) return false;
  return true;
}

export function pruefen(datei) {
  let inhalt = readFileSync(datei, "utf8");
  inhalt = leeren(inhalt, /\/\*[\s\S]*?\*\//g); // Blockkommentare
  inhalt = leeren(inhalt, /\/\/[^\n]*/g); // Zeilenkommentare
  inhalt = leeren(inhalt, /^import[^;]*;/gm); // Einbindungen
  // Der gehüllte Text samt Anführungszeichen. Die weiteren Werte eines
  // Aufrufs bleiben stehen und werden für sich geprüft.
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

    // 1. Text zwischen zwei JSX-Marken: `>Text<`.
    //
    //    Nur in `.tsx`: In TypeScript sieht `Promise<void>` genauso aus, und
    //    ohne diese Einschränkung meldete der Sucher jede zweite Typangabe.
    if (istJsx) {
      for (const treffer of zeile.matchAll(/(?<![=-])>([^<>{}"'`]+)</g))
        melden(treffer[1]);

      // 2. Zeilen, die nur aus Text bestehen oder auf eine Einsetzung
      //    zulaufen: `Zusammen {formatDuration(…)}`.
      //
      //    Der große Anfangsbuchstabe trennt Text von Quelltext: Ein Satz
      //    beginnt groß, ein Bezeichner wie `readOnly` oder `actionsRechts`
      //    klein. Rechenzeichen und Klammern schließen den Rest aus.
      const nurText = zeile.match(
        /^\s*([A-ZÄÖÜ][^<>{}"'`=;()[\]:,]*?)\s*(?:\{|$)/,
      );
      if (nurText) melden(nurText[1]);
    }

    // 3. Merkmale in JSX: `label="…"`.
    for (const treffer of zeile.matchAll(
      new RegExp(`\\b(?:${ANZEIGEFELDER})="([^"]*)"`, "g"),
    ))
      melden(treffer[1]);

    // 4. Felder in Objekten: `label: "…"` und `label: \`…\``.
    for (const treffer of zeile.matchAll(
      new RegExp(
        `\\b(?:${ANZEIGEFELDER}):\\s*(["'\`])((?:(?!\\1)[^\\\\])*)\\1`,
        "g",
      ),
    ))
      melden(treffer[2]);

    // 5. Zweige einer Bedingung, die Prettier auf eigene Zeilen setzt:
    //    `? "Zeitsynchron hinterlegt"`. Der Zeilenanfang ist entscheidend,
    //    sonst schlüge jedes Objektfeld `schluessel: "wert"` mit an.
    const zweig = zeile.match(/^\s*[?:]\s*"([^"]*)"/);
    if (zweig) melden(zweig[1]);
  });

  return funde;
}

/**
 * Alle Fundstellen unter einem Verzeichnis, als `datei:zeile  text`.
 *
 * Auch vom Test in `src/lib/deckung.test.ts` benutzt: Der Sucher soll bei
 * jedem Testlauf mitlaufen, nicht erst, wenn jemand an ihn denkt.
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

// Nur beim unmittelbaren Aufruf berichten, nicht beim Einbinden aus dem Test.
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
