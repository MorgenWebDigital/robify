import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage } from "../lib/api";
import { dataUrl } from "../lib/cover";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import { DownloadIcon, SearchIcon } from "./Icons";
import { Auswahl } from "./Auswahl";
import { Button, Field, inputClass, Modal } from "./Modal";
import type { MetadataCandidate, Track, TrackMetadata } from "../types";

export const emptyMetadata: TrackMetadata = {
  title: "",
  artist: "",
  featuredArtists: null,
  album: "",
  albumArtist: null,
  releaseType: null,
  year: null,
  trackNo: null,
  discNo: null,
  genre: null,
  coverBase64: null,
  coverMime: null,
  lyricsSynced: null,
  lyricsPlain: null,
};

interface MetadataFormProps {
  value: TrackMetadata;
  onChange: (value: TrackMetadata) => void;
  /** Dauer hilft beim Finden passender Lyrics. */
  durationMs?: number;
}

/**
 * Formular für alle editierbaren Metadaten. Wird sowohl beim Bearbeiten
 * vorhandener Titel als auch nach einem Download verwendet.
 */
/**
 * Als Funktion, nicht als feste Liste: Eine Liste auf Modulebene entsteht
 * einmal beim Laden. Wechselt der Nutzer danach die Sprache, baut die App sich
 * zwar neu auf, das Modul aber nicht, und die Beschriftungen blieben in der
 * Anfangssprache stehen.
 */
function releaseArten() {
  return [
    { id: "single", label: t("Single") },
    { id: "ep", label: "EP" },
    { id: "album", label: t("Album") },
  ];
}

export function MetadataForm({
  value,
  onChange,
  durationMs,
}: MetadataFormProps) {
  const notify = useUi((s) => s.notify);
  const fileInput = useRef<HTMLInputElement>(null);

  const [query, setQuery] = useState("");
  const [candidates, setCandidates] = useState<MetadataCandidate[] | null>(
    null,
  );
  const [searching, setSearching] = useState(false);
  const [fetchingLyrics, setFetchingLyrics] = useState(false);
  const [enriching, setEnriching] = useState(false);
  const [showLyrics, setShowLyrics] = useState(false);

  const patch = (changes: Partial<TrackMetadata>) =>
    onChange({ ...value, ...changes });

  const search = async () => {
    const term = (query || `${value.artist} ${value.title}`).trim();
    if (!term) return;
    setSearching(true);
    try {
      setCandidates(await api.searchMetadataOnline(term));
    } catch (error) {
      notify(errorMessage(error), "error");
      setCandidates([]);
    } finally {
      setSearching(false);
    }
  };

  const applyCandidate = async (candidate: MetadataCandidate) => {
    // Sofort übernehmen, was der Treffer schon mitbringt …
    const next: TrackMetadata = {
      ...value,
      title: candidate.title || value.title,
      artist: candidate.artist || value.artist,
      featuredArtists: candidate.featuredArtists ?? value.featuredArtists,
      album: candidate.album || value.album,
      albumArtist: candidate.albumArtist ?? value.albumArtist,
      releaseType: candidate.releaseType ?? value.releaseType,
      year: candidate.year ?? value.year,
      trackNo: candidate.trackNo ?? value.trackNo,
      discNo: candidate.discNo ?? value.discNo,
      genre: candidate.genre ?? value.genre,
    };
    onChange(next);
    setCandidates(null);
    setEnriching(true);

    // … und den Rest nachladen: Cover, Lyrics, Release-Art, Titelnummer.
    try {
      const full = await api.enrichCandidate(candidate, durationMs);
      onChange({
        ...next,
        albumArtist: full.albumArtist ?? next.albumArtist,
        releaseType: full.releaseType ?? next.releaseType,
        trackNo: full.trackNo ?? next.trackNo,
        year: full.year ?? next.year,
        genre: full.genre ?? next.genre,
        coverBase64: full.coverBase64 ?? next.coverBase64,
        coverMime: full.coverMime ?? next.coverMime,
        lyricsSynced: full.lyricsSynced ?? next.lyricsSynced,
        lyricsPlain: full.lyricsPlain ?? next.lyricsPlain,
      });
      if (full.lyricsSynced || full.lyricsPlain) {
        notify(
          full.lyricsSynced
            ? t("Zeitsynchrone Lyrics übernommen")
            : t("Lyrics übernommen"),
          "success",
        );
      }
    } catch {
      notify(
        t("Nicht alles konnte geladen werden, der Rest wurde übernommen."),
        "error",
      );
    } finally {
      setEnriching(false);
    }
  };

  const pickCoverFile = (file: File) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? "");
      const [, base64] = result.split(",");
      if (base64)
        patch({ coverBase64: base64, coverMime: file.type || "image/jpeg" });
    };
    reader.readAsDataURL(file);
  };

  const fetchLyrics = async () => {
    if (!value.artist || !value.title) {
      notify(t("Bitte zuerst Künstler und Titel eintragen."), "error");
      return;
    }
    setFetchingLyrics(true);
    try {
      const results = await api.searchLyricsOnline(
        `${value.artist} ${value.title}`,
      );
      const best =
        results.find(
          (r) =>
            r.syncedLyrics &&
            (!durationMs ||
              !r.duration ||
              Math.abs(r.duration * 1000 - durationMs) < 8000),
        ) ??
        results.find((r) => r.syncedLyrics) ??
        results.find((r) => r.plainLyrics);

      if (!best) {
        notify(t("Keine Lyrics gefunden."), "error");
        return;
      }
      patch({ lyricsSynced: best.syncedLyrics, lyricsPlain: best.plainLyrics });
      setShowLyrics(true);
      notify(
        best.syncedLyrics
          ? t("Zeitsynchrone Lyrics übernommen")
          : t("Lyrics übernommen"),
        "success",
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setFetchingLyrics(false);
    }
  };

  return (
    <div className="space-y-5">
      {/* Automatische Suche */}
      <div className="rounded-xl border border-ink-700 bg-ink-900/60 p-4">
        <p className="mb-2 eyebrow">{t("Metadaten automatisch suchen")}</p>
        <div className="flex items-center gap-3">
          <div className="relative min-w-0 flex-1">
            <SearchIcon
              size={16}
              className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
            />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void search();
                }
              }}
              placeholder={`${value.artist || t("Künstler")} ${value.title || t("Titel")}`}
              className="search-field ps-10"
            />
          </div>
          <Button
            onClick={() => void search()}
            variant="outline"
            disabled={searching}
          >
            <SearchIcon size={16} />
            {searching ? t("Suche…") : t("Suchen")}
          </Button>
        </div>

        {enriching && (
          <p className="mt-3 text-xs text-mute">
            {t("Cover, Lyrics und Albumdaten werden geholt…")}
          </p>
        )}

        {candidates && (
          <ul className="mt-3 max-h-56 space-y-1 overflow-y-auto">
            {candidates.length === 0 && (
              <li className="py-3 text-center text-sm text-mute">
                {t("Nichts gefunden.")}
              </li>
            )}
            {candidates.map((candidate, index) => (
              <li key={`${candidate.source}-${index}`}>
                <button
                  type="button"
                  onClick={() => void applyCandidate(candidate)}
                  className="w-full rounded-lg px-3 py-2 text-start text-sm transition hover:bg-ink-700"
                >
                  <span className="block truncate font-medium">
                    {candidate.artist} · {candidate.title}
                    {candidate.featuredArtists && (
                      <span className="text-mute">
                        {" "}
                        feat. {candidate.featuredArtists}
                      </span>
                    )}
                  </span>
                  <span className="block truncate text-xs text-mute">
                    {[
                      candidate.album,
                      candidate.releaseType,
                      candidate.year,
                      candidate.source,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="grid gap-5 sm:grid-cols-[10rem_1fr]">
        {/* Cover.

            Auf einer Handbreite ist die Spalte die ganze Breite, und ein
            quadratisches Bild darin nahm dreihundert Punkte Höhe ein: Vom
            Formular blieb darunter kein Feld mehr sichtbar. Ab `sm` steht es
            wieder in seiner zehn Zeichen breiten Spalte und füllt sie. */}
        <div className="space-y-2">
          <Cover
            src={dataUrl(value.coverBase64, value.coverMime)}
            alt={t("Cover")}
            seed={value.album || value.title}
            className="mx-auto aspect-square w-32 sm:mx-0 sm:w-full"
            rounded="rounded-xl"
          />
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) pickCoverFile(file);
              event.target.value = "";
            }}
          />
          <Button
            onClick={() => fileInput.current?.click()}
            variant="outline"
            className="w-full"
          >
            {t("Cover wählen")}
          </Button>
          {value.coverBase64 && (
            <Button
              onClick={() => patch({ coverBase64: null, coverMime: null })}
              variant="ghost"
              className="w-full"
            >
              {t("Entfernen")}
            </Button>
          )}
        </div>

        {/* Textfelder */}
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="sm:col-span-2">
            <Field label={t("Titel")}>
              <input
                value={value.title}
                onChange={(event) => patch({ title: event.target.value })}
                className={inputClass}
              />
            </Field>
          </div>
          <Field
            label={t("Künstler")}
            hint={t("Mehrere mit Semikolon trennen: A; B")}
          >
            <input
              value={value.artist}
              onChange={(event) => patch({ artist: event.target.value })}
              className={inputClass}
            />
          </Field>
          <Field
            label={t("Gastkünstler (feat.)")}
            hint={t("Mehrere mit Semikolon trennen")}
          >
            <input
              value={value.featuredArtists ?? ""}
              onChange={(event) =>
                patch({ featuredArtists: event.target.value || null })
              }
              className={inputClass}
            />
          </Field>
          <Field
            label={t("Album-Künstler")}
            hint={t("Leer lassen = wie Künstler")}
          >
            <input
              value={value.albumArtist ?? ""}
              onChange={(event) =>
                patch({ albumArtist: event.target.value || null })
              }
              className={inputClass}
            />
          </Field>
          <Field label={t("Album")}>
            <input
              value={value.album}
              onChange={(event) => patch({ album: event.target.value })}
              className={inputClass}
            />
          </Field>
          <Field label={t("Art der Veröffentlichung")}>
            <Auswahl
              // Ohne Album ist „Single“ die richtige Vorauswahl.
              value={
                value.releaseType ?? (value.album.trim() ? "album" : "single")
              }
              options={releaseArten()}
              onChange={(wert) => patch({ releaseType: wert })}
              label={t("Art der Veröffentlichung")}
            />
          </Field>
          <Field label={t("Genre")}>
            <input
              value={value.genre ?? ""}
              onChange={(event) => patch({ genre: event.target.value || null })}
              className={inputClass}
            />
          </Field>
          <Field label={t("Jahr")}>
            <input
              type="number"
              value={value.year ?? ""}
              onChange={(event) =>
                patch({
                  year: event.target.value ? Number(event.target.value) : null,
                })
              }
              className={inputClass}
            />
          </Field>
          <Field label={t("Titelnummer")}>
            <input
              type="number"
              value={value.trackNo ?? ""}
              onChange={(event) =>
                patch({
                  trackNo: event.target.value
                    ? Number(event.target.value)
                    : null,
                })
              }
              className={inputClass}
            />
          </Field>
          <Field label={t("CD-Nummer")}>
            <input
              type="number"
              value={value.discNo ?? ""}
              onChange={(event) =>
                patch({
                  discNo: event.target.value
                    ? Number(event.target.value)
                    : null,
                })
              }
              className={inputClass}
            />
          </Field>
        </div>
      </div>

      {/* Lyrics */}
      <div className="rounded-xl border border-ink-700 bg-ink-900/60 p-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium">{t("Lyrics")}</p>
            <p className="text-xs text-mute">
              {value.lyricsSynced
                ? t("Zeitsynchron hinterlegt")
                : value.lyricsPlain
                  ? t("Als Text hinterlegt")
                  : t("Noch keine Lyrics")}
            </p>
          </div>
          <div className="flex gap-2">
            <Button
              onClick={() => void fetchLyrics()}
              variant="outline"
              disabled={fetchingLyrics}
            >
              <DownloadIcon size={16} />
              {fetchingLyrics ? t("Suche…") : t("Online holen")}
            </Button>
            <Button onClick={() => setShowLyrics((v) => !v)} variant="ghost">
              {showLyrics ? t("Zuklappen") : t("Bearbeiten")}
            </Button>
          </div>
        </div>

        {showLyrics && (
          <div className="mt-4 space-y-3">
            <Field label={t("Zeitsynchron (LRC)")}>
              <textarea
                value={value.lyricsSynced ?? ""}
                onChange={(event) =>
                  patch({ lyricsSynced: event.target.value || null })
                }
                rows={7}
                spellCheck={false}
                className={`${inputClass} font-mono text-xs`}
              />
            </Field>
            <Field label={t("Einfacher Text")}>
              <textarea
                value={value.lyricsPlain ?? ""}
                onChange={(event) =>
                  patch({ lyricsPlain: event.target.value || null })
                }
                rows={5}
                className={inputClass}
              />
            </Field>
          </div>
        )}
      </div>
    </div>
  );
}

/** Modal zum Bearbeiten eines bereits importierten Titels. */
export function MetadataEditorModal({
  track,
  onClose,
  onSaved,
}: {
  track: Track | null;
  onClose: () => void;
  onSaved?: (track: Track) => void;
}) {
  const notify = useUi((s) => s.notify);
  const [metadata, setMetadata] = useState<TrackMetadata | null>(null);
  const [writeToFile, setWriteToFile] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!track) {
      setMetadata(null);
      return;
    }
    let cancelled = false;
    api
      .getTrackMetadata(track.id)
      .then((value) => {
        if (!cancelled) setMetadata(value);
      })
      .catch((error) => notify(errorMessage(error), "error"));
    return () => {
      cancelled = true;
    };
  }, [track, notify]);

  const save = async () => {
    if (!track || !metadata) return;
    setSaving(true);
    try {
      const updated = await api.updateTrackMetadata(
        track.id,
        metadata,
        writeToFile,
      );
      notify(t("Metadaten gespeichert"), "success");
      onSaved?.(updated);
      onClose();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open={Boolean(track)}
      title={t("Metadaten bearbeiten")}
      subtitle={track ? `${track.artistName} · ${track.title}` : undefined}
      onClose={onClose}
      width="max-w-3xl"
      footer={
        <>
          {/* Auf einer Handbreite eine Zeile für sich: Sonst blieb für die
              beiden Knöpfe so wenig übrig, dass „Speichern“ allein in die
              nächste Zeile rutschte und von „Abbrechen“ wegwanderte. */}
          <label className="flex w-full items-center gap-2 text-sm text-mute sm:mr-auto sm:w-auto">
            <input
              type="checkbox"
              checked={writeToFile}
              onChange={(event) => setWriteToFile(event.target.checked)}
              className="check-box"
            />
            {t("In die Audiodatei schreiben")}
          </label>
          <Button onClick={onClose} variant="ghost">
            {t("Abbrechen")}
          </Button>
          <Button
            onClick={() => void save()}
            variant="primary"
            disabled={saving || !metadata}
          >
            {saving ? t("Speichert…") : t("Speichern")}
          </Button>
        </>
      }
    >
      {metadata ? (
        <MetadataForm
          value={metadata}
          onChange={setMetadata}
          durationMs={track?.durationMs}
        />
      ) : (
        <p className="py-10 text-center text-sm text-mute">
          {t("Metadaten werden geladen…")}
        </p>
      )}
    </Modal>
  );
}
