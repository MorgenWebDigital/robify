import { Link, NavLink, useLocation, useNavigate } from "react-router-dom";
import { PlaylistMosaic } from "./Cards";
import { playlistCover } from "../lib/cover";
import { plural } from "../lib/format";
import { t } from "../lib/i18n";
import { NAV, istHier } from "../lib/navigation";
import { useLibrary } from "../store/library";
import { NotesMark, SettingsIcon } from "./Icons";

export function Sidebar() {
  const playlists = useLibrary((s) => s.playlists);
  const settings = useLibrary((s) => s.settings);
  // the review can be turned off entirely, and then it does not belong in
  // the navigation either
  const eintraege = NAV.filter(
    (item) => item.to !== "/wrapped" || settings?.wrappedMode !== "off",
  );
  const stats = useLibrary((s) => s.stats);
  const scanProgress = useLibrary((s) => s.scanProgress);
  const navigate = useNavigate();
  const { pathname: pfad } = useLocation();

  return (
    <nav className="sunken-panel mt-2 me-0 mb-3 ms-3 hidden w-60 shrink-0 flex-col gap-4 rounded-xl bg-ink-950 p-3 md:flex">
      <button
        type="button"
        onClick={() => navigate("/")}
        className="flex w-full items-center justify-center gap-2 px-2 py-1.5"
      >
        {/* no tile behind it: the mark stands for itself, in the accent colour. */}
        <NotesMark
          size={48}
          className="shrink-0"
          style={{ color: "var(--accent)" }}
        />
        <span className="text-lg font-bold tracking-tight">Robify</span>
      </button>

      <ul className="flex flex-col gap-0.5">
        {eintraege.map((eintrag) => {
          const { to, schluessel, icon: Glyph } = eintrag;
          const hier = istHier(pfad, eintrag);
          return (
            <li key={to}>
              {/* `Link` instead of `NavLink`: the matching runs through
                  `istHier` because the detail pages lie under different
                  addresses than their section. `aria-current` is therefore set
                  here by hand. */}
              <Link
                to={to}
                aria-current={hier ? "page" : undefined}
                className={`nav-item gap-3 px-3 py-2 text-sm font-medium ${
                  hier ? "is-active" : "hover:bg-ink-800"
                }`}
              >
                <Glyph size={18} />
                {t(schluessel)}
              </Link>
            </li>
          );
        })}
      </ul>

      <div className="min-h-0 flex-1 border-t border-ink-700 pt-3">
        <p className="px-3 pb-2 eyebrow">{t("Deine Playlists")}</p>
        <ul className="max-h-full space-y-0.5 overflow-y-auto">
          {playlists.length === 0 && (
            <li className="px-3 py-2 text-xs text-mute">
              {t("Noch keine Playlists.")}
            </li>
          )}
          {playlists.map((playlist) => (
            <li key={playlist.id}>
              <NavLink
                to={`/playlist/${playlist.id}`}
                className={({ isActive }) =>
                  `nav-item gap-2.5 px-2 py-1.5 text-sm ${
                    isActive ? "is-active" : "hover:bg-ink-800"
                  }`
                }
                title={playlist.name}
              >
                <PlaylistMosaic
                  albumIds={playlist.coverAlbumIds}
                  name={playlist.name}
                  size="h-8 w-8"
                  coverSrc={
                    playlist.hasCover ? playlistCover(playlist.id) : null
                  }
                />
                <span className="min-w-0 flex-1 truncate">{playlist.name}</span>
              </NavLink>
            </li>
          ))}
        </ul>
      </div>

      {scanProgress && (
        <div className="rounded-lg border border-ink-700 bg-ink-800 p-3">
          <p className="truncate text-xs text-mute" title={scanProgress.file}>
            {t("Importiere {0}/{1}", scanProgress.current, scanProgress.total)}
          </p>
          <div className="mt-2 h-1 overflow-hidden rounded-full bg-ink-600">
            <div
              className="h-full rounded-full transition-[width]"
              style={{
                width: `${(scanProgress.current / Math.max(scanProgress.total, 1)) * 100}%`,
                background: "var(--accent)",
              }}
            />
          </div>
        </div>
      )}

      <div className="flex items-center justify-between border-t border-ink-700 pt-3">
        <span className="px-2 text-xs text-mute">
          {stats ? plural(stats.trackCount, "Titel") : "…"}
        </span>
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            `nav-item justify-center p-2 ${isActive ? "is-active" : "hover:bg-ink-800"}`
          }
          aria-label={t("Einstellungen")}
        >
          <SettingsIcon size={18} />
        </NavLink>
      </div>
    </nav>
  );
}
