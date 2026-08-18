#!/usr/bin/env node
/**
 * Sammelt die Lizenzangaben aller Abhängigkeiten nach `public/lizenzen.json`.
 *
 * MIT und Apache-2.0, zusammen fast der gesamte Baum, verlangen beide, dass
 * Urheberrechtsvermerk und Lizenztext mitgeliefert werden, sobald man eine
 * Binärdatei weitergibt. Diese Datei erfüllt genau das; die Einstellungsseite
 * zeigt sie an.
 *
 * Erfasst wird nur, was tatsächlich im Programm landet: bei Rust die normalen
 * Abhängigkeiten (`cargo tree -e normal`, also ohne dev- und build-Kisten),
 * bei npm der Produktivbaum ohne devDependencies. Bauwerkzeuge wie Vite oder
 * lightningcss geben nichts an den Nutzer weiter und gehören darum nicht hinein.
 *
 * Aufruf: `npm run lizenzen`
 */
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

/** Dateinamen, unter denen Projekte ihren Lizenztext ablegen. */
const LIZENZDATEI = /^(LICEN[CS]E|COPYING|NOTICE|UNLICEN[CS]E)([-._].*)?$/i;

function rufe(befehl, argumente, cwd) {
  return execFileSync(befehl, argumente, {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
}

/**
 * Liest die Lizenztexte aus einem Paketordner.
 *
 * Manche Projekte legen mehrere ab (LICENSE-MIT und LICENSE-APACHE bei der
 * Doppellizenz); dann gehören beide dazu, denn welche von beiden gilt, sucht
 * sich der Nutzer aus.
 */
function texteAus(ordner) {
  if (!ordner || !existsSync(ordner)) return [];
  const treffer = [];
  for (const name of readdirSync(ordner)) {
    if (!LIZENZDATEI.test(name)) continue;
    try {
      const inhalt = readFileSync(join(ordner, name), "utf8").trim();
      // Verweise statt Text (»siehe LICENSE«) helfen niemandem.
      if (inhalt.length > 120) treffer.push(inhalt);
    } catch {
      /* Ordner statt Datei, oder nicht lesbar: überspringen. */
    }
  }
  return treffer;
}

// ------------------------------------------------------------------ Rust

function rustPakete() {
  const tauri = join(WURZEL, "src-tauri");

  // Was wirklich mitgelinkt wird: normale Kanten, ohne dev und build.
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
      // "name v1.2.3" oder "name v1.2.3 (/pfad)" → "name v1.2.3"
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
    if (paket.name === "robify") continue; // das eigene Werk

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

// ------------------------------------------------------------------- npm

function npmPakete() {
  let baum;
  try {
    baum = JSON.parse(
      rufe("npm", ["ls", "--omit=dev", "--all", "--json"], WURZEL),
    );
  } catch (fehler) {
    // `npm ls` endet mit Code 1, sobald irgendetwas fehlt; die Ausgabe ist
    // trotzdem brauchbar.
    baum = JSON.parse(fehler.stdout || "{}");
  }

  const gefunden = new Map();
  const laufe = (knoten) => {
    for (const [name, wert] of Object.entries(knoten.dependencies ?? {})) {
      // Nicht aufgelöste Knoten (unerfüllte Gegenstücke, ausgelassene
      // Wahlabhängigkeiten) haben keine Version und liegen auch nicht auf der
      // Platte, die werden nicht mitgeliefert und gehören nicht in die Liste.
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
      /* ohne package.json bleibt es bei den Vorgaben */
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

// ---------------------------------------------------------------- Ablage

const alle = [...rustPakete(), ...npmPakete()].sort(
  (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
);

// Gleiche Lizenztexte wiederholen sich hundertfach. Einmal ablegen, überall
// darauf verweisen: Das drückt die Datei von mehreren Megabyte auf ein Zehntel.
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
  // Kein Beinbruch: Die SPDX-Angabe steht trotzdem da. Aber es ist der Punkt,
  // an dem man von Hand nachsehen sollte, bevor man ausliefert.
  console.log(`  ohne mitgelieferten Lizenztext: ${ohneText.length}`);
  for (const p of ohneText.slice(0, 10))
    console.log(`    ${p.name} ${p.version} (${p.lizenz})`);
  if (ohneText.length > 10)
    console.log(`    … und ${ohneText.length - 10} weitere`);
}
