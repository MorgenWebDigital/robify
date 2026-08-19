import { describe, expect, it } from "vitest";
import { ausSchluessel } from "./datum";
import { spracheSetzen } from "./i18n";

describe("ausSchluessel", () => {
  it("liest Tages- und Monatsschlüssel", () => {
    spracheSetzen("de");
    expect(ausSchluessel("2026-08-19")).toBe("19. Aug.");
    // without a day next to it the library writes the month without a dot
    expect(ausSchluessel("2026-08")).toBe("Aug");
  });

  // the review of everything counts in months and reaches over years.
  // without a year, "Aug." stood at both ends of the history meaning two
  // different ones
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
