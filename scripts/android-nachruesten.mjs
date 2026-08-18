#!/usr/bin/env node
/**
 * Trägt in das erzeugte Android-Projekt nach, was Tauri nicht anbietet.
 *
 * `src-tauri/gen/android` entsteht bei jedem `tauri android init` neu; von
 * Hand geänderte Dateien sind danach fort. Statt sie ins Repository zu legen
 * und mit Tauris Vorlagen zu verheiraten, läuft dieses Skript nach dem
 * Erzeugen und setzt die wenigen Zeilen erneut.
 *
 * Idempotent: Zweimal aufgerufen ändert es beim zweiten Mal nichts.
 */
import { readFileSync, writeFileSync, existsSync, copyFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";

const MAIN_ACTIVITY =
  "src-tauri/gen/android/app/src/main/java/de/robify/player/MainActivity.kt";
const APP_GRADLE = "src-tauri/gen/android/app/build.gradle.kts";
const PAKET_ORDNER = "src-tauri/gen/android/app/src/main/java/de/robify/player";
/** Fassung von `youtubedl-android`; bringt yt-dlp und Python selbst mit. */
const YTDLP_FASSUNG = "0.18.1";

/**
 * Der Zurück-Knopf soll durch die App führen, nicht aus ihr heraus.
 *
 * `TauriActivity` setzt `handleBackNavigation = false`; ohne Gegensteuer
 * beendet der erste Druck die App, auch wenn man drei Seiten tief steht.
 * `WryActivity` kann es besser: Es blättert in der WebView zurück, solange
 * dort etwas liegt, und beendet erst danach. Genau das wollen wir, denn die
 * Seitenwechsel der App stehen als Verlauf in der WebView.
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
 * Bindet den Java-Teil der Zertifikatsprüfung ein.
 *
 * `rustls-platform-verifier` prüft Zertifikate über den Vertrauensspeicher von
 * Android und ruft dafür in die Java-Laufzeit. Die Klasse dazu liegt als
 * fertiges Maven-Paket in der Kiste `rustls-platform-verifier-android`, muss
 * aber im Gradle-Bau benannt werden. Fehlt sie, startet die App zwar, doch
 * jede HTTPS-Anfrage endet mit `ClassNotFoundException` und die Oberfläche
 * wartet ewig auf eine Antwort.
 *
 * Der Pfad wird bei jedem Lauf frisch von `cargo metadata` erfragt statt fest
 * eingetragen: Er zeigt in den Paketspeicher von Cargo und sieht auf jedem
 * Rechner anders aus, auch auf dem Bauläufer.
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
      // Feste Fassung statt `latest.release`: Für eine bewegliche Angabe
      // bräuchte Gradle eine `maven-metadata.xml`, und die legt die Kiste
      // nicht bei. `@aar` ist nötig, weil dort ein Android-Archiv liegt und
      // kein Jar; ohne die Endung sucht Gradle eine Datei, die es nicht gibt.
      `    implementation("rustls:rustls-platform-verifier:${paket.version}@aar")`,
    ].join("\n"),
  );

  writeFileSync(APP_GRADLE, mitRepo);
  console.log("Zertifikatsprüfung: eingebunden");
}

/**
 * Bringt yt-dlp aufs Telefon.
 *
 * Das Programm gibt es für Android nicht: Es ist Python, und selbst die
 * Linux-Binärdatei läuft hier nicht, weil Android eine andere C-Bibliothek
 * verwendet. `youtubedl-android` liefert yt-dlp samt Python-Laufzeit als
 * Bibliothek; das kostet rund hundert Megabyte im Paket, ist aber der einzige
 * Weg, denselben Funktionsumfang zu behalten.
 *
 * `ffmpeg` kommt aus demselben Haus und wird zum Umwandeln gebraucht. Ohne es
 * gäbe es nur das Format, das die Quelle liefert.
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

  // Native Bibliotheken müssen beim Installieren ausgepackt werden.
  //
  // `youtubedl-android` legt seine Python-Laufzeit als `libpython.zip.so` im
  // Bibliotheksordner ab und liest sie zur Laufzeit als gewöhnliche Datei.
  // Moderne Android-Pakete lassen die Bibliotheken jedoch im Archiv liegen
  // und laden sie von dort; dann gibt es die Datei nicht, und die Einrichtung
  // scheitert mit `FileNotFoundException`. Die ältere Verpackung packt sie
  // beim Installieren aus.
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

  // Die Brücke liegt im Projekt, nicht in diesem Skript: Sie ist Kotlin und
  // gehört dorthin, wo man sie liest und ändert.
  copyFileSync("src-tauri/android/Ytdlp.kt", join(PAKET_ORDNER, "Ytdlp.kt"));

  const activity = readFileSync(MAIN_ACTIVITY, "utf8");
  if (activity.includes("YoutubeDL.getInstance().init")) {
    console.log("yt-dlp: Einrichtung schon in der Activity");
    return;
  }

  // Die Bibliothek packt ihre Python-Laufzeit beim ersten Start aus und muss
  // dafür einmal eingerichtet werden. In einem eigenen Faden, weil das ein
  // paar Sekunden dauert und den Aufbau der Oberfläche sonst aufhielte.
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

zurueckKnopfAnschalten();
zertifikatspruefungEinbinden();
ytdlpEinbinden();
