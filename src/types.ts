export type ReleaseType = "single" | "ep" | "album";

export interface Artist {
  id: number;
  name: string;
  sortName: string;
  mbid: string | null;
  trackCount: number;
  releaseCount: number;
  /** whether a profile image is stored. */
  hasImage: boolean;
  bio: string | null;
  sourceUrl: string | null;
}

export interface ArtistCandidate {
  source: string;
  name: string;
  imageUrl: string | null;
  bio: string | null;
  url: string | null;
  geniusId: number | null;
}

export interface Album {
  id: number;
  title: string;
  artistId: number;
  artistName: string;
  releaseType: ReleaseType;
  year: number | null;
  mbid: string | null;
  hasCover: boolean;
  trackCount: number;
  durationMs: number;
}

export interface TrackArtist {
  id: number;
  name: string;
  /** "main" or "feature". */
  role: string;
}

export interface Track {
  id: number;
  path: string;
  title: string;
  /** lead artist. */
  artistId: number;
  artistName: string;
  /** everyone involved, lead artist first. */
  artists: TrackArtist[];
  albumId: number;
  albumTitle: string;
  releaseType: ReleaseType;
  trackNo: number | null;
  discNo: number | null;
  durationMs: number;
  genre: string | null;
  year: number | null;
  format: string;
  hasCover: boolean;
  hasLyrics: boolean;
  addedAt: number;
  playCount: number;
  favorite: boolean;
  source: string | null;
  /** removed from the library but still carried in the review. */
  deleted: boolean;
}

export interface Playlist {
  id: number;
  name: string;
  description: string | null;
  createdAt: number;
  trackCount: number;
  durationMs: number;
  coverAlbumIds: number[];
  /** an image of its own is stored, otherwise the tile grows from the covers. */
  hasCover: boolean;
}

export interface PlaylistFill {
  playlist: Playlist;
  /** whether it was newly created or an existing one extended. */
  created: boolean;
  /** how many tracks came along this time. */
  added: number;
}

export interface Lyrics {
  trackId: number;
  synced: string | null;
  plain: string | null;
  source: string | null;
  updatedAt: number;
}

export interface TrackMetadata {
  title: string;
  /** lead artists, several of them separated by semicolons. */
  artist: string;
  /** guest artists, separated by semicolons. */
  featuredArtists: string | null;
  album: string;
  albumArtist: string | null;
  releaseType: string | null;
  year: number | null;
  trackNo: number | null;
  discNo: number | null;
  genre: string | null;
  coverBase64: string | null;
  coverMime: string | null;
  lyricsSynced: string | null;
  lyricsPlain: string | null;
}

export interface LibraryStats {
  trackCount: number;
  artistCount: number;
  albumCount: number;
  playlistCount: number;
  totalDurationMs: number;
  totalListenedMs: number;
}

export type RepeatMode = "off" | "all" | "one";
export type SleepTimerMode = "duration" | "endOfTrack";

export interface SleepTimerState {
  mode: SleepTimerMode;
  totalMs: number;
  remainingMs: number;
}

export interface PlayerState {
  playing: boolean;
  trackId: number | null;
  positionMs: number;
  durationMs: number;
  volume: number;
  muted: boolean;
  repeat: RepeatMode;
  shuffle: boolean;
  queue: number[];
  queueIndex: number | null;
  /**
   * positions of the queue in the order they run in.
   *
   * without shuffle plainly `0, 1, 2, …`, with it the drawn sequence. needed
   * to show what is still to come: under shuffle the coming tracks do not
   * stand behind the running one but lie scattered.
   */
  order: number[];
  /** where in `order` the running track stands. */
  orderPos: number | null;
  sleepTimer: SleepTimerState | null;
}

export interface PlayerTick {
  playing: boolean;
  positionMs: number;
  durationMs: number;
  sleepRemainingMs: number | null;
}

export interface ScanResult {
  scanned: number;
  imported: number;
  skipped: number;
  errors: string[];
}

export interface LibraryCheck {
  /** audio files in the library folder without a row in the database. */
  orphanCount: number;
  orphanSamples: string[];
  /** tracks whose file no longer exists. */
  missingCount: number;
  missingSamples: string[];
}

export interface ScanProgress {
  current: number;
  total: number;
  file: string;
}

export interface MetadataCandidate {
  source: string;
  title: string;
  artist: string;
  featuredArtists: string | null;
  album: string;
  albumArtist: string | null;
  releaseType: string | null;
  year: number | null;
  trackNo: number | null;
  discNo: number | null;
  genre: string | null;
  coverUrl: string | null;
  mbid: string | null;
  durationMs: number | null;
  /** genius page carrying the lyrics. */
  lyricsUrl: string | null;
  geniusSongId: number | null;
  geniusAlbumId: number | null;
}

export interface LyricsCandidate {
  id: number;
  trackName: string;
  artistName: string;
  albumName: string | null;
  duration: number | null;
  plainLyrics: string | null;
  syncedLyrics: string | null;
}

export interface DownloadOptions {
  url: string;
  format: string;
  quality?: string | null;
  embedThumbnail: boolean;
  /** metadata known beforehand, which wins over the tags of the file. */
  metadata?: TrackMetadata | null;
  /** fallback addresses where the first source delivers nothing. */
  fallbacks?: string[];
  /** the known length, results that are too short count as a preview. */
  expectedDurationMs?: number | null;
  /** search term the best hit is determined from. */
  matchQuery?: string | null;
  /** the user's search input, for the check after downloading. */
  intent?: string | null;
}

export interface DownloadPlan {
  url: string;
  fallbacks: string[];
  matchQuery: string | null;
  /** what was searched for, the basis of the check after downloading. */
  intent: string | null;
  title: string;
  subtitle: string | null;
  thumbnail: string | null;
  durationMs: number | null;
  source: string;
  metadata: TrackMetadata | null;
  /** whether a track of this name by the same artist lies there already. */
  alreadyInLibrary: boolean;
}

/** a hint whose wording the ui contributes itself. */
export interface PlanHinweis {
  /** which hint. the wording stands in the text table. */
  code: string;
  /** values for the placeholders `{0}`, `{1}` and so on. */
  args: string[];
}

export interface LinkPlan {
  /** with `kind === "search"` the input itself, otherwise the name found. */
  label: string;
  /** "search", "link" or "spotify-album" for instance. */
  kind: string;
  notes: PlanHinweis[];
  /** whether the entries belong together. downloading all of them makes sense then. */
  batch: boolean;
  items: DownloadPlan[];
}

export interface DownloadProgress {
  jobId: string;
  status: "starting" | "downloading" | "processing" | "done" | "error";
  percent: number;
  downloadedBytes: number | null;
  totalBytes: number | null;
  speedBytes: number | null;
  etaSeconds: number | null;
  message: string | null;
}

export interface DownloadOutcome {
  jobId: string;
  path: string;
  durationMs: number;
  format: string;
  metadata: TrackMetadata;
  sourceUrl: string;
  /** set where the result does not match the search input. */
  warning: string | null;
}

export interface DownloaderStatus {
  ytdlpPath: string | null;
  ytdlpVersion: string | null;
  ffmpegAvailable: boolean;
  /** needed for youtube, 403 errors otherwise. */
  jsRuntime: string | null;
  /**
   * whether anything can be done about a missing runtime at all.
   *
   * on android it cannot: neither node nor deno exists there, and they cannot
   * be installed either. the warning is dropped there.
   */
  jsRuntimeRelevant: boolean;
  activeJobs: string[];
}

export interface WrappedTrack {
  track: Track;
  playCount: number;
  msPlayed: number;
}

export interface WrappedArtist {
  artistId: number;
  name: string;
  playCount: number;
  msPlayed: number;
  trackCount: number;
  hasImage: boolean;
}

export interface WrappedAlbum {
  albumId: number;
  title: string;
  artistName: string;
  msPlayed: number;
  hasCover: boolean;
}

export interface TimeBucket {
  label: string;
  msPlayed: number;
}

export interface Wrapped {
  period: string;
  start: number;
  end: number;
  totalMs: number;
  totalPlays: number;
  distinctTracks: number;
  distinctArtists: number;
  topTracksTotalMs: number;
  topTracks: WrappedTrack[];
  topArtists: WrappedArtist[];
  topAlbums: WrappedAlbum[];
  buckets: TimeBucket[];
  busiestDay: TimeBucket | null;
}

export interface Recommendation {
  track: Track;
  /** how often the track ran that week. */
  playCount: number;
  /** how long it ran in total while doing so. */
  msPlayed: number;
}

export interface WeeklyMixSummary {
  weekKey: string;
  number: number;
  start: number;
  end: number;
  offset: number;
  trackCount: number;
  /** albums of the most played tracks, the mosaic cover grows out of them. */
  coverAlbumIds: number[];
}

export interface WeeklyMix {
  /** calendar week, "2026-KW33" for instance. */
  weekKey: string;
  /**
   * running number counted from the first week with listening data.
   *
   * the display name grows out of it in `lib/mix.ts`, it is ui text and
   * follows the ui language.
   */
  number: number;
  start: number;
  end: number;
  /** how many weeks back. 0 is the running one. */
  offset: number;
  /** whether another week with listening data lies before it. */
  hasOlder: boolean;
  items: Recommendation[];
}

export interface Settings {
  libraryDir: string;
  downloadFormat: string;
  downloadQuality: string;
  ytdlpPath: string;
  autoFetchLyrics: boolean;
  autoFetchCover: boolean;
  autoFetchArtists: boolean;
  moveDownloadsIntoLibrary: boolean;
  theme: string;
  accent: string;
  /** "grid" or "list". */
  playlistView: string;
  /** "sm", "md" or "lg". */
  playlistSize: string;
  /** ask before deleting. */
  confirmDelete: boolean;
  /** turn a downloaded playlist into a playlist in the library. */
  playlistFromDownload: boolean;
  /** scope of the review: "all", "month", "year" or "off". */
  wrappedMode: string;
  /** look missing details up online at import. */
  autoFetchImport: boolean;
  /** sorting of the library: "added", "title", "artist", "album", "year". */
  librarySort: string;
  /** accent colours mixed by hand, comma separated. */
  accentCustom: string;
  /** ui language: "system", "de" or "en". */
  language: string;
  /**
   * whether the storage locations are fixed.
   *
   * on a phone the tracks lie in "Robify" and everything else in ".robify",
   * both in the device storage. the folder choice is dropped there.
   */
  festeOrte: boolean;
}

export interface AppPaths {
  database: string;
  downloads: string;
  library: string;
  appData: string | null;
}
