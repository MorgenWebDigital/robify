import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../lib/i18n";

interface Props {
  children: ReactNode;
  /** what failed, for the heading: "this page", "Robify". */
  scope: string;
  /** called on a click on the retry button, before the re-render. */
  onReset?: () => void;
}

interface State {
  error: Error | null;
}

// catches errors thrown while rendering.
//
// without this net a single error on any page tears the whole ui to black,
// react unmounts the entire tree, and an empty window without any hint is
// left. exactly that picture cost an hour during the attempt with the
// transparent window.
//
// react offers classes alone for this to this day, there are no hooks
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // to the console, so the developer tools show the origin
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

        {/* the wording helps when asking back: unwieldy but traceable. */}
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
