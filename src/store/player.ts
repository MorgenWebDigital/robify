// the state of playback, mirrored out of the rust side.
// note: what is heard is decided there, not here — this store only follows
// the events and holds what the interface needs to draw.

import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api } from "../lib/api";
import type {
  PlayerState,
  PlayerTick,
  RepeatMode,
  SleepTimerMode,
  Track,
} from "../types";

interface PlayerStore extends PlayerState {
  currentTrack: Track | null;
  queueTracks: Track[];
  ready: boolean;
  lastError: string | null;

  init: () => Promise<() => void>;
  playTracks: (trackIds: number[], startIndex?: number) => Promise<void>;
  toggle: () => Promise<void>;
  next: () => Promise<void>;
  previous: () => Promise<void>;
  seek: (positionMs: number) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  toggleMute: () => Promise<void>;
  cycleRepeat: () => Promise<void>;
  toggleShuffle: () => Promise<void>;
  setSleepTimer: (
    mode: SleepTimerMode | null,
    minutes?: number,
  ) => Promise<void>;
  clearError: () => void;
}

const REPEAT_ORDER: RepeatMode[] = ["off", "all", "one"];

export const usePlayer = create<PlayerStore>((set, get) => ({
  playing: false,
  trackId: null,
  positionMs: 0,
  durationMs: 0,
  volume: 1,
  muted: false,
  repeat: "off",
  shuffle: false,
  queue: [],
  queueIndex: null,
  order: [],
  orderPos: null,
  sleepTimer: null,
  currentTrack: null,
  queueTracks: [],
  ready: false,
  lastError: null,

  init: async () => {
    const applyState = async (state: PlayerState) => {
      const previous = get();
      set({ ...state, ready: true });

      if (state.trackId !== previous.trackId) {
        if (state.trackId === null) {
          set({ currentTrack: null });
        } else {
          try {
            set({ currentTrack: await api.getTrack(state.trackId) });
          } catch {
            set({ currentTrack: null });
          }
        }
      }

      const queueChanged =
        state.queue.length !== previous.queue.length ||
        state.queue.some((id, index) => previous.queue[index] !== id);
      if (queueChanged) {
        try {
          set({ queueTracks: await api.getTracks(state.queue) });
        } catch {
          set({ queueTracks: [] });
        }
      }
    };

    const unlisteners = await Promise.all([
      listen<PlayerState>(
        "player:state",
        (event) => void applyState(event.payload),
      ),
      listen<PlayerTick>("player:tick", (event) => {
        const tick = event.payload;
        // write only where something actually changed: every `set` notifies
        // all subscribers, and player, full screen and the running lyrics
        // hang off the position
        const jetzt = get();
        if (
          jetzt.playing !== tick.playing ||
          jetzt.positionMs !== tick.positionMs ||
          jetzt.durationMs !== tick.durationMs
        ) {
          set({
            playing: tick.playing,
            positionMs: tick.positionMs,
            durationMs: tick.durationMs,
          });
        }
        if (tick.sleepRemainingMs !== null) {
          const timer = get().sleepTimer;
          if (timer) {
            set({
              sleepTimer: { ...timer, remainingMs: tick.sleepRemainingMs },
            });
          }
        }
      }),
      listen<string>("player:error", (event) =>
        set({ lastError: event.payload }),
      ),
      listen<string>("player:sleep-timer-fired", (event) =>
        set({ lastError: event.payload, sleepTimer: null }),
      ),
    ]);

    try {
      await applyState(await api.playerState());
    } catch {
      set({ ready: true });
    }

    return () => unlisteners.forEach((off) => off());
  },

  playTracks: async (trackIds, startIndex = 0) => {
    if (trackIds.length === 0) return;
    await api.playTracks(trackIds, startIndex);
  },
  toggle: () => api.toggle(),
  next: () => api.next(),
  previous: () => api.previous(),
  seek: async (positionMs) => {
    set({ positionMs });
    await api.seek(Math.max(0, Math.round(positionMs)));
  },
  setVolume: async (volume) => {
    set({ volume });
    await api.setVolume(volume);
  },
  toggleMute: async () => {
    const muted = !get().muted;
    set({ muted });
    await api.setMuted(muted);
  },
  cycleRepeat: async () => {
    const nextMode =
      REPEAT_ORDER[
        (REPEAT_ORDER.indexOf(get().repeat) + 1) % REPEAT_ORDER.length
      ];
    set({ repeat: nextMode });
    await api.setRepeat(nextMode);
  },
  toggleShuffle: async () => {
    const shuffle = !get().shuffle;
    set({ shuffle });
    await api.setShuffle(shuffle);
  },
  setSleepTimer: async (mode, minutes) => {
    await api.setSleepTimer(mode, minutes);
    if (mode === null) {
      set({ sleepTimer: null });
    } else {
      const totalMs = mode === "duration" ? (minutes ?? 30) * 60_000 : 0;
      set({ sleepTimer: { mode, totalMs, remainingMs: totalMs } });
    }
  },
  clearError: () => set({ lastError: null }),
}));
