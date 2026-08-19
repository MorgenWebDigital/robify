import { useEffect } from "react";

// below this height a change does not count as a keyboard.
//
// the visible viewport wobbles by a few points when system bars fade in and
// out. a keyboard takes a multiple of that
const MINDESTHOEHE = 100;

/**
 * keeps the height of the on-screen keyboard in `--tastatur`.
 *
 * android pushes the keyboard over the content without telling the window
 * about it: `innerHeight` stays as it was, and the bar with the player lay
 * behind the keyboard. the visual viewport does know, and the difference
 * between the two is exactly the height of the keyboard.
 *
 * the result stands ready as a css value, and frame and bars fold it into
 * their bottom spacing. where the window does shrink by itself on another
 * device, zero comes out here and the calculation still holds.
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
    // scrolling while zoomed moves the viewport without changing its height,
    // and without measuring again the spacing would stay where it was
    sicht.addEventListener("scroll", messen);
    return () => {
      sicht.removeEventListener("resize", messen);
      sicht.removeEventListener("scroll", messen);
      document.documentElement.style.removeProperty("--tastatur");
    };
  }, []);
}
