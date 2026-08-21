import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { api, errorMessage, fallback } from "../lib/api";
import { activeLineIndex, parseLrc } from "../lib/lrc";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Button, Field, Modal, textareaClass } from "./Modal";
import { DownloadIcon, LyricsIcon, PencilIcon } from "./Icons";
import { LyricsSync } from "./LyricsSync";
import type { Lyrics, Track } from "../types";

interface LyricsPanelProps {
  track: Track | null;
  compact?: boolean;
}

// one lyric line.
//
// a memoised component of its own, because the list hangs off the playback
// position and is therefore re-evaluated four times a second. without it
// react diffed every line on each tick, over a hundred buttons with a longer
// text, for a change that only ever concerns two of them. it became visible
// as stutter as soon as anything else moved
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
   * the list itself, not the active line.
   *
   * a ref hanging off a different line depending on state would force exactly
   * those lines to re-render and undo the memoising. through the container
   * the right line is found just as well, by position.
   */
  const listeRef = useRef<HTMLUListElement>(null);
  const aktiveZeile = () =>
    listeRef.current?.children[aktivRef.current] ?? null;
  /** the active index, readable for the lookup above without a re-render */
  const aktivRef = useRef(0);
  // stable, otherwise every line would get a new function on each tick and
  // the memoising would come to nothing
  const springen = useCallback((timeMs: number) => void seek(timeMs), [seek]);
  /**
   * whether the view follows the playback. whoever scrolls themselves wants
   * to read, not to be torn away every few seconds.
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
  // written along while rendering so the follow-up finds the line without a
  // ref hanging off it. pure derivation, no state
  aktivRef.current = Math.max(0, active);

  /**
   * whether the running line has been jumped to once already.
   *
   * the first time happens without easing: it coincides with the full screen
   * view opening, and two animations at once, plus the reflow every scroll
   * forces, made both stutter. later the surface is at rest and it may glide.
   */
  const schonPositioniert = useRef(false);

  // keep the active line centred while the view follows
  useEffect(() => {
    if (!folgt) return;
    aktiveZeile()?.scrollIntoView({
      block: "center",
      behavior: schonPositioniert.current ? "smooth" : "auto",
    });
    schonPositioniert.current = true;
  }, [active, folgt]);

  // a new track starts following from the top again, and jumps without
  // easing again because the whole list is exchanged in doing so
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

  // no surface of its own: the text lies straight on the large surface of the
  // full screen view. a box of its own would make a second frame a few pixels
  // next to the existing one
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* no heading and no source line: the text is to stand for itself.
          fetching it online has travelled into the edit dialog. */}
      <div className="mb-1 flex shrink-0 justify-end">
        <Button
          onClick={() => setEditing(true)}
          variant="ghost"
          aria-label={t("Lyrics bearbeiten")}
        >
          <PencilIcon size={16} />
        </Button>
      </div>

      {/* the text surface tips away to the back, and the line being sung
          comes forward again.

          scrolling by hand releases the following: it is recognised by wheel,
          swipe and dragging the scrollbar, not by the scroll event itself,
          which would come from the follow-up too. */}
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

      {/* the way back. stands above the text so it does not scroll along. */}
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

  // search lyrics online. lives here rather than in the display: only the
  // text is to stand there, and whoever helps along is editing anyway
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
              className={`${textareaClass} font-mono text-xs`}
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
              className={textareaClass}
            />
          </Field>

          {/* the way from running text to lyrics that follow along, where
            nothing time-synced could be found online. */}
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
