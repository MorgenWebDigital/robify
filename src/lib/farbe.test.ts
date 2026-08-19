import { describe, expect, it } from "vitest";
import { akzentSchrift, eigeneFarben, normalisiereHex } from "./farbe";

describe("normalisiereHex", () => {
  it("nimmt die lange Form mit und ohne Doppelkreuz", () => {
    expect(normalisiereHex("#a8a8b3")).toBe("#a8a8b3");
    expect(normalisiereHex("a8a8b3")).toBe("#a8a8b3");
  });

  it("zieht die Kurzform auf", () => {
    expect(normalisiereHex("#abc")).toBe("#aabbcc");
    expect(normalisiereHex("f0f")).toBe("#ff00ff");
  });

  it("vereinheitlicht Schreibweise und Leerraum", () => {
    // what comes out of the clipboard is rarely clean
    expect(normalisiereHex("  #A8A8B3  ")).toBe("#a8a8b3");
    expect(normalisiereHex("FFF")).toBe("#ffffff");
  });

  it("weist zurück, was keine Farbe ist", () => {
    for (const unsinn of [
      "",
      "#",
      "#ab",
      "#abcd",
      "#abcde",
      "#1234567",
      "rot",
      "#gggggg",
    ]) {
      expect(normalisiereHex(unsinn), unsinn).toBeNull();
    }
  });
});

describe("eigeneFarben", () => {
  it("zerlegt die gespeicherte Liste", () => {
    expect(eigeneFarben("#ff0000,#00ff00")).toEqual(["#ff0000", "#00ff00"]);
  });

  it("bleibt bei leerer Ablage leer", () => {
    expect(eigeneFarben("")).toEqual([]);
  });

  it("wirft kaputte Einträge weg, statt an ihnen zu scheitern", () => {
    // the list lies in the database as text, and a half-written value must
    // not paralyse the whole colour picker
    expect(eigeneFarben("#ff0000,,quatsch,#abc")).toEqual([
      "#ff0000",
      "#aabbcc",
    ]);
  });
});

describe("akzentSchrift", () => {
  it("setzt helle Schrift auf die dunklen Vorgaben", () => {
    for (const dunkel of ["#8b0000", "#4b0082", "#191970", "#006400"]) {
      expect(akzentSchrift(dunkel), dunkel).toBe("#ffffff");
    }
  });

  it("setzt dunkle Schrift auf die hellen Vorgaben", () => {
    for (const hell of ["#a8a8b3", "#daa520"]) {
      expect(akzentSchrift(hell), hell).toBe("#16161a");
    }
  });

  it("entscheidet an den Rändern richtig", () => {
    expect(akzentSchrift("#ffffff")).toBe("#16161a");
    expect(akzentSchrift("#000000")).toBe("#ffffff");
  });

  it("wiegt Grün schwerer als Blau", () => {
    // the same number, a different effect: pure green is light enough for
    // dark type, pure blue is not. a calculation without weighting would miss
    // that
    expect(akzentSchrift("#00ff00")).toBe("#16161a");
    expect(akzentSchrift("#0000ff")).toBe("#ffffff");
  });

  it("fällt bei Unsinn auf die dunkle Schrift zurück", () => {
    expect(akzentSchrift("quatsch")).toBe("#16161a");
  });
});
