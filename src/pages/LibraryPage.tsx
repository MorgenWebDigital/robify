import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { PageHeader } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import {
  ChevronDownIcon,
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
  /** `null`, solange noch nicht geladen, sonst blitzt der Leerzustand auf. */
  const [tracks, setTracks] = useState<Track[] | null>(null);
  /** Nur für die Zählung in der Kopfzeile, die Releases haben eigene Seiten. */
  const [albums, setAlbums] = useState<Album[]>([]);
  const [check, setCheck] = useState<LibraryCheck | null>(null);
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);
  const [importing, setImporting] = useState(false);
  const [fragtNachEntfernen, setFragtNachEntfernen] = useState(false);
  const [nichtMehrFragen, setNichtMehrFragen] = useState(false);

  const ordnung = settings?.librarySort ?? "added";
  // Sortiert wird hier statt in der Datenbank: Die Liste liegt ohnehin
  // vollständig vor, und das Umschalten wirkt dann ohne neue Abfrage.
  const sortiert = useMemo(
    () =>
      tracks === null
        ? null
        : [...tracks].sort((a, b) => vergleicheTitel(a, b, ordnung)),
    [tracks, ordnung],
  );

  useEffect(() => {
    let cancelled = false;
    // Kurze Verzögerung, damit die Suche nicht bei jedem Zeichen feuert.
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

  /** Einzelne Dateien statt eines ganzen Ordners. */
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

  /**
   * Ordner und Datenbank laufen im Alltag auseinander: Dateien werden von
   * außen verschoben, Importe brechen ab. Der Abgleich meldet beides und
   * fasst nichts an, bevor der Nutzer zustimmt.
   */
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

  /**
   * Entfernt die Einträge ohne Datei. Gelöscht wird weich, darum steht danach
   * ein Zurück bereit, dieselbe Zusage wie beim Löschen eines einzelnen
   * Titels. Die Datei selbst ist ohnehin schon fort, angefasst wird nichts.
   */
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
        <div className="aktionsfeld relative flex-1">
          <SearchIcon
            size={16}
            className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
          />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("Titel, Künstler oder Album suchen")}
            className="search-field ps-10"
          />
        </div>

        <Auswahl
          value={ordnung}
          options={sortierungen()}
          onChange={(wert) => void saveSetting("librarySort", wert)}
          label={t("Sortierung")}
          className="aktionsfeld w-52 shrink"
        />

        {/* Die Werkzeuge stehen bei dem, worauf sie wirken: der Titelliste.
            Gleiche Bauweise wie die Schalter auf Playlists und Wrapped. */}
        <button
          type="button"
          onClick={() => void runCheck()}
          disabled={importing}
          title={t("Aktualisieren")}
          className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
        >
          <RefreshIcon size={16} />
          <span className="beschriftung truncate">{t("Aktualisieren")}</span>
        </button>

        {/* Ein Knopf, zwei Wege: Ein ganzer Ordner ist der übliche Fall,
            einzelne Dateien braucht man trotzdem. Zwei Knöpfe nebeneinander
            wären dafür zu viel. */}
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
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <FolderIcon size={16} />
              <span className="beschriftung truncate">
                {importing ? t("Importiere…") : t("Importieren")}
              </span>
              {/* Der Pfeil gehört zur Beschriftung: Im Kreis säße er neben dem
                  Ordner und machte aus dem Zeichen ein Gedränge. */}
              <ChevronDownIcon size={14} className="beschriftung" />
            </button>
          )}
        />

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
            className="pill-btn is-raised is-accent aktionsknopf aktionsknopf-kurz shrink-0"
          >
            <PlayIcon size={16} />
            <span className="beschriftung">{t("Von vorn hören")}</span>
          </button>
        )}
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
