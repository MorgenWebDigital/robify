import { invoke } from "@tauri-apps/api/core";
import { useUi } from "../store/ui";
import { t } from "./i18n";
import type {
  Album,
  AppPaths,
  Artist,
  ArtistCandidate,
  DownloaderStatus,
  DownloadOptions,
  DownloadOutcome,
  LibraryStats,
  LinkPlan,
  Lyrics,
  LyricsCandidate,
  MetadataCandidate,
  Playlist,
  PlaylistFill,
  PlayerState,
  RepeatMode,
  LibraryCheck,
  ScanResult,
  Settings,
  SleepTimerMode,
  Track,
  TrackMetadata,
  WeeklyMix,
  WeeklyMixSummary,
  Wrapped,
} from "../types";

export const api = {
  // Bibliothek
  libraryStats: () => invoke<LibraryStats>("library_stats"),
  scanFolders: (paths: string[]) =>
    invoke<ScanResult>("scan_folders", { paths }),
  checkLibrary: () => invoke<LibraryCheck>("check_library"),
  /** Liefert die Kennungen der entfernten Titel, damit sich das zurücknehmen lässt. */
  removeMissingTracks: () => invoke<number[]>("remove_missing_tracks"),
  backupDatabase: () => invoke<string>("backup_database"),
  resetApp: (deleteFiles: boolean, keepSettings: boolean) =>
    invoke<string>("reset_app", { deleteFiles, keepSettings }),
  listTracks: (search?: string, limit?: number) =>
    invoke<Track[]>("list_tracks", {
      search: search ?? null,
      limit: limit ?? null,
    }),
  getTrack: (id: number) => invoke<Track>("get_track", { id }),
  getTracks: (ids: number[]) => invoke<Track[]>("get_tracks", { ids }),
  listArtists: (search?: string) =>
    invoke<Artist[]>("list_artists", { search: search ?? null }),
  getArtist: (id: number) => invoke<Artist>("get_artist", { id }),
  artistReleases: (artistId: number) =>
    invoke<Album[]>("artist_releases", { artistId }),
  artistTracks: (artistId: number) =>
    invoke<Track[]>("artist_tracks", { artistId }),
  artistFeatures: (artistId: number) =>
    invoke<Track[]>("artist_features", { artistId }),
  searchArtistsOnline: (name: string) =>
    invoke<ArtistCandidate[]>("search_artists_online", { name }),
  applyArtistMetadata: (artistId: number, candidate: ArtistCandidate) =>
    invoke<Artist>("apply_artist_metadata", { artistId, candidate }),
  fetchArtistMetadata: (artistId: number) =>
    invoke<Artist>("fetch_artist_metadata", { artistId }),
  updateArtist: (
    artistId: number,
    name: string,
    bio: string | null,
    imageBase64: string | null,
    imageMime: string | null,
    removeImage: boolean,
  ) =>
    invoke<Artist>("update_artist", {
      artistId,
      name,
      bio,
      imageBase64,
      imageMime,
      removeImage,
    }),
  updateAlbum: (
    albumId: number,
    title: string,
    year: number | null,
    releaseType: string,
    coverBase64: string | null,
    coverMime: string | null,
    removeCover: boolean,
  ) =>
    invoke<Album>("update_album", {
      albumId,
      title,
      year,
      releaseType,
      coverBase64,
      coverMime,
      removeCover,
    }),
  listAlbums: (search?: string) =>
    invoke<Album[]>("list_albums", { search: search ?? null }),
  getAlbum: (id: number) => invoke<Album>("get_album", { id }),
  albumTracks: (albumId: number) =>
    invoke<Track[]>("album_tracks", { albumId }),
  favoriteTracks: () => invoke<Track[]>("favorite_tracks"),
  setFavorite: (trackId: number, favorite: boolean) =>
    invoke<void>("set_favorite", { trackId, favorite }),
  restoreTrack: (trackId: number) => invoke<void>("restore_track", { trackId }),
  deleteTrack: (trackId: number, deleteFile: boolean) =>
    invoke<void>("delete_track", { trackId, deleteFile }),

  // Metadaten & Lyrics
  getTrackMetadata: (trackId: number) =>
    invoke<TrackMetadata>("get_track_metadata", { trackId }),
  updateTrackMetadata: (
    trackId: number,
    metadata: TrackMetadata,
    writeToFile: boolean,
  ) =>
    invoke<Track>("update_track_metadata", { trackId, metadata, writeToFile }),
  searchMetadataOnline: (query: string) =>
    invoke<MetadataCandidate[]>("search_metadata_online", { query }),
  /** Holt zu einem Treffer Cover, Lyrics, Release-Art und Titelnummer nach. */
  enrichCandidate: (candidate: MetadataCandidate, durationMs?: number) =>
    invoke<TrackMetadata>("enrich_candidate", {
      candidate,
      durationMs: durationMs ?? null,
    }),
  fetchCover: (url: string) =>
    invoke<{ base64: string; mime: string }>("fetch_cover", { url }),
  getLyrics: (trackId: number) =>
    invoke<Lyrics | null>("get_lyrics", { trackId }),
  saveLyrics: (
    trackId: number,
    synced: string | null,
    plain: string | null,
    writeToFile: boolean,
  ) => invoke<void>("save_lyrics", { trackId, synced, plain, writeToFile }),
  fetchLyricsOnline: (trackId: number) =>
    invoke<Lyrics>("fetch_lyrics_online", { trackId }),
  searchLyricsOnline: (query: string) =>
    invoke<LyricsCandidate[]>("search_lyrics_online", { query }),

  // Playlists
  listPlaylists: () => invoke<Playlist[]>("list_playlists"),
  getPlaylist: (id: number) => invoke<Playlist>("get_playlist", { id }),
  createPlaylist: (
    name: string,
    description?: string | null,
    cover?: { base64: string; mime: string } | null,
  ) =>
    invoke<Playlist>("create_playlist", {
      name,
      description: description ?? null,
      coverBase64: cover?.base64 ?? null,
      coverMime: cover?.mime ?? null,
    }),
  updatePlaylist: (
    id: number,
    name: string,
    description?: string | null,
    cover?: { base64: string; mime: string } | null,
    removeCover = false,
  ) =>
    invoke<Playlist>("update_playlist", {
      id,
      name,
      description: description ?? null,
      coverBase64: cover?.base64 ?? null,
      coverMime: cover?.mime ?? null,
      removeCover,
    }),
  deletePlaylist: (id: number) => invoke<void>("delete_playlist", { id }),
  restorePlaylist: (id: number) => invoke<Playlist>("restore_playlist", { id }),
  createPlaylistFromEntries: (
    name: string,
    entries: { artist: string; title: string }[],
  ) => invoke<PlaylistFill>("create_playlist_from_entries", { name, entries }),
  playlistTracks: (playlistId: number) =>
    invoke<Track[]>("playlist_tracks", { playlistId }),
  playlistsContaining: (trackIds: number[]) =>
    invoke<[number, number][]>("playlists_containing", { trackIds }),
  addToPlaylist: (playlistId: number, trackIds: number[]) =>
    invoke<void>("add_to_playlist", { playlistId, trackIds }),
  removeFromPlaylist: (playlistId: number, trackId: number) =>
    invoke<void>("remove_from_playlist", { playlistId, trackId }),
  reorderPlaylist: (playlistId: number, trackIds: number[]) =>
    invoke<void>("reorder_playlist", { playlistId, trackIds }),
  /** Reihenfolge der Sammlung selbst, nicht der Titel darin. */
  reorderPlaylists: (playlistIds: number[]) =>
    invoke<void>("reorder_playlists", { playlistIds }),

  // Player
  playerState: () => invoke<PlayerState>("player_state"),
  playTracks: (trackIds: number[], startIndex = 0) =>
    invoke<void>("play_tracks", { trackIds, startIndex }),
  toggle: () => invoke<void>("player_toggle"),
  play: () => invoke<void>("player_play"),
  next: () => invoke<void>("player_next"),
  previous: () => invoke<void>("player_previous"),
  seek: (positionMs: number) => invoke<void>("player_seek", { positionMs }),
  setVolume: (volume: number) => invoke<void>("player_set_volume", { volume }),
  setMuted: (muted: boolean) => invoke<void>("player_set_muted", { muted }),
  setRepeat: (mode: RepeatMode) => invoke<void>("player_set_repeat", { mode }),
  setShuffle: (shuffle: boolean) =>
    invoke<void>("player_set_shuffle", { shuffle }),
  queueAdd: (trackIds: number[]) => invoke<void>("queue_add", { trackIds }),
  queuePlayNext: (trackIds: number[]) =>
    invoke<void>("queue_play_next", { trackIds }),
  queueRemove: (index: number) => invoke<void>("queue_remove", { index }),
  queueClear: () => invoke<void>("queue_clear"),
  setSleepTimer: (mode: SleepTimerMode | null, minutes?: number) =>
    invoke<void>("set_sleep_timer", { mode, minutes: minutes ?? null }),

  // Statistiken
  weeklyMix: (offset = 0) => invoke<WeeklyMix>("weekly_mix", { offset }),
  weeklyMixes: (limit = 12) =>
    invoke<WeeklyMixSummary[]>("weekly_mixes", { limit }),
  saveWeeklyMix: (offset: number, name: string, description: string) =>
    invoke<Playlist>("save_weekly_mix", { offset, name, description }),
  recentlyPlayed: (limit = 20) => invoke<Track[]>("recently_played", { limit }),
  wrapped: (period: "month" | "year" | "all", offset = 0) =>
    invoke<Wrapped>("wrapped", { period, offset }),

  // Downloader
  downloaderStatus: () => invoke<DownloaderStatus>("downloader_status"),
  /** Erkennt selbst, ob ein Link eingefügt oder gesucht wurde. */
  resolveInput: (input: string, limit?: number) =>
    invoke<LinkPlan>("resolve_input", { input, limit: limit ?? null }),
  startDownload: (jobId: string, options: DownloadOptions) =>
    invoke<DownloadOutcome>("start_download", { jobId, options }),
  cancelDownload: (jobId: string) =>
    invoke<boolean>("cancel_download", { jobId }),
  importDownload: (
    path: string,
    metadata: TrackMetadata,
    sourceUrl: string | null,
    moveIntoLibrary: boolean,
  ) =>
    invoke<Track>("import_download", {
      path,
      metadata,
      sourceUrl,
      moveIntoLibrary,
    }),

  // Einstellungen
  getSettings: () => invoke<Settings>("get_settings"),
  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  appPaths: () => invoke<AppPaths>("app_paths"),
};

/** Fehlermeldungen aus dem Backend kommen als String an. */
/** Trennt Vorlage und Einsetzwerte; muss zu `meldung.rs` passen. */
const TRENNER = "\u001f";
/** Trennt zwei eigenständige Meldungen; muss zu `meldung.rs` passen. */
const ABSATZ = "\u001e";

/**
 * Fehlertext für die Anzeige, übersetzt.
 *
 * Meldungen aus dem Rust-Teil kommen als deutsche Vorlage samt Einsetzwerten,
 * durch Steuerzeichen getrennt. Der deutsche Text ist dabei der Schlüssel,
 * genau wie sonst in der App, nur mit `{0}` an den Stellen, die erst zur
 * Laufzeit feststehen: Ein Dateiname gehört nicht in eine Texttabelle.
 *
 * Was keine Steuerzeichen enthält, läuft trotzdem durch `t()`. Meldungen ohne
 * Einsetzwerte brauchen im Rust-Teil deshalb gar keine Behandlung, ihr
 * Wortlaut *ist* schon der Nachschlagebegriff.
 */
export function errorMessage(error: unknown): string {
  const roh =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : String(error);

  return roh
    .split(ABSATZ)
    .map((teil) => {
      const [vorlage, ...werte] = teil.split(TRENNER);
      return t(vorlage, ...werte);
    })
    .join("\n\n");
}

/** Zuletzt gemeldeter Ausfall, samt Zeitpunkt. */
let letzteMeldung = { text: "", zeit: 0 };

/**
 * Ersatzwert für eine gescheiterte Ladeanfrage, aber nicht lautlos.
 *
 * Seiten holen ihre Daten in Bündeln (`Promise.all`), und ein einzelner
 * Fehlschlag darf den Rest nicht mitreißen. Bisher stand dafür überall
 * `.catch(() => [])`: Die Seite blieb heil, aber leer, und niemand erfuhr
 * warum, eine leere Bibliothek sieht aus wie eine Bibliothek ohne Musik.
 *
 * Statt dessen kommt der Ersatzwert zurück *und* eine Meldung. Gleiche
 * Meldungen innerhalb von fünf Sekunden werden zusammengefasst, sonst
 * überschütten vier gleichzeitig fehlgeschlagene Abfragen den Nutzer.
 */
export function fallback<T>(ersatz: T, was: string): (error: unknown) => T {
  return (error: unknown): T => {
    const text = t("{0} konnte nicht geladen werden", was);
    console.error(`Robify: ${text}`, error);

    const jetzt = Date.now();
    if (letzteMeldung.text !== text || jetzt - letzteMeldung.zeit > 5000) {
      letzteMeldung = { text, zeit: jetzt };
      // Erst im nächsten Durchlauf melden: Der Aufruf steckt oft noch im
      // Rendern einer Seite, und ein Zustandswechsel mittendrin wäre ein
      // Verstoß gegen Reacts Regeln.
      queueMicrotask(() =>
        useUi.getState().notify(`${text}: ${errorMessage(error)}`, "error"),
      );
    }
    return ersatz;
  };
}
