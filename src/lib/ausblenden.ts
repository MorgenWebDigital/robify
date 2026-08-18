import { useEffect, useState } from "react";

/**
 * Hält ein Element nach dem Schließen kurz stehen, damit es ausblenden kann.
 *
 * React hängt ein Bauteil in dem Augenblick aus, in dem seine Bedingung
 * umspringt, für eine Abgangsbewegung bleibt dann keine Zeit. Dieser Haken
 * schiebt das Aushängen um die Dauer der Bewegung hinaus und meldet
 * währenddessen `schliesst`, damit die Hülle die passende Klasse tragen kann.
 *
 * `dauer` muss zur Bewegung im Stylesheet passen; läuft sie länger, wird das
 * Element mitten in der Bewegung entfernt.
 */
export function useAusblenden(offen: boolean, dauer: number) {
  const [sichtbar, setSichtbar] = useState(offen);
  const [schliesst, setSchliesst] = useState(false);

  useEffect(() => {
    if (offen) {
      setSichtbar(true);
      setSchliesst(false);
      return;
    }
    if (!sichtbar) return;
    setSchliesst(true);
    const uhr = window.setTimeout(() => {
      setSichtbar(false);
      setSchliesst(false);
    }, dauer);
    return () => window.clearTimeout(uhr);
  }, [offen, sichtbar, dauer]);

  return { sichtbar, schliesst };
}
