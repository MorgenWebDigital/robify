import type { ComponentType, ReactNode } from "react";

/**
 * Leerer Bereich mit Symbol, Erklärung und einem Weg weiter.
 *
 * Ein bloßes „Nichts gefunden“ lässt den Nutzer stehen. Jede leere Stelle in
 * Robify soll sagen, was sie füllt und wo das geht.
 */
export function EmptyState({
  icon: Glyph,
  title,
  text,
  actions,
}: {
  icon: ComponentType<{ size?: number; className?: string }>;
  title: string;
  text: ReactNode;
  /** Knöpfe oder Links. Ohne sie bleibt der Kasten reine Auskunft. */
  actions?: ReactNode;
}) {
  return (
    <div className="surface flex flex-col items-center gap-4 px-6 py-16 text-center">
      <Glyph size={34} className="text-mute" />
      <div className="max-w-md">
        <h2 className="text-lg font-semibold">{title}</h2>
        <p className="mt-1 text-sm text-mute">{text}</p>
      </div>
      {actions && (
        <div className="flex flex-wrap justify-center gap-2">{actions}</div>
      )}
    </div>
  );
}
