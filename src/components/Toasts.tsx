import { useEffect } from "react";
import { t } from "../lib/i18n";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { CloseIcon } from "./Icons";

const TONE_STYLES = {
  info: "border-ink-600 bg-ink-800",
  success: "border-success/40 bg-success-soft",
  error: "border-danger/40 bg-danger-soft",
} as const;

export function Toasts() {
  const { toasts, dismiss, notify } = useUi();
  const playerError = usePlayer((s) => s.lastError);
  const clearError = usePlayer((s) => s.clearError);

  // Fehler aus der Wiedergabe-Engine als Toast spiegeln.
  useEffect(() => {
    if (!playerError) return;
    notify(playerError, "error");
    clearError();
  }, [playerError, notify, clearError]);

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-28 left-1/2 z-[60] flex w-full max-w-md -translate-x-1/2 flex-col gap-2 px-4">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role="status"
          className={`animate-rise pointer-events-auto flex items-start gap-3 rounded-xl border px-4 py-3 text-sm shadow-xl backdrop-blur ${TONE_STYLES[toast.tone]}`}
        >
          <span className="flex-1">{toast.message}</span>
          {toast.undo && (
            <button
              type="button"
              onClick={() => {
                dismiss(toast.id);
                void toast.undo?.();
              }}
              className="shrink-0 font-semibold underline-offset-2 hover:underline"
            >
              {t("Rückgängig")}
            </button>
          )}
          <button
            type="button"
            onClick={() => dismiss(toast.id)}
            aria-label={t("Meldung schließen")}
            className="text-mute transition hover:text-fg"
          >
            <CloseIcon size={16} />
          </button>
        </div>
      ))}
    </div>
  );
}
