import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../lib/i18n";

// a slim title bar of our own.
//
// the native gnome/gtk window band is switched off (`decorations: false` in
// tauri.conf.json) because it is thick and light and does not match the
// morgenweb design. this bar is deliberately flat (30 px), kept in the design
// of the app and at the same time the drag area
// (`data-tauri-drag-region`) for moving the window. the three buttons on the
// right replace the window controls that fell away
export function TitleBar() {
  const win = getCurrentWindow();

  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="titlebar-title" data-tauri-drag-region>
        {t("Robify")}
      </div>
      <div className="titlebar-controls">
        <button
          className="titlebar-btn"
          onClick={() => void win.minimize()}
          title={t("Minimieren")}
          aria-label={t("Minimieren")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <line
              x1="1"
              y1="5"
              x2="9"
              y2="5"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        </button>
        <button
          className="titlebar-btn"
          onClick={() => void win.toggleMaximize()}
          title={t("Maximieren")}
          aria-label={t("Maximieren")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect
              x="1.5"
              y="1.5"
              width="7"
              height="7"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        </button>
        <button
          className="titlebar-btn titlebar-close"
          onClick={() => void win.close()}
          title={t("Schließen")}
          aria-label={t("Schließen")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <line
              x1="1.5"
              y1="1.5"
              x2="8.5"
              y2="8.5"
              stroke="currentColor"
              strokeWidth="1"
            />
            <line
              x1="8.5"
              y1="1.5"
              x2="1.5"
              y2="8.5"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
