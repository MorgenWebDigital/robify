import { useEffect, useState } from "react";

/**
 * keeps an element standing briefly after closing so it can fade out.
 *
 * react unmounts a component the moment its condition flips, which leaves no
 * time for an exit animation. this hook pushes the unmount out by the
 * duration of the animation and reports `schliesst` meanwhile, so the wrapper
 * can carry the matching class.
 *
 * `dauer` has to match the animation in the stylesheet. runs it longer, the
 * element is removed mid-animation.
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
