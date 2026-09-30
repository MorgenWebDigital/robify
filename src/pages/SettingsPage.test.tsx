import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// the page asks the backend for paths and versions as soon as it appears
vi.mock("../lib/api", () => ({
  api: {
    appPaths: () =>
      Promise.resolve({
        library: "/musik",
        downloads: "/downloads",
        database: "/robify.db",
        backups: "/sicherungen",
      }),
    downloaderStatus: () =>
      Promise.resolve({
        ytdlpPath: "eingebaut",
        ytdlpVersion: "2026.08.19",
        ffmpegAvailable: true,
        jsRuntime: null,
        jsRuntimeRelevant: false,
        ffmpegHolbar: false,
        jsRuntimeHolbar: false,
        activeJobs: [],
      }),
    checkLibrary: () =>
      Promise.resolve({ missingCount: 0, orphanCount: 0, orphanSamples: [] }),
  },
  errorMessage: (fehler: unknown) => String(fehler),
  fallback: () => () => null,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(null),
}));

import { SettingsPage } from "./SettingsPage";
import { formate } from "../lib/formate";
import { useLibrary } from "../store/library";
import { einstellungen } from "../test/einstellungen";

describe("Einstellungen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // without settings the page shows no downloader section at all — they
    // come out of the database in the real app
    useLibrary.setState({ settings: einstellungen() });
  });

  // the format list stood twice, once here and once in the downloader. that
  // is how "OGG Vorbis" came to be on offer while the android build carried
  // no vorbis encoder. both read the same module now, and this holds it
  it("nennt dasselbe Standardformat wie der Downloader", async () => {
    render(<SettingsPage />);
    const auswahl = await screen.findByRole("button", {
      name: "Standardformat",
    });
    const beschriftungen = formate().map((f) => f.label);
    expect(beschriftungen).toContain(auswahl.textContent?.trim());
  });
});
