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

const MAIN_ACTIVITY =
  "src-tauri/gen/android/app/src/main/java/de/robify/player/MainActivity.kt";

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

zurueckKnopfAnschalten();
