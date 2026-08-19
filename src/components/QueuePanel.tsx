import { api } from "../lib/api";
import { t } from "../lib/i18n";
import { albumCover } from "../lib/cover";
import { formatTime } from "../lib/format";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import { CloseIcon, PlayingBars, TrashIcon } from "./Icons";
import { Button } from "./Modal";
import { useAusblenden } from "../lib/ausblenden";
import { useSchliesstBeimSeitenwechsel } from "../lib/seitenwechsel";

/** Muss zur Dauer von `.animate-slide-out` im Stylesheet passen. */
const AUSFAHREN_MS = 240;

export function QueuePanel() {
  const queueTracks = usePlayer((s) => s.queueTracks);
  const queueIndex = usePlayer((s) => s.queueIndex);
  const playing = usePlayer((s) => s.playing);
  const queue = usePlayer((s) => s.queue);
  const order = usePlayer((s) => s.order);
  const orderPos = usePlayer((s) => s.orderPos);
  const { queueOpen, setQueueOpen } = useUi();
  const { sichtbar, schliesst } = useAusblenden(queueOpen, AUSFAHREN_MS);
  useSchliesstBeimSeitenwechsel(setQueueOpen);

  /**
   * Der laufende Titel und alles danach, in der Reihenfolge des Abspielens.
   *
   * Was schon lief, verschwindet aus der Liste: Wer einen Titel überspringt,
   * will ihn dort nicht weiter stehen sehen. Aus der Warteschlange entfernt
   * wird er dabei nicht, nur aus der Anzeige, und der Rückwärtsschritt holt
   * ihn wieder hervor.
   *
   * Gegangen wird über `order`, nicht über die Warteschlange selbst: Bei
   * Zufallswiedergabe steht das Kommende nicht hinter dem laufenden Titel,
   * sondern über die ganze Liste verstreut. Läuft nichts, steht alles bevor.
   */
  // Rückfall auf die Warteschlangenreihenfolge, falls noch keine Abspielfolge
  // steht: Besser die Liste in der falschen Reihenfolge als eine leere.
  const folge = order.length > 0 ? order : queue.map((_, stelle) => stelle);
  const kommend = (orderPos === null ? folge : folge.slice(orderPos))
    .map((stelle) => ({ stelle, track: queueTracks[stelle] }))
    .filter((eintrag) => eintrag.track !== undefined);

  if (!sichtbar) return null;

  // Der laufende Titel zählt nicht zur Restzeit, er ist ja angebrochen.
  const remaining = kommend
    .slice(orderPos === null ? 0 : 1)
    .reduce((summe, eintrag) => summe + eintrag.track.durationMs, 0);

  return (
    // Die Hülle trägt die Bewegung und wächst in der Breite; der Inhalt darin
    // behält seine festen 20rem, damit er beim Fahren nicht umbricht.
    <aside
      className={`shrink-0 overflow-hidden ${
        schliesst ? "animate-slide-out" : "animate-slide-in"
      } max-md:absolute max-md:inset-0 max-md:z-40 max-md:!w-auto`}
    >
      {/* Abstände liegen innerhalb der Hülle, nicht an ihr: Sie beschneidet,
          und ein äußerer Rand bliebe beim Einfahren als Lücke stehen. */}
      <div className="h-full pt-2 pe-3 pb-3 ps-px max-md:p-0">
        {/* Dieselbe Bauweise wie der Seiteninhalt daneben: eine abgerundete
            Insel im Rahmen, vertieft und im Inselton. Vorher war es eine
            flache Fläche mit einer Linie links, die neben den abgerundeten
            Nachbarn wie ein Fremdkörper wirkte. */}
        {/* Am Telefon nimmt die Warteschlange das ganze Fenster ein: Eine
            Spalte von zwanzig Zeichen Breite neben einem Inhalt von zehn
            wäre für beides zu wenig. */}
        <div className="sunken-panel flex h-full w-80 flex-col overflow-hidden rounded-xl bg-ink-950 max-md:w-full max-md:rounded-none">
          <header className="flex items-center justify-between border-b border-ink-700 px-4 py-3">
            <div>
              <h2 className="text-sm font-semibold">{t("Warteschlange")}</h2>
              <p className="text-xs text-mute">
                {t(
                  "{0} Titel · noch {1}",
                  kommend.length,
                  formatTime(remaining),
                )}
              </p>
            </div>
            <button
              type="button"
              onClick={() => setQueueOpen(false)}
              aria-label={t("Warteschlange schließen")}
              className="icon-btn"
            >
              <CloseIcon size={18} />
            </button>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {kommend.length === 0 ? (
              <p className="px-3 py-8 text-center text-sm text-mute">
                {t("Die Warteschlange ist leer.")}
              </p>
            ) : (
              <ul className="space-y-0.5">
                {kommend.map(({ stelle: index, track }) => {
                  const isCurrent = index === queueIndex;
                  return (
                    <li
                      key={`${track.id}-${index}`}
                      className={`group flex items-center gap-2.5 rounded-lg p-2 transition hover:bg-ink-800 ${
                        isCurrent ? "bg-ink-800" : ""
                      }`}
                    >
                      <button
                        type="button"
                        onClick={() => void api.playTracks(queue, index)}
                        className="flex min-w-0 flex-1 items-center gap-2.5 text-start"
                      >
                        <Cover
                          src={albumCover(track.albumId)}
                          alt={track.albumTitle}
                          seed={track.albumId}
                          className="h-9 w-9 shrink-0"
                          rounded="rounded"
                        />
                        <span className="min-w-0 flex-1">
                          <span
                            className="block truncate text-sm"
                            style={
                              isCurrent ? { color: "var(--accent)" } : undefined
                            }
                          >
                            {track.title}
                          </span>
                          <span className="block truncate text-xs text-mute">
                            {track.artistName}
                          </span>
                        </span>
                        {isCurrent && playing && <PlayingBars />}
                      </button>
                      <button
                        type="button"
                        onClick={() => void api.queueRemove(index)}
                        aria-label={t("Aus Warteschlange entfernen")}
                        className="icon-btn opacity-0 group-hover:opacity-100"
                      >
                        <TrashIcon size={16} />
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          {kommend.length > 0 && (
            <div className="border-t border-ink-700 p-3">
              <Button
                onClick={() => void api.queueClear()}
                variant="outline"
                className="w-full"
              >
                {t("Warteschlange leeren")}
              </Button>
            </div>
          )}
        </div>
      </div>
    </aside>
  );
}
