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
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";

const MAIN_ACTIVITY =
  "src-tauri/gen/android/app/src/main/java/de/robify/player/MainActivity.kt";
const APP_GRADLE = "src-tauri/gen/android/app/build.gradle.kts";

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

zurueckKnopfAnschalten();
zertifikatspruefungEinbinden();
