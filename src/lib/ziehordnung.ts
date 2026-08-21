import { useRef, useState } from "react";

/**
 * changes the order of a list by dragging.
 *
 * sits here and not in a component because two lists need the same behaviour:
 * the tracks of a playlist and the playlists themselves. written twice the
 * two would drift apart sooner or later, and a difference in dragging reads
 * as a bug straight away.
 *
 * the callback gets the complete new order as ids, not a pair of old and new:
 * what lands in the database is the whole list anyway, and a caller has
 * nothing to recalculate this way.
 */
export function useZiehordnung(
  kennungen: number[],
  aufNeueOrdnung: ((kennungen: number[]) => void) | undefined,
) {
  /** position of the entry currently being dragged */
  const [zieht, setZieht] = useState<number | null>(null);
  /**
   * where it would land: 0 means the very top, `length` the very bottom.
   *
   * so the gap between two entries, not an entry, only that way can "to the
   * end" be expressed at all.
   */
  const [luecke, setLuecke] = useState<number | null>(null);
  /**
   * the same value as `zieht`, but readable right away.
   *
   * state changes take effect at the next render while `dragover` fires
   * before that. without this copy the first pass still saw `null` and
   * refused the drop.
   */
  const ziehtRef = useRef<number | null>(null);

  const beenden = () => {
    ziehtRef.current = null;
    setZieht(null);
    setLuecke(null);
  };

  const ablegen = () => {
    const start = ziehtRef.current;
    const ziel = luecke;
    beenden();
    if (start === null || ziel === null || !aufNeueOrdnung) return;

    const neu = [...kennungen];
    const [bewegt] = neu.splice(start, 1);
    // after taking it out everything behind it moves up one place
    neu.splice(ziel > start ? ziel - 1 : ziel, 0, bewegt);
    if (neu.every((id, i) => id === kennungen[i])) return;
    aufNeueOrdnung(neu);
  };

  /**
   * the props for one entry.
   *
   * `waagerecht` for tile grids: which gap is meant is decided by left or
   * right there, not by top or bottom.
   */
  const merkmale = (index: number, waagerecht = false) => ({
    draggable: Boolean(aufNeueOrdnung),
    onDragStart: (event: React.DragEvent) => {
      if (!aufNeueOrdnung) return;
      ziehtRef.current = index;
      setZieht(index);
      // webkit starts a drag only where something is handed along. without
      // this line plainly nothing happened
      event.dataTransfer.setData("text/plain", String(kennungen[index]));
      event.dataTransfer.effectAllowed = "move";
    },
    onDragEnd: beenden,
    onDragOver: (event: React.DragEvent) => {
      if (!aufNeueOrdnung || ziehtRef.current === null) return;
      // without this the browser refuses the drop
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      const kasten = event.currentTarget.getBoundingClientRect();
      const dahinter = waagerecht
        ? event.clientX - kasten.left > kasten.width / 2
        : event.clientY - kasten.top > kasten.height / 2;
      setLuecke(index + (dahinter ? 1 : 0));
    },
    onDrop: (event: React.DragEvent) => {
      event.preventDefault();
      ablegen();
    },
  });

  return { zieht, luecke, merkmale, ablegen, laenge: kennungen.length };
}
