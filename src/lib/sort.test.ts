import { describe, expect, it } from "vitest";
import { vergleicheTitel } from "./sort";
import type { Track } from "../types";

let naechsteId = 1;

/** Titel mit allen Pflichtfeldern; für den Test zählen nur wenige davon. */
function titel(teil: Partial<Track>): Track {
  return {
    id: naechsteId++,
    path: "/musik/datei.mp3",
    title: "Titel",
    artistId: 1,
    artistName: "Künstler",
    artists: [],
    albumId: 1,
    albumTitle: "Album",
    releaseType: "album",
    trackNo: null,
    discNo: null,
    durationMs: 200_000,
    genre: null,
    year: null,
    format: "mp3",
    hasCover: false,
    hasLyrics: false,
    addedAt: 0,
    playCount: 0,
    favorite: false,
    source: null,
    deleted: false,
    ...teil,
  };
}

/** Sortiert und gibt nur zurück, was sich im Test ablesen lässt. */
function ordne(
  tracks: Track[],
  ordnung: string,
  zeige: (t: Track) => string,
): string[] {
  return [...tracks].sort((a, b) => vergleicheTitel(a, b, ordnung)).map(zeige);
}

describe("Ordnung nach Künstler", () => {
  it("gruppiert alphabetisch, darin Releases neu vor alt und Titel nach Nummer", () => {
    const tracks = [
      titel({
        artistName: "Beta",
        albumTitle: "Zweites",
        year: 2020,
        trackNo: 1,
      }),
      titel({
        artistName: "Alpha",
        albumTitle: "Altes",
        year: 2010,
        trackNo: 1,
      }),
      titel({
        artistName: "Alpha",
        albumTitle: "Neues",
        year: 2024,
        trackNo: 2,
      }),
      titel({
        artistName: "Alpha",
        albumTitle: "Neues",
        year: 2024,
        trackNo: 1,
      }),
    ];

    expect(
      ordne(
        tracks,
        "artist",
        (t) => `${t.artistName}/${t.albumTitle}/${t.trackNo}`,
      ),
    ).toEqual([
      "Alpha/Neues/1",
      "Alpha/Neues/2",
      "Alpha/Altes/1",
      "Beta/Zweites/1",
    ]);
  });

  it("sortiert Umlaute wie Grundbuchstaben, nicht hinter Z", () => {
    const tracks = [
      titel({ artistName: "Zaz" }),
      titel({ artistName: "Ätna" }),
      titel({ artistName: "Adele" }),
    ];
    expect(ordne(tracks, "artist", (t) => t.artistName)).toEqual([
      "Adele",
      "Ätna",
      "Zaz",
    ]);
  });

  it("stellt Titel ohne Jahresangabe ans Ende des Künstlers", () => {
    const tracks = [
      titel({ artistName: "A", albumTitle: "Ohne Jahr", year: null }),
      titel({ artistName: "A", albumTitle: "Mit Jahr", year: 1999 }),
    ];
    expect(ordne(tracks, "artist", (t) => t.albumTitle)).toEqual([
      "Mit Jahr",
      "Ohne Jahr",
    ]);
  });

  it("hält gleichjährige Releases zusammen statt sie zu verschränken", () => {
    // Ohne den Albumnamen als Anker stünden die Titelnummern beider Alben
    // abwechselnd untereinander.
    const tracks = [
      titel({ artistName: "A", albumTitle: "Eins", year: 2020, trackNo: 2 }),
      titel({ artistName: "A", albumTitle: "Zwei", year: 2020, trackNo: 1 }),
      titel({ artistName: "A", albumTitle: "Eins", year: 2020, trackNo: 1 }),
      titel({ artistName: "A", albumTitle: "Zwei", year: 2020, trackNo: 2 }),
    ];
    expect(
      ordne(tracks, "artist", (t) => `${t.albumTitle}/${t.trackNo}`),
    ).toEqual(["Eins/1", "Eins/2", "Zwei/1", "Zwei/2"]);
  });
});

describe("übrige Ordnungen", () => {
  it("nach Album: Datenträger vor Titelnummer", () => {
    const tracks = [
      titel({ albumTitle: "Werk", discNo: 2, trackNo: 1 }),
      titel({ albumTitle: "Werk", discNo: 1, trackNo: 9 }),
    ];
    expect(ordne(tracks, "album", (t) => `${t.discNo}-${t.trackNo}`)).toEqual([
      "1-9",
      "2-1",
    ]);
  });

  it("nach Jahr: neueste zuerst", () => {
    const tracks = [
      titel({ year: 1999 }),
      titel({ year: 2024 }),
      titel({ year: 2010 }),
    ];
    expect(ordne(tracks, "year", (t) => String(t.year))).toEqual([
      "2024",
      "2010",
      "1999",
    ]);
  });

  it("nach Titel: alphabetisch, bei Gleichstand entscheidet der Künstler", () => {
    const tracks = [
      titel({ title: "Echo", artistName: "Zaz" }),
      titel({ title: "Echo", artistName: "Adele" }),
      titel({ title: "Anfang", artistName: "Zaz" }),
    ];
    expect(ordne(tracks, "title", (t) => `${t.title}/${t.artistName}`)).toEqual(
      ["Anfang/Zaz", "Echo/Adele", "Echo/Zaz"],
    );
  });

  it("ohne Angabe: zuletzt hinzugefügt steht oben", () => {
    const tracks = [
      titel({ title: "alt", addedAt: 100 }),
      titel({ title: "neu", addedAt: 300 }),
      titel({ title: "mittel", addedAt: 200 }),
    ];
    expect(ordne(tracks, "added", (t) => t.title)).toEqual([
      "neu",
      "mittel",
      "alt",
    ]);
    // Ein unbekannter Schlüssel darf nicht durchfallen, sondern landet hier.
    expect(ordne(tracks, "quatsch", (t) => t.title)).toEqual([
      "neu",
      "mittel",
      "alt",
    ]);
  });
});
