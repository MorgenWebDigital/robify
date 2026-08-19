import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";

// shared setup for every test that needs a document.
//
// two things jsdom lacks which this app takes for granted:
//
// `matchMedia` asks for the appearance of the system. without a stand-in
// every test that builds a page would die on an `undefined is not a function`
// instead of on what it actually checks.
//
// `scrollIntoView` is called by the track list to bring the highlighted track
// into view. jsdom knows no scrolling areas and therefore does not bring the
// method
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (abfrage: string) => ({
    matches: false,
    media: abfrage,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }),
});

Element.prototype.scrollIntoView = vi.fn();

// nothing stays in the document between two tests, otherwise a query for
// "the button" would find the one from the test before along with it
afterEach(cleanup);
