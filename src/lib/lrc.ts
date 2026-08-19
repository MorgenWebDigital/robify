export interface LyricLine {
  timeMs: number;
  text: string;
}

const TIME_TAG = /\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/g;

/**
 * splits lrc text into lines with a timestamp.
 *
 * one line may carry several timestamps, for a recurring chorus, and it is
 * emitted several times then.
 */
export function parseLrc(input: string): LyricLine[] {
  const lines: LyricLine[] = [];

  for (const raw of input.split(/\r?\n/)) {
    TIME_TAG.lastIndex = 0;
    const stamps: number[] = [];
    let match: RegExpExecArray | null;
    while ((match = TIME_TAG.exec(raw)) !== null) {
      const minutes = Number(match[1]);
      const seconds = Number(match[2]);
      const fraction = match[3] ?? "0";
      // fractions of two and of three digits both occur
      const ms = Number(fraction.padEnd(3, "0").slice(0, 3));
      stamps.push(minutes * 60_000 + seconds * 1000 + ms);
    }
    if (stamps.length === 0) continue;

    const text = raw.replace(TIME_TAG, "").trim();
    for (const timeMs of stamps) {
      lines.push({ timeMs, text });
    }
  }

  lines.sort((a, b) => a.timeMs - b.timeMs);
  return lines;
}

/** index of the line being sung at `positionMs`. */
export function activeLineIndex(
  lines: LyricLine[],
  positionMs: number,
): number {
  if (lines.length === 0) return -1;
  let low = 0;
  let high = lines.length - 1;
  let result = -1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (lines[mid].timeMs <= positionMs) {
      result = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return result;
}

/**
 * forms lines with timestamps back into lrc text.
 *
 * two decimal places as usual, no player resolves finer than that, and
 * hundredths are imperceptible while reading along anyway.
 */
export function formatLrc(lines: LyricLine[]): string {
  return lines
    .map(({ timeMs, text }) => {
      const gesamt = Math.max(0, Math.round(timeMs));
      const minuten = Math.floor(gesamt / 60_000);
      const sekunden = Math.floor((gesamt % 60_000) / 1000);
      const hundertstel = Math.floor((gesamt % 1000) / 10);
      const zwei = (wert: number) => String(wert).padStart(2, "0");
      return `[${zwei(minuten)}:${zwei(sekunden)}.${zwei(hundertstel)}] ${text}`.trimEnd();
    })
    .join("\n");
}
