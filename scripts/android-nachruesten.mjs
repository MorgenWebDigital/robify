#!/usr/bin/env node
// android-nachruesten.mjs — adds to the generated android project what tauri
// does not offer.
//
// `src-tauri/gen/android` is created anew at every `tauri android init`, and
// files changed by hand are gone afterwards. instead of putting them into the
// repository and marrying them to tauri's templates, this script runs after
// the generation and sets the few lines again.
//
// idempotent: called twice it changes nothing the second time
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";

const MAIN_ACTIVITY =
  "src-tauri/gen/android/app/src/main/java/de/robify/player/MainActivity.kt";
const APP_GRADLE = "src-tauri/gen/android/app/build.gradle.kts";
const PROGUARD = "src-tauri/gen/android/app/robify-regeln.pro";
const PAKET_ORDNER = "src-tauri/gen/android/app/src/main/java/de/robify/player";
/** the version of `youtubedl-android`, it brings yt-dlp and python itself. */
const YTDLP_FASSUNG = "0.18.1";
/** the version of `commons-io`, the 2.5 demanded by `youtubedl-android` is unusable. */
const COMMONS_IO_FASSUNG = "2.16.1";
/** the path to the manifest of the app, recreated at every `tauri android init`. */
const MANIFEST = "src-tauri/gen/android/app/src/main/AndroidManifest.xml";
const DRAWABLE = "src-tauri/gen/android/app/src/main/res/drawable";
const GRADLE_EIGENSCHAFTEN = "src-tauri/gen/android/gradle.properties";
const SYMBOLE = "src-tauri/icons/android";
const RES = "src-tauri/gen/android/app/src/main/res";
/** the version of `androidx.media`, it brings mediasession and the media keys. */
const MEDIA_FASSUNG = "1.7.0";

/**
 * the back button is to lead through the app, not out of it.
 *
 * `TauriActivity` sets `handleBackNavigation = false`, and without a
 * counter-measure the first press ends the app even three pages deep.
 * `WryActivity` does it better: it goes back in the webview while something
 * lies there and ends only afterwards. that is what is wanted here, as the
 * page changes of the app stand in the webview as history.
 */
function zurueckKnopfAnschalten() {
  if (!existsSync(MAIN_ACTIVITY)) {
    console.error(
      `${MAIN_ACTIVITY} fehlt. Erst \`tauri android init\` laufen lassen.`,
    );
    process.exit(1);
  }

  const inhalt = readFileSync(MAIN_ACTIVITY, "utf8");
  if (inhalt.includes("handleBackNavigation")) {
    console.log("Zurück-Knopf: schon nachgerüstet");
    return;
  }

  const alt = "class MainActivity : TauriActivity() {";
  if (!inhalt.includes(alt)) {
    console.error(
      "MainActivity sieht anders aus als erwartet, nichts geändert.",
    );
    process.exit(1);
  }

  const neu = [
    "class MainActivity : TauriActivity() {",
    "  // Der Zurück-Knopf blättert in der App zurück, statt sie zu beenden.",
    "  // TauriActivity schaltet das ab, WryActivity bringt die Behandlung mit.",
    "  // Nachgetragen von scripts/android-nachruesten.mjs, weil dieses",
    "  // Verzeichnis bei jedem 'tauri android init' neu entsteht.",
    "  override val handleBackNavigation: Boolean = true",
    "",
  ].join("\n");

  writeFileSync(MAIN_ACTIVITY, inhalt.replace(alt, neu));
  console.log("Zurück-Knopf: angeschaltet");
}

/**
 * wires in the java part of the certificate check.
 *
 * `rustls-platform-verifier` checks certificates against the android trust
 * store and calls into the java runtime for it. the class for that lies as a
 * finished maven package in the crate `rustls-platform-verifier-android` but
 * has to be named in the gradle build. without it the app does start, but
 * every https request ends in a `ClassNotFoundException` and the ui waits for
 * an answer forever.
 *
 * the path is asked from `cargo metadata` freshly at every run instead of
 * being written down: it points into cargo's package store and looks
 * different on every machine, on the build runner too.
 */
function zertifikatspruefungEinbinden() {
  if (!existsSync(APP_GRADLE)) {
    console.error(
      `${APP_GRADLE} fehlt. Erst \`tauri android init\` laufen lassen.`,
    );
    process.exit(1);
  }

  const inhalt = readFileSync(APP_GRADLE, "utf8");
  if (inhalt.includes("rustls-platform-verifier")) {
    console.log("Zertifikatsprüfung: schon eingebunden");
    return;
  }

  const roh = execFileSync(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--filter-platform",
      "aarch64-linux-android",
      "--manifest-path",
      "src-tauri/Cargo.toml",
    ],
    { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  const paket = JSON.parse(roh).packages.find(
    (p) => p.name === "rustls-platform-verifier-android",
  );
  if (!paket) {
    console.error(
      "rustls-platform-verifier-android steckt nicht im Abhängigkeitsbaum.",
    );
    process.exit(1);
  }
  const maven = join(dirname(paket.manifest_path), "maven");

  const mitRepo = inhalt.replace(
    "dependencies {",
    [
      "// Der Java-Teil der Zertifikatsprüfung, siehe scripts/android-nachruesten.mjs.",
      "repositories {",
      `    maven {`,
      `        url = uri("${maven}")`,
      "        metadataSources { artifact() }",
      "    }",
      "}",
      "",
      "dependencies {",
      // a fixed version instead of `latest.release`: for a moving value
      // gradle would need a `maven-metadata.xml`, and the crate does not ship
      // one. `@aar` is needed because an android archive lies there and not a
      // jar, and without the extension gradle looks for a file that does not
      // exist
      `    implementation("rustls:rustls-platform-verifier:${paket.version}@aar")`,
    ].join("\n"),
  );

  writeFileSync(APP_GRADLE, mitRepo);
  console.log("Zertifikatsprüfung: eingebunden");
}

/**
 * brings yt-dlp onto the phone.
 *
 * the program does not exist for android: it is python, and even the linux
 * binary does not run here because android uses a different c library.
 * `youtubedl-android` delivers yt-dlp together with a python runtime as a
 * library. that costs around a hundred megabytes in the package but is the
 * only way to keep the same feature set.
 *
 * `ffmpeg` comes from the same house and is needed for converting. without it
 * only the format the source delivers would exist.
 */
function ytdlpEinbinden() {
  const inhalt = readFileSync(APP_GRADLE, "utf8");
  if (!inhalt.includes("youtubedl-android")) {
    const mit = inhalt.replace(
      "dependencies {",
      [
        "dependencies {",
        "    // yt-dlp samt Python-Laufzeit, siehe scripts/android-nachruesten.mjs.",
        `    implementation("io.github.junkfood02.youtubedl-android:library:${YTDLP_FASSUNG}")`,
        `    implementation("io.github.junkfood02.youtubedl-android:ffmpeg:${YTDLP_FASSUNG}")`,
      ].join("\n"),
    );
    writeFileSync(APP_GRADLE, mit);
    console.log("yt-dlp: Abhängigkeiten eingetragen");
  } else {
    console.log("yt-dlp: Abhängigkeiten schon da");
  }

  // lift commons-io to a version that still exists.
  //
  // `youtubedl-android` demands commons-io 2.5 from 2016. its `FileUtils`
  // reaches `java.nio.file` through the helper class `Java7Support`, and
  // exactly that does not end up in the finished package, d8 drops it. while
  // nobody uses `FileUtils` that goes unnoticed, at a yt-dlp update it does
  // not, and the app broke off with `NoClassDefFoundError:
  // org.apache.commons.io.Java7Support`. newer versions get by without the
  // detour and offer the same methods
  const mitCommons = readFileSync(APP_GRADLE, "utf8");
  if (!mitCommons.includes("commons-io")) {
    writeFileSync(
      APP_GRADLE,
      mitCommons.replace(
        "dependencies {",
        [
          "configurations.configureEach {",
          "    resolutionStrategy {",
          "        // Siehe scripts/android-nachruesten.mjs.",
          `        force("commons-io:commons-io:${COMMONS_IO_FASSUNG}")`,
          "    }",
          "}",
          "",
          "dependencies {",
        ].join("\n"),
      ),
    );
    console.log("yt-dlp: commons-io angehoben");
  }

  // native libraries have to be unpacked at install time.
  //
  // `youtubedl-android` puts its python runtime into the library folder as
  // `libpython.zip.so` and reads it at runtime as an ordinary file. modern
  // android packages leave the libraries in the archive and load them from
  // there though, and then the file does not exist and the setup fails with a
  // `FileNotFoundException`. the older packaging unpacks them at install
  // time
  const mitPackung = readFileSync(APP_GRADLE, "utf8");
  if (!mitPackung.includes("useLegacyPackaging")) {
    writeFileSync(
      APP_GRADLE,
      mitPackung.replace(
        "    buildTypes {",
        [
          "    packaging {",
          "        jniLibs {",
          "            useLegacyPackaging = true",
          "        }",
          "    }",
          "    buildTypes {",
        ].join("\n"),
      ),
    );
    console.log("yt-dlp: Bibliotheken werden ausgepackt");
  }

  // the bridge lives in the project and not in this script: it is kotlin and
  // belongs where it is read and changed
  copyFileSync("src-tauri/android/Ytdlp.kt", join(PAKET_ORDNER, "Ytdlp.kt"));

  const activity = readFileSync(MAIN_ACTIVITY, "utf8");
  if (activity.includes("YoutubeDL.getInstance().init")) {
    console.log("yt-dlp: Einrichtung schon in der Activity");
    return;
  }

  // the library unpacks its python runtime at the first start and has to be
  // set up once for it. on a thread of its own, because that takes a few
  // seconds and would hold up the build-up of the ui otherwise
  const mitInit = activity
    .replace(
      "import android.os.Bundle",
      [
        "import android.os.Bundle",
        "import android.util.Log",
        "import com.yausername.ffmpeg.FFmpeg",
        "import com.yausername.youtubedl_android.YoutubeDL",
      ].join("\n"),
    )
    .replace(
      "    super.onCreate(savedInstanceState)",
      [
        "    super.onCreate(savedInstanceState)",
        "",
        "    // Packt beim ersten Start die Python-Laufzeit aus; das dauert",
        "    // einige Sekunden und darf die Oberfläche nicht aufhalten.",
        "    Thread {",
        "      try {",
        "        YoutubeDL.getInstance().init(this)",
        "        FFmpeg.getInstance().init(this)",
        '        Log.i("Robify", "yt-dlp und ffmpeg bereit")',
        "      } catch (fehler: Throwable) {",
        '        Log.e("Robify", "yt-dlp nicht eingerichtet", fehler)',
        "      }",
        "    }.start()",
      ].join("\n"),
    );

  writeFileSync(MAIN_ACTIVITY, mitInit);
  console.log("yt-dlp: Einrichtung in die Activity getragen");
}

/**
 * registers the player of the system.
 *
 * it consists of two things android wants to see declared:
 *
 * * a foreground service keeps the app alive while music is running. without
 *   it android may clear the process away in the background and playback
 *   breaks off mid-track. it has to name its type, `mediaPlayback`, otherwise
 *   android 14 refuses the start.
 * * a receiver for the media keys. the buttons from the notification and from
 *   the lock screen come back through it, as do the keys of headphones and car
 *   radios.
 */
function systemplayerEinbinden() {
  const gradle = readFileSync(APP_GRADLE, "utf8");
  if (!gradle.includes("androidx.media:media")) {
    writeFileSync(
      APP_GRADLE,
      gradle.replace(
        "dependencies {",
        [
          "dependencies {",
          "    // MediaSession und Medientasten, siehe scripts/android-nachruesten.mjs.",
          `    implementation("androidx.media:media:${MEDIA_FASSUNG}")`,
        ].join("\n"),
      ),
    );
    console.log("Systemplayer: Abhängigkeit eingetragen");
  }

  copyFileSync(
    "src-tauri/android/Wiedergabe.kt",
    join(PAKET_ORDNER, "Wiedergabe.kt"),
  );

  // turns an address from the file picker into a file with a path
  copyFileSync(
    "src-tauri/android/Dateien.kt",
    join(PAKET_ORDNER, "Dateien.kt"),
  );

  // the icon for the notification. without it the triangle of the system
  // would stand there, the same as with every other app that plays audio
  copyFileSync(
    "src-tauri/android/ic_notification.xml",
    join(DRAWABLE, "ic_notification.xml"),
  );

  const manifest = readFileSync(MANIFEST, "utf8");
  if (manifest.includes("Wiedergabedienst")) {
    console.log("Systemplayer: schon im Manifest");
    return;
  }

  const mitRechten = manifest.replace(
    '<uses-permission android:name="android.permission.INTERNET" />',
    [
      '<uses-permission android:name="android.permission.INTERNET" />',
      // without these three the service does not even start, and from
      // android 13 on the display would stay invisible even while it runs
      '    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />',
      '    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK" />',
      '    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />',
    ].join("\n"),
  );

  const dienst = [
    "        <service",
    '            android:name=".Wiedergabedienst"',
    '            android:exported="false"',
    '            android:foregroundServiceType="mediaPlayback">',
    "            <intent-filter>",
    '                <action android:name="android.intent.action.MEDIA_BUTTON" />',
    "            </intent-filter>",
    "        </service>",
    "",
    "        <receiver",
    '            android:name="androidx.media.session.MediaButtonReceiver"',
    '            android:exported="true">',
    "            <intent-filter>",
    '                <action android:name="android.intent.action.MEDIA_BUTTON" />',
    "            </intent-filter>",
    "        </receiver>",
    "",
    "        <provider",
  ].join("\n");

  writeFileSync(MANIFEST, mitRechten.replace("        <provider", dienst));
  console.log("Systemplayer: Dienst und Rechte eingetragen");
}

/**
 * asks for the notification permission.
 *
 * since android 13 it has to be requested. without it the foreground service
 * does run but its display stays invisible, and that display is the player in
 * question. the request comes at startup and only once, and where the user
 * says no, robify keeps playing, only without a display.
 */
function benachrichtigungenErbitten() {
  const activity = readFileSync(MAIN_ACTIVITY, "utf8");
  if (activity.includes("POST_NOTIFICATIONS")) {
    console.log("Benachrichtigungen: Frage schon in der Activity");
    return;
  }

  writeFileSync(
    MAIN_ACTIVITY,
    activity.replace(
      "    super.onCreate(savedInstanceState)",
      [
        "    super.onCreate(savedInstanceState)",
        "",
        "    // Seit Android 13 ist die Anzeige des Players ohne diese Erlaubnis",
        "    // unsichtbar. Nachgetragen von scripts/android-nachruesten.mjs.",
        "    if (android.os.Build.VERSION.SDK_INT >= 33 &&",
        '        checkSelfPermission("android.permission.POST_NOTIFICATIONS") !=',
        "          android.content.pm.PackageManager.PERMISSION_GRANTED) {",
        '      requestPermissions(arrayOf("android.permission.POST_NOTIFICATIONS"), 1)',
        "    }",
      ].join("\n"),
    ),
  );
  console.log("Benachrichtigungen: Frage in die Activity getragen");
}

/**
 * requests access to the device storage.
 *
 * robify puts its tracks into `Robify` and everything else into `.robify`,
 * both directly in the device storage. they are visible in the file manager
 * there and survive the removal of the app, unlike everything under
 * `Android/data`.
 *
 * since android 11 no app may do that just like that. the permission needed
 * is not granted in a dialog but on a page of the system settings the app may
 * open. it is asked once per installation: whoever declines is not to face
 * the same page at every start. robify then carries on working in its own
 * folder.
 *
 * where the user grants it, the app restarts itself. the folders are settled
 * once the rust side has come up, and rehanging it afterwards would be more
 * work and more error-prone than a restart one sees only once in the life of
 * the installation anyway.
 *
 * on android 10 and older this permission does not exist. the app's own
 * folder stays there, and the rust side notices by itself that it may not
 * write in the device storage and gives way.
 */
function dateizugriffErbitten() {
  const activity = readFileSync(MAIN_ACTIVITY, "utf8");
  if (activity.includes("MANAGE_APP_ALL_FILES_ACCESS")) {
    console.log("Dateizugriff: Frage schon in der Activity");
  } else {
    const felder = [
      "  // Fester Speicherort, siehe scripts/android-nachruesten.mjs.",
      "  private var durfteBeimStart = false",
      "",
      "  private fun darfAlleDateien(): Boolean =",
      "    android.os.Build.VERSION.SDK_INT >= 30 &&",
      "      android.os.Environment.isExternalStorageManager()",
      "",
      "  private fun dateizugriffErbitten() {",
      "    durfteBeimStart = darfAlleDateien()",
      "    if (durfteBeimStart || android.os.Build.VERSION.SDK_INT < 30) return",
      "",
      '    val merker = getSharedPreferences("robify", MODE_PRIVATE)',
      '    if (merker.getBoolean("dateizugriff-gefragt", false)) return',
      '    merker.edit().putBoolean("dateizugriff-gefragt", true).apply()',
      "",
      "    val seite =",
      "      android.content.Intent(",
      "        android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,",
      '        android.net.Uri.parse("package:$packageName"),',
      "      )",
      "    // Nicht jedes Gerät kennt die Seite für eine einzelne App; dann die",
      "    // allgemeine Liste, in der Robify zu finden ist.",
      "    if (runCatching { startActivity(seite) }.isFailure) {",
      "      runCatching {",
      "        startActivity(",
      "          android.content.Intent(",
      "            android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION",
      "          )",
      "        )",
      "      }",
      "    }",
      "  }",
      "",
      "  override fun onResume() {",
      "    super.onResume()",
      "    if (!durfteBeimStart && darfAlleDateien()) {",
      "      durfteBeimStart = true",
      "      val neu = packageManager.getLaunchIntentForPackage(packageName)",
      "      if (neu != null) {",
      "        neu.addFlags(",
      "          android.content.Intent.FLAG_ACTIVITY_CLEAR_TASK or",
      "            android.content.Intent.FLAG_ACTIVITY_NEW_TASK",
      "        )",
      "        startActivity(neu)",
      "      }",
      "      Runtime.getRuntime().exit(0)",
      "    }",
      "  }",
      "",
      "  override fun onCreate(savedInstanceState: Bundle?) {",
    ].join("\n");

    writeFileSync(
      MAIN_ACTIVITY,
      activity
        .replace(
          "  override fun onCreate(savedInstanceState: Bundle?) {",
          felder,
        )
        .replace(
          "    super.onCreate(savedInstanceState)",
          [
            "    super.onCreate(savedInstanceState)",
            "",
            "    dateizugriffErbitten()",
          ].join("\n"),
        ),
    );
    console.log("Dateizugriff: Frage in die Activity getragen");
  }

  const manifest = readFileSync(MANIFEST, "utf8");
  if (manifest.includes("MANAGE_EXTERNAL_STORAGE")) {
    console.log("Dateizugriff: Recht schon im Manifest");
    return;
  }

  writeFileSync(
    MANIFEST,
    manifest.replace(
      '<uses-permission android:name="android.permission.INTERNET" />',
      [
        '<uses-permission android:name="android.permission.INTERNET" />',
        '    <uses-permission android:name="android.permission.MANAGE_EXTERNAL_STORAGE" />',
      ].join("\n"),
    ),
  );
  console.log("Dateizugriff: Recht ins Manifest getragen");
}

/**
 * keeps the memory appetite of the build in check.
 *
 * gradle starts a background daemon that stays between two builds, and kotlin
 * a second one next to it. with the default values the two together took over
 * a gigabyte, and that on a machine with something else running alongside:
 * while building, the machine ran out of memory, swap file included.
 *
 * one and a half gigabytes are comfortably enough for a project of this size,
 * and two workers at a time instead of as many as the machine has cores cost a
 * few seconds and save a multiple of that in memory.
 */
/**
 * lets the window shrink when the keyboard slides out.
 *
 * without a value android decides for itself, and it decided on pushing: the
 * whole page slid up and the title row disappeared under the status bar.
 * `adjustResize` shrinks the window instead.
 *
 * the value in `index.html` (`interactive-widget=resizes-content`) says the
 * same thing to the webview once more, and the two together cover old and new
 * android versions alike.
 */
// keeps the bridges to rust from being optimised away.
//
// the release build runs r8 over the kotlin part and throws out what nothing
// calls. nothing in kotlin calls these three classes: rust reaches them
// through jni, by their name, at run time, and r8 cannot see that. so it
// removed them, the app started, and it died at the first tap on the settings
// with `NoSuchMethodError: no static method Wiedergabe.melden`.
//
// the debug build is not minified and showed nothing of it. the fault
// therefore appeared only in the installers, which is the worst place for it
// to appear.
//
// its own file rather than the `proguard-rules.pro` of the template: the
// gradle part reads every `.pro` under the app folder, and what is ours stays
// apart from what tauri generates.
function brueckenSchuetzen() {
  writeFileSync(
    PROGUARD,
    [
      "# rust reaches these classes through jni, by their name. r8 sees no",
      "# call to them and would remove them.",
      "-keep class de.robify.player.Wiedergabe { *; }",
      "-keep class de.robify.player.Ytdlp { *; }",
      "-keep class de.robify.player.Dateien { *; }",
      "",
      "# the same for the certificate check. it comes with the crate",
      "# `rustls-platform-verifier` and is likewise called from rust alone:",
      "# without it every https connection out of the rust part fails, and",
      "# with it metadata, covers, lyrics and the update check.",
      "-keep class org.rustls.platformverifier.** { *; }",
      "-keepclassmembers class org.rustls.platformverifier.** { *; }",
      "",
      "# what is declared in kotlin and implemented in rust",
      "-keepclasseswithmembernames class * {",
      "    native <methods>;",
      "}",
      "",
    ].join("\n"),
  );
  console.log("Brücken zu Rust: vor der Optimierung geschützt");
}

function tastaturVerhaltenSetzen() {
  const manifest = readFileSync(MANIFEST, "utf8");
  if (manifest.includes("windowSoftInputMode")) {
    console.log("Tastatur: Verhalten schon gesetzt");
    return;
  }

  writeFileSync(
    MANIFEST,
    manifest.replace(
      '            android:launchMode="singleTask"',
      [
        '            android:launchMode="singleTask"',
        '            android:windowSoftInputMode="adjustResize"',
      ].join("\n"),
    ),
  );
  console.log("Tastatur: Fenster schrumpft statt zu schieben");
}

function speicherZuegeln() {
  const alt = readFileSync(GRADLE_EIGENSCHAFTEN, "utf8");
  if (alt.includes("workers.max")) {
    console.log("Speicher: Grenzen schon gesetzt");
    return;
  }

  const neu = [
    alt
      .replace(
        "org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8",
        "org.gradle.jvmargs=-Xmx1536m -Dfile.encoding=UTF-8",
      )
      .trimEnd(),
    "",
    "# Grenzen von scripts/android-nachruesten.mjs, damit der Bau den Rechner",
    "# nicht leerräumt.",
    "org.gradle.workers.max=2",
    "kotlin.daemon.jvmargs=-Xmx768m",
    "",
  ].join("\n");

  writeFileSync(GRADLE_EIGENSCHAFTEN, neu);
  console.log("Speicher: Grenzen für Gradle und Kotlin gesetzt");
}

/**
 * puts our mark in as the launcher icon.
 *
 * `tauri icon` generates the whole set into `src-tauri/icons/android`, but
 * `tauri android init` creates `gen/android` with tauri's default icons, the
 * blue and yellow circle. whoever generated the icons before the last
 * regeneration does not find them in the build afterwards, and that is
 * exactly how a foreign mark stood on the home screen of the phone.
 *
 * everything belonging to the adaptive icon is copied: the images in every
 * resolution, the description in `mipmap-anydpi-v26` and the colour of the
 * background.
 */
function startsymbolEinlegen() {
  if (!existsSync(SYMBOLE)) {
    console.log("Startsymbol: keine Vorlage, übersprungen");
    return;
  }

  let gelegt = 0;
  for (const ordner of readdirSync(SYMBOLE)) {
    const quelle = join(SYMBOLE, ordner);
    const ziel = join(RES, ordner);
    mkdirSync(ziel, { recursive: true });
    for (const datei of readdirSync(quelle)) {
      copyFileSync(join(quelle, datei), join(ziel, datei));
      gelegt += 1;
    }
  }
  // the foreground as a vector over it: the generated image fills the surface
  // up to the edge, and inside the circle the note heads would stand clipped.
  // the image files stay, they carry the version for android 7 and older,
  // which knows no adaptive icon yet
  copyFileSync(
    "src-tauri/android/ic_launcher_foreground.xml",
    join(DRAWABLE, "ic_launcher_foreground.xml"),
  );
  for (const dichte of ["hdpi", "mdpi", "xhdpi", "xxhdpi", "xxxhdpi"]) {
    const alt = join(RES, `mipmap-${dichte}`, "ic_launcher_foreground.png");
    if (existsSync(alt)) rmSync(alt);
  }
  // and the version from tauri, which outvotes everything: a `drawable-v24`
  // applies before the plain `drawable` on every device from android 7 on. it
  // stayed behind, and the blue and yellow circle kept standing on the home
  // screen.
  //
  // overwritten instead of deleted: gradle did not notice the deletion and
  // kept packing the file from its cache. the same drawing in both folders is
  // more reliable anyway than relying on which folder wins
  const v24 = join(RES, "drawable-v24");
  if (existsSync(v24)) {
    copyFileSync(
      "src-tauri/android/ic_launcher_foreground.xml",
      join(v24, "ic_launcher_foreground.xml"),
    );
  }
  writeFileSync(
    join(RES, "mipmap-anydpi-v26", "ic_launcher.xml"),
    [
      '<?xml version="1.0" encoding="utf-8"?>',
      '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">',
      '  <foreground android:drawable="@drawable/ic_launcher_foreground"/>',
      '  <background android:drawable="@color/ic_launcher_background"/>',
      "</adaptive-icon>",
      "",
    ].join("\n"),
  );

  // the background stays transparent: the white disc turned the mark into a
  // badge on a plate. `tauri icon` stores the colour as `#fff`, it comes along
  // with the icons and is overwritten here
  writeFileSync(
    join(RES, "values", "ic_launcher_background.xml"),
    [
      '<?xml version="1.0" encoding="utf-8"?>',
      "<resources>",
      '  <color name="ic_launcher_background">#00000000</color>',
      "</resources>",
      "",
    ].join("\n"),
  );

  console.log(
    `Startsymbol: ${gelegt} Dateien eingelegt, Vorderseite als Vektor`,
  );
}

zurueckKnopfAnschalten();
zertifikatspruefungEinbinden();
ytdlpEinbinden();
systemplayerEinbinden();
benachrichtigungenErbitten();
dateizugriffErbitten();
speicherZuegeln();
brueckenSchuetzen();
tastaturVerhaltenSetzen();
startsymbolEinlegen();
