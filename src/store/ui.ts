import { create } from "zustand";
import type { Track } from "../types";

export interface Toast {
  id: number;
  message: string;
  tone: "info" | "success" | "error";
  /** Zurücknehmen der eben ausgeführten Handlung. */
  undo?: () => void | Promise<void>;
}

interface UiStore {
  toasts: Toast[];
  /** Titel, dessen Metadaten gerade bearbeitet werden. */
  editingTrack: Track | null;
  /** Titel, die einer Playlist hinzugefügt werden sollen. */
  addToPlaylistIds: number[] | null;
  nowPlayingOpen: boolean;
  queueOpen: boolean;

  notify: (message: string, tone?: Toast["tone"]) => void;
  /**
   * Meldung mit Rückgängig-Knopf. Steht länger als eine gewöhnliche Meldung,
   * weil man erst lesen und dann entscheiden muss.
   */
  notifyUndo: (message: string, undo: () => void | Promise<void>) => void;
  dismiss: (id: number) => void;
  editTrack: (track: Track | null) => void;
  openAddToPlaylist: (trackIds: number[] | null) => void;
  setNowPlayingOpen: (open: boolean) => void;
  setQueueOpen: (open: boolean) => void;
}

let nextToastId = 1;

export const useUi = create<UiStore>((set) => ({
  toasts: [],
  editingTrack: null,
  addToPlaylistIds: null,
  nowPlayingOpen: false,
  queueOpen: false,

  notify: (message, tone = "info") => {
    const id = nextToastId++;
    set((state) => ({ toasts: [...state.toasts, { id, message, tone }] }));
    window.setTimeout(
      () => {
        set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
      },
      tone === "error" ? 7000 : 4000,
    );
  },
  notifyUndo: (message, undo) => {
    const id = nextToastId++;
    set((state) => ({
      toasts: [...state.toasts, { id, message, tone: "info", undo }],
    }));
    window.setTimeout(() => {
      set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
    }, 9000);
  },
  dismiss: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  editTrack: (track) => set({ editingTrack: track }),
  openAddToPlaylist: (trackIds) => set({ addToPlaylistIds: trackIds }),
  /*
   * Vollbild-Ansicht und Warteschlange schließen sich gegenseitig aus.
   *
   * In beide Richtungen: Die Lyrics legen sich über die ganze App, die
   * Warteschlange fährt daneben ein. Beides zugleich hieße, dass die
   * Warteschlange hinter den Lyrics läge, sichtbar geöffnet laut Schalter,
   * aber verdeckt. Wer den einen Schalter drückt, meint immer den Wechsel.
   */
  setNowPlayingOpen: (open) =>
    set(
      open
        ? { nowPlayingOpen: true, queueOpen: false }
        : { nowPlayingOpen: false },
    ),
  setQueueOpen: (open) =>
    set(
      open ? { queueOpen: true, nowPlayingOpen: false } : { queueOpen: false },
    ),
}));
