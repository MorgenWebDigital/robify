#!/usr/bin/env node
// quickjs-bauen.mjs — builds the javascript runtime yt-dlp needs on android.
//
// since 2025.11.12 yt-dlp requires an external javascript runtime for full
// youtube support: without one the n-parameter challenge stays unsolved and
// formats fall away. deno, node and bun do not exist on android, quickjs does
// — it is a single c file tree and cross-compiles with the ndk in seconds.
//
// the result is deliberately named `libqjs.so`. android extracts and marks
// executable only what lies in the library folder under that name pattern;
// an ordinary file next to it may not be started. the same trick carried
// ffmpeg before.
//
// call: `node scripts/quickjs-bauen.mjs`
// idempotent: with the built version already in place it does nothing.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WURZEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** the version of quickjs-ng, the maintained fork of bellard's quickjs. */
const FASSUNG = "0.16.2";
/** checksum of the source archive, checked before anything is unpacked. */
const PRUEFSUMME =
  "97c80625b26775a4c7ca618c004d4ea24cf99cbf867e4eba78bd927a8b23d106";
/** the lowest android version robify runs on, see tauri.conf.json. */
const PLATTFORM = "android-26";

/** the four architectures the universal apk carries. */
const ARCHITEKTUREN = ["arm64-v8a", "armeabi-v7a", "x86", "x86_64"];

const ZIEL = join(WURZEL, "src-tauri", "android", "quickjs");
const MARKE = join(ZIEL, ".fassung");
const ARBEIT = join(WURZEL, "src-tauri", "android", ".quickjs-bau");

function rufe(befehl, argumente, ordner) {
  return execFileSync(befehl, argumente, {
    cwd: ordner ?? WURZEL,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 64 * 1024 * 1024,
  });
}

/** the ndk, from the environment or from the usual place. */
function ndkFinden() {
  const aus = process.env.NDK_HOME || process.env.ANDROID_NDK_HOME;
  if (aus && existsSync(aus)) return aus;

  const sdk =
    process.env.ANDROID_HOME ||
    process.env.ANDROID_SDK_ROOT ||
    join(process.env.HOME ?? "", "Android", "Sdk");
  const ndks = join(sdk, "ndk");
  if (!existsSync(ndks)) return null;
  // the newest of what is installed
  const vorhanden = readdirSync(ndks).sort();
  return vorhanden.length > 0
    ? join(ndks, vorhanden[vorhanden.length - 1])
    : null;
}

/** whether everything is already there in the wanted version. */
function schonDa() {
  if (!existsSync(MARKE)) return false;
  if (readFileSync(MARKE, "utf8").trim() !== FASSUNG) return false;
  return ARCHITEKTUREN.every((abi) => existsSync(join(ZIEL, abi, "libqjs.so")));
}

function quelleHolen() {
  mkdirSync(ARBEIT, { recursive: true });
  const archiv = join(ARBEIT, `quickjs-ng-${FASSUNG}.tar.gz`);

  if (!existsSync(archiv)) {
    const url = `https://github.com/quickjs-ng/quickjs/archive/refs/tags/v${FASSUNG}.tar.gz`;
    console.log(`QuickJS: hole ${FASSUNG}`);
    rufe("curl", ["-sL", "--fail", "-o", archiv, url]);
  }

  const gefunden = createHash("sha256")
    .update(readFileSync(archiv))
    .digest("hex");
  if (gefunden !== PRUEFSUMME) {
    console.error(
      `::error::QuickJS: Prüfsumme stimmt nicht.\n  erwartet ${PRUEFSUMME}\n  gefunden ${gefunden}`,
    );
    process.exit(1);
  }

  const quelle = join(ARBEIT, `quickjs-${FASSUNG}`);
  if (!existsSync(quelle)) {
    rufe("tar", ["xzf", archiv], ARBEIT);
  }
  return quelle;
}

function bauen(quelle, ndk, abi) {
  const bau = join(ARBEIT, `bau-${abi}`);
  rufe("cmake", [
    "-S",
    quelle,
    "-B",
    bau,
    `-DCMAKE_TOOLCHAIN_FILE=${join(ndk, "build", "cmake", "android.toolchain.cmake")}`,
    `-DANDROID_ABI=${abi}`,
    `-DANDROID_PLATFORM=${PLATTFORM}`,
    "-DCMAKE_BUILD_TYPE=Release",
    "-DBUILD_EXAMPLES=OFF",
  ]);
  rufe("cmake", ["--build", bau, "--target", "qjs_exe", "-j", "4"]);

  const strip = join(
    ndk,
    "toolchains",
    "llvm",
    "prebuilt",
    "linux-x86_64",
    "bin",
    "llvm-strip",
  );
  const ordner = join(ZIEL, abi);
  mkdirSync(ordner, { recursive: true });
  // stripped it shrinks from about seven megabytes to one and a bit, and that
  // times four architectures in one apk
  rufe(strip, ["-s", join(bau, "qjs"), "-o", join(ordner, "libqjs.so")]);
}

if (schonDa()) {
  console.log(`QuickJS: ${FASSUNG} liegt für alle Architekturen bereit`);
  process.exit(0);
}

const ndk = ndkFinden();
if (!ndk) {
  console.error(
    "::error::QuickJS: kein NDK gefunden. Setze NDK_HOME oder ANDROID_HOME.",
  );
  process.exit(1);
}
console.log(`QuickJS: baue ${FASSUNG} mit ${ndk}`);

const quelle = quelleHolen();
for (const abi of ARCHITEKTUREN) {
  process.stdout.write(`QuickJS: ${abi} … `);
  bauen(quelle, ndk, abi);
  console.log("fertig");
}
writeFileSync(MARKE, `${FASSUNG}\n`);

// the build folder holds a few hundred megabytes of object files and is of no
// further use; the archive stays so a rebuild does not fetch it again
for (const abi of ARCHITEKTUREN) {
  rmSync(join(ARBEIT, `bau-${abi}`), { recursive: true, force: true });
}
rmSync(join(ARBEIT, `quickjs-${FASSUNG}`), { recursive: true, force: true });

console.log(
  `QuickJS: ${FASSUNG} für ${ARCHITEKTUREN.length} Architekturen bereit`,
);
