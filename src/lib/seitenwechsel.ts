import { useEffect, useRef } from "react";
import { useLocation } from "react-router-dom";

/**
 * closes an overlay as soon as the page changes.
 *
 * the full screen view and the queue lie over the page content, not inside
 * it. a press on the bottom bar therefore did lead to the new page while the
 * running track stayed visible: one stood in the downloader and could not see
 * it. whoever navigates away does not want to see the overlay any more.
 *
 * nothing happens on the first pass. the components hang in the tree for the
 * whole runtime of the app and show themselves only while open, and without
 * this exception the call would close something at startup that nobody has
 * opened yet. opening itself does not change the page, so it triggers
 * nothing.
 *
 * `setzen` comes out of the store and stays the same, which keeps the
 * dependency list stable.
 */
export function useSchliesstBeimSeitenwechsel(
  setzen: (offen: boolean) => void,
): void {
  const { pathname } = useLocation();
  const erster = useRef(true);

  useEffect(() => {
    if (erster.current) {
      erster.current = false;
      return;
    }
    setzen(false);
  }, [pathname, setzen]);
}
