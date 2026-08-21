// the watchdog is meant for a fault that never repeated, so the tests carry
// the burden of proof: a call that never returns has to be named.

import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  alsText,
  auffaelligkeiten,
  beobachten,
  GEDULD_MS,
  zuruecksetzen,
} from "./wachhund";

describe("Wachhund", () => {
  beforeEach(() => {
    zuruecksetzen();
    vi.useRealTimers();
  });

  it("merkt sich nichts, was schnell zurückkommt", async () => {
    await beobachten("library_stats", () => Promise.resolve(1));
    expect(auffaelligkeiten()).toEqual([]);
    expect(alsText()).toBe("");
  });

  it("behält einen langsamen Aufruf", async () => {
    const jetzt = vi.spyOn(Date, "now");
    jetzt.mockReturnValueOnce(0).mockReturnValueOnce(GEDULD_MS + 5_000);

    await beobachten("scan_folders", () => Promise.resolve(1));

    const liste = auffaelligkeiten();
    expect(liste).toHaveLength(1);
    expect(liste[0].befehl).toBe("scan_folders");
    expect(liste[0].dauerMs).toBe(GEDULD_MS + 5_000);
    jetzt.mockRestore();
  });

  // this is the case the whole thing exists for: a call that never comes back
  it("nennt einen Aufruf, der nie zurückkommt", () => {
    const jetzt = vi.spyOn(Date, "now").mockReturnValue(0);
    void beobachten("download", () => new Promise(() => {}));
    jetzt.mockRestore();

    // shortly after the start nothing stands out yet
    expect(auffaelligkeiten(GEDULD_MS - 1)).toEqual([]);

    // once the patience is used up it is named, and without a running time
    const liste = auffaelligkeiten(GEDULD_MS + 60_000);
    expect(liste).toHaveLength(1);
    expect(liste[0].befehl).toBe("download");
    expect(liste[0].dauerMs).toBeNull();
    expect(alsText(GEDULD_MS + 60_000)).toContain("läuft noch");
  });

  it("gibt das Ergebnis unverändert weiter", async () => {
    await expect(beobachten("x", () => Promise.resolve(42))).resolves.toBe(42);
    await expect(
      beobachten("y", () => Promise.reject(new Error("kaputt"))),
    ).rejects.toThrow("kaputt");
  });

  // a failed call must not stay in the list as if it were still running
  it("vergisst einen gescheiterten Aufruf", async () => {
    const jetzt = vi.spyOn(Date, "now").mockReturnValue(0);
    await expect(
      beobachten("z", () => Promise.reject(new Error("weg"))),
    ).rejects.toThrow();
    jetzt.mockRestore();
    expect(auffaelligkeiten(GEDULD_MS + 1_000)).toEqual([]);
  });
});
