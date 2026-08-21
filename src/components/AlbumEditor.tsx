import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage } from "../lib/api";
import { albumCover, dataUrl } from "../lib/cover";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import { SearchIcon } from "./Icons";
import { Auswahl } from "./Auswahl";
import { Button, Field, inputClass, Modal } from "./Modal";
import type { Album, MetadataCandidate, ReleaseType } from "../types";

// a function and not a fixed list: a list at module level comes into being
// once at load time. does the user switch language afterwards, the app
// rebuilds itself but the module does not, and the labels would stay in the
// starting language
function arten(): { id: ReleaseType; label: string }[] {
  return [
    { id: "single", label: t("Single") },
    { id: "ep", label: "EP" },
    { id: "album", label: t("Album") },
  ];
}

export function AlbumEditor({
  album,
  open,
  onClose,
  onSaved,
}: {
  album: Album;
  open: boolean;
  onClose: () => void;
  onSaved: (album: Album) => void;
}) {
  const notify = useUi((s) => s.notify);
  const fileInput = useRef<HTMLInputElement>(null);

  const [title, setTitle] = useState(album.title);
  const [year, setYear] = useState<string>(
    album.year ? String(album.year) : "",
  );
  const [releaseType, setReleaseType] = useState<string>(album.releaseType);
  const [cover, setCover] = useState<{ base64: string; mime: string } | null>(
    null,
  );
  const [removeCover, setRemoveCover] = useState(false);
  const [saving, setSaving] = useState(false);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (!open) return;
    setTitle(album.title);
    setYear(album.year ? String(album.year) : "");
    setReleaseType(album.releaseType);
    setCover(null);
    setRemoveCover(false);
  }, [open, album]);

  const pickFile = (file: File) => {
    const reader = new FileReader();
    reader.onload = () => {
      const [, base64] = String(reader.result ?? "").split(",");
      if (base64) {
        setCover({ base64, mime: file.type || "image/jpeg" });
        setRemoveCover(false);
      }
    };
    reader.readAsDataURL(file);
  };

  /** searches online for this release and takes cover, year and type over */
  const searchOnline = async () => {
    setSearching(true);
    try {
      const results = await api.searchMetadataOnline(
        `${album.artistName} ${title}`,
      );
      const best: MetadataCandidate | undefined =
        results.find((candidate) => candidate.coverUrl) ?? results[0];
      if (!best) {
        notify(t("Nichts gefunden."), "error");
        return;
      }
      if (best.year) setYear(String(best.year));
      if (best.releaseType) setReleaseType(best.releaseType);
      if (best.coverUrl) {
        const image = await api.fetchCover(best.coverUrl);
        setCover({ base64: image.base64, mime: image.mime });
        setRemoveCover(false);
      }
      notify(t("Angaben von {0} übernommen", best.source), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSearching(false);
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const updated = await api.updateAlbum(
        album.id,
        title,
        year ? Number(year) : null,
        releaseType,
        cover?.base64 ?? null,
        cover?.mime ?? null,
        removeCover,
      );
      notify(t("Release gespeichert"), "success");
      onSaved(updated);
      onClose();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  const preview = removeCover
    ? null
    : (dataUrl(cover?.base64 ?? null, cover?.mime ?? null) ??
      albumCover(album.id));

  return (
    <Modal
      open={open}
      title={t("Release bearbeiten")}
      subtitle={`${album.artistName} · ${album.title}`}
      onClose={onClose}
      width="max-w-xl"
      footer={
        <>
          <Button onClick={onClose} variant="ghost">
            {t("Abbrechen")}
          </Button>
          <Button
            onClick={() => void save()}
            variant="primary"
            disabled={saving || !title.trim()}
          >
            {saving ? t("Speichert…") : t("Speichern")}
          </Button>
        </>
      }
    >
      <div className="grid gap-5 sm:grid-cols-[9rem_1fr]">
        <div className="space-y-2">
          <Cover
            src={preview}
            alt={album.title}
            seed={album.id}
            className="aspect-square w-full"
            rounded="rounded-xl"
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
            {t("Cover wählen")}
          </Button>
          {(album.hasCover || cover) && !removeCover && (
            <Button
              onClick={() => {
                setCover(null);
                setRemoveCover(true);
              }}
              variant="ghost"
              className="w-full"
            >
              {t("Entfernen")}
            </Button>
          )}
        </div>

        <div className="space-y-4">
          <Button
            onClick={() => void searchOnline()}
            variant="outline"
            disabled={searching}
            className="w-full"
          >
            <SearchIcon size={16} />
            {searching ? t("Suche…") : t("Angaben online suchen")}
          </Button>

          <Field label={t("Titel")}>
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label={t("Jahr")}>
            <input
              type="number"
              value={year}
              onChange={(event) => setYear(event.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label={t("Art der Veröffentlichung")}>
            <Auswahl
              value={releaseType}
              options={arten()}
              onChange={setReleaseType}
              label={t("Art der Veröffentlichung")}
            />
          </Field>
          <p className="text-xs text-mute">
            {t(
              "Künstler und Titelnummern änderst du über die Titel selbst, im Kontextmenü unter „Metadaten bearbeiten“.",
            )}
          </p>
        </div>
      </div>
    </Modal>
  );
}
