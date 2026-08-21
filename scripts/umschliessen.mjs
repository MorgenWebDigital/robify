#!/usr/bin/env node
// umschliessen.mjs — wraps the visible german texts of a file in `t(...)`.
//
// two patterns are caught:
//
//   - text nodes in jsx, so everything between `>` and `<`. across several
//     lines too: the break is folded into a space, as the browser would do
//     anyway. that is the most important point, a line-by-line approach would
//     cut long paragraphs into fragments, and fragments of a sentence cannot
//     be translated.
//   - strings behind known attributes (`label=`, `title=` and so on) and in
//     object fields (`label: "…"`).
//
// deliberately not caught: strings in comparisons, keys of settings and
// everything in comments. the search therefore runs inside jsx and at clearly
// named fields alone, not at arbitrary strings.
//
// call: `node scripts/umschliessen.mjs src/pages/Foo.tsx [--pruefen]`
import { readFileSync, writeFileSync } from "node:fs";

const datei = process.argv[2];
const nurPruefen = process.argv.includes("--pruefen");
if (!datei) {
  console.error("Aufruf: node scripts/umschliessen.mjs <datei> [--pruefen]");
  process.exit(1);
}

/** attributes whose value appears on the screen */
const MERKMALE = [
  "label",
  "hint",
  "title",
  "subtitle",
  "placeholder",
  "aria-label",
  "text",
  "removeLabel",
  "eyebrow",
  "scope",
  "emptyMessage",
];

let inhalt = readFileSync(datei, "utf8");
const gefunden = [];

/** starts with a capital or a quote and carries substance */
const istText = (text) =>
  /^[A-ZÄÖÜ„][^]{2,}$/.test(text) && /[a-zäöüß]/.test(text);

// --- text nodes in jsx, multi-line too ---
inhalt = inhalt.replace(
  />(\s*)([^<>{}]+?)(\s*)</g,
  (treffer, vorn, roh, hinten) => {
    const text = roh.replace(/\s+/g, " ").trim();
    if (!istText(text)) return treffer;
    gefunden.push(text);
    return `>${vorn}{t(${JSON.stringify(text)})}${hinten}<`;
  },
);

// --- attributes ---
for (const merkmal of MERKMALE) {
  const muster = new RegExp(`(\\b${merkmal}=)"([^"]+)"`, "g");
  inhalt = inhalt.replace(muster, (treffer, kopf, text) => {
    if (!istText(text)) return treffer;
    gefunden.push(text);
    return `${kopf}{t(${JSON.stringify(text)})}`;
  });
}

// --- object fields such as `label: "Kacheln"` ---
inhalt = inhalt.replace(
  /\b(label|title|hint|text):\s*"([^"]+)"/g,
  (treffer, feld, text) => {
    if (!istText(text)) return treffer;
    gefunden.push(text);
    return `${feld}: t(${JSON.stringify(text)})`;
  },
);

const einmalig = [...new Set(gefunden)];
if (!nurPruefen) {
  // add the import where it is missing
  if (einmalig.length > 0 && !/from "[^"]*lib\/i18n"/.test(inhalt)) {
    const tiefe =
      datei.includes("/pages/") || datei.includes("/components/") ? ".." : ".";
    inhalt = inhalt.replace(
      /^(import .*\n)/,
      `$1import { t } from "${tiefe}/lib/i18n";\n`,
    );
  }
  writeFileSync(datei, inhalt);
}

console.log(`${datei}: ${einmalig.length} Texte`);
for (const text of einmalig) console.log(`  ${text}`);
