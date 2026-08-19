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

/** has to match the duration of `.animate-slide-out` in the stylesheet. */
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

  // the running track and everything after it, in playback order.
  //
  // what has already run disappears from the list: whoever skips a track does
  // not want to keep seeing it there. it is not removed from the queue in
  // doing so, only from the display, and the step backwards brings it out
  // again.
  //
  // it walks `order` and not the queue itself: under shuffle the coming
  // tracks do not stand behind the running one but lie scattered over the
  // whole list. where nothing is running, everything lies ahead
  // falls back to the queue order where no playback order stands yet: better
  // the list in the wrong order than an empty one
  const folge = order.length > 0 ? order : queue.map((_, stelle) => stelle);
  const kommend = (orderPos === null ? folge : folge.slice(orderPos))
    .map((stelle) => ({ stelle, track: queueTracks[stelle] }))
    .filter((eintrag) => eintrag.track !== undefined);

  if (!sichtbar) return null;

  // the running track does not count towards the time left, it is under way
  const remaining = kommend
    .slice(orderPos === null ? 0 : 1)
    .reduce((summe, eintrag) => summe + eintrag.track.durationMs, 0);

  return (
    // the wrapper carries the animation and grows in width, while the content
    // inside keeps its fixed 20rem so it does not reflow while sliding
    <aside
      className={`shrink-0 overflow-hidden ${
        schliesst ? "animate-slide-out" : "animate-slide-in"
      } max-md:absolute max-md:inset-0 max-md:z-40 max-md:!w-auto`}
    >
      {/* the spacing lies inside the wrapper, not on it: it clips, and an
          outer margin would stay standing as a gap while sliding in. */}
      <div className="h-full pt-2 pe-3 pb-3 ps-px max-md:p-0">
        {/* the same build as the page content next to it: a rounded island in
            the frame, sunken and in the island tone. before it was a flat
            surface with a line on the left, which looked like a foreign body
            next to its rounded neighbours. */}
        {/* on a phone the queue takes the whole window: a column twenty
            characters wide next to a content of ten would be too little for
            either. */}
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
