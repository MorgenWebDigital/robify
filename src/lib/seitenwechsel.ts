import { useEffect, useRef } from "react";
import { useLocation } from "react-router-dom";

/**
 * Schließt eine Überlagerung, sobald die Seite wechselt.
 *
 * Die Vollbildansicht und die Warteschlange liegen über dem Seiteninhalt,
 * nicht in ihm. Ein Druck auf die Leiste unten führte darum zwar zur neuen
 * Seite, sichtbar blieb aber der laufende Titel — man stand im Downloader und
 * sah ihn nicht. Wer wegnavigiert, will die Überlagerung nicht mehr sehen.
 *
 * Beim ersten Durchlauf passiert nichts. Die Bauteile hängen für die gesamte
 * Laufzeit der App im Baum und zeigen sich nur, wenn sie offen sind; ohne
 * diese Ausnahme schlösse der Aufruf beim Start etwas, das noch niemand
 * geöffnet hat. Das Öffnen selbst wechselt die Seite nicht, es löst also
 * nichts aus.
 *
 * `setzen` kommt aus dem Zustandsspeicher und bleibt derselbe; die
 * Abhängigkeitsliste ist damit stabil.
 */
export function useSchliesstBeimSeitenwechsel(
  setzen: (offen: boolean) => void,
): void {
  const { pathname } = useLocation();
  const erster = useRef(true);

  useEffect(() => {
    if (erster.current) {
      erster.current = false;
      return;
    }
    setzen(false);
  }, [pathname, setzen]);
}
