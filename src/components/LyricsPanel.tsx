import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage, fallback } from "../lib/api";
import { activeLineIndex, parseLrc } from "../lib/lrc";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Button, Field, inputClass, Modal } from "./Modal";
import { DownloadIcon, LyricsIcon, PencilIcon } from "./Icons";
import { LyricsSync } from "./LyricsSync";
import type { Lyrics, Track } from "../types";

interface LyricsPanelProps {
  track: Track | null;
  compact?: boolean;
}

/**
 * Eine Lyric-Zeile.
 *
 * Als eigenes, gemerktes Bauteil, weil die Liste an der Wiedergabeposition
 * hängt und damit viermal je Sekunde neu bewertet wird. Ohne das glich React
 * bei jedem Takt sämtliche Zeilen ab, bei einem längeren Text über hundert
 * Knöpfe, für eine Änderung, die immer nur zwei davon betrifft. Sichtbar
 * wurde das als Stocken, sobald sich sonst noch etwas bewegte.
 */
const Zeile = memo(function Zeile({
  text,
  timeMs,
  aktiv,
  compact,
  onSeek,
}: {
  text: string;
  timeMs: number;
  aktiv: boolean;
  compact: boolean;
  onSeek: (timeMs: number) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onSeek(timeMs)}
      className={`w-full text-start transition ${
        aktiv ? "lyrics-active font-semibold" : "text-mute hover:text-fg/80"
      } ${compact ? "text-sm" : "text-lg"}`}
      style={aktiv ? { color: "var(--accent)" } : undefined}
    >
      {text || "\u266a"}
    </button>
  );
});

export function LyricsPanel({ track, compact = false }: LyricsPanelProps) {
  const positionMs = usePlayer((s) => s.positionMs);
  const seek = usePlayer((s) => s.seek);

  const [lyrics, setLyrics] = useState<Lyrics | null>(null);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState(false);
  /**
   * Die Liste selbst, nicht die aktive Zeile.
   *
   * Ein Verweis, der je nach Zustand an einer anderen Zeile hängt, zwänge
   * genau diese Zeilen zum Neuzeichnen und machte das Merken zunichte. Über
   * den Behälter findet sich die richtige Zeile ebenso, per Position.
   */
  const listeRef = useRef<HTMLUListElement>(null);
  const aktiveZeile = () =>
    listeRef.current?.children[aktivRef.current] ?? null;
  /** Der aktive Index, für die Suche oben ohne Neuzeichnen lesbar. */
  const aktivRef = useRef(0);
  // Stabil, sonst bekäme jede Zeile bei jedem Takt eine neue Funktion und
  // das Merken liefe ins Leere.
  const springen = useCallback((timeMs: number) => void seek(timeMs), [seek]);
  /**
   * Läuft die Ansicht der Wiedergabe hinterher? Wer selbst blättert, will
   * lesen, nicht alle paar Sekunden weggerissen werden.
   */
  const [folgt, setFolgt] = useState(true);

  useEffect(() => {
    if (!track) {
      setLyrics(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    api
      .getLyrics(track.id)
      .then((value) => {
        if (!cancelled) setLyrics(value);
      })
      .catch((error) => {
        if (!cancelled) setLyrics(fallback(null, t("Lyrics"))(error));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [track]);

  const lines = useMemo(
    () => (lyrics?.synced ? parseLrc(lyrics.synced) : []),
    [lyrics?.synced],
  );
  const active = lines.length > 0 ? activeLineIndex(lines, positionMs) : -1;
  // Beim Zeichnen mitgeschrieben, damit das Nachführen die Zeile findet, ohne
  // dass ein Verweis an ihr hängt. Reine Ableitung, kein Zustand.
  aktivRef.current = Math.max(0, active);

  /**
   * Ob schon einmal auf die laufende Zeile gesprungen wurde.
   *
   * Das erste Mal geschieht ohne weichen Lauf: Es fällt mit dem Aufgehen der
   * Vollbildansicht zusammen, und zwei Bewegungen zugleich, dazu die
   * Neuberechnung, die jedes Scrollen erzwingt, ließen beide stocken. Später
   * ist die Fläche in Ruhe, dann darf es gleiten.
   */
  const schonPositioniert = useRef(false);

  // Aktive Zeile mittig halten, solange die Ansicht mitläuft.
  useEffect(() => {
    if (!folgt) return;
    aktiveZeile()?.scrollIntoView({
      block: "center",
      behavior: schonPositioniert.current ? "smooth" : "auto",
    });
    schonPositioniert.current = true;
  }, [active, folgt]);

  // Ein neuer Titel fängt wieder von vorne an mitzulaufen, und springt
  // wieder ohne Lauf, weil dabei die ganze Liste ausgetauscht wird.
  useEffect(() => {
    setFolgt(true);
    schonPositioniert.current = false;
  }, [track?.id]);

  const zurueckZurZeile = () => {
    setFolgt(true);
    aktiveZeile()?.scrollIntoView({ block: "center", behavior: "smooth" });
  };

  if (!track) {
    return <Placeholder text={t("Kein Titel ausgewählt.")} />;
  }

  const hasContent = Boolean(lyrics?.synced || lyrics?.plain);

  // Ohne eigene Fläche: Der Text liegt unmittelbar auf der großen Fläche der
  // Vollbild-Ansicht. Ein eigener Kasten ergäbe dort einen zweiten Rahmen
  // wenige Pixel neben dem vorhandenen.
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Ohne Überschrift und Quellenzeile: Der Text soll für sich stehen.
          Das Online-Holen ist in den Bearbeiten-Dialog gewandert. */}
      <div className="mb-1 flex shrink-0 justify-end">
        <Button
          onClick={() => setEditing(true)}
          variant="ghost"
          aria-label={t("Lyrics bearbeiten")}
        >
          <PencilIcon size={16} />
        </Button>
      </div>

      {/* Die Textfläche kippt nach hinten weg; die gerade gesungene Zeile
          kommt wieder nach vorn.

          Eigenes Blättern löst das Mitlaufen: Erkannt wird es an Mausrad,
          Wischen und Ziehen der Bildlaufleiste, nicht am Blätter-Ereignis
          selbst, das käme auch vom eigenen Nachführen. */}
      <div
        className="lyrics-stage relative min-h-0 flex-1 overflow-y-auto pe-1"
        onWheel={() => setFolgt(false)}
        onTouchMove={() => setFolgt(false)}
        onMouseDown={() => setFolgt(false)}
      >
        {loading && <Placeholder text={t("Lyrics werden geladen…")} />}

        {!loading && !hasContent && (
          <Placeholder
            text={t("Für diesen Titel sind noch keine Lyrics hinterlegt.")}
          />
        )}

        {!loading && lines.length > 0 && (
          <ul
            ref={listeRef}
            className={`lyrics-plane ${compact ? "space-y-1.5" : "space-y-2.5"}`}
          >
            {lines.map((line, index) => (
              <li key={`${line.timeMs}-${index}`}>
                <Zeile
                  text={line.text}
                  timeMs={line.timeMs}
                  aktiv={index === active}
                  compact={compact}
                  onSeek={springen}
                />
              </li>
            ))}
          </ul>
        )}

        {!loading && lines.length === 0 && lyrics?.plain && (
          <p
            className={`lyrics-plane whitespace-pre-wrap text-mute ${compact ? "text-sm" : "text-base leading-relaxed"}`}
          >
            {lyrics.plain}
          </p>
        )}
      </div>

      {/* Der Weg zurück. Steht über dem Text, damit er nicht mitblättert. */}
      {!folgt && lines.length > 0 && (
        <div className="pointer-events-none relative">
          <button
            type="button"
            onClick={zurueckZurZeile}
            className="pill-btn is-raised is-accent pointer-events-auto absolute bottom-2 left-1/2 h-9 -translate-x-1/2 px-4 text-sm font-semibold"
          >
            {t("Zur aktuellen Zeile")}
          </button>
        </div>
      )}

      <LyricsEditor
        open={editing}
        track={track}
        lyrics={lyrics}
        onClose={() => setEditing(false)}
        onSaved={(value) => {
          setLyrics(value);
          setEditing(false);
        }}
      />
    </div>
  );
}

function Placeholder({ text }: { text: string }) {
  return (
    <p className="grid h-full min-h-24 place-items-center text-center text-sm text-mute">
      {text}
    </p>
  );
}

function LyricsEditor({
  open,
  track,
  lyrics,
  onClose,
  onSaved,
}: {
  open: boolean;
  track: Track;
  lyrics: Lyrics | null;
  onClose: () => void;
  onSaved: (lyrics: Lyrics) => void;
}) {
  const notify = useUi((s) => s.notify);
  const [synced, setSynced] = useState("");
  const [plain, setPlain] = useState("");
  const [writeToFile, setWriteToFile] = useState(true);
  const [takten, setTakten] = useState(false);
  const [saving, setSaving] = useState(false);
  const [fetching, setFetching] = useState(false);

  /**
   * Lyrics online suchen. Steht hier statt in der Anzeige: Dort soll nur
   * der Text stehen, und wer nachhilft, ist ohnehin schon am Bearbeiten.
   */
  const fetchOnline = async () => {
    setFetching(true);
    try {
      const gefunden = await api.fetchLyricsOnline(track.id);
      setSynced(gefunden.synced ?? "");
      setPlain(gefunden.plain ?? "");
      notify(t("Lyrics gefunden, prüfen und speichern"), "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setFetching(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    setSynced(lyrics?.synced ?? "");
    setPlain(lyrics?.plain ?? "");
  }, [open, lyrics]);

  const save = async () => {
    setSaving(true);
    try {
      await api.saveLyrics(
        track.id,
        synced.trim() || null,
        plain.trim() || null,
        writeToFile,
      );
      const updated = await api.getLyrics(track.id);
      notify(t("Lyrics gespeichert"), "success");
      if (updated) onSaved(updated);
      else onClose();
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open={open}
      title={t("Lyrics bearbeiten")}
      subtitle={`${track.artistName} · ${track.title}`}
      onClose={onClose}
      footer={
        <>
          <Button
            onClick={() => void fetchOnline()}
            variant="outline"
            disabled={fetching}
          >
            <DownloadIcon size={16} />
            {fetching ? t("Suche…") : t("Online holen")}
          </Button>
          <span className="flex-1" />
          <Button onClick={onClose} variant="ghost">
            {t("Abbrechen")}
          </Button>
          <Button
            onClick={() => void save()}
            variant="primary"
            disabled={saving}
          >
            {saving ? t("Speichert…") : t("Speichern")}
          </Button>
        </>
      }
    >
      {takten ? (
        <LyricsSync
          track={track}
          plain={plain}
          onCancel={() => setTakten(false)}
          onDone={(lrc, bereinigt) => {
            setSynced(lrc);
            setPlain(bereinigt);
            setTakten(false);
          }}
        />
      ) : (
        <div className="space-y-4">
          <Field
            label={t("Zeitsynchron (LRC)")}
            hint={t(
              "Format: [mm:ss.xx] Textzeile. Wird im Player mitlaufend hervorgehoben.",
            )}
          >
            <textarea
              value={synced}
              onChange={(event) => setSynced(event.target.value)}
              rows={10}
              spellCheck={false}
              className={`${inputClass} font-mono text-xs`}
              placeholder={t("[00:12.30] Erste Zeile")}
            />
          </Field>
          <Field
            label={t("Einfacher Text")}
            hint={t("Wird genutzt, wenn keine Zeitmarken vorliegen.")}
          >
            <textarea
              value={plain}
              onChange={(event) => setPlain(event.target.value)}
              rows={7}
              className={inputClass}
            />
          </Field>

          {/* Der Weg vom Fließtext zu mitlaufenden Lyrics, wenn online nichts
            Zeitsynchrones zu finden war. */}
          <Button
            onClick={() => setTakten(true)}
            variant="outline"
            disabled={!plain.trim()}
          >
            <LyricsIcon size={16} />
            {t("Zeitmarken selbst setzen")}
          </Button>
          <label className="flex items-center gap-2 text-sm text-mute">
            <input
              type="checkbox"
              checked={writeToFile}
              onChange={(event) => setWriteToFile(event.target.checked)}
              className="check-box"
            />
            {t("Auch in die Audiodatei schreiben")}
          </label>
        </div>
      )}
    </Modal>
  );
}
