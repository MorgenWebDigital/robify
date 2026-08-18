#!/usr/bin/env node
/**
 * Umschließt sichtbare deutsche Texte einer Datei mit `t(...)`.
 *
 * Zwei Muster werden erfasst:
 *
 *   - Textknoten in JSX, also alles zwischen `>` und `<`. Auch über mehrere
 *     Zeilen: Der Umbruch wird zu einem Leerzeichen zusammengezogen, wie es
 *     der Browser ohnehin täte. Das ist der wichtigste Punkt, ein
 *     zeilenweiser Ansatz zerschnitte lange Absätze in Fragmente, und
 *     Satzfetzen lassen sich nicht übersetzen.
 *   - Zeichenketten hinter bekannten Merkmalen (`label=`, `title=` …) und in
 *     Objektfeldern (`label: "…"`).
 *
 * Bewusst nicht erfasst: Zeichenketten in Vergleichen, Schlüssel von
 * Einstellungen und alles in Kommentaren. Darum wird ausschließlich innerhalb
 * von JSX und an klar benannten Feldern gesucht, nicht an beliebigen
 * Zeichenketten.
 *
 * Aufruf: `node scripts/umschliessen.mjs src/pages/Foo.tsx [--pruefen]`
 */
import { readFileSync, writeFileSync } from "node:fs";

const datei = process.argv[2];
const nurPruefen = process.argv.includes("--pruefen");
if (!datei) {
  console.error("Aufruf: node scripts/umschliessen.mjs <datei> [--pruefen]");
  process.exit(1);
}

/** Merkmale, deren Wert auf dem Bildschirm erscheint. */
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

/** Beginnt mit einem Großbuchstaben oder Anführungszeichen und hat Substanz. */
const istText = (text) =>
  /^[A-ZÄÖÜ„][^]{2,}$/.test(text) && /[a-zäöüß]/.test(text);

// ── Textknoten in JSX, auch mehrzeilig ────────────────────────────────────
inhalt = inhalt.replace(
  />(\s*)([^<>{}]+?)(\s*)</g,
  (treffer, vorn, roh, hinten) => {
    const text = roh.replace(/\s+/g, " ").trim();
    if (!istText(text)) return treffer;
    gefunden.push(text);
    return `>${vorn}{t(${JSON.stringify(text)})}${hinten}<`;
  },
);

// ── Merkmale ──────────────────────────────────────────────────────────────
for (const merkmal of MERKMALE) {
  const muster = new RegExp(`(\\b${merkmal}=)"([^"]+)"`, "g");
  inhalt = inhalt.replace(muster, (treffer, kopf, text) => {
    if (!istText(text)) return treffer;
    gefunden.push(text);
    return `${kopf}{t(${JSON.stringify(text)})}`;
  });
}

// ── Objektfelder wie `label: "Kacheln"` ───────────────────────────────────
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
  // Einfuhr ergänzen, falls sie fehlt.
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
