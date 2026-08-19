import { useEffect, useState } from "react";
import { DiscIcon } from "./Icons";

interface CoverProps {
  src: string | null;
  alt: string;
  className?: string;
  rounded?: string;
  /** used for the placeholder pattern so it stays stable. */
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

// reports whether an image is fully loaded already.
//
// coming out of the cache it is finished before react can attach its `load`
// listener, the call would then never come and the image would stay invisible
// forever. `complete` is therefore asked for on attaching as well
export function useBildFertig(src: string | null) {
  const [fertig, setFertig] = useState(false);

  // deliberately no reset when the address changes: after every change to the
  // library a new counter hangs on all images so the cache does not show the
  // old one. were it reset here, every image would fall back to the
  // placeholder briefly. an `img` keeps its old frame until the new one is
  // decoded anyway, so the change stays seamless without any help
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
    // the placeholder stays lying underneath instead of being replaced by the
    // image. otherwise a gap yawned for a moment and the image struck into it
    // hard, visible as a flash
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
