import { t } from "./i18n";

// the formats a download can be converted into.
//
// in one place and not in two. the list stood twice, once in the downloader
// and once in the settings, and that is how "OGG Vorbis" came to be on offer
// while the ffmpeg built for android carried no vorbis encoder — nothing
// held the two against each other.
//
// `scripts/ffmpeg-bauen.mjs` reads this list and refuses to build where a
// format has no encoder.

/** what `--audio-format` is given, and what the interface calls it. */
export type Format = { id: string; label: string };

/** "best" is no conversion: the file stays as the source delivered it. */
export function formate(): Format[] {
  return [
    { id: "best", label: t("Beste Qualität") },
    { id: "mp3", label: "MP3" },
    { id: "m4a", label: "M4A / AAC" },
    { id: "flac", label: "FLAC" },
    { id: "vorbis", label: "OGG Vorbis" },
  ];
}
