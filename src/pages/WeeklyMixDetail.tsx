import { PlaylistMosaic, ZurueckKnopf } from "../components/Cards";
import { t } from "../lib/i18n";
import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import {
  PlaylistIcon,
  PlayIcon,
  PlusIcon,
  ShuffleIcon,
} from "../components/Icons";
import { TrackList } from "../components/TrackList";
import { api, errorMessage, fallback } from "../lib/api";
import { vollesDatum, zeitraum } from "../lib/datum";
import { formatDuration, plural } from "../lib/format";
import { mixName } from "../lib/mix";
import { useLibrary } from "../store/library";
import { useUi } from "../store/ui";
import type { WeeklyMix } from "../types";

/**
 * Zeitraum der Woche als „12.-18. August 2026“.
 *
 * `zeitraum` kürzt selbst, was sich wiederholt, und setzt das Trennzeichen der
 * Sprache. Vorher stand hier ein von Hand gebautes „{0}. bis {1}“ mit fest
 * deutschen Monatsnamen.
 */
function weekLabel(mix: WeeklyMix): string {
  return zeitraum(new Date(mix.start * 1000), new Date((mix.end - 1) * 1000));
}

export function WeeklyMixDetail() {
  const { offset } = useParams();
  const week = Number(offset ?? 0);
  const revision = useLibrary((s) => s.revision);
  const refresh = useLibrary((s) => s.refresh);
  const { notify, openAddToPlaylist } = useUi();

  const [mix, setMix] = useState<WeeklyMix | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!Number.isFinite(week)) return;
    let cancelled = false;
    void api
      .weeklyMix(week)
      .then((value) => {
        if (!cancelled) setMix(value);
      })
      .catch((error) => {
        if (!cancelled) setMix(fallback(null, t("Wochenmix"))(error));
      });
    return () => {
      cancelled = true;
    };
  }, [week, revision]);

  if (!mix) {
    return (
      <p className="py-20 text-center text-mute">
        {t("Wochenmix wird geladen…")}
      </p>
    );
  }

  const tracks = mix.items.map((item) => item.track);
  const albumIds = [...new Set(tracks.map((track) => track.albumId))];
  const totalMs = tracks.reduce((sum, track) => sum + track.durationMs, 0);

  const play = (shuffle: boolean) => {
    if (tracks.length === 0) return;
    void api.setShuffle(shuffle);
    void api.playTracks(
      tracks.map((track) => track.id),
      0,
    );
  };

  /**
   * Der Mix selbst bleibt ein Rückblick, hiervon gibt es eine Kopie.
   *
   * Name und Beschreibung entstehen hier statt im Rust-Teil: Sie werden in der
   * Datenbank abgelegt und sollen in der Sprache stehen, die beim Übernehmen
   * eingestellt war.
   */
  const save = async () => {
    if (!mix) return;
    setSaving(true);
    try {
      const playlist = await api.saveWeeklyMix(
        week,
        mixName(mix),
        t(
          "Die meistgehörten Titel der Woche {0}, übernommen am {1}.",
          mix.weekKey,
          vollesDatum(new Date()),
        ),
      );
      await refresh();
      notify(
        t("„{0}“ zu deinen Playlists hinzugefügt", playlist.name),
        "success",
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <ZurueckKnopf ziel="/" />
      <div className="mb-8 flex flex-wrap items-end gap-6">
        <PlaylistMosaic
          albumIds={albumIds}
          name={mixName(mix)}
          size="h-44 w-44 shadow-2xl"
        />
        <div className="min-w-0 flex-1">
          <p className="eyebrow">
            {mix.offset === 0 ? t("Diese Woche") : mix.weekKey}
          </p>
          <h1 className="mt-1 text-3xl font-bold tracking-tight">
            {mixName(mix)}
          </h1>
          <p className="mt-2 text-sm text-mute">
            {weekLabel(mix)} · {plural(tracks.length, "Titel")} ·{" "}
            {formatDuration(totalMs)}
          </p>
          {/* Steht hier statt auf der Startseite: Wer den Mix öffnet, will
              wissen, wie er zustande kommt, auf der Übersicht war es nur
              Beiwerk neben der Überschrift. */}
          <p className="mt-1 text-sm text-mute/80">
            {t("Die 30 meistgehörten Titel je Woche")}
          </p>

          {/* Erst was den Mix aufbewahrt, dann was ihn hört. Alle vier klein,
              wie auf den anderen Seiten: Beschriftet lief die Reihe 21
              Bildpunkte über den Rand hinaus, und „In bestehende Playlist“
              war auf 14 Punkte zusammengequetscht.

              Der letzte Knopf hatte kein Zeichen. Zusammengefallen wäre er
              ein leerer Kreis geworden; das Notenblatt steht auch sonst für
              eine Playlist, das Plus für „neu anlegen“. */}
          <div className="aktionsreihe mt-4 gap-2">
            <button
              type="button"
              onClick={() => void save()}
              disabled={saving || !tracks.length}
              title={saving ? t("Wird übernommen…") : t("Zu meinen Playlists")}
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <PlusIcon size={16} />
              <span className="beschriftung truncate">
                {saving ? t("Wird übernommen…") : t("Zu meinen Playlists")}
              </span>
            </button>
            <button
              type="button"
              onClick={() => openAddToPlaylist(tracks.map((track) => track.id))}
              disabled={!tracks.length}
              title={t("In bestehende Playlist")}
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <PlaylistIcon size={16} />
              <span className="beschriftung truncate">
                {t("In bestehende Playlist")}
              </span>
            </button>
            <button
              type="button"
              onClick={() => play(true)}
              disabled={!tracks.length}
              title={t("Zufällig")}
              className="pill-btn is-raised aktionsknopf aktionsknopf-kurz"
            >
              <ShuffleIcon size={16} />
              <span className="beschriftung truncate">{t("Zufällig")}</span>
            </button>
            <button
              type="button"
              onClick={() => play(false)}
              disabled={!tracks.length}
              title={t("Abspielen")}
              className="pill-btn is-raised is-accent aktionsknopf aktionsknopf-kurz"
            >
              <PlayIcon size={16} />
              <span className="beschriftung truncate">{t("Abspielen")}</span>
            </button>
          </div>
        </div>
      </div>

      {tracks.length > 0 ? (
        <TrackList tracks={tracks} onChanged={() => void refresh()} />
      ) : (
        <p className="rounded-xl border border-dashed border-ink-700 px-5 py-10 text-center text-sm text-mute">
          {t("In dieser Woche lief nichts.")}
        </p>
      )}
    </div>
  );
}
