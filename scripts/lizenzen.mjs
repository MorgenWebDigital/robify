#!/usr/bin/env node
// lizenzen.mjs — collects the licence details of every dependency into
// `public/lizenzen.json`.
//
// mit and apache-2.0, together almost the whole tree, both demand that
// copyright notice and licence text ship along as soon as a binary is passed
// on. this file does exactly that, and the settings page displays it.
//
// only what actually ends up in the program is covered: for rust the normal
// dependencies (`cargo tree -e normal`, so without dev and build crates), for
// npm the production tree without devDependencies. build tools such as vite or
// lightningcss pass nothing on to the user and therefore do not belong in it.
//
// call: `npm run lizenzen`
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WURZEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ZIEL = join(WURZEL, "public", "lizenzen.json");

/** the filenames projects store their licence text under. */
const LIZENZDATEI = /^(LICEN[CS]E|COPYING|NOTICE|UNLICEN[CS]E)([-._].*)?$/i;

function rufe(befehl, argumente, cwd) {
  return execFileSync(befehl, argumente, {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
}

/**
 * reads the licence texts out of a package folder.
 *
 * some projects store several of them (LICENSE-MIT and LICENSE-APACHE under a
 * dual licence), and both belong to it then, as which of the two applies is
 * for the user to pick.
 */
function texteAus(ordner) {
  if (!ordner || !existsSync(ordner)) return [];
  const treffer = [];
  for (const name of readdirSync(ordner)) {
    if (!LIZENZDATEI.test(name)) continue;
    try {
      const inhalt = readFileSync(join(ordner, name), "utf8").trim();
      // pointers instead of text ("see LICENSE") help nobody
      if (inhalt.length > 120) treffer.push(inhalt);
    } catch {
      /* a folder instead of a file, or unreadable: skip it. */
    }
  }
  return treffer;
}

// --- rust ---

function rustPakete() {
  const tauri = join(WURZEL, "src-tauri");

  // what is really linked in: normal edges, without dev and build
  const baum = rufe(
    "cargo",
    ["tree", "-e", "normal", "--prefix", "none", "--format", "{p}"],
    tauri,
  );
  const ausgeliefert = new Set(
    baum
      .split("\n")
      .map((zeile) => zeile.trim().replace(/ \(\*\)$/, ""))
      .filter(Boolean)
      // "name v1.2.3" or "name v1.2.3 (/path)" becomes "name v1.2.3"
      .map((zeile) => zeile.split(" ").slice(0, 2).join(" ")),
  );

  const metadaten = JSON.parse(
    rufe(
      "cargo",
      ["metadata", "--format-version", "1", "--all-features"],
      tauri,
    ),
  );

  const pakete = [];
  for (const paket of metadaten.packages) {
    const schluessel = `${paket.name} v${paket.version}`;
    if (!ausgeliefert.has(schluessel)) continue;
    if (paket.name === "robify") continue; // our own work

    pakete.push({
      name: paket.name,
      version: paket.version,
      lizenz:
        paket.license ??
        (paket.license_file ? "siehe Lizenzdatei" : "nicht angegeben"),
      quelle: paket.repository ?? null,
      herkunft: "Rust",
      texte: texteAus(
        paket.manifest_path ? dirname(paket.manifest_path) : null,
      ),
    });
  }
  return pakete;
}

// --- npm ---

function npmPakete() {
  let baum;
  try {
    baum = JSON.parse(
      rufe("npm", ["ls", "--omit=dev", "--all", "--json"], WURZEL),
    );
  } catch (fehler) {
    // `npm ls` ends with code 1 as soon as anything is missing, and the
    // output is usable all the same
    baum = JSON.parse(fehler.stdout || "{}");
  }

  const gefunden = new Map();
  const laufe = (knoten) => {
    for (const [name, wert] of Object.entries(knoten.dependencies ?? {})) {
      // unresolved nodes (unmet peers, skipped optional dependencies) carry
      // no version and do not lie on disk either, they do not ship along and
      // do not belong in the list
      if (!wert.version) continue;
      const schluessel = `${name}@${wert.version}`;
      if (!gefunden.has(schluessel)) {
        gefunden.set(schluessel, {
          name,
          version: wert.version,
          pfad: wert.path,
        });
        laufe(wert);
      }
    }
  };
  laufe(baum);

  const pakete = [];
  for (const { name, version, pfad } of gefunden.values()) {
    const ordner = pfad ?? join(WURZEL, "node_modules", name);
    let angabe = "nicht angegeben";
    let quelle = null;
    try {
      const beschreibung = JSON.parse(
        readFileSync(join(ordner, "package.json"), "utf8"),
      );
      const lizenz = beschreibung.license ?? beschreibung.licenses;
      angabe =
        typeof lizenz === "string"
          ? lizenz
          : Array.isArray(lizenz)
            ? lizenz.map((l) => l.type ?? l).join(" OR ")
            : (lizenz?.type ?? "nicht angegeben");
      const repo = beschreibung.repository;
      quelle = typeof repo === "string" ? repo : (repo?.url ?? null);
    } catch {
      /* without a package.json the defaults stand */
    }

    pakete.push({
      name,
      version,
      lizenz: angabe,
      quelle,
      herkunft: "npm",
      texte: texteAus(ordner),
    });
  }
  return pakete;
}

// --- writing it out ---

const alle = [...rustPakete(), ...npmPakete()].sort(
  (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
);

// identical licence texts repeat themselves a hundred times over. stored
// once and pointed at everywhere, that presses the file from several megabytes
// down to a tenth
const texte = [];
const nummer = new Map();
for (const paket of alle) {
  paket.textIds = paket.texte.map((text) => {
    if (!nummer.has(text)) {
      nummer.set(text, texte.length);
      texte.push(text);
    }
    return nummer.get(text);
  });
  delete paket.texte;
}

const ohneText = alle.filter((p) => p.textIds.length === 0);
const ergebnis = {
  erzeugt: new Date().toISOString().slice(0, 10),
  pakete: alle,
  texte,
};

mkdirSync(dirname(ZIEL), { recursive: true });
writeFileSync(ZIEL, JSON.stringify(ergebnis));

const groesse = (readFileSync(ZIEL).length / 1024).toFixed(0);
console.log(
  `${alle.length} Pakete, ${texte.length} verschiedene Lizenztexte → ${groesse} kB`,
);
console.log(`  Rust: ${alle.filter((p) => p.herkunft === "Rust").length}`);
console.log(`  npm:  ${alle.filter((p) => p.herkunft === "npm").length}`);
if (ohneText.length > 0) {
  // no great harm: the spdx value stands there all the same. but it is the
  // point at which to look by hand before shipping
  console.log(`  ohne mitgelieferten Lizenztext: ${ohneText.length}`);
  for (const p of ohneText.slice(0, 10))
    console.log(`    ${p.name} ${p.version} (${p.lizenz})`);
  if (ohneText.length > 10)
    console.log(`    … und ${ohneText.length - 10} weitere`);
}
