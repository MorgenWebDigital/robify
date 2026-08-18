import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { spracheAufloesen, spracheSetzen } from "./lib/i18n";
import { erscheinungWiederherstellen, useLibrary } from "./store/library";
import "./index.css";

/**
 * Setzt die Sprache und baut die App bei einem Wechsel neu auf.
 *
 * Der Schlüssel ist der Kniff: `t()` liest eine Modulvariable, damit es auch
 * außerhalb von Bauteilen aufrufbar bleibt, von allein bemerkt React eine
 * Änderung daran also nicht. Ein Wechsel des Schlüssels baut den Baum einmal
 * neu, und alle Texte kommen übersetzt zurück. Die geöffnete Seite steht in
 * der Adresse und geht dabei nicht verloren.
 */
function Wurzel() {
  const einstellung = useLibrary((s) => s.settings?.language);
  const sprache = spracheAufloesen(einstellung);
  // Vor dem Zeichnen, nicht danach: Die Kinder fragen `t()` beim Aufbau ab.
  spracheSetzen(sprache);

  return (
    // Das äußere Netz fängt, was das innere nicht kann: Fehler in der
    // Titelleiste, der Seitenleiste oder im Player. Dann steht zwar nur noch
    // eine Meldung da, aber eben eine Meldung statt eines schwarzen Fensters.
    <ErrorBoundary scope="Robify">
      <HashRouter>
        <App key={sprache} />
      </HashRouter>
    </ErrorBoundary>
  );
}

// Vor dem ersten Zeichnen: Akzent und Erscheinungsbild aus der letzten
// Sitzung, damit nichts kurz in der Vorgabefarbe aufblitzt.
erscheinungWiederherstellen();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Wurzel />
  </StrictMode>,
);
