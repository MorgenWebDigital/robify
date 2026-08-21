import type { Settings } from "../types";

// a complete set of settings for the tests.
//
// the pages read them without asking whether they are there, and a single
// missing field ends in `Cannot read properties of undefined`. writing them
// out in every test would mean touching twenty places whenever one is added —
// here it is one.
export function einstellungen(abweichend: Partial<Settings> = {}): Settings {
  return {
    libraryDir: "/musik",
    downloadFormat: "mp3",
    downloadQuality: "0",
    ytdlpPath: "",
    autoFetchLyrics: true,
    autoFetchCover: true,
    autoFetchArtists: true,
    moveDownloadsIntoLibrary: true,
    theme: "dark",
    accent: "rot",
    playlistView: "grid",
    playlistSize: "md",
    confirmDelete: true,
    playlistFromDownload: false,
    wrappedMode: "all",
    autoFetchImport: true,
    librarySort: "added",
    accentCustom: "",
    language: "de",
    festeOrte: false,
    ...abweichend,
  };
}
