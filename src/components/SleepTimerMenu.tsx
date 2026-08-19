import { formatTime } from "../lib/format";
import { t } from "../lib/i18n";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { MoonIcon } from "./Icons";
import { Menu } from "./Menu";

const PRESETS = [5, 10, 15, 30, 45, 60, 90];

export function SleepTimerMenu() {
  const sleepTimer = usePlayer((s) => s.sleepTimer);
  const setSleepTimer = usePlayer((s) => s.setSleepTimer);
  const notify = useUi((s) => s.notify);

  const active = sleepTimer !== null;
  const remaining =
    sleepTimer?.mode === "duration" ? formatTime(sleepTimer.remainingMs) : null;

  const start = async (minutes: number) => {
    await setSleepTimer("duration", minutes);
    notify(t("Sleeptimer läuft: {0} Minuten", minutes), "success");
  };

  return (
    <div className="flex items-center gap-1.5">
      <Menu
        align="right"
        // the player sits at the very bottom, there would be no room below
        side="top"
        items={[
          ...PRESETS.map((minutes) => ({
            label: t("{0} Minuten", minutes),
            onSelect: () => void start(minutes),
          })),
          {
            label: t("Am Ende des Titels"),
            onSelect: () => {
              void setSleepTimer("endOfTrack");
              notify(t("Sleeptimer: stoppt nach diesem Titel"), "success");
            },
          },
          ...(active
            ? [
                {
                  label: t("Timer abbrechen"),
                  tone: "danger" as const,
                  onSelect: () => {
                    void setSleepTimer(null);
                    notify(t("Sleeptimer abgebrochen"));
                  },
                },
              ]
            : []),
        ]}
        trigger={({ open, toggle }) => (
          <button
            type="button"
            onClick={toggle}
            aria-label={t("Sleeptimer")}
            title={active ? t("Sleeptimer aktiv") : t("Sleeptimer")}
            aria-pressed={active}
            aria-expanded={open}
            // two reasons to light up: while a timer is running, and while
            // the menu stands open. the second one was missing, and without a
            // timer set the button gave no feedback at all on a tap, unlike
            // queue and lyrics next to it
            className={`pill-btn is-raised h-9 w-9 ${active || open ? "is-on" : ""}`}
          >
            <MoonIcon size={18} />
          </button>
        )}
      />

      {active && (
        <span
          className="text-xs tabular-nums"
          style={{ color: "var(--accent)" }}
        >
          {remaining ?? "Titelende"}
        </span>
      )}
    </div>
  );
}
