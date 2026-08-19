import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatDuration,
  formatTime,
  plural,
  releaseLabel,
} from "./format";
import { spracheSetzen } from "./i18n";

describe("formatTime", () => {
  it("schreibt mm:ss und ab einer Stunde h:mm:ss", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(9_000)).toBe("0:09");
    expect(formatTime(200_000)).toBe("3:20");
    expect(formatTime(3_600_000)).toBe("1:00:00");
    expect(formatTime(3_725_000)).toBe("1:02:05");
  });

  it("macht aus Unsinn eine Null statt „NaN:NaN“", () => {
    // running times come out of files, and where the value is missing
    // nothing usable stands there now and then
    expect(formatTime(-5000)).toBe("0:00");
    expect(formatTime(Number.NaN)).toBe("0:00");
    expect(formatTime(Number.POSITIVE_INFINITY)).toBe("0:00");
  });
});

describe("formatDuration", () => {
  it("wählt die Einheit nach der Größe", () => {
    expect(formatDuration(20_000)).toBe("20 Sek.");
    expect(formatDuration(5 * 60_000)).toBe("5 Min.");
    expect(formatDuration(65 * 60_000)).toBe("1 Std. 5 Min.");
    expect(formatDuration(25 * 3_600_000)).toBe("1 Tg. 1 Std.");
  });

  it("rundet auf ganze Minuten, sobald es sich lohnt", () => {
    // from half a minute on it reads "1 Min." instead of "30 Sek.", which is
    // wanted, the value is only meant to convey a magnitude anyway
    expect(formatDuration(29_000)).toBe("29 Sek.");
    expect(formatDuration(30_000)).toBe("1 Min.");
  });
});

describe("formatBytes", () => {
  it("rundet und hängt die passende Einheit an", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(20 * 1024 * 1024)).toBe("20 MB");
  });

  it("nennt Fehlendes beim Namen", () => {
    expect(formatBytes(null)).toBe("unbekannt");
    expect(formatBytes(undefined)).toBe("unbekannt");
    expect(formatBytes(0)).toBe("unbekannt");
  });
});

describe("plural", () => {
  it("unterscheidet Einzahl und Mehrzahl", () => {
    expect(plural(1, "Titel")).toBe("1 Titel");
    expect(plural(1, "Playlist")).toBe("1 Playlist");
    expect(plural(0, "Playlist")).toBe("0 Playlists");
    expect(plural(2, "Playlist")).toBe("2 Playlists");
  });

  it("setzt den deutschen Tausenderpunkt", () => {
    expect(plural(1234, "Titel")).toBe("1.234 Titel");
  });

  // the actual reason for the change: russian inflects after 2, 3, 4
  // differently from after 5 and more. with two words handed over, the app
  // read "7 Треки", the form for 2 to 4, where "треков" belongs
  it("beugt im Russischen nach der Zahl", () => {
    spracheSetzen("ru");
    expect(plural(1, "Titel")).toBe("1 трек");
    expect(plural(3, "Titel")).toBe("3 трека");
    expect(plural(7, "Titel")).toBe("7 треков");
    spracheSetzen("de");
  });

  it("kommt im Chinesischen mit einer Form aus", () => {
    spracheSetzen("zh");
    expect(plural(1, "Titel")).toBe("1首曲目");
    expect(plural(7, "Titel")).toBe("7首曲目");
    spracheSetzen("de");
  });
});

describe("releaseLabel", () => {
  it("übersetzt die bekannten Arten", () => {
    expect(releaseLabel("single")).toBe("Single");
    expect(releaseLabel("ep")).toBe("EP");
    expect(releaseLabel("album")).toBe("Album");
  });

  it("nennt Unbekanntes Album statt gar nichts", () => {
    expect(releaseLabel("compilation")).toBe("Album");
    expect(releaseLabel("")).toBe("Album");
  });
});
