import { useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage } from "../lib/api";
import { dataUrl } from "../lib/cover";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import { CoverPicker, type CoverChoice } from "./CoverPicker";
import { Button, Field, inputClass, Modal, textareaClass } from "./Modal";
import type { Playlist } from "../types";

// the one dialog for creating a playlist, with cover, name and description.
//
// used from the playlist overview and from the add dialog, so both ways show
// the same thing
export function PlaylistCreateDialog({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  /** receives the freshly created playlist, to fill tracks into it for instance. */
  onCreated?: (playlist: Playlist) => void | Promise<void>;
}) {
  const reloadPlaylists = useLibrary((s) => s.reloadPlaylists);
  const notify = useUi((s) => s.notify);

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [cover, setCover] = useState<CoverChoice | null>(null);
  const [busy, setBusy] = useState(false);

  const schliessen = () => {
    setName("");
    setDescription("");
    setCover(null);
    onClose();
  };

  const create = async () => {
    const wert = name.trim();
    if (!wert) return;
    setBusy(true);
    try {
      const playlist = await api.createPlaylist(
        wert,
        description.trim() || null,
        cover,
      );
      await reloadPlaylists();
      notify(t("Playlist „{0}“ erstellt", wert), "success");
      await onCreated?.(playlist);
      schliessen();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      title={t("Neue Playlist")}
      onClose={schliessen}
      width="max-w-md"
      footer={
        <>
          <Button onClick={schliessen} variant="ghost">
            {t("Abbrechen")}
          </Button>
          <Button
            onClick={() => void create()}
            variant="primary"
            disabled={busy || !name.trim()}
          >
            {t("Anlegen")}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label={t("Cover")} hint={t("Optional")}>
          <CoverPicker
            preview={dataUrl(cover?.base64 ?? null, cover?.mime ?? null)}
            onPick={setCover}
            onRemove={() => setCover(null)}
          />
        </Field>
        <Field label={t("Name")}>
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void create();
              }
            }}
            className={inputClass}
          />
        </Field>
        <Field label={t("Beschreibung")} hint={t("Optional")}>
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            rows={3}
            className={textareaClass}
          />
        </Field>
      </div>
    </Modal>
  );
}
