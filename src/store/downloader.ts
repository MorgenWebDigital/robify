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
  /** batch downloads land in the library without asking. */
  autoImport: boolean;
}

/**
 * the state of the downloader lives outside the page.
 *
 * otherwise everything disappears on switching to another tab: react tears
 * the page down and with it the result list and the running downloads. the
 * downloads themselves keep running on the rust side, only their display
 * would be gone. this store listens for the progress events as well, not the
 * page.
 */
/** what has become of one entry of a batch. */
export type Stapelstand = {
  zustand: "laeuft" | "fertig" | "pruefen" | "fehler";
  /** the job it hangs on, for the progress. */
  jobId?: string;
};

/**
 * how long the search survives a change of tab.
 *
 * two cases pull in opposite directions. whoever taps the wrong tab and comes
 * straight back wants to find what they typed still standing — retyping it
 * would be a punishment for a slip. whoever returns much later wants a clean
 * field: the old text is then no longer a starting point but something in the
 * way, and the next thing typed hangs itself onto it.
 *
 * ten seconds are enough for the slip and far too short for a detour.
 */
export const SUCHE_HAELT_MS = 10_000;

interface DownloaderStore {
  input: string;
  plan: LinkPlan | null;
  jobs: Job[];
  busy: boolean;
  review: { job: Job; metadata: TrackMetadata } | null;
  /**
   * what has become of each entry of the open batch, by its position.
   *
   * lies here and not with the page: a change of tab tears the page down, and
   * with it went the marks — while the download carried on. `setPlan` clears
   * it, because with a new plan the positions mean something else.
   */
  stapel: Record<number, Stapelstand>;
  /** when the page was last left, as `Date.now()`. */
  verlassenAm: number | null;

  setInput: (value: string) => void;
  /** notes that the page has gone; the search keeps for a moment. */
  seiteVerlassen: () => void;
  /**
   * back on the page: hands back whether the search was cleared.
   *
   * running downloads and an open review are untouched — those have nothing
   * to do with the search field, and losing them at a change of tab was a
   * bug of its own once.
   */
  seiteBetreten: () => boolean;
  setPlan: (plan: LinkPlan | null) => void;
  setStand: (stelle: number, stand: Stapelstand) => void;
  setBusy: (busy: boolean) => void;
  setReview: (review: { job: Job; metadata: TrackMetadata } | null) => void;
  addJob: (job: Job) => void;
  patchJob: (id: string, changes: Partial<Job>) => void;
  removeJob: (id: string) => void;
  clearJobs: () => void;

  /** to be called once at the start of the app. */
  init: () => Promise<() => void>;
}

export const useDownloader = create<DownloaderStore>((set) => ({
  input: "",
  plan: null,
  jobs: [],
  busy: false,
  review: null,
  stapel: {},
  verlassenAm: null,

  setInput: (input) => set({ input }),
  seiteVerlassen: () => set({ verlassenAm: Date.now() }),
  seiteBetreten: () => {
    let geleert = false;
    set((state) => {
      const zulange =
        state.verlassenAm !== null &&
        Date.now() - state.verlassenAm > SUCHE_HAELT_MS;
      if (!zulange || (state.input === "" && state.plan === null)) {
        return { verlassenAm: null };
      }
      geleert = true;
      // the result list goes with it: "hits for …" above an empty field
      // would name a search nobody can see any more
      return { verlassenAm: null, input: "", plan: null, stapel: {} };
    });
    return geleert;
  },
  setPlan: (plan) => set({ plan, stapel: {} }),
  setStand: (stelle, stand) =>
    set((state) => ({ stapel: { ...state.stapel, [stelle]: stand } })),
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
