#!/usr/bin/env node
// apk-lizenzen.mjs — reads out of the finished apk what it actually carries,
// and refuses what does not fit robify's licence.
//
// the point is the direction: not what somebody wrote into a list, but what
// lies in the package. `youtubedl-android` was gpl and stood in no list of
// ours; mutagen came in later through `yt-dlp[default]` and would have been
// just as invisible. both would have made the apk impossible to pass on.
//
// chaquopy puts the metadata of every python package into the apk, and the
// licence stands in it. the native libraries are built by robify itself, and
// what they are stands in the licence list.
//
// call: `node scripts/apk-lizenzen.mjs [pfad/zum.apk]`
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WURZEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** where `tauri android build --apk` leaves its result */
const STANDARD = join(
  WURZEL,
  "src-tauri/gen/android/app/build/outputs/apk/universal/release",
  "app-universal-release-unsigned.apk",
);

/**
 * licences that would force the source of our own to be disclosed.
 *
 * the lgpl is not among them: it asks that its own parts stay exchangeable,
 * and they do — they lie beside the program as shared libraries. the mpl acts
 * on its own files alone and is likewise fine.
 */
const UNVERTRAEGLICH = [
  /\bA?GPL(-|\s|v)?[23]/i,
  /\bGNU General Public/i,
  /\bAffero/i,
  /\bSSPL/i,
];

/** the lgpl reads like the gpl and must not be caught by the patterns above */
function istLgpl(text) {
  return /\bLGPL|Lesser General Public/i.test(text);
}

function rufe(befehl, argumente, ordner) {
  return execFileSync(befehl, argumente, {
    cwd: ordner,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
}

const apk = process.argv[2] ?? STANDARD;
if (!existsSync(apk)) {
  console.error(`::error::Kein APK unter ${apk}. Erst bauen.`);
  process.exit(1);
}

const arbeit = mkdtempSync(join(tmpdir(), "robify-apk-"));
try {
  rufe(
    "unzip",
    ["-q", "-o", apk, "assets/chaquopy/requirements-common.imy"],
    arbeit,
  );
  const paketarchiv = join(arbeit, "assets/chaquopy/requirements-common.imy");
  if (!existsSync(paketarchiv)) {
    console.error("::error::Im APK steckt kein Python-Paketarchiv.");
    process.exit(1);
  }

  const ausgepackt = join(arbeit, "pakete");
  rufe(
    "unzip",
    ["-q", "-o", paketarchiv, "*.dist-info/METADATA", "-d", ausgepackt],
    arbeit,
  );

  const ordner = readdirSync(ausgepackt).filter((n) =>
    n.endsWith(".dist-info"),
  );
  if (ordner.length === 0) {
    console.error("::error::Keine Paketangaben im Archiv gefunden.");
    process.exit(1);
  }

  const beanstandet = [];
  console.log(`APK-Lizenzen: ${ordner.length} Python-Pakete`);
  for (const name of ordner.sort()) {
    const daten = readFileSync(join(ausgepackt, name, "METADATA"), "utf8");
    const kopf = daten.slice(0, daten.indexOf("\n\n"));
    const zeilen = kopf
      .split("\n")
      .filter((z) => /^(License(-Expression)?|Classifier: License)/i.test(z));
    const angabe = zeilen
      .join(" | ")
      .replace(/^License(-Expression)?:\s*/i, "");

    const schlimm =
      !istLgpl(angabe) && UNVERTRAEGLICH.some((muster) => muster.test(angabe));
    console.log(
      `  ${schlimm ? "!!" : "  "} ${name.replace(".dist-info", "").padEnd(30)} ${angabe.slice(0, 60) || "—"}`,
    );
    if (schlimm)
      beanstandet.push(`${name.replace(".dist-info", "")}: ${angabe}`);
  }

  if (beanstandet.length > 0) {
    console.error(
      "::error::Im APK stehen Pakete, deren Lizenz die Weitergabe unter " +
        "PolyForm Noncommercial verbietet:\n  " +
        beanstandet.join("\n  ") +
        "\n  Entfernen oder ersetzen, siehe PIP_PAKETE in scripts/android-nachruesten.mjs.",
    );
    process.exit(1);
  }
  console.log("APK-Lizenzen: nichts Unverträgliches");
} finally {
  rmSync(arbeit, { recursive: true, force: true });
}
