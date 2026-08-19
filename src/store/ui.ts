import { create } from "zustand";
import type { Track } from "../types";

export interface Toast {
  id: number;
  message: string;
  tone: "info" | "success" | "error";
  /** undoes the action just carried out. */
  undo?: () => void | Promise<void>;
}

interface UiStore {
  toasts: Toast[];
  /** the track whose metadata is being edited. */
  editingTrack: Track | null;
  /** tracks about to be added to a playlist. */
  addToPlaylistIds: number[] | null;
  nowPlayingOpen: boolean;
  queueOpen: boolean;

  notify: (message: string, tone?: Toast["tone"]) => void;
  /**
   * a message with an undo button.
   *
   * stands longer than an ordinary message because one has to read first and
   * decide afterwards.
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
  // the full screen view and the queue exclude each other.
  //
  // in both directions: the lyrics lay themselves over the whole app while
  // the queue slides in beside it. both at once would mean the queue lying
  // behind the lyrics, open according to its switch but covered. whoever
  // presses either switch always means the change
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
