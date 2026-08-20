import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
// the finder lives with the tools and not in `src`: it reaches into the file
// system and does not belong in what is shipped
import { ungehuellteStellen } from "../../scripts/deutsch-finden.mjs";
import { istGefuehrt } from "./i18n";
import { hatFormen, stichwoerter } from "./mehrzahl";
import { SPRACHEN } from "./sprachen";

// watches over finished files staying finished.
//
// what is checked is whether the term stands in the table, not whether its
// version reads differently from the german word. "Single" is called "Single"
// in french as well, and a comparison of values would take that for a gap.
//
// the list grows as soon as another file has been worked through. it is
// therefore a progress indicator and a guard against regressions at once:
// whoever builds in a new text without `t()` notices at the next test run
const FERTIG = [
  "App.tsx",
  "lib/datum.ts",
  "lib/format.ts",
  "lib/mix.ts",
  "lib/sort.ts",
  "components/AddToPlaylistDialog.tsx",
  "components/Aktualisierung.tsx",
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
  "components/Mitmachen.tsx",
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
  "components/Unterleiste.tsx",
  "components/TrackList.tsx",
  "pages/AlbumDetail.tsx",
  "pages/ArtistDetail.tsx",
  "pages/ArtistsPage.tsx",
  "pages/DownloaderPage.tsx",
  "pages/FavoritesPage.tsx",
  "pages/Home.tsx",
  "pages/LibraryPage.tsx",
  "pages/MixesPage.tsx",
  "pages/PlaylistDetail.tsx",
  "pages/PlaylistsPage.tsx",
  "pages/SettingsPage.tsx",
  "pages/WeeklyMixDetail.tsx",
  "pages/WrappedPage.tsx",
];

/**
 * words that read the same in every language.
 *
 * the product name and file formats are not translated, they stand that way
 * on every package. without this exception the test would report them as a
 * gap.
 */
const EIGENNAMEN = new Set([
  "Robify",
  "yt-dlp",
  "OGG Vorbis",
  "MP3",
  "FLAC",
  "M4A / AAC",
]);

const SRC = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * every `t("…")` call in a file.
 *
 * whitespace may stand between bracket and quote: prettier wraps long texts
 * onto the next line. without that freedom the longest paragraphs of all
 * would stay unchecked, and those were exactly the untranslated ones.
 */
function benutzteTexte(datei: string): string[] {
  const inhalt = readFileSync(join(SRC, datei), "utf8");
  // the lookbehind prevents hits such as `split(",")`, those end in `t(` too
  return [
    ...inhalt.matchAll(/(?<![A-Za-z0-9_$])t\(\s*"((?:[^"\\]|\\.)*)"/g),
    // the source is read, not the value: a line break stands there as two
    // characters. what is looked up is the text as `t` sees it
  ].map((m) => m[1].replace(/\\"/g, '"').replace(/\\n/g, "\n"));
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

  // `t()` must not stand at module level.
  //
  // a list such as `const FORMATE = [{ label: t("Beste Qualität") }]` comes
  // into being once when the module loads. does the user switch language
  // afterwards, the app rebuilds itself but the module does not, and the
  // label would stay in the starting language. that was exactly the case in
  // the downloader, in the review and in both editors.
  //
  // it is recognised by the indentation: what belongs to module level starts
  // at the left margin and runs to the closing bracket in column 0
  it("keine Übersetzung auf Modulebene", () => {
    const fundstellen: string[] = [];

    for (const datei of FERTIG) {
      const zeilen = readFileSync(join(SRC, datei), "utf8").split("\n");
      let tiefe = 0;
      zeilen.forEach((zeile, nummer) => {
        const beginnt =
          tiefe === 0 && /^(export )?const [A-Za-z_]+[^=]*= [[{]/.test(zeile);
        if (!beginnt && tiefe === 0) return;

        // count brackets instead of waiting for a closing line: a list such
        // as `const A = [1, 2];` opens and closes on the same line. without
        // the counting the test would take the whole rest of the file for
        // part of the list and report every later translation
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

  // every counted noun needs its inflections.
  //
  // `plural(anzahl, "Titel")` looks the keyword up in `mehrzahl.ts`. where it
  // is missing there, the display falls back to the german word and the
  // russian version would read "7 Titel" again
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

  // no german text without `t()`.
  //
  // the checks above start from the `t()` calls and therefore find gaps in
  // the table alone. text nobody wrapped is invisible to them. that is how
  // "Musik, davon" stayed in the settings, in the middle of wrapped values
  // all around, and only showed up when clicking through
  it("kein deutscher Text ohne Hülle", () => {
    expect(ungehuellteStellen(SRC)).toEqual([]);
  });

  // the messages from the rust side are translated as well.
  //
  // they appear only on an error but stood in german whatever the language:
  // the rust side does not know the interface language. since then it sends
  // the german template together with its values, and the ui looks it up like
  // any other text.
  //
  // the source is checked, not the build: the test is to fire even where
  // nobody has compiled the rust side freshly
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
