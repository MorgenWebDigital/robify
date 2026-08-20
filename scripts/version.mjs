#!/usr/bin/env node
// version.mjs — checks that the version number is the same everywhere.
//
// it stands in four places, and each one is read by a different tool:
// `package.json` by the footer of the app, `Cargo.toml` by the program
// itself, `tauri.conf.json` by the installers, the `PKGBUILD` by the arch
// package. whoever forgets one of them at release notices only through a
// package calling itself differently from the rest, and by then it is out.
//
// runs along with every `npm run build` and stops on a mismatch.
//
// call: `npm run version`, or with a number to set it everywhere
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WURZEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** where the number stands and what it looks like there. */
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
