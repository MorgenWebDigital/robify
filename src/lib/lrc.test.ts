import { describe, expect, it } from "vitest";
import { activeLineIndex, formatLrc, parseLrc } from "./lrc";

describe("parseLrc", () => {
  it("liest Zeitmarken mit Hundertsteln und Tausendsteln", () => {
    const lines = parseLrc("[00:12.50] erste\n[01:05.250] zweite");
    expect(lines).toEqual([
      { timeMs: 12_500, text: "erste" },
      { timeMs: 65_250, text: "zweite" },
    ]);
  });

  it("gibt eine Zeile mit mehreren Marken mehrfach aus", () => {
    // in lrc a chorus often stands once, with all of its times in front
    const lines = parseLrc("[00:10.00][01:10.00][02:10.00] Refrain");
    expect(lines.map((l) => l.timeMs)).toEqual([10_000, 70_000, 130_000]);
    expect(new Set(lines.map((l) => l.text))).toEqual(new Set(["Refrain"]));
  });

  it("überspringt Kopfzeilen ohne Zeitmarke", () => {
    const lines = parseLrc("[ar:Kanye West]\n[ti:Monster]\n\n[00:01.00] los");
    expect(lines).toEqual([{ timeMs: 1000, text: "los" }]);
  });

  it("sortiert nach Zeit, auch wenn die Datei es nicht tut", () => {
    const lines = parseLrc("[02:00.00] spät\n[00:30.00] früh");
    expect(lines.map((l) => l.text)).toEqual(["früh", "spät"]);
  });

  it("hält leere Zeilen als Pause fest", () => {
    // the gap belongs to it: without one the last line would stay standing
    // long after nothing is being sung any more
    expect(parseLrc("[00:05.00]")).toEqual([{ timeMs: 5000, text: "" }]);
  });

  it("bleibt bei Unsinn ruhig", () => {
    expect(parseLrc("")).toEqual([]);
    expect(parseLrc("nur Fließtext ohne alles")).toEqual([]);
  });
});

describe("activeLineIndex", () => {
  const lines = parseLrc("[00:00.00] a\n[00:10.00] b\n[00:20.00] c");

  it("findet die Zeile, die gerade läuft", () => {
    expect(activeLineIndex(lines, 0)).toBe(0);
    expect(activeLineIndex(lines, 9_999)).toBe(0);
    expect(activeLineIndex(lines, 10_000)).toBe(1);
    expect(activeLineIndex(lines, 15_000)).toBe(1);
    expect(activeLineIndex(lines, 999_999)).toBe(2);
  });

  it("meldet vor der ersten Marke nichts", () => {
    const spaet = parseLrc("[00:30.00] erst später");
    expect(activeLineIndex(spaet, 0)).toBe(-1);
    expect(activeLineIndex(spaet, 29_999)).toBe(-1);
    expect(activeLineIndex(spaet, 30_000)).toBe(0);
  });

  it("kommt mit leerer Liste zurecht", () => {
    expect(activeLineIndex([], 5000)).toBe(-1);
  });
});

describe("formatLrc", () => {
  it("schreibt mm:ss.hh", () => {
    expect(formatLrc([{ timeMs: 65_250, text: "zweite" }])).toBe(
      "[01:05.25] zweite",
    );
  });

  it("übersteht Hin und Zurück", () => {
    const quelle = "[00:12.50] erste\n[01:05.25] zweite\n[02:00.00] dritte";
    expect(formatLrc(parseLrc(quelle))).toBe(quelle);
  });

  it("lässt hinter leerem Text kein Leerzeichen stehen", () => {
    expect(formatLrc([{ timeMs: 5000, text: "" }])).toBe("[00:05.00]");
  });

  it("macht aus negativer Zeit keine kaputte Marke", () => {
    // timing by hand, the offset can push a line before the start
    expect(formatLrc([{ timeMs: -400, text: "zu früh" }])).toBe(
      "[00:00.00] zu früh",
    );
  });
});
