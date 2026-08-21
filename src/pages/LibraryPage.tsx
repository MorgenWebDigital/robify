// the library: every track, sortable, searchable, importable.
// note: the sorting and the view survive a restart, they live in the
// settings.

import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { PageHeader } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import {
  FolderIcon,
  PlayIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
} from "../components/Icons";
import { Auswahl } from "../components/Auswahl";
import { Menu } from "../components/Menu";
import { Button, Modal } from "../components/Modal";
import { TrackList } from "../components/TrackList";
import { api, errorMessage, fallback } from "../lib/api";
import { plural } from "../lib/format";
import { t } from "../lib/i18n";
import { sortierungen, vergleicheTitel } from "../lib/sort";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type { Album, LibraryCheck, Track } from "../types";

export function LibraryPage() {
  const revision = useLibrary((s) => s.revision);
  const refresh = useLibrary((s) => s.refresh);
  const notify = useUi((s) => s.notify);
  const notifyUndo = useUi((s) => s.notifyUndo);

  const [search, setSearch] = useState("");
  /** `null` while not loaded yet, otherwise the empty state flashes up */
  const [tracks, setTracks] = useState<Track[] | null>(null);
  /** for the count in the header only, the releases have pages of their own */
  const [albums, setAlbums] = useState<Album[]>([]);
  const [check, setCheck] = useState<LibraryCheck | null>(null);
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);
  const [importing, setImporting] = useState(false);
  const [fragtNachEntfernen, setFragtNachEntfernen] = useState(false);
  const [nichtMehrFragen, setNichtMehrFragen] = useState(false);

  const ordnung = settings?.librarySort ?? "added";
  // sorted here instead of in the database: the list lies complete anyway,
  // and switching then takes effect without a new query
  const sortiert = useMemo(
    () =>
      tracks === null
        ? null
        : [...tracks].sort((a, b) => vergleicheTitel(a, b, ordnung)),
    [tracks, ordnung],
  );

  useEffect(() => {
    let cancelled = false;
    // a short delay so the search does not fire on every character
    const timer = window.setTimeout(
      () => {
        void Promise.all([
          api.listTracks(search || undefined).catch(fallback([], t("Titel"))),
          api
            .listAlbums(search || undefined)
            .catch(fallback([], t("Releases"))),
        ]).then(([trackValue, albumValue]) => {
          if (cancelled) return;
          setTracks(trackValue);
          setAlbums(albumValue);
        });
      },
      search ? 220 : 0,
    );

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [search, revision]);

  const importFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: true,
        title: t("Musikordner wählen"),
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];

      setImporting(true);
      const result = await api.scanFolders(paths);
      await refresh();
      notify(
        `${plural(result.imported, "Titel importiert")}` +
          (result.skipped > 0 ? t(" · {0} übersprungen", result.skipped) : ""),
        "success",
      );
      if (result.errors.length > 0) {
        notify(result.errors[0], "error");
      }
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setImporting(false);
    }
  };

  /** single files instead of a whole folder */
  const importFiles = async () => {
    try {
      const selected = await open({
        multiple: true,
        title: t("Audiodateien wählen"),
        filters: [
          {
            name: "Audio",
            extensions: [
              "mp3",
              "flac",
              "m4a",
              "aac",
              "ogg",
              "opus",
              "wav",
              "aiff",
              "wv",
            ],
          },
        ],
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];

      setImporting(true);
      const result = await api.scanFolders(paths);
      await refresh();
      notify(plural(result.imported, "Titel importiert"), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setImporting(false);
    }
  };

  // folder and database drift apart in daily use: files are moved from
  // outside, imports break off. the reconciliation reports both and touches
  // nothing before the user agrees
  const runCheck = async () => {
    try {
      const result = await api.checkLibrary();
      setCheck(result);
      if (result.orphanCount === 0 && result.missingCount === 0) {
        notify(t("Bibliothek und Datenbank stimmen überein"), "success");
      }
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  const importOrphans = async () => {
    if (!settings) return;
    setImporting(true);
    try {
      const result = await api.scanFolders([settings.libraryDir]);
      await refresh();
      setCheck(null);
      notify(plural(result.imported, "Titel nachgetragen"), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setImporting(false);
    }
  };

  // removes the rows without a file. the deletion is soft, so an undo stands
  // ready afterwards, the same promise as when deleting a single track. the
  // file itself is gone anyway and nothing is touched
  const removeMissing = async () => {
    setFragtNachEntfernen(false);
    if (nichtMehrFragen) {
      setNichtMehrFragen(false);
      void saveSetting("confirmDelete", false);
    }
    try {
      const entfernt = await api.removeMissingTracks();
      await refresh();
      setCheck(null);
      if (entfernt.length === 0) {
        notify(t("Nichts zu entfernen"), "info");
        return;
      }
      notifyUndo(plural(entfernt.length, "Eintrag entfernt"), async () => {
        try {
          for (const id of entfernt) await api.restoreTrack(id);
          await refresh();
          notify(plural(entfernt.length, "Eintrag zurückgeholt"), "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        }
      });
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  return (
    <div>
      <PageHeader
        eyebrow={t("Sammlung")}
        title={t("Bibliothek")}
        subtitle={`${plural(tracks?.length ?? 0, "Titel")} · ${plural(albums.length, "Release")}`}
      />

      {check && (check.orphanCount > 0 || check.missingCount > 0) && (
        <div className="surface mb-5 p-4 text-sm">
          <p className="font-medium">
            {t("Abgleich mit dem Bibliotheksordner")}
          </p>
          {check.orphanCount > 0 && (
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <span className="text-mute">
                {t(
                  "{0}, ohne Eintrag: {1}",
                  plural(check.orphanCount, "Datei liegt im Ordner"),
                  check.orphanSamples.join(", "),
                )}
                {check.orphanCount > check.orphanSamples.length && " …"}
              </span>
              <Button
                onClick={() => void importOrphans()}
                variant="outline"
                disabled={importing}
              >
                {t("Nachtragen")}
              </Button>
            </div>
          )}
          {check.missingCount > 0 && (
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <span className="text-mute">
                {plural(check.missingCount, "Titel ohne Datei")}:{" "}
                {check.missingSamples.join(", ")}
                {check.missingCount > check.missingSamples.length && " …"}
              </span>
              <Button
                onClick={() =>
                  settings && !settings.confirmDelete
                    ? void removeMissing()
                    : setFragtNachEntfernen(true)
                }
                variant="outline"
              >
                {t("Einträge entfernen")}
              </Button>
            </div>
          )}
        </div>
      )}

      <div className="aktionsreihe mb-5 gap-3">
        {/* the search first, then the tools, all three as icons alone: arrow,
            folder and triangle speak for themselves, while "refresh",
            "import" and "play from the top" next to each other sound like an
            announcement.

            the sorting stands at the other end: it is no action but the order
            of the list below, and with its width it would stand in the way
            between the icons. */}
        {/* the search as a button that slides out on tapping. without
            `aktionsfeld`: its minimum width does shrink under pressure but
            ends at 7rem and held the field exactly there, so the circle never
            came about. `suchfeld-kurz` handles the width itself, in both
            states. */}
        <div className="suchfeld-kurz relative">
          <SearchIcon
            size={16}
            className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
          />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("Titel, Künstler oder Album suchen")}
            aria-label={t("Titel, Künstler oder Album suchen")}
            className="search-field ps-10"
          />
        </div>
        {/* the tools stand with what they act on: the track list. the same
            build as the switches on playlists and the review. */}
        <button
          type="button"
          onClick={() => void runCheck()}
          disabled={importing}
          title={t("Aktualisieren")}
          className="pill-btn is-raised aktionsknopf aktionsknopf-symbol"
        >
          <RefreshIcon size={16} />
          <span className="beschriftung truncate">{t("Aktualisieren")}</span>
        </button>
        {/* one button, two ways: a whole folder is the usual case, and
            single files are needed all the same. two buttons side by side
            would be too much for it.

            on a phone there is only one way: android has no folder picker
            tauri could offer, and the call ended there literally with "Folder
            picker is not implemented on mobile". the button is then not to
            unfold a menu in which one of two options is certain to fail, it
            leads straight to the file picker. whole folders come in through
            the "Eigene Songs" folder there. */}
        {settings?.festeOrte ? (
          <button
            type="button"
            onClick={() => void importFiles()}
            disabled={importing}
            title={importing ? t("Importiere…") : t("Dateien wählen")}
            className="pill-btn is-raised aktionsknopf aktionsknopf-symbol"
          >
            <FolderIcon size={16} />
            <span className="beschriftung truncate">
              {importing ? t("Importiere…") : t("Dateien wählen")}
            </span>
          </button>
        ) : (
          <Menu
            align="left"
            items={[
              {
                label: t("Ordner wählen …"),
                icon: <FolderIcon size={16} />,
                onSelect: () => void importFolder(),
              },
              {
                label: t("Einzelne Dateien wählen …"),
                icon: <PlusIcon size={16} />,
                onSelect: () => void importFiles(),
              },
            ]}
            trigger={({ toggle }) => (
              <button
                type="button"
                onClick={toggle}
                disabled={importing}
                title={importing ? t("Importiere…") : t("Importieren")}
                className="pill-btn is-raised aktionsknopf aktionsknopf-symbol"
              >
                <FolderIcon size={16} />
                <span className="beschriftung truncate">
                  {importing ? t("Importiere…") : t("Importieren")}
                </span>
              </button>
            )}
          />
        )}
        {sortiert && sortiert.length > 0 && (
          <button
            type="button"
            onClick={() =>
              void api.playTracks(
                sortiert.map((t) => t.id),
                0,
              )
            }
            title={t("Von vorn hören")}
            className="pill-btn is-raised is-accent aktionsknopf aktionsknopf-symbol shrink-0"
          >
            <PlayIcon size={16} />
            <span className="beschriftung">{t("Von vorn hören")}</span>
          </button>
        )}
        <Auswahl
          value={ordnung}
          options={sortierungen()}
          onChange={(wert) => void saveSetting("librarySort", wert)}
          label={t("Sortierung")}
          className="aktionsfeld ms-auto w-52 shrink"
        />
      </div>

      {sortiert !== null && (
        <TrackList
          tracks={sortiert}
          onChanged={() => void refresh()}
          emptyMessage={
            search ? (
              t("Nichts passt zu „{0}“.", search)
            ) : (
              <EmptyState
                icon={FolderIcon}
                title={t("Noch keine Musik")}
                text={t(
                  "Importiere einen Ordner mit vorhandenen Dateien oder lade Titel über den Downloader. Beides landet in derselben Bibliothek.",
                )}
                actions={
                  <>
                    <button
                      type="button"
                      onClick={() => void importFolder()}
                      disabled={importing}
                      className="pill-btn is-raised is-accent h-9 px-4 text-sm font-semibold"
                    >
                      <FolderIcon size={16} />
                      {t("Ordner importieren")}
                    </button>
                    <Link
                      to="/downloader"
                      className="pill-btn is-raised h-9 px-4 text-sm font-semibold"
                    >
                      {t("Zum Downloader")}
                    </Link>
                  </>
                }
              />
            )
          }
        />
      )}

      <Modal
        open={fragtNachEntfernen}
        onClose={() => setFragtNachEntfernen(false)}
        title={t("Einträge entfernen?")}
        subtitle={
          check ? plural(check.missingCount, "Titel ohne Datei") : undefined
        }
        width="max-w-md"
        footer={
          <>
            <Button
              onClick={() => setFragtNachEntfernen(false)}
              variant="ghost"
            >
              {t("Abbrechen")}
            </Button>
            <Button onClick={() => void removeMissing()} variant="outline">
              {t("Entfernen")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-mute">
          {t(
            "Diese Titel verschwinden aus der Bibliothek. Ihre Dateien sind ohnehin nicht mehr da, angefasst wird also nichts auf der Platte. Über die Meldung lässt sich der Griff kurz danach zurücknehmen, samt Favoriten und Playlist-Zugehörigkeit.",
          )}
        </p>
        <label className="mt-4 flex cursor-pointer items-center gap-2.5 text-sm text-mute">
          <input
            type="checkbox"
            checked={nichtMehrFragen}
            onChange={(event) => setNichtMehrFragen(event.target.checked)}
            className="check-box"
          />
          {t("Nicht mehr nachfragen")}
        </label>
      </Modal>
    </div>
  );
}
