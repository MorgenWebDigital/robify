import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { spracheAufloesen, spracheSetzen } from "./lib/i18n";
import { erscheinungWiederherstellen, useLibrary } from "./store/library";
import "./index.css";

// sets the language and rebuilds the app on a switch.
//
// the key is the trick: `t()` reads a module variable so it stays callable
// outside components too, and react therefore does not notice a change to it
// by itself. changing the key rebuilds the tree once and every text comes
// back translated. the open page stands in the address and is not lost
function Wurzel() {
  const einstellung = useLibrary((s) => s.settings?.language);
  const sprache = spracheAufloesen(einstellung);
  // before the render, not after: the children ask `t()` while building up
  spracheSetzen(sprache);

  return (
    // the outer net catches what the inner one cannot: errors in the title
    // bar, the sidebar or the player. only a message stands there then, but a
    // message rather than a black window
    <ErrorBoundary scope="Robify">
      <HashRouter>
        <App key={sprache} />
      </HashRouter>
    </ErrorBoundary>
  );
}

// before the first render: accent and appearance from the last session, so
// nothing flashes up in the default colour
erscheinungWiederherstellen();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Wurzel />
  </StrictMode>,
);
