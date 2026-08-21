import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api, fallback } from "../lib/api";
import { bustCoverCache } from "../lib/cover";
import { akzentSchrift } from "../lib/farbe";
import type { LibraryStats, Playlist, ScanProgress, Settings } from "../types";

interface LibraryStore {
  /** raised on every change, and pages reload on it */
  revision: number;
  /**
   * whether the first fetch is through.
   *
   * without this distinction an empty list before loading would look exactly
   * like a truly empty collection, and the page would report nothing there
   * before having looked at all.
   */
  geladen: boolean;
  stats: LibraryStats | null;
  playlists: Playlist[];
  settings: Settings | null;
  scanProgress: ScanProgress | null;

  init: () => Promise<() => void>;
  refresh: () => Promise<void>;
  reloadPlaylists: () => Promise<void>;
  saveSetting: (
    key: SpeicherbareEinstellung,
    value: string | boolean,
  ) => Promise<void>;
}

/**
 * what can be stored.
 *
 * `festeOrte` does not belong to it: the value says whether the storage
 * locations of the system are fixed, and it is answered by the rust side, not
 * set.
 */
export type SpeicherbareEinstellung = Exclude<keyof Settings, "festeOrte">;

const SETTING_KEYS: Record<SpeicherbareEinstellung, string> = {
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

// where accent and appearance lie for the next start.
//
// the settings themselves stand in the database, and opening that takes a
// moment across the process boundary. until then the default from the
// stylesheet applied, a grey, and the open navigation entry stood grey at
// startup instead of in the chosen colour. local storage answers without a
// wait and bridges exactly that gap
const GEMERKT = { akzent: "robify:accent", erscheinung: "robify:theme" };

function merken(schluessel: string, wert: string): void {
  try {
    localStorage.setItem(schluessel, wert);
  } catch {
    // without local storage the brief grey stays. no reason to fail
  }
}

export function applyAccent(accent: string): void {
  const wurzel = document.documentElement;
  wurzel.style.setProperty("--accent", accent);
  // the type on the accent has to travel along: the presets range from light
  // grey to indigo, and a fixed tone would be unreadable on one half
  wurzel.style.setProperty("--accent-ink", akzentSchrift(accent));
  merken(GEMERKT.akzent, accent);
}

/**
 * restores what was last seen, before anything is painted.
 *
 * called in `main.tsx` before the build-up. what stands here is a guess, not
 * a finding: once the real settings arrive they overwrite it. in the very
 * first session nothing is remembered, and the previous behaviour stays.
 */
export function erscheinungWiederherstellen(): void {
  try {
    const akzent = localStorage.getItem(GEMERKT.akzent);
    if (akzent) applyAccent(akzent);
    const erscheinung = localStorage.getItem(GEMERKT.erscheinung);
    if (erscheinung) applyTheme(erscheinung);
  } catch {
    // see `merken`
  }
}

/**
 * sets light or dark. with "system" no attribute is set and the desktop
 * decides through `prefers-color-scheme`.
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
