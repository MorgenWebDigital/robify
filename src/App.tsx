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
  // the downloader listens for its progress events from here, so they arrive
  // while another page is open as well
  const initDownloader = useDownloader((s) => s.init);
  const { editingTrack, editTrack } = useUi();
  const openAddToPlaylist = useUi((s) => s.openAddToPlaylist);
  const location = useLocation();

  // what lies over the page goes with the page.
  //
  // these two dialogs hang off the app, not off the page that opened them.
  // "add to playlist" from an album therefore stayed standing when one tapped
  // library at the bottom, a dialog over a page it has nothing to do with.
  // the same held for the metadata editor.
  //
  // both setters come out of the store and stay the same, and the
  // `useCallback` keeps the dependency of this call stable too, otherwise it
  // would close the dialog again at the next render
  useSchliesstBeimSeitenwechsel(
    useCallback(() => {
      openAddToPlaylist(null);
      editTrack(null);
    }, [openAddToPlaylist, editTrack]),
  );
  // keeps `--tastatur` current so the player and the bar stay above the
  // keyboard instead of behind it
  useTastaturhoehe();

  const blaetterbereich = useRef<HTMLElement>(null);

  // every page starts at the top.
  //
  // the scrolling area stays where it was on a page change. whoever stood far
  // down in the library and then opened a playlist landed in the middle of
  // its track list, which looked like a jump rather than a change
  useEffect(() => {
    blaetterbereich.current?.scrollTo({ top: 0 });
  }, [location.pathname]);

  useEffect(() => {
    // the cleanup functions come back only after the await
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
      {/* on a phone there are no window buttons to operate, and the bar would
          lie under the status bar of the system. the safe area at the top is
          kept clear by the frame itself instead. */}
      <div className="hidden md:block">
        <TitleBar />
      </div>
      <div className="relative flex min-h-0 flex-1">
        <Sidebar />

        {/* a calm canvas instead of a gradient: in the morgenweb design the
            depth comes from the cards on it, not from the background.

            the surface lies in the window frame as a rounded island. the
            rounding becomes visible only because frame (title bar, sidebar,
            player) and content carry different tones.

            surface and scrolling area are kept apart so the rounding does not
            travel along while scrolling. the scrollbar stands inside on the
            surface. */}
        <div className="relative min-w-0 flex-1 md:mt-2 md:me-3 md:mb-3 md:ms-3">
          <div
            className="sunken-panel absolute inset-0 bg-ink-950 md:rounded-xl"
            aria-hidden="true"
          />
          <main
            ref={blaetterbereich}
            className="relative h-full overflow-y-auto"
          >
            {/* the key on the path rebuilds this area at every change. that
                is worth two things at once: the fade-in runs again, and an
                error left standing is cleared out along the way. */}
            <div
              key={location.pathname}
              className="animate-page mx-auto max-w-6xl px-4 py-4 pb-6 md:px-6 md:py-6 md:pb-10"
            >
              {/* the net sits around the page content, not around the whole
                  app: does a page stumble, sidebar and player stay operable,
                  the music keeps running, and the way back is one click
                  away. */}
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
