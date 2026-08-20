import { invoke } from "@tauri-apps/api/core";
import { useUi } from "../store/ui";
import { t } from "./i18n";
import type {
  Aktualisierungen,
  Album,
  AppPaths,
  Artist,
  ArtistCandidate,
  DownloaderStatus,
  DownloadOptions,
  DownloadOutcome,
  LibraryCheck,
  LibraryStats,
  LinkPlan,
  Lyrics,
  LyricsCandidate,
  MetadataCandidate,
  PlayerState,
  Playlist,
  PlaylistFill,
  RepeatMode,
  ScanResult,
  Settings,
  SleepTimerMode,
  Track,
  TrackMetadata,
  WeeklyMix,
  WeeklyMixSummary,
  Wrapped,
  YtdlpErneuert,
} from "../types";

export const api = {
  // --- library ---
  libraryStats: () => invoke<LibraryStats>("library_stats"),
  scanFolders: (paths: string[]) =>
    invoke<ScanResult>("scan_folders", { paths }),
  checkLibrary: () => invoke<LibraryCheck>("check_library"),
  /** returns the ids of the removed tracks, so it can be undone. */
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

  // --- metadata and lyrics ---
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
  /** fetches cover, lyrics, release type and track number for a hit. */
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

  // --- playlists ---
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
  reorderFavorites: (trackIds: number[]) =>
    invoke<void>("reorder_favorites", { trackIds }),
  /** order of the collection itself, not of the tracks inside it. */
  reorderPlaylists: (playlistIds: number[]) =>
    invoke<void>("reorder_playlists", { playlistIds }),

  // --- player ---
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

  // --- statistics ---
  weeklyMix: (offset = 0) => invoke<WeeklyMix>("weekly_mix", { offset }),
  weeklyMixes: (limit = 12) =>
    invoke<WeeklyMixSummary[]>("weekly_mixes", { limit }),
  saveWeeklyMix: (offset: number, name: string, description: string) =>
    invoke<Playlist>("save_weekly_mix", { offset, name, description }),
  recentlyPlayed: (limit = 20) => invoke<Track[]>("recently_played", { limit }),
  wrapped: (period: "month" | "year" | "all", offset = 0) =>
    invoke<Wrapped>("wrapped", { period, offset }),

  // --- downloader ---
  downloaderStatus: () => invoke<DownloaderStatus>("downloader_status"),
  /** fetches the newest version of yt-dlp and returns its number. */
  ytdlpAktualisieren: () => invoke<YtdlpErneuert>("update_ytdlp"),
  aktualisierungenPruefen: () => invoke<Aktualisierungen>("check_updates"),
  releaseSeiteOeffnen: () => invoke<void>("open_release_page"),
  /** recognises by itself whether a link was pasted or a search typed. */
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

  // --- settings ---
  getSettings: () => invoke<Settings>("get_settings"),
  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  appPaths: () => invoke<AppPaths>("app_paths"),
};

/** error messages from the backend arrive as a string. */
/** separates template and values, has to match `meldung.rs`. */
const TRENNER = "\u001f";
/** separates two independent messages, has to match `meldung.rs`. */
const ABSATZ = "\u001e";

/**
 * error text for display, translated.
 *
 * messages from the rust side arrive as a german template together with its
 * values, separated by control characters. the german text is the key there,
 * exactly as everywhere else in the app, only with `{0}` at the places that
 * are settled at runtime: a filename does not belong in a text table.
 *
 * what holds no control characters still runs through `t()`. messages without
 * values therefore need no handling on the rust side at all, their wording
 * already is the lookup term.
 */
export function errorMessage(error: unknown): string {
  const roh =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : String(error);

  return meldungText(roh);
}

/**
 * translates a message assembled on the rust side.
 *
 * not only errors travel this way: what a download is doing right now
 * ("reading the source…") arrives as a template together with its values too
 * and becomes a sentence in the selected language here. where the template is
 * not in the table, `t` returns it unchanged, so a verbatim message from
 * yt-dlp stays as it is.
 */
export function meldungText(roh: string): string {
  return roh
    .split(ABSATZ)
    .map((teil) => {
      const [vorlage, ...werte] = teil.split(TRENNER);
      return t(vorlage, ...werte);
    })
    .join("\n\n");
}

/** the last failure reported, together with its time. */
let letzteMeldung = { text: "", zeit: 0 };

/**
 * a fallback value for a failed load, but not a silent one.
 *
 * pages fetch their data in bundles (`Promise.all`), and a single failure
 * must not tear the rest along. `.catch(() => [])` used to stand everywhere
 * for that: the page stayed intact but empty, and nobody learned why, as an
 * empty library looks like a library without music.
 *
 * the fallback value comes back instead, and a message with it. identical
 * messages within five seconds are folded together, otherwise four queries
 * failing at once bury the user.
 */
export function fallback<T>(ersatz: T, was: string): (error: unknown) => T {
  return (error: unknown): T => {
    const text = t("{0} konnte nicht geladen werden", was);
    console.error(`Robify: ${text}`, error);

    const jetzt = Date.now();
    if (letzteMeldung.text !== text || jetzt - letzteMeldung.zeit > 5000) {
      letzteMeldung = { text, zeit: jetzt };
      // report on the next pass: the call often sits inside the render of a
      // page, and a state change in the middle of it would break react's
      // rules
      queueMicrotask(() =>
        useUi.getState().notify(`${text}: ${errorMessage(error)}`, "error"),
      );
    }
    return ersatz;
  };
}
