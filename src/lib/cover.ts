/**
 * Cover werden über das eigene `robify:`-Protokoll direkt aus der Datenbank
 * ausgeliefert. Windows und Android brauchen dafür die http-Variante.
 */
const useHttpScheme =
  typeof navigator !== "undefined" &&
  (navigator.userAgent.includes("Windows") ||
    navigator.userAgent.includes("Android"));

const base = useHttpScheme ? "http://robify.localhost" : "robify://localhost";

/** Wird nach Cover-Änderungen erhöht, damit der Cache nicht das alte Bild zeigt. */
let cacheBuster = 0;

export function bustCoverCache(): void {
  cacheBuster += 1;
}

export function albumCover(albumId: number | null | undefined): string | null {
  if (!albumId) return null;
  return `${base}/cover/album/${albumId}?v=${cacheBuster}`;
}

export function artistImage(
  artistId: number | null | undefined,
): string | null {
  if (!artistId) return null;
  return `${base}/cover/artist/${artistId}?v=${cacheBuster}`;
}

export function playlistCover(
  playlistId: number | null | undefined,
): string | null {
  if (!playlistId) return null;
  return `${base}/cover/playlist/${playlistId}?v=${cacheBuster}`;
}

export function dataUrl(
  base64: string | null,
  mime: string | null,
): string | null {
  if (!base64) return null;
  return `data:${mime || "image/jpeg"};base64,${base64}`;
}
