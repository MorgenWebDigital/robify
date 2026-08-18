import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type {
  DownloadOutcome,
  DownloadProgress,
  LinkPlan,
  TrackMetadata,
} from "../types";

export interface Job {
  id: string;
  label: string;
  progress: DownloadProgress | null;
  outcome: DownloadOutcome | null;
  error: string | null;
  /** Stapel-Downloads landen ohne Rückfrage in der Bibliothek. */
  autoImport: boolean;
}

/**
 * Der Zustand des Downloaders lebt außerhalb der Seite.
 *
 * Sonst verschwindet beim Wechsel auf einen anderen Tab alles: React baut die
 * Seite ab, und mit ihr die Trefferliste und die laufenden Downloads. Die
 * Downloads selbst laufen im Rust-Teil weiter, nur ihre Anzeige wäre weg.
 * Auch die Fortschrittsmeldungen hört dieser Speicher, nicht die Seite.
 */
interface DownloaderStore {
  input: string;
  plan: LinkPlan | null;
  jobs: Job[];
  busy: boolean;
  review: { job: Job; metadata: TrackMetadata } | null;

  setInput: (value: string) => void;
  setPlan: (plan: LinkPlan | null) => void;
  setBusy: (busy: boolean) => void;
  setReview: (review: { job: Job; metadata: TrackMetadata } | null) => void;
  addJob: (job: Job) => void;
  patchJob: (id: string, changes: Partial<Job>) => void;
  removeJob: (id: string) => void;
  clearJobs: () => void;

  /** Einmal beim Start der App aufrufen. */
  init: () => Promise<() => void>;
}

export const useDownloader = create<DownloaderStore>((set) => ({
  input: "",
  plan: null,
  jobs: [],
  busy: false,
  review: null,

  setInput: (input) => set({ input }),
  setPlan: (plan) => set({ plan }),
  setBusy: (busy) => set({ busy }),
  setReview: (review) => set({ review }),

  addJob: (job) => set((state) => ({ jobs: [job, ...state.jobs] })),
  patchJob: (id, changes) =>
    set((state) => ({
      jobs: state.jobs.map((job) =>
        job.id === id ? { ...job, ...changes } : job,
      ),
    })),
  removeJob: (id) =>
    set((state) => ({ jobs: state.jobs.filter((job) => job.id !== id) })),
  clearJobs: () => set({ jobs: [] }),

  init: async () => {
    const off = await listen<DownloadProgress>("download:progress", (event) => {
      const progress = event.payload;
      set((state) => ({
        jobs: state.jobs.map((job) =>
          job.id === progress.jobId ? { ...job, progress } : job,
        ),
      }));
    });
    return off;
  },
}));
