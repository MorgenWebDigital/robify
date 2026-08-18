#!/usr/bin/env node
/**
 * Prüft, dass die Versionsnummer überall dieselbe ist.
 *
 * Sie steht an vier Stellen, und jede wird von einem anderen Werkzeug
 * gelesen: `package.json` vom Fuß der App, `Cargo.toml` vom Programm selbst,
 * `tauri.conf.json` von den Installern, das `PKGBUILD` vom Arch-Paket. Wer
 * beim Release eine davon vergisst, merkt es erst an einem Paket, das sich
 * anders nennt als der Rest, und dann ist es schon draußen.
 *
 * Läuft bei jedem `npm run build` mit und bricht bei Abweichung ab.
 *
 * Aufruf: `npm run version` (oder mit einer Zahl, um sie überall zu setzen)
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WURZEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Wo die Nummer steht und wie sie dort aussieht. */
const STELLEN = [
  { datei: "package.json", muster: /("version"\s*:\s*")([^"]+)(")/ },
  { datei: "src-tauri/Cargo.toml", muster: /(^version\s*=\s*")([^"]+)(")/m },
  {
    datei: "src-tauri/tauri.conf.json",
    muster: /("version"\s*:\s*")([^"]+)(")/,
  },
  { datei: "packaging/arch/PKGBUILD", muster: /(^pkgver=)([^\s]+)()/m },
];

const neu = process.argv[2];
if (neu && !/^\d+\.\d+\.\d+$/.test(neu)) {
  console.error(`Keine gültige Version: ${neu} (erwartet wird z. B. 0.2.0)`);
  process.exit(1);
}

const gefunden = [];
for (const { datei, muster } of STELLEN) {
  const pfad = join(WURZEL, datei);
  const inhalt = readFileSync(pfad, "utf8");
  const treffer = inhalt.match(muster);
  if (!treffer) {
    console.error(`Keine Versionsangabe gefunden in ${datei}`);
    process.exit(1);
  }

  if (neu) {
    writeFileSync(pfad, inhalt.replace(muster, `$1${neu}$3`));
    console.log(`${datei} → ${neu}`);
  }
  gefunden.push({ datei, version: neu ?? treffer[2] });
}

const verschieden = [...new Set(gefunden.map((s) => s.version))];
if (verschieden.length > 1) {
  console.error("Die Versionsnummern gehen auseinander:\n");
  for (const { datei, version } of gefunden)
    console.error(`  ${version.padEnd(10)} ${datei}`);
  console.error("\nMit `npm run version -- <nummer>` überall gleichziehen.");
  process.exit(1);
}

if (!neu)
  console.log(
    `Version ${verschieden[0]}, an allen ${gefunden.length} Stellen gleich.`,
  );
