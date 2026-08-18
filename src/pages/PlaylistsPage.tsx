import { useState } from "react";
import type { ComponentType } from "react";
import {
  Grid,
  PageHeader,
  PlaylistCard,
  PlaylistRow,
} from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import type { GridSize } from "../components/Cards";
import {
  GridIcon,
  ListIcon,
  PlaylistIcon,
  PlusIcon,
  SizeLargeIcon,
  SizeMediumIcon,
  SizeSmallIcon,
} from "../components/Icons";
import { PlaylistCreateDialog } from "../components/PlaylistCreateDialog";
import { api, errorMessage } from "../lib/api";
import { plural } from "../lib/format";
import { t } from "../lib/i18n";
import { useZiehordnung } from "../lib/ziehordnung";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";

export function PlaylistsPage() {
  const playlists = useLibrary((s) => s.playlists);
  const geladen = useLibrary((s) => s.geladen);
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);

  // Ansicht und Größe bleiben über Neustarts hinweg erhalten.
  const asList = settings?.playlistView === "list";
  const size = (settings?.playlistSize ?? "md") as GridSize;

  const [creating, setCreating] = useState(false);
  const reloadPlaylists = useLibrary((s) => s.reloadPlaylists);
  const notify = useUi((s) => s.notify);

  /**
   * Die neue Ordnung geht sofort nach hinten und wird danach neu geladen.
   *
   * Kein eigener Zwischenzustand für die Anzeige: Die Liste kommt aus dem
   * Speicher, den auch die Seitenleiste liest. Würde hier eine eigene Kopie
   * umsortiert, stünden beide für einen Moment verschieden da.
   */
  const neuOrdnen = (ids: number[]) => {
    void api
      .reorderPlaylists(ids)
      .then(() => reloadPlaylists())
      .catch((fehler) => notify(errorMessage(fehler), "error"));
  };

  const { zieht, luecke, merkmale } = useZiehordnung(
    playlists.map((playlist) => playlist.id),
    neuOrdnen,
  );

  return (
    <div>
      <PageHeader
        actionsRechts
        eyebrow={t("Sammlung")}
        title={t("Playlists")}
        subtitle={plural(playlists.length, "Playlist")}
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Segmented
              options={[
                { id: "grid", label: t("Kacheln"), icon: GridIcon },
                { id: "list", label: t("Liste"), icon: ListIcon },
              ]}
              value={asList ? "list" : "grid"}
              onChange={(value) => void saveSetting("playlistView", value)}
            />
            <Segmented
              options={[
                { id: "sm", label: t("Klein"), icon: SizeSmallIcon },
                { id: "md", label: t("Mittel"), icon: SizeMediumIcon },
                { id: "lg", label: t("Groß"), icon: SizeLargeIcon },
              ]}
              value={size}
              onChange={(value) => void saveSetting("playlistSize", value)}
            />
            <button
              type="button"
              onClick={() => setCreating(true)}
              className="pill-btn is-raised is-accent h-9 min-w-40 px-4 text-sm font-semibold"
            >
              <PlusIcon size={16} />
              {t("Neue Playlist")}
            </button>
          </div>
        }
      />

      {!geladen ? null : playlists.length > 0 && asList ? (
        <ul className="space-y-0.5">
          {playlists.map((playlist, index) => (
            <li
              key={playlist.id}
              {...merkmale(index)}
              className={`relative rounded-lg ${zieht === index ? "opacity-40" : ""}`}
            >
              <Einfuegemarke
                zieht={zieht}
                luecke={luecke}
                index={index}
                anzahl={playlists.length}
              />
              <PlaylistRow playlist={playlist} size={size} />
            </li>
          ))}
        </ul>
      ) : playlists.length === 0 ? (
        <EmptyState
          icon={PlaylistIcon}
          title={t("Noch keine Playlists")}
          text={t(
            "Lege eine an und sammle darin deine Lieblingstitel. Beim Laden einer Playlist aus dem Netz entsteht sie außerdem von selbst.",
          )}
          actions={
            <button
              type="button"
              onClick={() => setCreating(true)}
              className="pill-btn is-raised is-accent h-9 min-w-40 px-4 text-sm font-semibold"
            >
              <PlusIcon size={16} />
              {t("Neue Playlist")}
            </button>
          }
        />
      ) : (
        <Grid size={size}>
          {playlists.map((playlist, index) => (
            <div
              key={playlist.id}
              // Im Gitter zählt links oder rechts, nicht oben oder unten.
              {...merkmale(index, true)}
              className={`relative min-w-0 ${zieht === index ? "opacity-40" : ""}`}
            >
              <Einfuegemarke
                zieht={zieht}
                luecke={luecke}
                index={index}
                anzahl={playlists.length}
                senkrecht
              />
              <PlaylistCard playlist={playlist} size={size} />
            </div>
          ))}
        </Grid>
      )}

      <PlaylistCreateDialog
        open={creating}
        onClose={() => setCreating(false)}
      />
    </div>
  );
}

/**
 * Schalterreihe aus Symbolen. Der Text bleibt als Beschriftung erhalten, für
 * Vorlesewerkzeuge und als Kurzhinweis beim Verweilen mit dem Zeiger.
 */
function Segmented({
  options,
  value,
  onChange,
}: {
  options: {
    id: string;
    label: string;
    icon: ComponentType<{ size?: number }>;
  }[];
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="pill-bar">
      {options.map(({ id, label, icon: Glyph }) => (
        <button
          key={id}
          type="button"
          onClick={() => onChange(id)}
          aria-pressed={value === id}
          aria-label={label}
          title={label}
          className="pill-btn h-7 w-7"
        >
          <Glyph size={16} />
        </button>
      ))}
    </div>
  );
}

/**
 * Zeigt die Lücke, in die der gezogene Eintrag fällt.
 *
 * Ein Strich sagt das genauer als ein Rahmen um einen Eintrag: Der Rahmen
 * ließe offen, ob es davor oder dahinter wird. Am letzten Eintrag steht er
 * zusätzlich an dessen Ende, sonst ließe sich „ganz nach hinten“ gar nicht
 * anzeigen.
 */
function Einfuegemarke({
  zieht,
  luecke,
  index,
  anzahl,
  senkrecht = false,
}: {
  zieht: number | null;
  luecke: number | null;
  index: number;
  anzahl: number;
  /** Im Kachelgitter steht der Strich hochkant zwischen zwei Kacheln. */
  senkrecht?: boolean;
}) {
  if (zieht === null) return null;

  const davor = luecke === index;
  const dahinter = luecke === index + 1 && index === anzahl - 1;
  if (!davor && !dahinter) return null;

  const lage = senkrecht
    ? `top-0 bottom-0 w-0.5 ${davor ? "-start-1" : "-end-1"}`
    : `inset-x-0 h-0.5 ${davor ? "-top-px" : "-bottom-px"}`;

  return (
    <span
      aria-hidden="true"
      className={`pointer-events-none absolute z-10 rounded-full ${lage}`}
      style={{ background: "var(--accent)" }}
    />
  );
}
