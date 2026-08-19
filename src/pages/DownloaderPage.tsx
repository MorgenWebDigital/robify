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

// the audio formats to choose from. opus is deliberately absent, the
// built-in player cannot play it.
//
// a function and not a fixed list: a list at module level comes into being
// once at load time. does the user switch language afterwards, the app
// rebuilds itself but the module does not, and the labels would stay in the
// starting language
function formate() {
  return [
    { id: "best", label: t("Beste Qualität") },
    { id: "mp3", label: "MP3" },
    { id: "m4a", label: "M4A / AAC" },
    { id: "flac", label: "FLAC" },
    { id: "vorbis", label: "OGG Vorbis" },
  ];
}

// the heading above the result list.
//
// on a search the rust side delivers the input alone, and the ui builds the
// sentence around it so it follows the ui language. with a link the label is
// the name found and stays as it is
function planTitel(plan: LinkPlan): string {
  return plan.kind === "search"
    ? t("Treffer für „{0}“", plan.label)
    : plan.label;
}

// the wording for a hint id coming from the rust side.
//
// only which hint applies stands there, and the sentence belongs here because
// the ui alone knows the selected language
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

  // result list and running downloads survive a tab change because they lie
  // outside this page
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

  // one field for everything, the backend recognises by itself whether a
  // link was pasted or a search typed, and picks the matching source
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
        // where the first source delivers nothing usable, these take hold
        fallbacks: item.fallbacks,
        // where the running time is known the backend picks the best matching recording
        matchQuery: item.matchQuery,
        // what was searched for, the backend checks the result against it
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
        // where the result does not match the search, a batch asks as well
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

  // one after another, so the source is not run over with requests.
  //
  // what already lies in the library is skipped. that is the case where one
  // downloaded a single track and the album to it later: the batch does not
  // have to search it a second time
  // a batch that is no album release is a playlist. with an album a playlist
  // of the same name would only be a second version of what the library
  // carries as a release anyway
  const alsPlaylist =
    Boolean(plan?.batch) &&
    plan?.kind !== "spotify-album" &&
    settings?.playlistFromDownload !== false;

  // a name suggested from the heading. at spotify the creator stands in front
  // of it ("Spotify · Today's Top Hits"), and that does not belong in the name
  // of one's own playlist
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

      // the playlist grows out of every entry, the skipped ones included:
      // those lie in the library already and belong in it just as much. this
      // step therefore runs even where there was nothing to download
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
      // the track is in, and the input has served its purpose. left standing
      // it was a leftover from last time: whoever wanted to paste the next
      // link had to delete the old one by hand first
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
              /* kept short: on a phone the field is a good two hundred points
                 wide, and the long sentence broke off mid-word. what is
                 recognised stands below it anyway. */
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
          {/* the hint stands with the jobs and not above the result list
              alone: that one goes away while downloading, and a bar would
              stand there barely moving for minutes without a word on it. */}
          {jobs.some((job) => !job.outcome && !job.error) && (
            <p className="mb-3 text-xs text-mute">
              {t(
                "Das dauert meist eine halbe bis eine Minute je Titel. Robify sucht ihn in mehreren Quellen, lädt ihn und schreibt die Metadaten hinein.",
              )}
            </p>
          )}
          <ul className="space-y-2">
            {jobs.map((job) => (
              <li key={job.id} className="surface p-4">
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{job.label}</p>
                    {job.error ? (
                      // the error is the most important part of a failed job
                      // and must not disappear into one line: robify explains
                      // in it what went wrong and what helps, and appends the
                      // verbatim message of the source. truncated, all that
                      // was left of it was the first half of a sentence
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

          {/* every track is searched, downloaded and converted separately.
              with an album that takes minutes, and it is to surprise
              nobody. */}
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
                {/* the result list goes away as soon as one of them is being
                    downloaded. it has served its purpose, and the job used to
                    stand below it where one had to look for it through ten
                    hits. the list comes back with the same search. */}
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
            {/* where the download does not match the search, it stands here
                before the track travels into the library. */}
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

// what the job is doing right now, in one sentence.
//
// the intermediate states come from the rust side as a template together with
// its values and are translated here. before that, "reading the source…" and
// "searching the metadata…" stood in german above an english interface too
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
