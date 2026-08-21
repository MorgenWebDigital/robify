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

// --- android ---
//
// the gradle tree is not walked here, and deliberately not: it exists only
// after `tauri android init`, and reading it would mean starting gradle. what
// is in it is few and stable, so it stands written out — and a check below
// makes sure nothing new slips in unnoticed.

/** what the apk carries beyond rust and npm. */
const ANDROID_PAKETE = [
  {
    name: "Chaquopy",
    version: "17.0.0",
    lizenz: "MIT",
    quelle: "https://github.com/chaquo/chaquopy",
    text: "chaquopy.txt",
    hinweis: "Bringt die Python-Laufzeit in die App.",
  },
  {
    name: "CPython",
    version: "3.11",
    lizenz: "PSF-2.0",
    quelle: "https://www.python.org/",
    hinweis: "Als Laufzeit von Chaquopy mitgeliefert.",
  },
  {
    name: "yt-dlp",
    version: "aus yt-dlp[default]",
    lizenz: "Unlicense",
    quelle: "https://github.com/yt-dlp/yt-dlp",
    text: "yt-dlp.txt",
    hinweis: "Lädt die Titel; wird beim Bauen in die App installiert.",
  },
  {
    name: "yt-dlp-ejs",
    version: "aus yt-dlp[default]",
    lizenz: "Unlicense",
    quelle: "https://github.com/yt-dlp/ejs",
    hinweis: "Führt die JavaScript-Prüfungen von YouTube aus.",
  },
  {
    name: "quickjs-ng",
    version: "0.16.2",
    lizenz: "MIT",
    quelle: "https://github.com/quickjs-ng/quickjs",
    text: "quickjs.txt",
    hinweis: "Die JavaScript-Laufzeit, die yt-dlp dafür braucht.",
  },
  {
    name: "FFmpeg",
    version: "9.0.1",
    lizenz: "LGPL-2.1-or-later",
    quelle: "https://ffmpeg.org/",
    text: "ffmpeg.txt",
    hinweis:
      "Wandelt nach MP3, M4A und FLAC. Eigener Bau ohne die GPL-Teile; " +
      "als eigenständige Bibliotheken beigelegt, siehe scripts/ffmpeg-bauen.mjs.",
  },
  {
    name: "LAME",
    version: "3.100",
    lizenz: "LGPL-2.0-or-later",
    quelle: "https://lame.sourceforge.io/",
    text: "lame.txt",
    hinweis: "Der MP3-Kodierer, den FFmpeg dafür benutzt.",
  },
  {
    name: "libvorbis / libogg",
    version: "1.3.7 / 1.3.6",
    lizenz: "BSD-3-Clause",
    quelle: "https://xiph.org/vorbis/",
    text: "xiph.txt",
    hinweis: "Der Vorbis-Kodierer, den FFmpeg für OGG benutzt.",
  },
  {
    name: "androidx.media",
    version: "1.7.0",
    lizenz: "Apache-2.0",
    quelle: "https://developer.android.com/jetpack/androidx",
    hinweis: "MediaSession und Medientasten.",
  },
];

/**
 * every gradle dependency of the app has to stand in the list above.
 *
 * without this the list would silently fall behind: the build file is written
 * anew at every `tauri android init`, and what tauri adds there changes with
 * its versions. a name nobody has looked at is the case to notice.
 */
const ANDROID_BEKANNT = [
  "com.chaquo.python",
  "androidx.media",
  "rustls:rustls-platform-verifier",
  "androidx.webkit",
  "androidx.appcompat",
  "androidx.activity",
  "com.google.android.material",
  "androidx.lifecycle",
  // test only, they do not ship
  "junit:junit",
  "androidx.test",
];

function androidPruefen() {
  const bau = join(
    WURZEL,
    "src-tauri",
    "gen",
    "android",
    "app",
    "build.gradle.kts",
  );
  if (!existsSync(bau)) return;

  const unbekannt = readFileSync(bau, "utf8")
    .split("\n")
    .map((zeile) => zeile.match(/implementation\("([^"]+)"\)/)?.[1])
    .filter(Boolean)
    .filter(
      (kennung) =>
        !ANDROID_BEKANNT.some((bekannt) => kennung.startsWith(bekannt)),
    );

  if (unbekannt.length > 0) {
    console.error(
      "::error::Neue Android-Abhängigkeit ohne Lizenzangabe: " +
        unbekannt.join(", ") +
        "\n  Eintragen in ANDROID_PAKETE in scripts/lizenzen.mjs.",
    );
    process.exit(1);
  }
}

function androidPakete() {
  androidPruefen();
  const ordner = join(WURZEL, "src-tauri", "android", "lizenzen");
  return ANDROID_PAKETE.map((paket) => {
    const datei = paket.text ? join(ordner, paket.text) : null;
    return {
      name: paket.name,
      version: paket.version,
      lizenz: paket.lizenz,
      quelle: paket.quelle,
      hinweis: paket.hinweis,
      herkunft: "Android",
      texte: datei && existsSync(datei) ? [readFileSync(datei, "utf8")] : [],
    };
  });
}

// --- writing it out ---

const alle = [...rustPakete(), ...npmPakete(), ...androidPakete()].sort(
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
console.log(
  `  Android: ${alle.filter((p) => p.herkunft === "Android").length}`,
);
if (ohneText.length > 0) {
  // no great harm: the spdx value stands there all the same. but it is the
  // point at which to look by hand before shipping
  console.log(`  ohne mitgelieferten Lizenztext: ${ohneText.length}`);
  for (const p of ohneText.slice(0, 10))
    console.log(`    ${p.name} ${p.version} (${p.lizenz})`);
  if (ohneText.length > 10)
    console.log(`    … und ${ohneText.length - 10} weitere`);
}
