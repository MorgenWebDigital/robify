import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api, fallback } from "../lib/api";
import { bustCoverCache } from "../lib/cover";
import { akzentSchrift } from "../lib/farbe";
import type { LibraryStats, Playlist, ScanProgress, Settings } from "../types";

interface LibraryStore {
  /** Wird bei jeder Änderung erhöht; Seiten laden daraufhin neu. */
  revision: number;
  /**
   * Ob der erste Abruf durch ist.
   *
   * Ohne diese Unterscheidung sähe eine leere Liste vor dem Laden genauso aus
   * wie eine wirklich leere Sammlung, und die Seite meldete „nichts
   * vorhanden“, bevor sie überhaupt nachgesehen hat.
   */
  geladen: boolean;
  stats: LibraryStats | null;
  playlists: Playlist[];
  settings: Settings | null;
  scanProgress: ScanProgress | null;

  init: () => Promise<() => void>;
  refresh: () => Promise<void>;
  reloadPlaylists: () => Promise<void>;
  saveSetting: (key: keyof Settings, value: string | boolean) => Promise<void>;
}

const SETTING_KEYS: Record<keyof Settings, string> = {
  libraryDir: "library_dir",
  downloadFormat: "download_format",
  downloadQuality: "download_quality",
  ytdlpPath: "ytdlp_path",
  autoFetchLyrics: "auto_fetch_lyrics",
  autoFetchCover: "auto_fetch_cover",
  autoFetchArtists: "auto_fetch_artists",
  moveDownloadsIntoLibrary: "move_downloads_into_library",
  accent: "accent",
  theme: "theme",
  playlistView: "playlist_view",
  playlistSize: "playlist_size",
  confirmDelete: "confirm_delete",
  playlistFromDownload: "playlist_from_download",
  wrappedMode: "wrapped_mode",
  autoFetchImport: "auto_fetch_import",
  librarySort: "library_sort",
  accentCustom: "accent_custom",
  language: "language",
};

/**
 * Wo Akzent und Erscheinungsbild für den nächsten Start liegen.
 *
 * Die Einstellungen selbst stehen in der Datenbank, und die zu öffnen dauert
 * über die Prozessgrenze hinweg einen Moment. Bis dahin galt die Vorgabe aus
 * dem Stylesheet, ein Grau, und der geöffnete Navigationspunkt stand beim
 * Start grau statt in der gewählten Farbe da. Der Browserspeicher antwortet
 * ohne Warten und überbrückt genau diese Lücke.
 */
const GEMERKT = { akzent: "robify:accent", erscheinung: "robify:theme" };

function merken(schluessel: string, wert: string): void {
  try {
    localStorage.setItem(schluessel, wert);
  } catch {
    // Ohne Browserspeicher bleibt es beim kurzen Grau. Kein Grund zu scheitern.
  }
}

export function applyAccent(accent: string): void {
  const wurzel = document.documentElement;
  wurzel.style.setProperty("--accent", accent);
  // Die Schrift auf dem Akzent muss mit: Die Vorgaben reichen von hellem Grau
  // bis Indigo, ein fester Ton wäre auf der einen Hälfte unlesbar.
  wurzel.style.setProperty("--accent-ink", akzentSchrift(accent));
  merken(GEMERKT.akzent, accent);
}

/**
 * Stellt das zuletzt Gesehene her, noch bevor gezeichnet wird.
 *
 * Wird in `main.tsx` vor dem Aufbau gerufen. Was hier steht, ist eine
 * Vermutung, kein Befund: Kommen die echten Einstellungen an, überschreiben
 * sie es. Bei der ersten Sitzung überhaupt ist nichts gemerkt, dann bleibt es
 * beim bisherigen Verhalten.
 */
export function erscheinungWiederherstellen(): void {
  try {
    const akzent = localStorage.getItem(GEMERKT.akzent);
    if (akzent) applyAccent(akzent);
    const erscheinung = localStorage.getItem(GEMERKT.erscheinung);
    if (erscheinung) applyTheme(erscheinung);
  } catch {
    // Siehe `merken`.
  }
}

/**
 * Hell/Dunkel festlegen. Bei „System“ wird kein Merkmal gesetzt, dann
 * entscheidet der Schreibtisch über `prefers-color-scheme`.
 */
export function applyTheme(theme: string): void {
  if (theme === "light" || theme === "dark") {
    document.documentElement.setAttribute("data-theme", theme);
  } else {
    document.documentElement.removeAttribute("data-theme");
  }
  merken(GEMERKT.erscheinung, theme);
}

export const useLibrary = create<LibraryStore>((set, get) => ({
  revision: 0,
  geladen: false,
  stats: null,
  playlists: [],
  settings: null,
  scanProgress: null,

  init: async () => {
    const unlisteners = await Promise.all([
      listen("library:changed", () => {
        bustCoverCache();
        void get().refresh();
      }),
      listen("library:plays-changed", () =>
        set((s) => ({ revision: s.revision + 1 })),
      ),
      listen<ScanProgress>("library:scan-progress", (event) => {
        const progress = event.payload;
        set({
          scanProgress: progress.current >= progress.total ? null : progress,
        });
      }),
    ]);

    await get().refresh();
    const settings = get().settings;
    if (settings) {
      applyAccent(settings.accent);
      applyTheme(settings.theme);
    }

    return () => unlisteners.forEach((off) => off());
  },

  refresh: async () => {
    const [stats, playlists, settings] = await Promise.all([
      api.libraryStats().catch(fallback(null, "Bibliotheksdaten")),
      api.listPlaylists().catch(fallback([], "Playlists")),
      api.getSettings().catch(fallback(null, "Einstellungen")),
    ]);
    set((s) => ({
      revision: s.revision + 1,
      geladen: true,
      stats,
      playlists,
      settings,
    }));
  },

  reloadPlaylists: async () => {
    set({ playlists: await api.listPlaylists() });
  },

  saveSetting: async (key, value) => {
    const stored = typeof value === "boolean" ? (value ? "1" : "0") : value;
    await api.setSetting(SETTING_KEYS[key], stored);
    const settings = await api.getSettings();
    set({ settings });
    if (key === "accent") applyAccent(settings.accent);
    if (key === "theme") applyTheme(settings.theme);
  },
}));
