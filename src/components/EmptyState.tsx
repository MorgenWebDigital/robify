import type { ComponentType, ReactNode } from "react";

// an empty area with an icon, an explanation and a way on.
//
// a plain "nothing found" leaves the user standing. every empty place in
// robify is to say what fills it and where that happens
export function EmptyState({
  icon: Glyph,
  title,
  text,
  actions,
}: {
  icon: ComponentType<{ size?: number; className?: string }>;
  title: string;
  text: ReactNode;
  /** buttons or links. without them the box stays pure information. */
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
