import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

// the page talks to the backend the moment it appears. without a stand-in
// every test would die on the missing tauri bridge instead of on what it
// actually checks
const status = {
  ytdlpPath: "eingebaut",
  ytdlpVersion: "2026.08.19",
  ffmpegAvailable: true,
  jsRuntime: null as string | null,
  jsRuntimeRelevant: false,
  activeJobs: [] as string[],
};

vi.mock("../lib/api", () => ({
  api: {
    downloaderStatus: () => Promise.resolve(status),
    resolveInput: () => Promise.resolve(null),
  },
  errorMessage: (fehler: unknown) => String(fehler),
  meldungText: (text: string) => text,
  fallback: () => () => null,
}));

import { DownloaderPage } from "./DownloaderPage";
import { formate } from "../lib/formate";
import { useDownloader } from "../store/downloader";

describe("Downloader-Seite", () => {
  beforeEach(() => {
    status.ffmpegAvailable = true;
    useDownloader.setState({ input: "", plan: null, jobs: [], busy: false });
  });

  it("bietet jedes Format aus der gemeinsamen Liste an", async () => {
    render(<DownloaderPage />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Format" }),
    );

    for (const format of formate()) {
      expect(
        screen.getAllByText(format.label).length,
        `„${format.label}“ fehlt in der Auswahl`,
      ).toBeGreaterThan(0);
    }
  });

  // on android robify carries its own ffmpeg, on a desktop it may be missing.
  // the sentence is the only thing that tells the user why only the original
  // format works, so it must appear exactly then and not otherwise
  it("nennt ein fehlendes ffmpeg", async () => {
    status.ffmpegAvailable = false;
    render(<DownloaderPage />);
    expect(await screen.findByText(/ffmpeg fehlt/)).toBeInTheDocument();
  });

  it("schweigt, wo ffmpeg da ist", async () => {
    render(<DownloaderPage />);
    await screen.findByRole("button", { name: "Format" });
    expect(screen.queryByText(/ffmpeg fehlt/)).not.toBeInTheDocument();
  });
});
