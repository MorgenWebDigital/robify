import { beforeEach, describe, expect, it, vi } from "vitest";
import { SUCHE_HAELT_MS, useDownloader } from "./downloader";

// the search field of the downloader keeps for a moment across a change of
// tab and no longer. both directions were annoying in use: retyping after a
// mis-tap, and the old text still standing much later, which the next thing
// typed then hangs itself onto
describe("Suche über einen Tabwechsel", () => {
  beforeEach(() => {
    useDownloader.setState({
      input: "",
      plan: null,
      jobs: [],
      verlassenAm: null,
      stapel: {},
    });
    vi.restoreAllMocks();
  });

  const tippen = (text: string) => useDownloader.getState().setInput(text);
  const gehen = () => useDownloader.getState().seiteVerlassen();
  const kommen = () => useDownloader.getState().seiteBetreten();

  it("hält den Text bei einem kurzen Verklicken", () => {
    tippen("bicep glue");
    const jetzt = vi.spyOn(Date, "now");
    jetzt.mockReturnValueOnce(0);
    gehen();
    jetzt.mockReturnValueOnce(SUCHE_HAELT_MS - 1);
    expect(kommen()).toBe(false);
    expect(useDownloader.getState().input).toBe("bicep glue");
  });

  it("leert ihn nach längerer Abwesenheit", () => {
    tippen("bicep glue");
    useDownloader.setState({ plan: { label: "bicep glue" } as never });
    const jetzt = vi.spyOn(Date, "now");
    jetzt.mockReturnValueOnce(0);
    gehen();
    jetzt.mockReturnValueOnce(SUCHE_HAELT_MS + 1);
    expect(kommen()).toBe(true);
    expect(useDownloader.getState().input).toBe("");
    // the result list goes with it, otherwise "hits for …" would name a
    // search nobody can see any more
    expect(useDownloader.getState().plan).toBeNull();
  });

  // the downloads live in the store precisely so a change of tab does not
  // take them along. clearing the search must not touch them
  it("lässt laufende Downloads unberührt", () => {
    const job = {
      id: "1",
      label: "Bicep – Glue",
      progress: null,
      outcome: null,
      error: null,
      autoImport: false,
    };
    tippen("bicep glue");
    useDownloader.setState({ jobs: [job] });
    const jetzt = vi.spyOn(Date, "now");
    jetzt.mockReturnValueOnce(0);
    gehen();
    jetzt.mockReturnValueOnce(SUCHE_HAELT_MS + 60_000);
    kommen();
    expect(useDownloader.getState().jobs).toEqual([job]);
  });

  it("meldet nichts, wo nichts zu leeren war", () => {
    const jetzt = vi.spyOn(Date, "now");
    jetzt.mockReturnValueOnce(0);
    gehen();
    jetzt.mockReturnValueOnce(SUCHE_HAELT_MS + 1);
    expect(kommen()).toBe(false);
  });

  it("beim ersten Öffnen wird nichts geleert", () => {
    tippen("etwas");
    expect(kommen()).toBe(false);
    expect(useDownloader.getState().input).toBe("etwas");
  });
});
