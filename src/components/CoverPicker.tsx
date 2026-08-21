import { useRef } from "react";
import { t } from "../lib/i18n";
import { PlaylistIcon } from "./Icons";
import { Button } from "./Modal";

/** an image chosen by the user, not stored yet */
export interface CoverChoice {
  base64: string;
  mime: string;
}

// the image picker for playlists: preview, choose a file, remove it again.
//
// it is read through the browser as base64, the same way artist image and
// album cover already go. the file dialog of the system would be of no use
// here because the image travels into the database anyway and is not
// remembered as a path
export function CoverPicker({
  preview,
  onPick,
  onRemove,
}: {
  /** the image to show, or `null` for the placeholder */
  preview: string | null;
  onPick: (choice: CoverChoice) => void;
  /** without the callback the image cannot be removed */
  onRemove?: () => void;
}) {
  const dateiFeld = useRef<HTMLInputElement>(null);

  const lesen = (datei: File) => {
    const leser = new FileReader();
    leser.onload = () => {
      const [, base64] = String(leser.result ?? "").split(",");
      if (base64) onPick({ base64, mime: datei.type || "image/jpeg" });
    };
    leser.readAsDataURL(datei);
  };

  return (
    <div className="flex items-center gap-4">
      {preview ? (
        <img
          src={preview}
          alt=""
          className="h-24 w-24 shrink-0 rounded-lg bg-ink-800 object-cover"
        />
      ) : (
        <div className="grid h-24 w-24 shrink-0 place-items-center rounded-lg bg-ink-700">
          <PlaylistIcon size={28} className="text-mute" />
        </div>
      )}

      <div className="flex min-w-0 flex-col items-start gap-2">
        <input
          ref={dateiFeld}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(event) => {
            const datei = event.target.files?.[0];
            if (datei) lesen(datei);
            event.target.value = "";
          }}
        />
        <Button onClick={() => dateiFeld.current?.click()} variant="outline">
          {t("Bild wählen")}
        </Button>
        {preview && onRemove && (
          <Button onClick={onRemove} variant="outline">
            {t("Entfernen")}
          </Button>
        )}
        <p className="text-xs text-mute">
          {t("Ohne eigenes Bild entsteht die Kachel aus den Covern der Titel.")}
        </p>
      </div>
    </div>
  );
}
