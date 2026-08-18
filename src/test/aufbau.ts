import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";

/**
 * Gemeinsame Vorbereitung für alle Tests, die ein Dokument brauchen.
 *
 * Zwei Dinge fehlen jsdom, die diese App voraussetzt:
 *
 * `matchMedia` fragt das Erscheinungsbild des Systems ab. Ohne Ersatz stürbe
 * jeder Test, der eine Seite aufbaut, an einem `undefined is not a function`
 * statt an dem, was er eigentlich prüft.
 *
 * `scrollIntoView` ruft die Titelliste auf, um den hervorgehobenen Titel in
 * den Blick zu holen. jsdom kennt keine Bildlaufflächen und bringt die
 * Methode darum nicht mit.
 */
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

// Zwischen zwei Tests bleibt nichts im Dokument stehen; sonst fände eine
// Abfrage nach „dem Knopf“ den aus dem Test davor gleich mit.
afterEach(cleanup);
