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

/**
 * Auswahllisten als Funktionen, nicht als feste Listen: Eine Liste auf
 * Modulebene entsteht einmal beim Laden. Wechselt der Nutzer danach die
 * Sprache, baut die App sich zwar neu auf, das Modul aber nicht, und die
 * Beschriftungen blieben in der Anfangssprache stehen.
 */
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

/* Opus fehlt bewusst: Der eingebaute Player kann es nicht abspielen. */
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

/**
 * Sprachen zur Auswahl: erst „System", dann alle in ihrer eigenen Schrift.
 *
 * Als Funktion, nicht als feste Liste: „System" ist der einzige Eintrag, der
 * übersetzt wird, und `t` steht erst fest, wenn die Sprache gesetzt ist. Eine
 * Liste auf Modulebene fröre den Wert beim Laden ein.
 *
 * Der Suchtext trägt zusätzlich den deutschen Namen und das Kürzel, wer
 * „Japanisch" oder „ja" tippt, findet 日本語, ohne es schreiben zu können.
 */
function sprachOptionen() {
  return [
    {
      id: "system",
      label: t("System"),
      symbol: "🌐",
      suchtext: "system automatisch",
    },
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
  const [holt, setHolt] = useState(false);
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

  /*
   * yt-dlp altert schneller als Robify.
   *
   * YouTube ändert seinen Abspieler laufend und weist alte Fassungen mit
   * „403“ ab. Auf dem Rechner ist yt-dlp eine Datei, die sich selbst
   * erneuert; auf Android steckt es in der Bibliothek und war dort im Stand
   * vom November 2025 stehen geblieben, acht Monate hinter dem aktuellen.
   * Genau daran scheiterten die Downloads von YouTube auf dem Telefon.
   */
  const werkzeugHolen = async () => {
    setHolt(true);
    try {
      const fassung = await api.ytdlpAktualisieren();
      setWerkzeuge((vorher) =>
        vorher ? { ...vorher, ytdlpVersion: fassung } : vorher,
      );
      notify(t("yt-dlp steht jetzt auf {0}.", fassung), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setHolt(false);
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
          {/* Auf dem Telefon steht der Ordner fest, dort gibt es nichts zu
              wählen. Statt eines Feldes, das nur seinen eigenen Pfad zeigt,
              ein Satz, der sagt, wo die Musik liegt. */}
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
          {/* Der Weg zurück: Im Löschdialog lässt sich die Rückfrage abstellen,
              hier kommt sie wieder. */}
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

        {/* Auf dem Telefon steht in diesem Abschnitt kein Ort mehr, sondern
            nur noch die Sicherung der Datenbank. Dann soll er auch danach
            heißen und nicht nach etwas, das dort nicht mehr steht. */}
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
            {/* Die Pfadzeilen nur dort, wo sie etwas nützen: Auf dem
                Telefon stehen die Orte fest, „Öffnen“ führt zu keinem
                Dateimanager, und der Pfad selbst steht schon oben. */}
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
            <div className="flex flex-wrap items-center gap-3">
              <Button
                onClick={() => void werkzeugHolen()}
                variant="outline"
                disabled={holt}
              >
                {holt ? t("Holt…") : t("yt-dlp aktualisieren")}
              </Button>
              <span className="text-xs text-mute">
                {t(
                  "YouTube weist alte Fassungen mit „403“ ab. Hilft eine Aktualisierung nicht, liegt es an der Quelle.",
                )}
              </span>
            </div>
            <AngabeZeile
              label={t("yt-dlp")}
              value={werkzeuge.ytdlpVersion ?? t("unbekannt")}
            />
          </Section>
        )}

        {/* Nach den Werkzeugen, vor dem Zurücksetzen: Wer bis hierher
            gelesen hat, hat die App eingerichtet und weiß, was ihm
            fehlt. */}
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
    // Mittig zum ganzen Block: Beschriftung und Erklärung gehören zusammen,
    // und das Kästchen gilt für beides. Es steht darum auf halber Höhe
    // zwischen ihnen, nicht auf der Zeile der Überschrift.
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

/**
 * Eine reine Angabe, ohne Knopf.
 *
 * `PathRow` taugt dafür nicht: Es hält die Beschriftung auf fester Breite und
 * bietet „Öffnen“ an. Auf dem Telefon blieb von „2025.11.12“ eine „2“ übrig,
 * und der Knopf hätte versucht, eine Fassungsnummer als Ordner zu öffnen.
 */
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
    /* Auf schmalen Anzeigen steht die Bezeichnung über dem Pfad. Nebenan
       nahm sie feste 208 von 338 Bildpunkten ein; für den Pfad blieben
       sechsunddreißig, und in der Zeile stand nur noch „/…“. */
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
