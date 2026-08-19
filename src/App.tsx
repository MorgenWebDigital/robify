import { useCallback, useEffect, useRef } from "react";
import { t } from "./lib/i18n";
import { Route, Routes, useLocation } from "react-router-dom";
import { AddToPlaylistDialog } from "./components/AddToPlaylistDialog";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { MetadataEditorModal } from "./components/MetadataEditor";
import { NowPlaying } from "./components/NowPlaying";
import { PlayerBar } from "./components/PlayerBar";
import { QueuePanel } from "./components/QueuePanel";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Unterleiste } from "./components/Unterleiste";
import { Toasts } from "./components/Toasts";
import { AlbumDetail } from "./pages/AlbumDetail";
import { ArtistDetail } from "./pages/ArtistDetail";
import { ArtistsPage } from "./pages/ArtistsPage";
import { DownloaderPage } from "./pages/DownloaderPage";
import { Home } from "./pages/Home";
import { LibraryPage } from "./pages/LibraryPage";
import { MixesPage } from "./pages/MixesPage";
import { PlaylistDetail } from "./pages/PlaylistDetail";
import { FavoritesPage } from "./pages/FavoritesPage";
import { PlaylistsPage } from "./pages/PlaylistsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { WeeklyMixDetail } from "./pages/WeeklyMixDetail";
import { WrappedPage } from "./pages/WrappedPage";
import { useDownloader } from "./store/downloader";
import { useLibrary } from "./store/library";
import { usePlayer } from "./store/player";
import { useUi } from "./store/ui";
import { useSchliesstBeimSeitenwechsel } from "./lib/seitenwechsel";
import { useTastaturhoehe } from "./lib/tastatur";

export function App() {
  const initPlayer = usePlayer((s) => s.init);
  const initLibrary = useLibrary((s) => s.init);
  // Der Downloader hört seine Fortschrittsmeldungen von hier aus, damit sie
  // auch ankommen, während eine andere Seite offen ist.
  const initDownloader = useDownloader((s) => s.init);
  const { editingTrack, editTrack } = useUi();
  const openAddToPlaylist = useUi((s) => s.openAddToPlaylist);
  const location = useLocation();

  /**
   * Was über der Seite liegt, geht mit der Seite.
   *
   * Diese beiden Dialoge hängen an der App, nicht an der Seite, die sie
   * geöffnet hat. „Zu Playlist hinzufügen“ vom Album blieb darum stehen, wenn
   * man unten auf Bibliothek tippte — ein Dialog über einer Seite, mit der er
   * nichts zu tun hat. Dasselbe galt für den Metadaten-Editor.
   *
   * Beide Setzer kommen aus dem Zustandsspeicher und bleiben dieselben; das
   * `useCallback` hält auch die Abhängigkeit dieses Aufrufs stabil, sonst
   * schlösse er den Dialog beim nächsten Neuzeichnen gleich wieder.
   */
  useSchliesstBeimSeitenwechsel(
    useCallback(() => {
      openAddToPlaylist(null);
      editTrack(null);
    }, [openAddToPlaylist, editTrack]),
  );
  // Hält `--tastatur` aktuell, damit Player und Leiste über der Tastatur
  // bleiben statt dahinter.
  useTastaturhoehe();

  const blaetterbereich = useRef<HTMLElement>(null);

  /**
   * Jede Seite beginnt oben.
   *
   * Der Blätterbereich bleibt beim Seitenwechsel stehen, wo er war. Wer weit
   * unten in der Bibliothek stand und dann eine Playlist öffnete, landete
   * mitten in deren Titelliste, das sah nach einem Sprung aus, nicht nach
   * einem Wechsel.
   */
  useEffect(() => {
    blaetterbereich.current?.scrollTo({ top: 0 });
  }, [location.pathname]);

  useEffect(() => {
    // Die Aufräumfunktionen kommen erst nach dem await zurück.
    const pending = Promise.all([
      initPlayer(),
      initLibrary(),
      initDownloader(),
    ]);
    return () => {
      void pending.then((cleanups) => cleanups.forEach((off) => off()));
    };
  }, [initPlayer, initLibrary, initDownloader]);

  return (
    <div className="app-rahmen flex h-full flex-col overflow-hidden">
      {/* Am Telefon gibt es keine Fensterknöpfe zu bedienen, und die Leiste
          läge unter der Statusleiste des Systems. Der sichere Bereich oben
          wird stattdessen vom Rahmen selbst freigehalten. */}
      <div className="hidden md:block">
        <TitleBar />
      </div>
      <div className="relative flex min-h-0 flex-1">
        <Sidebar />

        {/* Ruhiger Canvas statt Verlauf: Die Tiefe entsteht im MorgenWeb-Design
            aus den Karten darauf, nicht aus dem Hintergrund.

            Die Fläche liegt als abgerundete Insel im Fensterrahmen. Sichtbar
            wird die Rundung nur, weil Rahmen (Titelleiste, Seitenleiste,
            Player) und Inhalt verschiedene Töne tragen.

            Fläche und Blätterbereich sind getrennt, damit die Rundung beim
            Blättern nicht mitwandert. Die Bildlaufleiste steht innen auf der
            Fläche. */}
        <div className="relative min-w-0 flex-1 md:mt-2 md:me-3 md:mb-3 md:ms-3">
          <div
            className="sunken-panel absolute inset-0 bg-ink-950 md:rounded-xl"
            aria-hidden="true"
          />
          <main
            ref={blaetterbereich}
            className="relative h-full overflow-y-auto"
          >
            {/* Der Schlüssel am Pfad baut diesen Bereich bei jedem Wechsel
                neu auf. Das ist gleich zweierlei wert: Die Einblendung läuft
                erneut an, und ein stehengebliebener Fehler wird dabei
                abgeräumt. */}
            <div
              key={location.pathname}
              className="animate-page mx-auto max-w-6xl px-4 py-4 pb-6 md:px-6 md:py-6 md:pb-10"
            >
              {/* Das Netz sitzt um den Seiteninhalt, nicht um die ganze App:
                  Stolpert eine Seite, bleiben Seitenleiste und Player
                  bedienbar, die Musik läuft weiter, und der Weg zurück ist
                  einen Klick entfernt. */}
              <ErrorBoundary scope={t("Diese Seite")}>
                <Routes>
                  <Route path="/" element={<Home />} />
                  <Route path="/library" element={<LibraryPage />} />
                  <Route path="/artists" element={<ArtistsPage />} />
                  <Route path="/artist/:id" element={<ArtistDetail />} />
                  <Route path="/album/:id" element={<AlbumDetail />} />
                  <Route path="/playlists" element={<PlaylistsPage />} />
                  <Route path="/favorites" element={<FavoritesPage />} />
                  <Route path="/playlist/:id" element={<PlaylistDetail />} />
                  <Route path="/mixes" element={<MixesPage />} />
                  <Route path="/mix/:offset" element={<WeeklyMixDetail />} />
                  <Route path="/wrapped" element={<WrappedPage />} />
                  <Route path="/downloader" element={<DownloaderPage />} />
                  <Route path="/settings" element={<SettingsPage />} />
                  <Route
                    path="*"
                    element={
                      <p className="py-20 text-center text-mute">
                        {t("Seite nicht gefunden.")}
                      </p>
                    }
                  />
                </Routes>
              </ErrorBoundary>
            </div>
          </main>
        </div>

        <QueuePanel />
        <NowPlaying />
      </div>

      <PlayerBar />
      <Unterleiste />

      <Toasts />
      <AddToPlaylistDialog />
      <MetadataEditorModal
        track={editingTrack}
        onClose={() => editTrack(null)}
      />
    </div>
  );
}
