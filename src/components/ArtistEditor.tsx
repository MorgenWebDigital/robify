import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage } from "../lib/api";
import { artistImage, dataUrl } from "../lib/cover";
import { useUi } from "../store/ui";
import { useBildFertig } from "./Cover";
import { ArtistIcon, DownloadIcon, SearchIcon } from "./Icons";
import { Button, Field, inputClass, Modal } from "./Modal";
import type { Artist, ArtistCandidate } from "../types";

/** a round artist image falling back to an icon. */
export function ArtistAvatar({
  artist,
  className = "",
  preview,
}: {
  artist: Pick<Artist, "id" | "name" | "hasImage">;
  className?: string;
  /** the preview while editing, not stored yet. */
  preview?: string | null;
}) {
  const [failed, setFailed] = useState(false);
  const src = preview ?? (artist.hasImage ? artistImage(artist.id) : null);
  const bild = useBildFertig(src);

  useEffect(() => setFailed(false), [src]);

  const platzhalter = (
    <span
      className="absolute inset-0 grid place-items-center bg-gradient-to-br from-ink-700 to-ink-850"
      aria-hidden="true"
    >
      <ArtistIcon className="h-1/3 w-1/3 text-mute" size={undefined} />
    </span>
  );

  if (!src || failed) {
    return (
      <span
        className={`relative block overflow-hidden rounded-full ${className}`}
        role="img"
        aria-label={artist.name}
      >
        {platzhalter}
      </span>
    );
  }

  return (
    // placeholder and image lie on top of each other, and the image fades in
    // over it. artist images come from genius and are large, and the hard
    // switch was most visible here
    <span
      className={`relative block overflow-hidden rounded-full ${className}`}
    >
      {platzhalter}
      <img
        src={src}
        alt={artist.name}
        draggable={false}
        ref={bild.pruefen}
        onLoad={bild.melden}
        onError={() => setFailed(true)}
        className={`absolute inset-0 h-full w-full object-cover transition-opacity duration-300 ease-out ${
          bild.fertig ? "opacity-100" : "opacity-0"
        }`}
      />
    </span>
  );
}

export function ArtistEditor({
  artist,
  open,
  onClose,
  onSaved,
}: {
  artist: Artist;
  open: boolean;
  onClose: () => void;
  onSaved: (artist: Artist) => void;
}) {
  const notify = useUi((s) => s.notify);
  const fileInput = useRef<HTMLInputElement>(null);

  const [name, setName] = useState(artist.name);
  const [bio, setBio] = useState(artist.bio ?? "");
  const [image, setImage] = useState<{ base64: string; mime: string } | null>(
    null,
  );
  const [removeImage, setRemoveImage] = useState(false);
  const [saving, setSaving] = useState(false);

  const [candidates, setCandidates] = useState<ArtistCandidate[] | null>(null);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(artist.name);
    setBio(artist.bio ?? "");
    setImage(null);
    setRemoveImage(false);
    setCandidates(null);
  }, [open, artist]);

  const search = async () => {
    setSearching(true);
    try {
      setCandidates(await api.searchArtistsOnline(name || artist.name));
    } catch (error) {
      notify(errorMessage(error), "error");
      setCandidates([]);
    } finally {
      setSearching(false);
    }
  };

  /** takes a suggestion over directly, image included. */
  const apply = async (candidate: ArtistCandidate) => {
    setSaving(true);
    try {
      const updated = await api.applyArtistMetadata(artist.id, candidate);
      notify(t("Angaben zu „{0}“ übernommen", updated.name), "success");
      onSaved(updated);
      onClose();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  const pickFile = (file: File) => {
    const reader = new FileReader();
    reader.onload = () => {
      const [, base64] = String(reader.result ?? "").split(",");
      if (base64) {
        setImage({ base64, mime: file.type || "image/jpeg" });
        setRemoveImage(false);
      }
    };
    reader.readAsDataURL(file);
  };

  const save = async () => {
    setSaving(true);
    try {
      const updated = await api.updateArtist(
        artist.id,
        name,
        bio.trim() || null,
        image?.base64 ?? null,
        image?.mime ?? null,
        removeImage,
      );
      notify(t("Künstler gespeichert"), "success");
      onSaved(updated);
      onClose();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  const preview = removeImage
    ? null
    : dataUrl(image?.base64 ?? null, image?.mime ?? null);

  return (
    <Modal
      open={open}
      title={t("Künstler bearbeiten")}
      subtitle={artist.name}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} variant="ghost">
            {t("Abbrechen")}
          </Button>
          <Button
            onClick={() => void save()}
            variant="primary"
            disabled={saving || !name.trim()}
          >
            {saving ? t("Speichert…") : t("Speichern")}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div className="rounded-xl border border-ink-700 bg-ink-900/60 p-4">
          <p className="mb-2 eyebrow">{t("Angaben online suchen")}</p>
          <div className="flex items-center gap-3">
            <div className="relative min-w-0 flex-1">
              <SearchIcon
                size={16}
                className="pointer-events-none absolute top-1/2 start-3.5 -translate-y-1/2 text-mute"
              />
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void search();
                  }
                }}
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

          {candidates && (
            <ul className="mt-3 max-h-56 space-y-1 overflow-y-auto">
              {candidates.length === 0 && (
                <li className="py-3 text-center text-sm text-mute">
                  {t("Nichts gefunden.")}
                </li>
              )}
              {candidates.map((candidate, index) => (
                <li key={`${candidate.geniusId ?? index}`}>
                  <button
                    type="button"
                    disabled={saving}
                    onClick={() => void apply(candidate)}
                    className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-start transition hover:bg-ink-700 disabled:opacity-50"
                  >
                    {candidate.imageUrl ? (
                      <img
                        src={candidate.imageUrl}
                        alt=""
                        className="h-10 w-10 shrink-0 rounded-full object-cover"
                      />
                    ) : (
                      <span className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-ink-700">
                        <ArtistIcon size={18} className="text-mute" />
                      </span>
                    )}
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium">
                        {candidate.name}
                      </span>
                      <span className="block truncate text-xs text-mute">
                        {candidate.bio ?? candidate.source}
                      </span>
                    </span>
                    <DownloadIcon size={16} className="shrink-0 text-mute" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="grid gap-5 sm:grid-cols-[9rem_1fr]">
          <div className="space-y-2">
            <ArtistAvatar
              artist={{
                ...artist,
                hasImage: removeImage ? false : artist.hasImage,
              }}
              preview={preview}
              className="aspect-square w-full"
            />
            <input
              ref={fileInput}
              type="file"
              accept="image/*"
              hidden
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) pickFile(file);
                event.target.value = "";
              }}
            />
            <Button
              onClick={() => fileInput.current?.click()}
              variant="outline"
              className="w-full"
            >
              {t("Bild wählen")}
            </Button>
            {(artist.hasImage || image) && !removeImage && (
              <Button
                onClick={() => {
                  setImage(null);
                  setRemoveImage(true);
                }}
                variant="ghost"
                className="w-full"
              >
                {t("Entfernen")}
              </Button>
            )}
          </div>

          <div className="space-y-4">
            <Field label={t("Name")}>
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                className={inputClass}
              />
            </Field>
            <Field label={t("Beschreibung")} hint={t("Optional")}>
              <textarea
                value={bio}
                onChange={(event) => setBio(event.target.value)}
                rows={8}
                className={inputClass}
              />
            </Field>
            {artist.sourceUrl && (
              <p className="text-xs text-mute">
                Quelle:{" "}
                <button
                  type="button"
                  onClick={() => void api.openPath(artist.sourceUrl!)}
                  className="underline hover:text-fg"
                >
                  {artist.sourceUrl}
                </button>
              </p>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}
