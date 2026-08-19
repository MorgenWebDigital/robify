import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { PageHeader } from "../components/Cards";
import { Cover } from "../components/Cover";
import {
  CheckIcon,
  CloseIcon,
  DownloadIcon,
  SearchIcon,
} from "../components/Icons";
import { Auswahl } from "../components/Auswahl";
import { MetadataForm } from "../components/MetadataEditor";
import { Button, Field, inputClass, Modal } from "../components/Modal";
import { DownloadHinweis } from "../components/Rechtliches";
import { api, errorMessage, fallback, meldungText } from "../lib/api";
import { formatBytes, formatTime, plural } from "../lib/format";
import { useDownloader, type Job } from "../store/downloader";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type {
  DownloaderStatus,
  DownloadPlan,
  DownloadProgress,
  LinkPlan,
  PlanHinweis,
} from "../types";

/* Opus fehlt bewusst: Der eingebaute Player kann es nicht abspielen. */
/**
 * Als Funktion, nicht als feste Liste: Eine Liste auf Modulebene entsteht
 * einmal beim Laden. Wechselt der Nutzer danach die Sprache, baut die App sich
 * zwar neu auf, das Modul aber nicht, und die Beschriftungen blieben in der
 * Anfangssprache stehen.
 */
function formate() {
  return [
    { id: "best", label: t("Beste Qualität") },
    { id: "mp3", label: "MP3" },
    { id: "m4a", label: "M4A / AAC" },
    { id: "flac", label: "FLAC" },
    { id: "vorbis", label: "OGG Vorbis" },
  ];
}

/**
 * Überschrift über der Trefferliste.
 *
 * Bei einer Suche liefert der Rust-Teil nur die Eingabe; den Satz drumherum
 * baut die Oberfläche, damit er ihrer Sprache folgt. Bei einem Link ist das
 * Etikett der Name des Fundes und bleibt, wie er ist.
 */
function planTitel(plan: LinkPlan): string {
  return plan.kind === "search"
    ? t("Treffer für „{0}“", plan.label)
    : plan.label;
}

/**
 * Wortlaut zu einer Hinweis-Kennung aus dem Rust-Teil.
 *
 * Dort steht nur, *welcher* Hinweis gilt; der Satz gehört hierher, weil nur
 * die Oberfläche die eingestellte Sprache kennt.
 */
function hinweisText(hinweis: PlanHinweis): string {
  if (hinweis.code === "spotify-nur-metadaten") {
    return t(
      "Spotify gibt seine Aufnahmen nur verschlüsselt heraus. Übernommen werden die Metadaten; die Audiospur wird passend dazu über die übrigen Quellen geladen.",
    );
  }
  if (hinweis.code === "spotify-grenze") {
    return t(
      "Sichtbar sind höchstens {0} Titel je Playlist; längere Listen brechen hier ab.",
      hinweis.args[0] ?? "",
    );
  }
  if (hinweis.code === "quellen-ausgefallen") {
    return t(
      "{0} antwortete nicht, die Liste ist womöglich unvollständig.",
      hinweis.args[0] ?? "",
    );
  }
  return "";
}

export function DownloaderPage() {
  const settings = useLibrary((s) => s.settings);
  const refresh = useLibrary((s) => s.refresh);
  const reloadPlaylists = useLibrary((s) => s.reloadPlaylists);
  const notify = useUi((s) => s.notify);

  const [status, setStatus] = useState<DownloaderStatus | null>(null);
  const [format, setFormat] = useState("mp3");
  const [importing, setImporting] = useState(false);

  // Trefferliste und laufende Downloads überleben den Tabwechsel, weil sie
  // außerhalb dieser Seite liegen.
  const {
    input,
    plan,
    jobs,
    busy,
    review,
    setInput,
    setPlan,
    setBusy,
    setReview,
    addJob,
    patchJob,
    removeJob,
  } = useDownloader();

  const jobCounter = useRef(0);

  useEffect(() => {
    void api
      .downloaderStatus()
      .then(setStatus)
      .catch((error) => setStatus(fallback(null, t("Werkzeugstatus"))(error)));
  }, []);

  useEffect(() => {
    if (settings) setFormat(settings.downloadFormat);
  }, [settings]);

  /**
   * Ein Feld für alles, das Backend erkennt selbst, ob ein Link eingefügt
   * oder gesucht wurde, und wählt die passende Quelle.
   */
  const submit = async () => {
    const value = input.trim();
    if (!value) return;

    setBusy(true);
    try {
      setPlan(await api.resolveInput(value));
    } catch (error) {
      notify(errorMessage(error), "error");
      setPlan(null);
    } finally {
      setBusy(false);
    }
  };

  const download = async (
    item: Pick<
      DownloadPlan,
      "url" | "fallbacks" | "matchQuery" | "intent" | "metadata" | "durationMs"
    >,
    label: string,
    autoImport: boolean,
  ) => {
    jobCounter.current += 1;
    const id = `job-${Date.now()}-${jobCounter.current}`;
    const job: Job = {
      id,
      label,
      progress: null,
      outcome: null,
      error: null,
      autoImport,
    };
    addJob(job);

    try {
      const outcome = await api.startDownload(id, {
        url: item.url,
        // Liefert die erste Quelle nichts Brauchbares, greifen diese.
        fallbacks: item.fallbacks,
        // Ist die Laufzeit bekannt, sucht das Backend die passendste Aufnahme.
        matchQuery: item.matchQuery,
        // Wonach gesucht wurde, das Backend prüft das Ergebnis dagegen.
        intent: item.intent,
        expectedDurationMs: item.durationMs,
        format,
        quality: settings?.downloadQuality ?? "0",
        embedThumbnail: true,
        metadata: item.metadata,
      });
      patchJob(id, { outcome });

      if (autoImport && !outcome.warning) {
        await api.importDownload(
          outcome.path,
          outcome.metadata,
          outcome.sourceUrl,
          settings?.moveDownloadsIntoLibrary ?? true,
        );
        removeJob(id);
        await refresh();
      } else {
        // Passt das Ergebnis nicht zur Suche, wird auch im Stapel gefragt.
        setReview({ job: { ...job, outcome }, metadata: outcome.metadata });
        notify(
          outcome.warning
            ? t("„{0}“: Das Ergebnis passt nicht zur Suche", label)
            : t("„{0}“ heruntergeladen, Metadaten prüfen", label),
          outcome.warning ? "error" : "success",
        );
      }
    } catch (error) {
      const message = errorMessage(error);
      patchJob(id, { error: message });
      notify(`${label}: ${message}`, "error");
    }
  };

  /**
   * Nacheinander, damit die Quelle nicht mit Anfragen überfahren wird.
   *
   * Was schon in der Bibliothek liegt, wird übersprungen. Das ist der Fall,
   * wenn man einen einzelnen Titel geladen hat und später das Album dazu:
   * Der Stapel muss ihn nicht ein zweites Mal suchen.
   */
  /**
   * Ein Stapel, der keine Albumveröffentlichung ist, ist eine Playlist. Bei
   * einem Album wäre eine gleichnamige Playlist nur eine zweite Fassung
   * dessen, was die Bibliothek ohnehin als Release führt.
   */
  const alsPlaylist =
    Boolean(plan?.batch) &&
    plan?.kind !== "spotify-album" &&
    settings?.playlistFromDownload !== false;

  /**
   * Namensvorschlag aus der Überschrift. Bei Spotify steht davor der
   * Ersteller („Spotify · Today's Top Hits“), der gehört nicht in den Namen
   * der eigenen Playlist.
   */
  const playlistName = (plan?.label ?? "").split(" · ").pop()?.trim() ?? "";

  const downloadAll = async (items: DownloadPlan[], name = "") => {
    const offen = items.filter((item) => !item.alreadyInLibrary);
    const uebersprungen = items.length - offen.length;

    setBusy(true);
    try {
      for (const item of offen) {
        await download(item, item.title, true);
      }

      if (offen.length === 0) {
        notify(t("Alle Titel liegen schon in der Bibliothek"), "success");
      } else {
        notify(
          t("{0} verarbeitet", plural(offen.length, "Titel")) +
            (uebersprungen > 0
              ? t(" · {0} übersprungen, weil schon vorhanden", uebersprungen)
              : ""),
          "success",
        );
      }

      // Die Playlist entsteht aus allen Einträgen, auch den übersprungenen:
      // Die liegen ja bereits in der Bibliothek und gehören genauso hinein.
      // Deshalb läuft dieser Schritt auch dann, wenn nichts zu laden war.
      if (alsPlaylist && name.trim()) {
        try {
          const ergebnis = await api.createPlaylistFromEntries(
            name.trim(),
            items.map((item) => ({
              artist: item.metadata?.artist || item.subtitle || "",
              title: item.metadata?.title || item.title,
            })),
          );
          await reloadPlaylists();
          notify(
            ergebnis.created
              ? t(
                  "Playlist „{0}“ mit {1} angelegt",
                  ergebnis.playlist.name,
                  plural(ergebnis.playlist.trackCount, "Titel"),
                )
              : ergebnis.added > 0
                ? t(
                    "{0} zu „{1}“ ergänzt",
                    plural(ergebnis.added, "Titel"),
                    ergebnis.playlist.name,
                  )
                : t("„{0}“ war schon vollständig", ergebnis.playlist.name),
            "success",
          );
        } catch (error) {
          notify(errorMessage(error), "error");
        }
      }
    } finally {
      setBusy(false);
    }
  };

  const importToLibrary = async () => {
    if (!review?.job.outcome) return;
    setImporting(true);
    try {
      await api.importDownload(
        review.job.outcome.path,
        review.metadata,
        review.job.outcome.sourceUrl,
        settings?.moveDownloadsIntoLibrary ?? true,
      );
      await refresh();
      removeJob(review.job.id);
      setReview(null);
      // Der Titel ist drin, die Eingabe hat ihren Zweck erfüllt. Stehen
      // geblieben war sie ein Rest vom letzten Mal: Wer den nächsten Link
      // einfügen wollte, musste erst den alten von Hand löschen.
      setInput("");
      notify(t("In die Bibliothek übernommen"), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setImporting(false);
    }
  };

  const missingTools = status !== null && !status.ytdlpPath;

  return (
    <div>
      <PageHeader
        eyebrow={t("Neue Musik")}
        title={t("Downloader")}
        subtitle={t(
          "Link einfügen oder einfach suchen, die Quelle findet sich von selbst.",
        )}
      />

      {missingTools && (
        <div className="mb-6 rounded-xl border border-ink-700 bg-ink-900/60 px-5 py-4 text-sm text-mute">
          {t(
            "Beim ersten Download holt sich Robify yt-dlp selbst, rund 30 MB, einmalig. Das dauert einen Moment länger als sonst.",
          )}
        </div>
      )}

      {status && !status.jsRuntime && status.jsRuntimeRelevant && (
        <div className="mb-6 rounded-xl border border-warning/40 bg-warning-soft px-5 py-4 text-sm text-fg/80">
          {t(
            "Es wurde keine JavaScript-Laufzeit gefunden. Ohne sie lehnt YouTube jeden Download mit „403“ ab; die übrigen Quellen bleiben davon unberührt. Abhilfe: Node.js, Deno oder Bun installieren.",
          )}
        </div>
      )}

      {status && !status.ffmpegAvailable && (
        <div className="mb-6 rounded-xl border border-warning/40 bg-warning-soft px-5 py-4 text-sm text-fg/80">
          {t(
            "ffmpeg fehlt. Ohne ffmpeg ist nur das Originalformat („Original“) möglich.",
          )}
        </div>
      )}

      <section className="surface mb-6 p-5">
        <div className="flex items-center gap-3">
          <div className="relative flex-1">
            <SearchIcon
              size={16}
              className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
            />
            <input
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void submit();
                }
              }}
              autoFocus
              /* Kurz gehalten: Auf einem Telefon ist das Feld gut zweihundert
                 Punkte breit, und der lange Satz brach mitten im Wort ab —
                 „Künstler und Titel suchen oder“. Was alles erkannt wird,
                 steht ohnehin darunter. */
              placeholder={t("Suchbegriff oder Link")}
              aria-label={t("Suchbegriff oder Link")}
              className="search-field ps-10"
            />
          </div>
          <Button
            onClick={() => void submit()}
            variant="primary"
            disabled={busy || !input.trim()}
          >
            <SearchIcon size={16} />
            {busy ? t("Moment…") : t("Los")}
          </Button>
        </div>

        <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
          <p className="text-xs text-mute">
            {t(
              "Erkannt werden Links von Spotify, YouTube, SoundCloud, Bandcamp und hunderten weiteren Seiten, auch ganze Alben und Playlists. Ohne Link wird in allen Quellen gleichzeitig gesucht.",
            )}
          </p>
          <div className="flex shrink-0 items-center gap-4">
            <label className="text-xs text-mute" htmlFor="download-format">
              {t("Format")}
            </label>
            <Auswahl
              value={format}
              options={formate()}
              onChange={setFormat}
              label={t("Format")}
              className="w-44"
            />
          </div>
        </div>
      </section>

      {jobs.length > 0 && (
        <section className="mb-6">
          <h2 className="mb-3 text-xl font-semibold tracking-tight">
            {t("Downloads")}
          </h2>
          <ul className="space-y-2">
            {jobs.map((job) => (
              <li key={job.id} className="surface p-4">
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{job.label}</p>
                    {job.error ? (
                      // Der Fehler ist das Wichtigste an einem gescheiterten
                      // Auftrag und darf nicht in einer Zeile verschwinden:
                      // Robify erklärt darin, woran es lag und was hilft, und
                      // hängt die wörtliche Meldung der Quelle an. Abgeschnitten
                      // blieb davon „Die Quelle hat den Zugriff abgelehnt (403).
                      // Das k…“ übrig.
                      <p className="text-xs text-mute">{job.error}</p>
                    ) : (
                      <p className="truncate text-xs text-mute">
                        {job.outcome
                          ? t("Fertig, Metadaten prüfen")
                          : describe(job.progress)}
                      </p>
                    )}
                  </div>

                  {job.outcome ? (
                    <Button
                      onClick={() =>
                        setReview({ job, metadata: job.outcome!.metadata })
                      }
                      variant="primary"
                    >
                      <CheckIcon size={16} />
                      {t("Übernehmen")}
                    </Button>
                  ) : job.error ? (
                    <Button
                      onClick={() => removeJob(job.id)}
                      variant="ghost"
                      aria-label={t("Eintrag entfernen")}
                    >
                      <CloseIcon size={16} />
                    </Button>
                  ) : (
                    <Button
                      onClick={() => void api.cancelDownload(job.id)}
                      variant="outline"
                    >
                      {t("Abbrechen")}
                    </Button>
                  )}
                </div>

                {!job.outcome && !job.error && (
                  <div className="mt-3 h-1 overflow-hidden rounded-full bg-ink-700">
                    <div
                      className="h-full rounded-full transition-[width]"
                      style={{
                        width: `${job.progress?.percent ?? 0}%`,
                        background: "var(--accent)",
                      }}
                    />
                  </div>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      {plan && (
        <section className="surface mb-6 p-5">
          <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <h2 className="truncate text-lg font-semibold">
                {planTitel(plan)}
              </h2>
              <p className="text-xs text-mute">
                {plural(plan.items.length, "Eintrag")}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {plan.batch && (
                <Button
                  onClick={() => void downloadAll(plan.items, playlistName)}
                  variant="primary"
                  disabled={busy}
                >
                  <DownloadIcon size={16} />
                  {t("Alle laden")}
                </Button>
              )}
              <Button
                onClick={() => setPlan(null)}
                variant="ghost"
                aria-label={t("Schließen")}
              >
                <CloseIcon size={16} />
              </Button>
            </div>
          </div>

          {plan.notes.length > 0 && (
            <p className="mb-2 rounded-lg border border-ink-700 bg-ink-900/60 px-3.5 py-2.5 text-xs text-mute">
              {plan.notes.map((hinweis) => hinweisText(hinweis)).join(" ")}
            </p>
          )}

          {/* Jeder Titel wird einzeln gesucht, geladen und umgewandelt. Bei
              einem Album dauert das Minuten, das soll niemanden überraschen. */}
          <p className="mb-4 text-xs text-mute">
            {t(
              "Rechne mit etwa einer halben bis einer Minute je Titel. Robify sucht jeden einzeln in mehreren Quellen, lädt ihn und schreibt die Metadaten hinein. Das Fenster kann dabei offen bleiben.",
            )}
            {plan.items.some((item) => item.alreadyInLibrary) &&
              ` ${t("Bereits vorhandene Titel werden übersprungen.")}`}
            {alsPlaylist &&
              ` ${t("Am Ende entsteht daraus die Playlist „{0}“. Gibt es sie schon, wird sie nur ergänzt.", playlistName)}`}
          </p>

          <ul className="space-y-1">
            {plan.items.map((item, index) => (
              <li
                key={`${item.url}-${index}`}
                className="flex items-center gap-3 rounded-xl p-2 transition hover:bg-ink-800"
              >
                <Cover
                  src={item.thumbnail}
                  alt={item.title}
                  seed={item.title}
                  className="h-11 w-14 shrink-0"
                  rounded="rounded-md"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {item.metadata?.trackNo ? `${item.metadata.trackNo}. ` : ""}
                    {item.title}
                  </p>
                  <p className="flex items-center gap-1.5 truncate text-xs text-mute">
                    <span className="shrink-0 rounded bg-ink-700 px-1.5 py-0.5 text-[11px] tracking-wide uppercase">
                      {item.source}
                    </span>
                    {item.subtitle}
                    {item.alreadyInLibrary && (
                      <span className="shrink-0 rounded bg-ink-700 px-1.5 py-0.5 text-[11px]">
                        {t("schon vorhanden")}
                      </span>
                    )}
                  </p>
                </div>
                <span className="shrink-0 text-xs text-mute tabular-nums">
                  {item.durationMs ? formatTime(item.durationMs) : "unbekannt"}
                </span>
                {/* Die Trefferliste geht weg, sobald einer davon geladen
                    wird. Sie hat ihren Zweck erfüllt, und darunter stand
                    bisher der Auftrag, den man erst durch zehn Treffer
                    hindurch suchen musste. Die Liste kommt mit derselben
                    Suche zurück. */}
                <Button
                  onClick={() => {
                    setPlan(null);
                    void download(item, item.title, false);
                  }}
                  variant={item.alreadyInLibrary ? "ghost" : "outline"}
                  className="shrink-0"
                >
                  <DownloadIcon size={16} />
                  {item.alreadyInLibrary ? t("Trotzdem") : t("Laden")}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <Modal
        open={review !== null}
        title={t("Metadaten prüfen")}
        subtitle={t(
          "Alles übernehmen oder vorher anpassen, dann wandert der Titel in die Bibliothek.",
        )}
        onClose={() => setReview(null)}
        width="max-w-3xl"
        footer={
          <>
            <Button onClick={() => setReview(null)} variant="ghost">
              {t("Später")}
            </Button>
            <Button
              onClick={() => void importToLibrary()}
              variant="primary"
              disabled={importing}
            >
              {importing ? t("Übernehme…") : t("In Bibliothek übernehmen")}
            </Button>
          </>
        }
      >
        {review && (
          <div className="space-y-5">
            {/* Passt das Geladene nicht zur Suche, steht es hier, bevor
                der Titel in die Bibliothek wandert. */}
            {review.job.outcome?.warning && (
              <div className="rounded-xl border border-warning/40 bg-warning-soft px-4 py-3 text-sm">
                <p className="font-medium text-warning">{t("Passt das?")}</p>
                <p className="mt-1 text-fg/80">{review.job.outcome.warning}</p>
              </div>
            )}
            <Field label={t("Quelle")}>
              <input
                readOnly
                value={review.job.outcome?.sourceUrl ?? ""}
                className={`${inputClass} text-mute`}
              />
            </Field>
            <MetadataForm
              value={review.metadata}
              onChange={(metadata) => setReview({ ...review, metadata })}
              durationMs={review.job.outcome?.durationMs}
            />
          </div>
        )}
      </Modal>

      <DownloadHinweis />
    </div>
  );
}

/**
 * Was der Auftrag gerade tut, in einem Satz.
 *
 * Die Zwischenstände kommen aus dem Rust-Teil als Vorlage samt Werten und
 * werden hier übersetzt — vorher standen „Quelle wird gelesen…“ und
 * „Metadaten werden gesucht…“ auch über einer englischen Oberfläche auf
 * Deutsch.
 */
function describe(progress: DownloadProgress | null): string {
  if (!progress) return t("Wird vorbereitet…");
  if (progress.status === "processing")
    return progress.message
      ? meldungText(progress.message)
      : t("Wird umgewandelt…");
  if (progress.status === "starting")
    return progress.message ? meldungText(progress.message) : t("Startet…");

  const parts = [`${Math.round(progress.percent)} %`];
  if (progress.totalBytes) {
    parts.push(
      `${formatBytes(progress.downloadedBytes)} / ${formatBytes(progress.totalBytes)}`,
    );
  }
  if (progress.speedBytes) parts.push(`${formatBytes(progress.speedBytes)}/s`);
  if (progress.etaSeconds)
    parts.push(t("noch {0}", formatTime(progress.etaSeconds * 1000)));
  return parts.join(" · ");
}
