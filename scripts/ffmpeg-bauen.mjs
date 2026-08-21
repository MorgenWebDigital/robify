#!/usr/bin/env node
// ffmpeg-bauen.mjs — builds the converter for android, without gpl parts.
//
// robify offers mp3, m4a and flac. aac and flac ffmpeg encodes on its own,
// mp3 needs lame. both are lgpl, and that goes together with robify's licence
// — unlike the ready-made android builds, which are gpl throughout because
// they carry x264 and friends along. those are video encoders; nothing here
// touches video.
//
// deliberately shared libraries and not one static binary: the lgpl asks that
// whoever gets the program can exchange its lgpl parts. with `libavcodec.so`
// and `libmp3lame.so` lying beside it that is a matter of copying a file.
//
// the names carry the `lib*.so` pattern for a second reason as well: android
// unpacks and marks executable only what is called that and lies in the
// library folder. hence `libffmpeg.so` rather than `ffmpeg`.
//
// call: `node scripts/ffmpeg-bauen.mjs [architektur …]`
// idempotent: with the built version already in place it does nothing.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WURZEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const FFMPEG_FASSUNG = "9.0.1";
const FFMPEG_PRUEFSUMME =
  "cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635";
const LAME_FASSUNG = "3.100";
const LAME_PRUEFSUMME =
  "ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e";
// ogg vorbis stands in robify's format list as well, and ffmpeg's own vorbis
// encoder is marked experimental. these two are bsd, so they raise no licence
// question at all
const OGG_FASSUNG = "1.3.6";
const OGG_PRUEFSUMME =
  "83e6704730683d004d20e21b8f7f55dcb3383cdf84c0daedf30bde175f774638";
const VORBIS_FASSUNG = "1.3.7";
const VORBIS_PRUEFSUMME =
  "0e982409a9c3fc82ee06e08205b1355e5c6aa4c36bca58146ef399621b0ce5ab";

/** the lowest android version robify runs on, see tauri.conf.json */
const API = 26;

/** the four architectures the universal apk carries */
const ARCHITEKTUREN = {
  "arm64-v8a": {
    dreiklang: "aarch64-linux-android",
    lameWirt: "aarch64-linux-android",
    ffmpegArch: "aarch64",
  },
  "armeabi-v7a": {
    dreiklang: "armv7a-linux-androideabi",
    lameWirt: "arm-linux-androideabi",
    ffmpegArch: "arm",
    ffmpegZusatz: ["--cpu=armv7-a", "--enable-neon"],
  },
  // the hand-written assembly of the two x86 targets needs nasm, and that is
  // not on every machine. they belong to emulators, where speed decides
  // nothing — the c paths do the same work.
  //
  // on 32-bit x86 the whole assembly has to go, not only the part nasm would
  // build: what remains is not position-independent, and the linker refuses
  // it with "relocation R_386_32 cannot be used against local symbol"
  x86: {
    dreiklang: "i686-linux-android",
    lameWirt: "i686-linux-android",
    ffmpegArch: "x86",
    ffmpegZusatz: ["--disable-asm"],
  },
  x86_64: {
    dreiklang: "x86_64-linux-android",
    lameWirt: "x86_64-linux-android",
    ffmpegArch: "x86_64",
    ffmpegZusatz: ["--disable-x86asm"],
  },
};

const ZIEL = join(WURZEL, "src-tauri", "android", "ffmpeg");
const MARKE = join(ZIEL, ".fassung");
const ARBEIT = join(WURZEL, "src-tauri", "android", ".ffmpeg-bau");

/** what has to lie in the app afterwards */
const DATEIEN = [
  "libffmpeg.so",
  "libffprobe.so",
  "libavcodec.so",
  "libavfilter.so",
  "libavformat.so",
  "libavutil.so",
  "libswresample.so",
  "libswscale.so",
  "libmp3lame.so",
  "libogg.so",
  "libvorbis.so",
  "libvorbisenc.so",
];

// what the conversion needs, and nothing more. a full ffmpeg weighs some
// thirty megabytes per architecture; this one stays far below that.
const FFMPEG_TEILE = [
  "--disable-everything",
  "--enable-decoder=aac,aac_latm,mp3,mp3float,flac,alac,vorbis,opus,pcm_s16le,pcm_s16be,pcm_s24le,pcm_f32le,wavpack,wmav1,wmav2,ac3,eac3,mjpeg,png",
  "--enable-encoder=libmp3lame,libvorbis,aac,flac,alac,pcm_s16le,pcm_s24le,mjpeg,png",
  "--enable-demuxer=mov,mp3,flac,ogg,matroska,wav,aac,aiff,asf,wv,ac3,image2,webm_dash_manifest",
  "--enable-muxer=mp3,ipod,mp4,flac,wav,ogg,adts,image2",
  "--enable-parser=aac,aac_latm,mpegaudio,flac,opus,vorbis,ac3,mjpeg,png",
  "--enable-protocol=file,pipe",
  "--enable-filter=aresample,aformat,anull,atrim,volume,acopy,copy,null,scale,format",
  "--enable-bsf=aac_adtstoasc,extract_extradata,vp9_superframe",
  "--enable-libmp3lame",
  "--enable-libvorbis",
];

/**
 * which encoder each offered format needs.
 *
 * the list in the interface and the build here have to agree. they did not
 * once: "OGG Vorbis" stood on offer while the build carried no vorbis encoder,
 * and nothing said so — the conversion would have failed on the phone.
 */
const FORMATE = {
  best: null,
  mp3: "libmp3lame",
  m4a: "aac",
  flac: "flac",
  vorbis: "libvorbis",
};

/** where the interface keeps its format list */
const FORMATQUELLE = join(WURZEL, "src", "lib", "formate.ts");

/**
 * checks that every format on offer can be encoded.
 *
 * read out of the sources rather than written down twice: a format added to
 * the interface must not slip through here unnoticed.
 */
function formatePruefen() {
  const quelle = readFileSync(FORMATQUELLE, "utf8");
  const stelle = quelle.indexOf("function formate()");
  if (stelle < 0) {
    console.error(
      `::error::In ${FORMATQUELLE} steht keine Funktion formate().`,
    );
    process.exit(1);
  }
  const block = quelle.slice(
    stelle,
    quelle.indexOf("}", quelle.indexOf("];", stelle)),
  );
  const angeboten = [...block.matchAll(/id:\s*"([^"]+)"/g)].map((t) => t[1]);

  if (angeboten.length === 0) {
    console.error(
      `::error::Aus ${FORMATQUELLE} ließ sich keine Formatliste lesen.`,
    );
    process.exit(1);
  }

  const kodierer = FFMPEG_TEILE.find((t) => t.startsWith("--enable-encoder="))
    .slice("--enable-encoder=".length)
    .split(",");

  const fehlend = angeboten.filter((id) => {
    const gebraucht = FORMATE[id];
    if (gebraucht === null) return false;
    if (gebraucht === undefined) return true;
    return !kodierer.includes(gebraucht);
  });

  if (fehlend.length > 0) {
    console.error(
      `::error::Für diese Formate fehlt der Kodierer: ${fehlend.join(", ")}.\n` +
        "  Eintragen in FORMATE und in --enable-encoder in scripts/ffmpeg-bauen.mjs.",
    );
    process.exit(1);
  }
  console.log(`ffmpeg: ${angeboten.length} angebotene Formate, alle abgedeckt`);
}

function rufe(befehl, argumente, ordner, umgebung) {
  return execFileSync(befehl, argumente, {
    cwd: ordner ?? WURZEL,
    env: { ...process.env, ...(umgebung ?? {}) },
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 128 * 1024 * 1024,
  });
}

function ndkFinden() {
  const aus = process.env.NDK_HOME || process.env.ANDROID_NDK_HOME;
  if (aus && existsSync(aus)) return aus;
  const sdk =
    process.env.ANDROID_HOME ||
    process.env.ANDROID_SDK_ROOT ||
    join(process.env.HOME ?? "", "Android", "Sdk");
  const ndks = join(sdk, "ndk");
  if (!existsSync(ndks)) return null;
  const vorhanden = readdirSync(ndks).sort();
  return vorhanden.length > 0
    ? join(ndks, vorhanden[vorhanden.length - 1])
    : null;
}

function holen(name, url, pruefsumme) {
  mkdirSync(ARBEIT, { recursive: true });
  const archiv = join(ARBEIT, name);
  if (!existsSync(archiv)) {
    console.log(`ffmpeg: hole ${name}`);
    rufe("curl", ["-sL", "--fail", "-o", archiv, url]);
  }
  const gefunden = createHash("sha256")
    .update(readFileSync(archiv))
    .digest("hex");
  if (gefunden !== pruefsumme) {
    console.error(
      `::error::${name}: Prüfsumme stimmt nicht.\n  erwartet ${pruefsumme}\n  gefunden ${gefunden}`,
    );
    process.exit(1);
  }
  return archiv;
}

function auspacken(archiv, ordner) {
  if (existsSync(ordner)) return ordner;
  rufe("tar", ["xf", archiv], ARBEIT);
  return ordner;
}

function werkzeuge(ndk, abi) {
  const kette = join(ndk, "toolchains", "llvm", "prebuilt", "linux-x86_64");
  const { dreiklang } = ARCHITEKTUREN[abi];
  return {
    bin: join(kette, "bin"),
    cc: join(kette, "bin", `${dreiklang}${API}-clang`),
    umgebung: {
      PATH: `${join(kette, "bin")}:${process.env.PATH}`,
      CC: join(kette, "bin", `${dreiklang}${API}-clang`),
      CXX: join(kette, "bin", `${dreiklang}${API}-clang++`),
      AR: join(kette, "bin", "llvm-ar"),
      RANLIB: join(kette, "bin", "llvm-ranlib"),
      STRIP: join(kette, "bin", "llvm-strip"),
      NM: join(kette, "bin", "llvm-nm"),
      LD: join(kette, "bin", "ld"),
    },
  };
}

function lameBauen(quelle, abi, prefix, wz) {
  const bau = join(ARBEIT, `lame-${abi}`);
  rmSync(bau, { recursive: true, force: true });
  mkdirSync(bau, { recursive: true });

  rufe(
    join(quelle, "configure"),
    [
      `--prefix=${prefix}`,
      `--host=${ARCHITEKTUREN[abi].lameWirt}`,
      "--enable-shared",
      "--disable-static",
      // only the command-line tool falls away. the decoder stays: switched
      // off, its symbols are missing while the version script still lists
      // them, and the linker stops at that. ffmpeg does not use it either way
      "--disable-frontend",
      "--disable-dependency-tracking",
    ],
    bau,
    wz.umgebung,
  );
  rufe("make", ["-j4"], bau, wz.umgebung);
  rufe("make", ["install"], bau, wz.umgebung);
}

/** the two xiph libraries, built the plain autotools way */
function xiphBauen(quelle, abi, prefix, wz, name, zusatz = []) {
  const bau = join(ARBEIT, `${name}-${abi}`);
  rmSync(bau, { recursive: true, force: true });
  mkdirSync(bau, { recursive: true });

  rufe(
    join(quelle, "configure"),
    [
      `--prefix=${prefix}`,
      `--host=${ARCHITEKTUREN[abi].lameWirt}`,
      "--enable-shared",
      "--disable-static",
      "--disable-dependency-tracking",
      ...zusatz,
    ],
    bau,
    {
      ...wz.umgebung,
      // vorbis looks for ogg through pkg-config, which knows nothing of the
      // cross build. the paths are handed over directly
      CPPFLAGS: `-I${join(prefix, "include")}`,
      LDFLAGS: `-L${join(prefix, "lib")}`,
      PKG_CONFIG_PATH: join(prefix, "lib", "pkgconfig"),
      PKG_CONFIG_LIBDIR: join(prefix, "lib", "pkgconfig"),
    },
  );
  rufe("make", ["-j4"], bau, wz.umgebung);
  rufe("make", ["install"], bau, wz.umgebung);
}

function ffmpegBauen(quelle, abi, prefix, wz) {
  const bau = join(ARBEIT, `ffmpeg-${abi}`);
  rmSync(bau, { recursive: true, force: true });
  mkdirSync(bau, { recursive: true });

  const { ffmpegArch, ffmpegZusatz = [] } = ARCHITEKTUREN[abi];
  rufe(
    join(quelle, "configure"),
    [
      `--prefix=${prefix}`,
      "--target-os=android",
      `--arch=${ffmpegArch}`,
      "--enable-cross-compile",
      `--cc=${wz.cc}`,
      `--cxx=${wz.cc}++`,
      `--nm=${join(wz.bin, "llvm-nm")}`,
      `--ar=${join(wz.bin, "llvm-ar")}`,
      `--ranlib=${join(wz.bin, "llvm-ranlib")}`,
      `--strip=${join(wz.bin, "llvm-strip")}`,
      "--enable-shared",
      "--disable-static",
      // gpl and non-free stay switched off — that is the whole point of this
      // build, and it is ffmpeg's default. there is nothing to disable: only
      // `--enable-gpl` exists, and it is not passed
      "--disable-doc",
      "--disable-avdevice",
      "--disable-devices",
      // ffmpeg reads local files here, yt-dlp fetches them itself. without
      // network there is no need for a tls library either
      "--disable-network",
      // ffmpeg and ffprobe are built, ffplay is not: it wants sdl, and
      // nothing here plays anything back
      "--disable-ffplay",
      `--extra-cflags=-I${join(prefix, "include")} -O2 -fPIC`,
      `--extra-ldflags=-L${join(prefix, "lib")}`,
      ...FFMPEG_TEILE,
      ...ffmpegZusatz,
    ],
    bau,
    {
      ...wz.umgebung,
      // ffmpeg asks pkg-config about libvorbis. without these two it looks
      // into the system of the build machine, finds the desktop version or
      // nothing at all, and switches the encoder off with a bare
      // "not found using pkg-config"
      PKG_CONFIG_PATH: join(prefix, "lib", "pkgconfig"),
      PKG_CONFIG_LIBDIR: join(prefix, "lib", "pkgconfig"),
    },
  );
  rufe("make", ["-j4"], bau, wz.umgebung);
  rufe("make", ["install"], bau, wz.umgebung);
}

function einsammeln(abi, prefix, wz) {
  const ordner = join(ZIEL, abi);
  mkdirSync(ordner, { recursive: true });
  const strip = join(wz.bin, "llvm-strip");

  const paare = [
    [join(prefix, "bin", "ffmpeg"), "libffmpeg.so"],
    [join(prefix, "bin", "ffprobe"), "libffprobe.so"],
  ];
  for (const name of DATEIEN) {
    if (name === "libffmpeg.so" || name === "libffprobe.so") continue;
    paare.push([join(prefix, "lib", name), name]);
  }

  for (const [quelle, name] of paare) {
    if (!existsSync(quelle)) {
      console.error(`::error::ffmpeg: ${quelle} fehlt nach dem Bau.`);
      process.exit(1);
    }
    rufe(strip, ["-s", quelle, "-o", join(ordner, name)]);
  }
}

function schonDa() {
  if (!existsSync(MARKE)) return false;
  if (
    readFileSync(MARKE, "utf8").trim() !==
    `${FFMPEG_FASSUNG}+${LAME_FASSUNG}+${VORBIS_FASSUNG}`
  ) {
    return false;
  }
  return Object.keys(ARCHITEKTUREN).every((abi) =>
    DATEIEN.every((name) => existsSync(join(ZIEL, abi, name))),
  );
}

formatePruefen();

const gewuenscht = process.argv.slice(2).filter((a) => a in ARCHITEKTUREN);
const liste = gewuenscht.length > 0 ? gewuenscht : Object.keys(ARCHITEKTUREN);

if (gewuenscht.length === 0 && schonDa()) {
  console.log(`ffmpeg: ${FFMPEG_FASSUNG} liegt für alle Architekturen bereit`);
  process.exit(0);
}

const ndk = ndkFinden();
if (!ndk) {
  console.error(
    "::error::ffmpeg: kein NDK gefunden. Setze NDK_HOME oder ANDROID_HOME.",
  );
  process.exit(1);
}
console.log(`ffmpeg: baue ${FFMPEG_FASSUNG} mit LAME ${LAME_FASSUNG}`);

const ffArchiv = holen(
  `ffmpeg-${FFMPEG_FASSUNG}.tar.xz`,
  `https://ffmpeg.org/releases/ffmpeg-${FFMPEG_FASSUNG}.tar.xz`,
  FFMPEG_PRUEFSUMME,
);
const lameArchiv = holen(
  `lame-${LAME_FASSUNG}.tar.gz`,
  `https://downloads.sourceforge.net/project/lame/lame/${LAME_FASSUNG}/lame-${LAME_FASSUNG}.tar.gz`,
  LAME_PRUEFSUMME,
);

const oggArchiv = holen(
  `libogg-${OGG_FASSUNG}.tar.gz`,
  `https://downloads.xiph.org/releases/ogg/libogg-${OGG_FASSUNG}.tar.gz`,
  OGG_PRUEFSUMME,
);
const vorbisArchiv = holen(
  `libvorbis-${VORBIS_FASSUNG}.tar.gz`,
  `https://downloads.xiph.org/releases/vorbis/libvorbis-${VORBIS_FASSUNG}.tar.gz`,
  VORBIS_PRUEFSUMME,
);

const ffQuelle = auspacken(ffArchiv, join(ARBEIT, `ffmpeg-${FFMPEG_FASSUNG}`));
const lameQuelle = auspacken(lameArchiv, join(ARBEIT, `lame-${LAME_FASSUNG}`));
const oggQuelle = auspacken(oggArchiv, join(ARBEIT, `libogg-${OGG_FASSUNG}`));
const vorbisQuelle = auspacken(
  vorbisArchiv,
  join(ARBEIT, `libvorbis-${VORBIS_FASSUNG}`),
);

// lame 3.100 lists a symbol its own sources no longer define, and the linker
// stops at it. the line is removed rather than the version changed: 3.100 has
// stood since 2017 and is what every distribution ships
const symbole = join(lameQuelle, "include", "libmp3lame.sym");
if (readFileSync(symbole, "utf8").includes("lame_init_old")) {
  writeFileSync(
    symbole,
    readFileSync(symbole, "utf8")
      .split("\n")
      .filter((zeile) => !zeile.includes("lame_init_old"))
      .join("\n"),
  );
  console.log("ffmpeg: LAME-Symbolliste bereinigt");
}

// vorbis hands `-mno-ieee-fp` to the compiler on x86 targets, and clang does
// not know that switch — gcc did, decades ago. the line is taken out rather
// than the flags overridden wholesale: everything else vorbis sets there is
// wanted
const vorbisConfigure = join(vorbisQuelle, "configure");
if (readFileSync(vorbisConfigure, "utf8").includes("-mno-ieee-fp")) {
  writeFileSync(
    vorbisConfigure,
    readFileSync(vorbisConfigure, "utf8").split(" -mno-ieee-fp").join(""),
  );
  console.log("ffmpeg: veralteten Schalter aus libvorbis entfernt");
}

for (const abi of liste) {
  process.stdout.write(`ffmpeg: ${abi} … `);
  const prefix = join(ARBEIT, `prefix-${abi}`);
  rmSync(prefix, { recursive: true, force: true });
  const wz = werkzeuge(ndk, abi);
  lameBauen(lameQuelle, abi, prefix, wz);
  process.stdout.write("LAME, ");
  xiphBauen(oggQuelle, abi, prefix, wz, "ogg");
  process.stdout.write("ogg, ");
  xiphBauen(vorbisQuelle, abi, prefix, wz, "vorbis");
  process.stdout.write("vorbis fertig, ffmpeg … ");
  ffmpegBauen(ffQuelle, abi, prefix, wz);
  einsammeln(abi, prefix, wz);
  console.log("fertig");
}

// the mark is set as soon as everything lies there, not only after a run over
// all four at once: built one after another, the next call would otherwise
// start from the beginning again
const vollstaendig = Object.keys(ARCHITEKTUREN).every((abi) =>
  DATEIEN.every((name) => existsSync(join(ZIEL, abi, name))),
);
if (vollstaendig) {
  writeFileSync(MARKE, `${FFMPEG_FASSUNG}+${LAME_FASSUNG}+${VORBIS_FASSUNG}\n`);
}
console.log(`ffmpeg: ${liste.length} Architektur(en) bereit`);
