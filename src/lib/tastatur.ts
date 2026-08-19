import { useEffect } from "react";

/**
 * Unterhalb dieser Höhe gilt eine Änderung nicht als Tastatur.
 *
 * Der sichtbare Ausschnitt wackelt um ein paar Punkte, wenn Leisten des
 * Systems ein- und ausblenden. Eine Tastatur nimmt ein Vielfaches davon.
 */
const MINDESTHOEHE = 100;

/**
 * Hält die Höhe der Bildschirmtastatur in `--tastatur` fest.
 *
 * Android schiebt die Tastatur über den Inhalt, ohne dem Fenster etwas davon
 * zu sagen: `innerHeight` bleibt, wie es war, und die Leiste mit dem Player
 * lag hinter der Tastatur. Der sichtbare Ausschnitt weiß es aber, und die
 * Differenz zwischen beiden ist genau die Höhe der Tastatur.
 *
 * Das Ergebnis steht als CSS-Größe bereit; Rahmen und Leisten rechnen sie in
 * ihren Abstand nach unten ein. Schrumpft das Fenster auf einem anderen Gerät
 * doch selbst, kommt hier null heraus, und die Rechnung stimmt weiterhin.
 */
export function useTastaturhoehe(): void {
  useEffect(() => {
    const sicht = window.visualViewport;
    if (!sicht) return;

    const messen = () => {
      const roh = window.innerHeight - sicht.height - sicht.offsetTop;
      const hoehe = roh > MINDESTHOEHE ? Math.round(roh) : 0;
      document.documentElement.style.setProperty("--tastatur", `${hoehe}px`);
    };

    messen();
    sicht.addEventListener("resize", messen);
    // Beim Rollen im gezoomten Zustand verschiebt sich der Ausschnitt, ohne
    // dass sich seine Höhe ändert; ohne das Nachmessen bliebe der Abstand
    // stehen, wo er war.
    sicht.addEventListener("scroll", messen);
    return () => {
      sicht.removeEventListener("resize", messen);
      sicht.removeEventListener("scroll", messen);
      document.documentElement.style.removeProperty("--tastatur");
    };
  }, []);
}
