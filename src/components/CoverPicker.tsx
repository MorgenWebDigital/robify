import { useRef } from "react";
import { t } from "../lib/i18n";
import { PlaylistIcon } from "./Icons";
import { Button } from "./Modal";

/** Ein vom Nutzer gewähltes Bild, noch nicht gespeichert. */
export interface CoverChoice {
  base64: string;
  mime: string;
}

/**
 * Bildauswahl für Playlists: Vorschau, Datei wählen, wieder entfernen.
 *
 * Gelesen wird über den Browser als Base64, denselben Weg gehen schon
 * Künstlerbild und Albumcover. Der Dateidialog des Systems bliebe hier ohne
 * Nutzen, weil das Bild ohnehin in die Datenbank wandert und nicht als Pfad
 * gemerkt wird.
 */
export function CoverPicker({
  preview,
  onPick,
  onRemove,
}: {
  /** Anzuzeigendes Bild, oder `null` für den Platzhalter. */
  preview: string | null;
  onPick: (choice: CoverChoice) => void;
  /** Fehlt der Rückruf, lässt sich das Bild nicht entfernen. */
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
