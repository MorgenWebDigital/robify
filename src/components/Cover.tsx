import { useEffect, useState } from "react";
import { DiscIcon } from "./Icons";

interface CoverProps {
  src: string | null;
  alt: string;
  className?: string;
  rounded?: string;
  /** Wird für das Platzhalter-Muster genutzt, damit es stabil bleibt. */
  seed?: string | number;
}

const GRADIENTS = [
  "from-sky-900 to-slate-900",
  "from-emerald-900 to-slate-900",
  "from-violet-900 to-slate-900",
  "from-rose-900 to-slate-900",
  "from-amber-900 to-slate-900",
  "from-cyan-900 to-slate-900",
];

function gradientFor(seed: string | number | undefined): string {
  const text = String(seed ?? "");
  let hash = 0;
  for (let i = 0; i < text.length; i += 1)
    hash = (hash * 31 + text.charCodeAt(i)) >>> 0;
  return GRADIENTS[hash % GRADIENTS.length];
}

/**
 * Meldet, ob ein Bild schon vollständig geladen ist.
 *
 * Kommt es aus dem Zwischenspeicher, ist es fertig, bevor React seinen
 * `load`-Empfänger anhängen kann, der Aufruf käme dann nie, und das Bild
 * bliebe für immer unsichtbar. Deshalb wird beim Anhängen zusätzlich `complete`
 * abgefragt.
 */
export function useBildFertig(src: string | null) {
  const [fertig, setFertig] = useState(false);

  // Bewusst kein Zurücksetzen beim Wechsel der Adresse: Nach jeder Änderung an
  // der Bibliothek hängt an allen Bildern eine neue Zählnummer, damit der
  // Zwischenspeicher nicht das alte Bild zeigt. Würde hier zurückgesetzt,
  // fiele jedes Bild dabei kurz auf den Platzhalter zurück. Ein `img` behält
  // sein altes Einzelbild ohnehin, bis das neue entschlüsselt ist, der
  // Wechsel bleibt also lückenlos, auch ohne Zutun.
  void src;

  return {
    fertig,
    melden: () => setFertig(true),
    pruefen: (element: HTMLImageElement | null) => {
      if (element?.complete && element.naturalWidth > 0) setFertig(true);
    },
  };
}

export function Cover({
  src,
  alt,
  className = "",
  rounded = "rounded-lg",
  seed,
}: CoverProps) {
  const [failed, setFailed] = useState(false);
  const bild = useBildFertig(src);

  useEffect(() => setFailed(false), [src]);

  const platzhalter = (
    <span
      className={`absolute inset-0 grid place-items-center bg-gradient-to-br ${gradientFor(seed ?? alt)}`}
      aria-hidden="true"
    >
      <DiscIcon className="h-1/3 w-1/3 text-white/25" size={undefined} />
    </span>
  );

  if (!src || failed) {
    return (
      <span
        className={`relative block overflow-hidden ${rounded} ${className}`}
        role="img"
        aria-label={alt}
      >
        {platzhalter}
      </span>
    );
  }

  return (
    // Der Platzhalter bleibt darunter liegen, statt vom Bild abgelöst zu
    // werden. Sonst klaffte für einen Moment eine Lücke, und das Bild schlug
    // hart hinein, als Blitzen sichtbar.
    <span className={`relative block overflow-hidden ${rounded} ${className}`}>
      {platzhalter}
      <img
        src={src}
        alt={alt}
        loading="lazy"
        draggable={false}
        ref={bild.pruefen}
        onLoad={bild.melden}
        onError={() => setFailed(true)}
        className={`absolute inset-0 h-full w-full object-cover transition-opacity duration-300 ease-out ${
          bild.fertig ? "opacity-100" : "opacity-0"
        }`}
      />
    </span>
  );
}
