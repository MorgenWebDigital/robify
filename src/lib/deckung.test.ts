import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
// Der Sucher liegt bei den Werkzeugen, nicht in `src`: Er greift auf das
// Dateisystem zu und gehört nicht in das, was ausgeliefert wird.
import { ungehuellteStellen } from "../../scripts/deutsch-finden.mjs";
import { istGefuehrt } from "./i18n";
import { hatFormen, stichwoerter } from "./mehrzahl";
import { SPRACHEN } from "./sprachen";

/**
 * Wacht darüber, dass fertig übersetzte Dateien fertig übersetzt bleiben.
 *
 * Geprüft wird, ob der Begriff in der Tabelle *steht*, nicht, ob seine
 * Fassung anders lautet als das deutsche Wort. „Single" heißt auf Französisch
 * ebenfalls „Single", und ein Wertvergleich hielte das für eine Lücke.
 *
 * Die Liste wächst, sobald eine weitere Datei durchgearbeitet ist. Sie ist
 * damit zugleich Fortschrittsanzeige und Schutz vor Rückschritten: Wer einen
 * neuen Text ohne `t()` einbaut, merkt es beim nächsten Testlauf.
 */
const FERTIG = [
  "App.tsx",
  "lib/datum.ts",
  "lib/format.ts",
  "lib/mix.ts",
  "lib/sort.ts",
  "components/AddToPlaylistDialog.tsx",
  "components/AkzentWahl.tsx",
  "components/AlbumEditor.tsx",
  "components/ArtistEditor.tsx",
  "components/Auswahl.tsx",
  "components/Cards.tsx",
  "components/CoverPicker.tsx",
  "components/ErrorBoundary.tsx",
  "components/LyricsPanel.tsx",
  "components/LyricsSync.tsx",
  "components/MetadataEditor.tsx",
  "components/Modal.tsx",
  "components/NowPlaying.tsx",
  "components/PlayerBar.tsx",
  "components/PlaylistCreateDialog.tsx",
  "components/QueuePanel.tsx",
  "components/Rechtliches.tsx",
  "components/Sidebar.tsx",
  "components/SleepTimerMenu.tsx",
  "components/TitleBar.tsx",
  "components/Toasts.tsx",
  "components/TrackList.tsx",
  "pages/AlbumDetail.tsx",
  "pages/ArtistDetail.tsx",
  "pages/ArtistsPage.tsx",
  "pages/DownloaderPage.tsx",
  "pages/FavoritesPage.tsx",
  "pages/Home.tsx",
  "pages/LibraryPage.tsx",
  "pages/PlaylistDetail.tsx",
  "pages/PlaylistsPage.tsx",
  "pages/SettingsPage.tsx",
  "pages/WeeklyMixDetail.tsx",
  "pages/WrappedPage.tsx",
];

/**
 * Wörter, die in jeder Sprache gleich lauten.
 *
 * Produktname und Dateiformate werden nicht übersetzt, sie stehen so auf
 * jeder Verpackung. Ohne diese Ausnahme meldete der Test sie als Lücke.
 */
const EIGENNAMEN = new Set([
  "Robify",
  "OGG Vorbis",
  "MP3",
  "FLAC",
  "M4A / AAC",
]);

const SRC = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Alle `t("…")`-Aufrufe einer Datei.
 *
 * Zwischen Klammer und Anführungszeichen darf Leerraum stehen: Prettier bricht
 * lange Texte auf die nächste Zeile um. Ohne diese Freiheit blieben ausgerechnet
 * die längsten Absätze ungeprüft, und genau die waren unübersetzt.
 */
function benutzteTexte(datei: string): string[] {
  const inhalt = readFileSync(join(SRC, datei), "utf8");
  // Der Rückblick verhindert Treffer wie `split(",")`, auch die enden auf `t(`.
  return [
    ...inhalt.matchAll(/(?<![A-Za-z0-9_$])t\(\s*"((?:[^"\\]|\\.)*)"/g),
  ].map((m) => m[1]);
}

describe("Übersetzungsdeckung", () => {
  for (const datei of FERTIG) {
    it(`${datei} ist vollständig übersetzt`, () => {
      const ohneFassung = benutzteTexte(datei).filter(
        (text) => !istGefuehrt(text) && !EIGENNAMEN.has(text),
      );
      expect(ohneFassung).toEqual([]);
    });
  }

  /**
   * `t()` darf nicht auf Modulebene stehen.
   *
   * Eine Liste wie `const FORMATE = [{ label: t("Beste Qualität") }]` entsteht
   * einmal beim Laden des Moduls. Wechselt der Nutzer danach die Sprache, baut
   * die App sich zwar neu auf, das Modul aber nicht, und die Beschriftung
   * bliebe in der Anfangssprache stehen. Genau so war es im Downloader, im
   * Rückblick und in beiden Editoren.
   *
   * Erkannt wird an der Einrückung: Was zur Modulebene gehört, beginnt am
   * linken Rand und läuft bis zur schließenden Klammer in Spalte 0.
   */
  it("keine Übersetzung auf Modulebene", () => {
    const fundstellen: string[] = [];

    for (const datei of FERTIG) {
      const zeilen = readFileSync(join(SRC, datei), "utf8").split("\n");
      let tiefe = 0;
      zeilen.forEach((zeile, nummer) => {
        const beginnt =
          tiefe === 0 && /^(export )?const [A-Za-z_]+[^=]*= [[{]/.test(zeile);
        if (!beginnt && tiefe === 0) return;

        // Klammern zählen statt auf eine schließende Zeile zu warten: Eine
        // Liste wie `const A = [1, 2];` öffnet und schließt in derselben
        // Zeile. Ohne die Zählung hielte der Test den ganzen Rest der Datei
        // für Teil der Liste, und meldete jede spätere Übersetzung.
        const offen = (zeile.match(/[[{]/g) ?? []).length;
        const zu = (zeile.match(/[\]}]/g) ?? []).length;

        if (/(?<![A-Za-z0-9_$])t\(/.test(zeile))
          fundstellen.push(`${datei}:${nummer + 1}`);
        tiefe += offen - zu;
        if (tiefe < 0) tiefe = 0;
      });
    }

    expect(fundstellen).toEqual([]);
  });

  /**
   * Jedes gezählte Hauptwort braucht seine Beugungen.
   *
   * `plural(anzahl, "Titel")` schlägt das Stichwort in `mehrzahl.ts` nach.
   * Fehlt es dort, fällt die Anzeige auf das deutsche Wort zurück, und in der
   * russischen Fassung stünde wieder „7 Titel“.
   */
  it("jedes gezählte Wort ist gebeugt geführt", () => {
    const luecken: string[] = [];

    for (const datei of FERTIG) {
      const inhalt = readFileSync(join(SRC, datei), "utf8");
      for (const treffer of inhalt.matchAll(/plural\([^,]+,\s*"([^"]+)"\)/g)) {
        if (!hatFormen(treffer[1])) luecken.push(`${datei}: ${treffer[1]}`);
      }
    }
    expect(luecken).toEqual([]);
  });

  it("jede Beugung liegt in allen geführten Sprachen vor", () => {
    const sprachen = SPRACHEN.map((eintrag) => eintrag.id);
    const luecken: string[] = [];

    for (const stichwort of stichwoerter()) {
      for (const sprache of sprachen) {
        if (!hatFormen(stichwort, sprache))
          luecken.push(`${sprache}: ${stichwort}`);
      }
    }
    expect(luecken).toEqual([]);
  });

  /**
   * Kein deutscher Text ohne `t()`.
   *
   * Die Prüfungen oben gehen von den `t()`-Aufrufen aus und finden darum nur
   * Lücken in der Tabelle. Text, den niemand gehüllt hat, sehen sie nicht. So
   * blieb in den Einstellungen „Musik, davon“ stehen, mitten zwischen lauter
   * gehüllten Angaben, und fiel erst beim Durchklicken auf.
   */
  it("kein deutscher Text ohne Hülle", () => {
    expect(ungehuellteStellen(SRC)).toEqual([]);
  });

  /**
   * Auch die Meldungen aus dem Rust-Teil liegen übersetzt vor.
   *
   * Sie erscheinen nur im Fehlerfall, standen dafür aber in jeder Sprache auf
   * Deutsch: Der Rust-Teil kennt die Oberflächensprache nicht. Er schickt
   * seither die deutsche Vorlage samt Einsetzwerten, und die Oberfläche
   * schlägt sie nach, genau wie jeden anderen Text.
   *
   * Geprüft wird der Quelltext, nicht der Bau: Der Test soll auch dann
   * anschlagen, wenn niemand den Rust-Teil frisch übersetzt hat.
   */
  it("jede Fehlermeldung aus dem Rust-Teil ist übersetzt", () => {
    const sprachen = SPRACHEN.map((eintrag) => eintrag.id).filter(
      (id) => id !== "de",
    );
    const rust = resolve(SRC, "..", "src-tauri", "src");
    const luecken: string[] = [];

    for (const datei of readdirSync(rust).filter((name) =>
      name.endsWith(".rs"),
    )) {
      const inhalt = readFileSync(join(rust, datei), "utf8");
      for (const treffer of inhalt.matchAll(
        /fehler!\(\s*"((?:[^"\\]|\\.)*)"/g,
      )) {
        const vorlage = treffer[1].replace(/\\"/g, '"').replace(/\\n/g, "\n");
        if (!istGefuehrt(vorlage)) {
          luecken.push(`${datei}: ${vorlage.slice(0, 50)}`);
          continue;
        }
        for (const sprache of sprachen) {
          if (!istGefuehrt(vorlage, sprache)) {
            luecken.push(`${sprache}: ${vorlage.slice(0, 50)}`);
          }
        }
      }
    }

    expect(luecken).toEqual([]);
  });

  it("jeder Begriff liegt in allen geführten Sprachen vor", () => {
    const sprachen = SPRACHEN.map((eintrag) => eintrag.id).filter(
      (id) => id !== "de",
    );
    const luecken: string[] = [];

    for (const datei of FERTIG) {
      for (const text of benutzteTexte(datei)) {
        for (const sprache of sprachen) {
          if (!istGefuehrt(text, sprache) && !EIGENNAMEN.has(text)) {
            luecken.push(`${sprache}: ${text.slice(0, 40)}`);
          }
        }
      }
    }

    expect(luecken).toEqual([]);
  });
});
