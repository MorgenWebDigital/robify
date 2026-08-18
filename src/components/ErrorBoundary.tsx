import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../lib/i18n";

interface Props {
  children: ReactNode;
  /** Was ausgefallen ist, für die Überschrift: „Diese Seite“, „Robify“. */
  scope: string;
  /** Wird beim Klick auf „Nochmal versuchen“ gerufen, bevor neu gerendert wird. */
  onReset?: () => void;
}

interface State {
  error: Error | null;
}

/**
 * Fängt Fehler beim Zeichnen ab.
 *
 * Ohne dieses Netz reißt ein einziger Fehler in irgendeiner Seite die gesamte
 * Oberfläche auf Schwarz, React hängt den ganzen Baum aus, und übrig bleibt
 * ein leeres Fenster ohne jeden Hinweis. Genau dieses Bild hat uns beim
 * Versuch mit dem durchsichtigen Fenster eine Stunde gekostet.
 *
 * React bietet dafür bis heute nur Klassen an; Haken gibt es keine.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // In die Konsole, damit die Entwicklerwerkzeuge den Ursprung zeigen.
    console.error("Robify: Fehler beim Zeichnen", error, info.componentStack);
  }

  private reset = () => {
    this.props.onReset?.();
    this.setState({ error: null });
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="flex min-h-full flex-col items-center justify-center gap-4 px-6 py-20 text-center">
        <h2 className="text-lg font-semibold">
          {t("{0} hat sich verschluckt", this.props.scope)}
        </h2>
        <p className="max-w-md text-sm text-mute">
          {t(
            "Der Fehler ist nur hier aufgetreten, deine Musik ist unberührt. Ein neuer Versuch reicht meistens; hilft er nicht, führt der Weg über eine andere Seite zurück.",
          )}
        </p>

        {/* Der Wortlaut hilft beim Nachfragen; sperrig, aber nachvollziehbar. */}
        <code className="max-w-lg overflow-x-auto rounded bg-ink-800 px-3 py-2 text-start text-xs text-mute">
          {error.message || String(error)}
        </code>

        <div className="flex flex-wrap items-center justify-center gap-2">
          <button
            type="button"
            onClick={this.reset}
            className="pill-btn is-raised is-accent h-9 px-4 text-sm font-semibold"
          >
            {t("Nochmal versuchen")}
          </button>
          <button
            type="button"
            onClick={() => window.location.reload()}
            className="pill-btn is-raised h-9 px-4 text-sm font-semibold"
          >
            {t("Robify neu laden")}
          </button>
        </div>
      </div>
    );
  }
}
