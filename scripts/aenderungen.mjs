// pulls the notes of one version out of the changelog.
//
// the release workflow writes them into the release, and robify reads them
// from there when someone asks what has changed. without this step the
// release page would say "see changelog.md", and an app that cannot open the
// file would have nothing to show.
//
// usage: node scripts/aenderungen.mjs v0.2.0
//
// without an argument it delivers the section `[Unreleased]`, the place where
// changes are collected between two releases.

import { readFileSync } from "node:fs";

/**
 * the text under one heading of the changelog, without the heading itself.
 *
 * the headings look like `## [0.2.0] - 2026-08-20`, and the link definitions
 * at the foot of the file (`[0.2.0]: https://…`) must not be mistaken for
 * them: they start at the beginning of a line as well but carry no `##`.
 */
export function abschnitt(inhalt, fassung) {
  const gesucht = fassung.replace(/^v/, "");
  const zeilen = inhalt.split("\n");
  const anfang = zeilen.findIndex((zeile) =>
    zeile.startsWith(`## [${gesucht}]`),
  );
  if (anfang === -1) return "";

  const rest = zeilen.slice(anfang + 1);
  const ende = rest.findIndex((zeile) => zeile.startsWith("## "));
  const teil = ende === -1 ? rest : rest.slice(0, ende);

  // the link definitions at the foot of the file belong to no version.
  // behind the newest one stands no further heading, so without this they
  // would be taken along and end up in the release as three loose lines
  while (
    teil.length > 0 &&
    /^(\[[^\]]+\]:\s|\s*$)/.test(teil[teil.length - 1])
  ) {
    teil.pop();
  }

  return teil.join("\n").trim();
}

const [, , fassung = "Unreleased", datei = "CHANGELOG.md"] = process.argv;
process.stdout.write(abschnitt(readFileSync(datei, "utf8"), fassung));
