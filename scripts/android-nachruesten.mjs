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
/** Fassung von `commons-io`; die von `youtubedl-android` verlangte 2.5 ist unbrauchbar. */
const COMMONS_IO_FASSUNG = "2.16.1";
/** Pfad zum Manifest der App; wird bei jedem `tauri android init` neu erzeugt. */
const MANIFEST = "src-tauri/gen/android/app/src/main/AndroidManifest.xml";
const DRAWABLE = "src-tauri/gen/android/app/src/main/res/drawable";
const GRADLE_EIGENSCHAFTEN = "src-tauri/gen/android/gradle.properties";
/** Fassung von `androidx.media`; bringt MediaSession und die Medientasten mit. */
const MEDIA_FASSUNG = "1.7.0";

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

  // Commons-IO auf eine Fassung heben, die es noch gibt.
  //
  // `youtubedl-android` verlangt commons-io 2.5 von 2016. Dessen `FileUtils`
  // greift über die Hilfsklasse `Java7Support` auf `java.nio.file` zu, und
  // genau die landet nicht im fertigen Paket — D8 lässt sie fallen. Solange
  // niemand `FileUtils` benutzt, fällt das nicht auf; beim Aktualisieren von
  // yt-dlp tut es das, und die App brach mit `NoClassDefFoundError:
  // org.apache.commons.io.Java7Support` ab. Neuere Fassungen kommen ohne den
  // Umweg aus und bieten dieselben Methoden.
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

/**
 * Trägt den Player des Systems ein.
 *
 * Er besteht aus zwei Dingen, die Android beide angemeldet sehen will:
 *
 * * Ein Vordergrunddienst hält die App am Leben, solange Musik läuft. Ohne
 *   ihn darf Android den Prozess im Hintergrund abräumen, und die Wiedergabe
 *   bricht mitten im Titel ab. Er muss seine Art nennen — `mediaPlayback` —,
 *   sonst lehnt Android 14 den Start ab.
 * * Ein Empfänger für die Medientasten. Über ihn kommen die Knöpfe aus der
 *   Benachrichtigung und vom Sperrbildschirm zurück, ebenso die Tasten von
 *   Kopfhörern und Autoradios.
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

  // Macht aus einer Adresse der Dateiauswahl eine Datei mit Pfad.
  copyFileSync("src-tauri/android/Dateien.kt", join(PAKET_ORDNER, "Dateien.kt"));

  // Das Zeichen für die Benachrichtigung. Ohne es stünde dort das Dreieck des
  // Systems, dasselbe wie bei jeder anderen App, die Ton abspielt.
  copyFileSync("src-tauri/android/ic_notification.xml", join(DRAWABLE, "ic_notification.xml"));

  const manifest = readFileSync(MANIFEST, "utf8");
  if (manifest.includes("Wiedergabedienst")) {
    console.log("Systemplayer: schon im Manifest");
    return;
  }

  const mitRechten = manifest.replace(
    '<uses-permission android:name="android.permission.INTERNET" />',
    [
      '<uses-permission android:name="android.permission.INTERNET" />',
      // Ohne diese drei startet der Dienst gar nicht erst, und ab Android 13
      // bliebe die Anzeige unsichtbar, auch wenn er läuft.
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
 * Fragt die Erlaubnis für Benachrichtigungen.
 *
 * Seit Android 13 muss man sie erbitten. Ohne sie läuft der Vordergrunddienst
 * zwar, seine Anzeige bleibt aber unsichtbar — und genau die ist der Player,
 * um den es geht. Die Frage kommt beim Start und nur einmal; sagt der Nutzer
 * nein, spielt Robify weiter, nur eben ohne Anzeige.
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
 * Erbittet den Zugriff auf den Gerätespeicher.
 *
 * Robify legt seine Titel in `Robify` ab und alles Übrige in `.robify`, beide
 * unmittelbar im Gerätespeicher. Dort sind sie im Dateimanager zu sehen und
 * überleben das Entfernen der App — anders als alles unter `Android/data`.
 *
 * Seit Android 11 darf das keine App mehr ohne Weiteres. Die nötige Erlaubnis
 * wird nicht in einem Dialog erteilt, sondern auf einer Seite der
 * Systemeinstellungen, die die App aufrufen darf. Gefragt wird einmal je
 * Installation: Wer ablehnt, soll nicht bei jedem Start dieselbe Seite vor
 * sich haben. Robify arbeitet dann in seinem eigenen Ordner weiter.
 *
 * Erteilt der Nutzer sie, startet die App sich neu. Die Ordner stehen fest,
 * seit der Rust-Teil hochgefahren ist; ihn nachträglich umzuhängen wäre
 * aufwendiger und fehleranfälliger als ein Neustart, den man ohnehin nur
 * einmal im Leben der Installation sieht.
 *
 * Auf Android 10 und älter gibt es diese Erlaubnis nicht. Dort bleibt es beim
 * eigenen Ordner der App; der Rust-Teil merkt selbst, dass er im
 * Gerätespeicher nicht schreiben darf, und weicht aus.
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
        .replace("  override fun onCreate(savedInstanceState: Bundle?) {", felder)
        .replace(
          "    super.onCreate(savedInstanceState)",
          ["    super.onCreate(savedInstanceState)", "", "    dateizugriffErbitten()"].join("\n"),
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
 * Hält den Speicherhunger des Baus im Zaum.
 *
 * Gradle startet einen Hintergrunddienst, der zwischen zwei Bauläufen stehen
 * bleibt, und Kotlin einen zweiten daneben. Mit den Vorgabewerten belegten die
 * beiden zusammen über ein Gigabyte, und das auf einem Rechner, auf dem
 * nebenher noch etwas anderes läuft — beim Bauen ging dem Gerät der Speicher
 * aus, samt Auslagerungsdatei.
 *
 * Anderthalb Gigabyte reichen für ein Projekt dieser Größe bequem; zwei
 * gleichzeitige Arbeiter statt so vieler, wie der Rechner Kerne hat, kosten
 * ein paar Sekunden und sparen ein Vielfaches davon an Speicher.
 */
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

zurueckKnopfAnschalten();
zertifikatspruefungEinbinden();
ytdlpEinbinden();
systemplayerEinbinden();
benachrichtigungenErbitten();
dateizugriffErbitten();
speicherZuegeln();
