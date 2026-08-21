import { describe, expect, it } from "vitest";
import { formate } from "./formate";

describe("Formatliste", () => {
  // the list decides two things at once: what stands in the interface, and
  // what `scripts/ffmpeg-bauen.mjs` builds an encoder for. it reads exactly
  // this file, so an id added here without an encoder stops the build
  it("führt genau die Formate, für die ein Kodierer gebaut wird", () => {
    expect(formate().map((f) => f.id)).toEqual([
      "best",
      "mp3",
      "m4a",
      "flac",
      "vorbis",
    ]);
  });

  it("gibt jedem Format eine sichtbare Beschriftung", () => {
    for (const format of formate()) {
      expect(format.label.trim()).not.toBe("");
    }
  });

  // "best" means: do not convert. yt-dlp is given the id as
  // `--audio-format`, and there is no encoder called "best"
  it("hält „best“ an erster Stelle", () => {
    expect(formate()[0].id).toBe("best");
  });
});
