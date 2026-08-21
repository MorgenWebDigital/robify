// covers are served straight out of the database over the custom `robify:`
// scheme. windows and android need the http variant for it
const useHttpScheme =
  typeof navigator !== "undefined" &&
  (navigator.userAgent.includes("Windows") ||
    navigator.userAgent.includes("Android"));

const base = useHttpScheme ? "http://robify.localhost" : "robify://localhost";

// raised after cover changes so the cache does not show the old image
let cacheBuster = 0;

/** invalidates every cover url, to be called after a cover has changed */
export function bustCoverCache(): void {
  cacheBuster += 1;
}

/** url of an album cover, `null` without an album */
export function albumCover(albumId: number | null | undefined): string | null {
  if (!albumId) return null;
  return `${base}/cover/album/${albumId}?v=${cacheBuster}`;
}

/** url of an artist image, `null` without an artist */
export function artistImage(
  artistId: number | null | undefined,
): string | null {
  if (!artistId) return null;
  return `${base}/cover/artist/${artistId}?v=${cacheBuster}`;
}

/** url of a playlist cover, `null` without a playlist */
export function playlistCover(
  playlistId: number | null | undefined,
): string | null {
  if (!playlistId) return null;
  return `${base}/cover/playlist/${playlistId}?v=${cacheBuster}`;
}

/** turns base64 image data into a data url, `null` without data */
export function dataUrl(
  base64: string | null,
  mime: string | null,
): string | null {
  if (!base64) return null;
  return `data:${mime || "image/jpeg"};base64,${base64}`;
}
