import { describe, expect, it } from "vitest";
import { ausSchluessel } from "./datum";
import { spracheSetzen } from "./i18n";

describe("ausSchluessel", () => {
  it("liest Tages- und Monatsschlüssel", () => {
    spracheSetzen("de");
    expect(ausSchluessel("2026-08-19")).toBe("19. Aug.");
    // Ohne Tag daneben schreibt die Bibliothek den Monat ohne Punkt.
    expect(ausSchluessel("2026-08")).toBe("Aug");
  });

  /*
   * Der Rückblick auf alles zählt in Monaten und reicht über Jahre. Ohne
   * Jahreszahl stand an beiden Enden des Verlaufs „Aug.“ und meinte zwei
   * verschiedene.
   */
  it("nennt auf Wunsch das Jahr dazu", () => {
    spracheSetzen("de");
    const mitJahr = ausSchluessel("2025-08", true);
    expect(mitJahr).toContain("2025");
    expect(ausSchluessel("2026-08", true)).not.toBe(mitJahr);
  });

  it("gibt unbekannte Schlüssel unverändert zurück", () => {
    expect(ausSchluessel("später")).toBe("später");
  });
});
