export type ReleaseType = "single" | "ep" | "album";

export interface Artist {
  id: number;
  name: string;
  sortName: string;
  mbid: string | null;
  trackCount: number;
  releaseCount: number;
  /** Profilbild hinterlegt? */
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
  /** "main" oder "feature". */
  role: string;
}

export interface Track {
  id: number;
  path: string;
  title: string;
  /** Hauptkünstler. */
  artistId: number;
  artistName: string;
  /** Alle Beteiligten, Hauptkünstler zuerst. */
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
  /** Aus der Bibliothek entfernt, im Rückblick aber noch geführt. */
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
  /** Eigenes Bild hinterlegt, sonst entsteht die Kachel aus den Covern. */
  hasCover: boolean;
}

export interface PlaylistFill {
  playlist: Playlist;
  /** Neu angelegt oder eine vorhandene ergänzt? */
  created: boolean;
  /** Wie viele Titel diesmal dazugekommen sind. */
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
  /** Hauptkünstler, mehrere mit Semikolon getrennt. */
  artist: string;
  /** Gastkünstler, mit Semikolon getrennt. */
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
   * Stellen der Warteschlange in der Reihenfolge, in der sie laufen.
   *
   * Ohne Zufallswiedergabe schlicht `0, 1, 2, …`; mit ist es die gewürfelte
   * Folge. Nötig, um zu zeigen, was noch kommt: Bei Zufallswiedergabe steht
   * das Kommende nicht hinter dem laufenden Titel, sondern verstreut.
   */
  order: number[];
  /** Wo in `order` der laufende Titel steht. */
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
  /** Audiodateien im Bibliotheksordner ohne Eintrag in der Datenbank. */
  orphanCount: number;
  orphanSamples: string[];
  /** Titel, deren Datei nicht mehr existiert. */
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
  /** Genius-Seite mit den Lyrics. */
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
  /** Vorbekannte Metadaten, die den Tags der Datei vorgehen. */
  metadata?: TrackMetadata | null;
  /** Ausweichadressen, falls die erste Quelle nichts liefert. */
  fallbacks?: string[];
  /** Bekannte Länge, zu kurze Ergebnisse gelten als Vorschau. */
  expectedDurationMs?: number | null;
  /** Suchbegriff, aus dem der beste Treffer bestimmt wird. */
  matchQuery?: string | null;
  /** Sucheingabe des Nutzers, für die Gegenprobe nach dem Laden. */
  intent?: string | null;
}

export interface DownloadPlan {
  url: string;
  fallbacks: string[];
  matchQuery: string | null;
  /** Wonach gesucht wurde. Grundlage der Gegenprobe nach dem Laden. */
  intent: string | null;
  title: string;
  subtitle: string | null;
  thumbnail: string | null;
  durationMs: number | null;
  source: string;
  metadata: TrackMetadata | null;
  /** Liegt bereits ein Titel dieses Namens vom selben Künstler vor? */
  alreadyInLibrary: boolean;
}

/** Ein Hinweis, dessen Wortlaut die Oberfläche selbst beisteuert. */
export interface PlanHinweis {
  /** Welcher Hinweis; der Wortlaut steht in der Texttabelle. */
  code: string;
  /** Einsetzwerte für die Platzhalter `{0}`, `{1}`, … */
  args: string[];
}

export interface LinkPlan {
  /** Bei `kind === "search"` die Eingabe selbst, sonst der Name des Fundes. */
  label: string;
  /** "search", "link" oder z. B. "spotify-album". */
  kind: string;
  notes: PlanHinweis[];
  /** Gehören die Einträge zusammen? Dann ist „Alle laden“ sinnvoll. */
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
  /** Gesetzt, wenn das Ergebnis nicht zur Sucheingabe passt. */
  warning: string | null;
}

export interface DownloaderStatus {
  ytdlpPath: string | null;
  ytdlpVersion: string | null;
  ffmpegAvailable: boolean;
  /** Für YouTube nötig, sonst 403-Fehler. */
  jsRuntime: string | null;
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
  /** Wie oft der Titel in dieser Woche lief. */
  playCount: number;
  /** Wie lange er dabei insgesamt lief. */
  msPlayed: number;
}

export interface WeeklyMixSummary {
  weekKey: string;
  number: number;
  start: number;
  end: number;
  offset: number;
  trackCount: number;
  /** Alben der meistgehörten Titel, daraus entsteht das Mosaik-Cover. */
  coverAlbumIds: number[];
}

export interface WeeklyMix {
  /** Kalenderwoche, z. B. „2026-KW33“. */
  weekKey: string;
  /**
   * Fortlaufende Nummer ab der ersten Woche mit Hördaten.
   *
   * Der Anzeigename entsteht daraus in `lib/mix.ts`, er ist Text der
   * Oberfläche und folgt ihrer Sprache.
   */
  number: number;
  start: number;
  end: number;
  /** Wie viele Wochen zurück. 0 ist die laufende. */
  offset: number;
  /** Gibt es davor noch eine Woche mit Hördaten? */
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
  /** „grid“ oder „list“. */
  playlistView: string;
  /** „sm“, „md“ oder „lg“. */
  playlistSize: string;
  /** Vor dem Löschen nachfragen. */
  confirmDelete: boolean;
  /** Aus einer geladenen Playlist eine Playlist in der Bibliothek machen. */
  playlistFromDownload: boolean;
  /** Umfang des Rückblicks: „all“, „month“, „year“ oder „off“. */
  wrappedMode: string;
  /** Beim Import fehlende Angaben online nachschlagen. */
  autoFetchImport: boolean;
  /** Sortierung der Bibliothek: „added“, „title“, „artist“, „album“, „year“. */
  librarySort: string;
  /** Selbst gemischte Akzentfarben, mit Komma getrennt. */
  accentCustom: string;
  /** Oberflächensprache: „system“, „de“ oder „en“. */
  language: string;
}

export interface AppPaths {
  database: string;
  downloads: string;
  library: string;
  appData: string | null;
}
