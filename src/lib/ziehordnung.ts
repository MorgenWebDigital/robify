import { useRef, useState } from "react";

/**
 * Reihenfolge einer Liste per Ziehen ändern.
 *
 * Steckt hier und nicht in einem Bauteil, weil zwei Listen dasselbe Verhalten
 * brauchen: die Titel einer Playlist und die Playlists selbst. Zweimal
 * geschrieben liefen die beiden über kurz oder lang auseinander, und ein
 * Unterschied im Ziehen fällt sofort als Fehler auf.
 *
 * Der Rückruf bekommt die vollständige neue Reihenfolge als Kennungen, nicht
 * ein Paar aus Alt und Neu: Was in der Datenbank landet, ist ohnehin die
 * ganze Liste, und ein Aufrufer muss so nichts nachrechnen.
 */
export function useZiehordnung(
  kennungen: number[],
  aufNeueOrdnung: ((kennungen: number[]) => void) | undefined,
) {
  /** Position des Eintrags, der gerade gezogen wird. */
  const [zieht, setZieht] = useState<number | null>(null);
  /**
   * Stelle, an der er landen würde: 0 heißt ganz oben, `länge` ganz unten.
   * Also die Lücke *zwischen* zwei Einträgen, nicht ein Eintrag, nur so lässt
   * sich „ans Ende“ überhaupt ausdrücken.
   */
  const [luecke, setLuecke] = useState<number | null>(null);
  /**
   * Dieselbe Angabe wie `zieht`, aber sofort lesbar.
   *
   * Zustandsänderungen greifen erst beim nächsten Zeichnen; `dragover` feuert
   * jedoch schon davor. Ohne diese Kopie sah der erste Durchlauf noch `null`
   * und lehnte das Ablegen ab.
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
    // Nach dem Herausnehmen rutscht alles dahinter eine Stelle vor.
    neu.splice(ziel > start ? ziel - 1 : ziel, 0, bewegt);
    if (neu.every((id, i) => id === kennungen[i])) return;
    aufNeueOrdnung(neu);
  };

  /**
   * Die Merkmale für einen Eintrag.
   *
   * `waagerecht` für Kachelgitter: Dort entscheidet nicht oben oder unten,
   * sondern links oder rechts, welche Lücke gemeint ist.
   */
  const merkmale = (index: number, waagerecht = false) => ({
    draggable: Boolean(aufNeueOrdnung),
    onDragStart: (event: React.DragEvent) => {
      if (!aufNeueOrdnung) return;
      ziehtRef.current = index;
      setZieht(index);
      // WebKit startet einen Zug nur, wenn etwas mitgegeben wird. Ohne diese
      // Zeile passierte schlicht gar nichts.
      event.dataTransfer.setData("text/plain", String(kennungen[index]));
      event.dataTransfer.effectAllowed = "move";
    },
    onDragEnd: beenden,
    onDragOver: (event: React.DragEvent) => {
      if (!aufNeueOrdnung || ziehtRef.current === null) return;
      // Ohne dieses Abfangen lehnt der Browser das Ablegen ab.
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
