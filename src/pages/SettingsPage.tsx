import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { PageHeader } from "../components/Cards";
import { FolderIcon, TrashIcon } from "../components/Icons";
import { Button, Field, inputClass, Modal } from "../components/Modal";
import { AkzentWahl } from "../components/AkzentWahl";
import { Auswahl } from "../components/Auswahl";
import { t } from "../lib/i18n";
import { SPRACHEN } from "../lib/sprachen";
import { Mitmachen } from "../components/Mitmachen";
import { RechtlichesFuss } from "../components/Rechtliches";
import { api, errorMessage, fallback } from "../lib/api";
import { formatDuration, plural } from "../lib/format";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type { AppPaths, DownloaderStatus } from "../types";

// the choice lists as functions and not as fixed lists: a list at module
// level comes into being once at load time. does the user switch language
// afterwards, the app rebuilds itself but the module does not, and the labels
// would stay in the starting language
function rueckblickModi() {
  return [
    { id: "all", label: t("Alles") },
    { id: "month", label: t("Letzter Monat") },
    { id: "year", label: t("Letztes Jahr") },
    { id: "off", label: t("Aus") },
  ];
}

function erscheinungsbilder() {
  return [
    { id: "system", label: t("System") },
    { id: "light", label: t("Hell") },
    { id: "dark", label: t("Dunkel") },
  ];
}

// opus is deliberately absent, the built-in player cannot play it
function formate() {
  return [
    { id: "best", label: t("Beste Qualität") },
    { id: "mp3", label: "MP3" },
    { id: "m4a", label: "M4A / AAC" },
    { id: "flac", label: "FLAC" },
    { id: "vorbis", label: "OGG Vorbis" },
  ];
}

function qualitaeten() {
  return [
    { id: "0", label: t("Beste") },
    { id: "3", label: t("Hoch") },
    { id: "5", label: t("Mittel") },
    { id: "7", label: t("Klein") },
  ];
}

// the languages to choose from: "system" first, then all of them in their own
// script.
//
// a function and not a fixed list: "system" is the only entry that gets
// translated, and `t` is settled only once the language is set. a list at
// module level would freeze the value at load time.
//
// the search text carries the german name and the code as well, so whoever
// types "Japanisch" or "ja" finds 日本語 without being able to write it
function sprachOptionen() {
  return [
    // without an icon: the flags next to it each stand for one country while
    // the globe would stand for none. it looked like another language and was
    // only the absence of a choice
    { id: "system", label: t("System"), suchtext: "system automatisch" },
    ...SPRACHEN.map((eintrag) => ({
      id: eintrag.id,
      label: eintrag.name,
      symbol: eintrag.flagge,
      suchtext: `${eintrag.deutsch} ${eintrag.id}`,
    })),
  ];
}

export function SettingsPage() {
  const settings = useLibrary((s) => s.settings);
  const stats = useLibrary((s) => s.stats);
  const saveSetting = useLibrary((s) => s.saveSetting);
  const refresh = useLibrary((s) => s.refresh);
  const notify = useUi((s) => s.notify);

  const [paths, setPaths] = useState<AppPaths | null>(null);
  const [werkzeuge, setWerkzeuge] = useState<DownloaderStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [zuruecksetzen, setZuruecksetzen] = useState(false);
  const [dateienLoeschen, setDateienLoeschen] = useState(false);
  const [einstellungenBehalten, setEinstellungenBehalten] = useState(true);

  useEffect(() => {
    void api
      .appPaths()
      .then(setPaths)
      .catch((error) => setPaths(fallback(null, t("Speicherorte"))(error)));
    void api
      .downloaderStatus()
      .then(setWerkzeuge)
      .catch(fallback(null, t("Werkzeuge")));
  }, [settings]);

  if (!settings) {
    return (
      <p className="py-20 text-center text-mute">
        {t("Einstellungen werden geladen…")}
      </p>
    );
  }

  const backup = async () => {
    setSaving(true);
    try {
      const ziel = await api.backupDatabase();
      notify(t("Sicherung angelegt: {0}", ziel), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  const chooseLibraryDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("Zielordner für die Bibliothek"),
      });
      if (typeof selected === "string") {
        await saveSetting("libraryDir", selected);
        notify(t("Bibliotheksordner gespeichert"), "success");
      }
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  };

  const resetAusfuehren = async () => {
    setZuruecksetzen(false);
    setSaving(true);
    try {
      const sicherung = await api.resetApp(
        dateienLoeschen,
        einstellungenBehalten,
      );
      await refresh();
      notify(
        t("Zurückgesetzt. Sicherung liegt unter {0}", sicherung),
        "success",
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mx-auto max-w-3xl">
      <PageHeader eyebrow={t("Konfiguration")} title={t("Einstellungen")} />

      <div className="space-y-6">
        <Section title={t("Bibliothek")}>
          {/* on a phone the folder is fixed and there is nothing to choose.
              instead of a field showing nothing but its own path, a sentence
              saying where the music lies. */}
          {settings.festeOrte ? (
            <div className="space-y-1.5 text-sm text-mute">
              <p>
                {t(
                  "Die Titel liegen im Ordner „Robify“ im Gerätespeicher, alles Übrige im Ordner „.robify“ daneben.",
                )}
              </p>
              <p>
                {t(
                  "Eigene Dateien kannst du in „Robify/Eigene Songs“ ablegen; beim nächsten Start kommen sie dazu.",
                )}
              </p>
            </div>
          ) : (
            <Field
              label={t("Zielordner")}
              hint={t(
                "Hierhin werden heruntergeladene Titel einsortiert (Künstler/Album/Titel).",
              )}
            >
              <div className="flex gap-2">
                <input
                  readOnly
                  value={settings.libraryDir}
                  className={inputClass}
                />
                <Button
                  onClick={() => void chooseLibraryDir()}
                  variant="outline"
                >
                  <FolderIcon size={16} />
                  {t("Wählen")}
                </Button>
              </div>
            </Field>
          )}

          <Toggle
            label={t("Downloads in die Bibliothek verschieben")}
            hint={t("Aus: Dateien bleiben im Download-Ordner liegen.")}
            checked={settings.moveDownloadsIntoLibrary}
            onChange={(value) =>
              void saveSetting("moveDownloadsIntoLibrary", value)
            }
          />

          {stats && (
            <p className="text-sm text-mute">
              {plural(stats.trackCount, "Titel")} ·{" "}
              {plural(stats.artistCount, "Künstler")} ·{" "}
              {plural(stats.albumCount, "Release")} ·{" "}
              {t(
                "{0} Musik, davon {1} gehört",
                formatDuration(stats.totalDurationMs),
                formatDuration(stats.totalListenedMs),
              )}
            </p>
          )}
        </Section>

        <Section title={t("Downloader")}>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label={t("Standardformat")}>
              <Auswahl
                value={settings.downloadFormat}
                options={formate()}
                onChange={(wert) => void saveSetting("downloadFormat", wert)}
                label={t("Standardformat")}
              />
            </Field>
            <Field
              label={t("Qualität")}
              hint={t("Wirkt nur beim Umwandeln, nicht bei „Beste Qualität“.")}
            >
              <Auswahl
                value={settings.downloadQuality}
                options={qualitaeten()}
                onChange={(wert) => void saveSetting("downloadQuality", wert)}
                label={t("Qualität")}
              />
            </Field>
          </div>

          <Toggle
            label={t("Lyrics automatisch suchen")}
            checked={settings.autoFetchLyrics}
            onChange={(value) => void saveSetting("autoFetchLyrics", value)}
          />
          <Toggle
            label={t("Cover automatisch suchen")}
            checked={settings.autoFetchCover}
            onChange={(value) => void saveSetting("autoFetchCover", value)}
          />
          <Toggle
            label={t("Angaben beim Import nachschlagen")}
            hint={t(
              "Dateien ohne brauchbare Tags werden nach dem Einlesen online abgeglichen. Übernommen wird nur, was eindeutig passt.",
            )}
            checked={settings.autoFetchImport}
            onChange={(value) => void saveSetting("autoFetchImport", value)}
          />
          <Toggle
            label={t("Aus geladenen Playlists eine Playlist anlegen")}
            hint={t(
              "Beim Laden einer Playlist entsteht am Ende eine gleichnamige in deiner Bibliothek. Alben sind ausgenommen.",
            )}
            checked={settings.playlistFromDownload}
            onChange={(value) =>
              void saveSetting("playlistFromDownload", value)
            }
          />
          <Toggle
            label={t("Künstlerangaben automatisch holen")}
            hint={t(
              "Beim ersten Titel eines Künstlers werden Profilbild und Beschreibung im Hintergrund nachgeladen.",
            )}
            checked={settings.autoFetchArtists}
            onChange={(value) => void saveSetting("autoFetchArtists", value)}
          />
        </Section>

        <Section title={t("Rückblick")}>
          <Field
            label={t("Wrapped")}
            hint={t(
              "„Letzter Monat“ zeigt im Februar den Januar, „Letztes Jahr“ zeigt 2026 das Jahr 2025, jeweils den abgeschlossenen Zeitraum. Zurückblättern geht weiterhin. „Aus“ blendet den Eintrag in der Seitenleiste aus.",
            )}
          >
            <div className="pill-bar">
              {rueckblickModi().map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => void saveSetting("wrappedMode", item.id)}
                  aria-pressed={settings.wrappedMode === item.id}
                  className="pill-btn h-7 px-3.5 text-sm"
                >
                  {item.label}
                </button>
              ))}
            </div>
          </Field>
        </Section>

        <Section title={t("Sicherheitsabfragen")}>
          {/* the way back: the confirmation can be turned off in the delete
              dialog, and here it comes back. */}
          <Toggle
            label={t("Vor dem Löschen nachfragen")}
            hint={t(
              "Ohne Rückfrage wird sofort gelöscht. Zurücknehmen geht über die Meldung weiterhin.",
            )}
            checked={settings.confirmDelete}
            onChange={(value) => void saveSetting("confirmDelete", value)}
          />
        </Section>

        <Section title={t("Darstellung")}>
          <Field
            label={t("Erscheinungsbild")}
            hint={t("„System“ folgt der Einstellung des Betriebssystems.")}
          >
            <div className="pill-bar">
              {erscheinungsbilder().map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => void saveSetting("theme", item.id)}
                  aria-pressed={settings.theme === item.id}
                  className="pill-btn h-7 px-3.5 text-sm"
                >
                  {item.label}
                </button>
              ))}
            </div>
          </Field>

          <Field
            label={t("Sprache")}
            hint={t("„System“ folgt der Einstellung des Betriebssystems.")}
          >
            <Auswahl
              value={settings.language}
              options={sprachOptionen()}
              onChange={(wert) => void saveSetting("language", wert)}
              label={t("Sprache")}
            />
          </Field>

          <Field
            label={t("Akzentfarbe")}
            hint={t(
              "Über das Plus lässt sich eine eigene Farbe mischen und ablegen.",
            )}
          >
            <AkzentWahl />
          </Field>
        </Section>

        {/* on a phone no location stands in this section any more, only the
            backup of the database. it is then to be named after that and not
            after something that no longer stands there. */}
        {paths && (
          <Section
            title={settings.festeOrte ? t("Datenbank") : t("Speicherorte")}
          >
            <div className="flex flex-wrap items-center gap-3">
              <Button
                onClick={() => void backup()}
                variant="outline"
                disabled={saving}
              >
                {saving ? t("Sichert…") : t("Sicherung anlegen")}
              </Button>
              <span className="text-xs text-mute">
                {t(
                  "Kopiert die Datenbank; die letzten fünf Sicherungen bleiben erhalten.",
                )}
              </span>
            </div>
            {/* the path rows only where they are of use: on a phone the
                locations are fixed, opening leads to no file manager, and the
                path itself stands above already. */}
            {!settings.festeOrte && (
              <>
                <PathRow label={t("Datenbank")} value={paths.database} />
                <PathRow
                  label={t("Download-Arbeitsordner")}
                  value={paths.downloads}
                />
                <PathRow label={t("Bibliothek")} value={paths.library} />
              </>
            )}
          </Section>
        )}

        {werkzeuge && (
          <Section title={t("Werkzeuge")}>
            <div className="flex flex-wrap items-center gap-3"></div>
            <AngabeZeile
              label={t("yt-dlp")}
              value={werkzeuge.ytdlpVersion ?? t("unbekannt")}
            />
          </Section>
        )}

        {/* after the tools, before the reset: whoever has read this far has
            set the app up and knows what they are missing. */}
        <Section title={t("Mitmachen")}>
          <Mitmachen ytdlp={werkzeuge?.ytdlpVersion} />
        </Section>

        <Section title={t("Zurücksetzen")}>
          <div className="flex flex-wrap items-center gap-3">
            <Button
              onClick={() => setZuruecksetzen(true)}
              variant="outline"
              disabled={saving}
            >
              <TrashIcon size={16} />
              {t("Alles zurücksetzen")}
            </Button>
            <span className="text-xs text-mute">
              {t(
                "Leert Bibliothek, Playlists und Hörhistorie. Vorher wird automatisch eine Sicherung angelegt.",
              )}
            </span>
          </div>
        </Section>
      </div>

      <RechtlichesFuss />

      <Modal
        open={zuruecksetzen}
        title={t("Robify zurücksetzen?")}
        onClose={() => setZuruecksetzen(false)}
        width="max-w-md"
        footer={
          <>
            <Button onClick={() => setZuruecksetzen(false)} variant="ghost">
              {t("Abbrechen")}
            </Button>
            <Button onClick={() => void resetAusfuehren()} variant="outline">
              {t("Zurücksetzen")}
            </Button>
          </>
        }
      >
        <div className="space-y-4 text-sm">
          <p className="text-mute">
            {t(
              "Entfernt werden alle Titel, Künstler, Releases, Playlists und die gesamte Hörhistorie.",
            )}
            {t("Der Rückblick fängt danach bei null an.")}
          </p>
          <p className="text-mute">
            {t(
              "Vorher legt Robify eine Sicherung der Datenbank an und nennt dir ihren Pfad. Damit lässt sich der Stand notfalls zurückholen.",
            )}
          </p>

          <label className="flex cursor-pointer items-center gap-2.5">
            <input
              type="checkbox"
              checked={einstellungenBehalten}
              onChange={(event) =>
                setEinstellungenBehalten(event.target.checked)
              }
              className="check-box"
            />
            <span>
              <span className="block">{t("Einstellungen behalten")}</span>
              <span className="block text-xs text-mute">
                {t(
                  "Erscheinungsbild, Akzentfarbe, Downloadformat und die übrigen Schalter bleiben, wie sie sind.",
                )}
              </span>
            </span>
          </label>

          <label className="flex cursor-pointer items-center gap-2.5">
            <input
              type="checkbox"
              checked={dateienLoeschen}
              onChange={(event) => setDateienLoeschen(event.target.checked)}
              className="check-box"
            />
            <span>
              <span className="block">
                {t("Auch die Audiodateien entfernen")}
              </span>
              <span className="block text-xs text-mute">
                {t(
                  "Ohne Haken bleiben deine Dateien liegen und lassen sich später erneut importieren.",
                )}
                {t(
                  "Mit Haken wandern sie in den Papierkorb von Robify und werden dort nach 30 Tagen endgültig gelöscht.",
                )}
              </span>
            </span>
          </label>
        </div>
      </Modal>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="surface p-5">
      <h2 className="mb-4 text-lg font-semibold tracking-tight">{title}</h2>
      <div className="space-y-4">{children}</div>
    </section>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    // centred on the whole block: label and explanation belong together, and
    // the checkbox applies to both. it therefore stands at half height
    // between them, not on the line of the heading
    <label className="flex cursor-pointer items-center gap-3">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="check-box"
      />
      <span className="min-w-0">
        <span className="block text-sm">{label}</span>
        {hint && <span className="block text-xs text-mute">{hint}</span>}
      </span>
    </label>
  );
}

// a plain value, without a button.
//
// `PathRow` does not do for it: it keeps the label at a fixed width and
// offers to open the value. on a phone a single "2" was left of "2025.11.12",
// and the button would have tried to open a version number as a folder
function AngabeZeile({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
      <span className="text-sm text-mute">{label}</span>
      <code className="rounded bg-ink-900 px-2 py-1 text-xs">{value}</code>
    </div>
  );
}

function PathRow({ label, value }: { label: string; value: string }) {
  const notify = useUi((s) => s.notify);
  return (
    /* on narrow displays the label stands above the path. beside it, it took
       a fixed 208 of 338 pixels, leaving thirty-six for the path, and only a
       slash and an ellipsis stood in the row. */
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      <span className="w-full shrink-0 text-sm text-mute sm:w-52">{label}</span>
      <code
        className="min-w-0 flex-1 truncate rounded bg-ink-900 px-2 py-1 text-xs"
        title={value}
      >
        {value}
      </code>
      <Button
        onClick={() =>
          void api
            .openPath(value)
            .catch((error) => notify(errorMessage(error), "error"))
        }
        variant="outline"
      >
        {t("Öffnen")}
      </Button>
    </div>
  );
}
