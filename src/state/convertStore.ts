import { create } from "zustand";
import { mediaExtractAudio, onConvertProgress, type Converted } from "../lib/tauri/mediaCommands";
import { baseName, folderOf, joinPath } from "../lib/media";

/**
 * The video converter's list: files waiting, the one being converted, and what each became. It
 * lives outside the screen so a conversion keeps going (and is still listed) while the user is
 * elsewhere in the app. One file at a time — each already uses every core it can.
 */
export interface ConvertJob {
  id: string;
  path: string;
  status: "waiting" | "running" | "done" | "failed";
  stage: "decode" | "encode";
  fraction: number;
  result?: Converted;
  error?: string;
}

interface ConvertState {
  jobs: ConvertJob[];
  /** Where the audio goes: `null` beside each video. */
  outDir: string | null;
  add: (paths: string[]) => void;
  remove: (id: string) => void;
  clearFinished: () => void;
  setOutDir: (dir: string | null) => void;
}

let listening = false;
let running = false;

function patch(id: string, change: Partial<ConvertJob>) {
  useConvertStore.setState((state) => ({ jobs: state.jobs.map((job) => (job.id === id ? { ...job, ...change } : job)) }));
}

async function pump() {
  if (running) return;
  running = true;
  try {
    for (;;) {
      const next = useConvertStore.getState().jobs.find((job) => job.status === "waiting");
      if (!next) return;
      patch(next.id, { status: "running", stage: "decode", fraction: 0 });
      const dir = useConvertStore.getState().outDir ?? folderOf(next.path);
      const target = joinPath(dir, `${baseName(next.path)}.m4a`);
      try {
        const result = await mediaExtractAudio(next.path, target, next.id);
        patch(next.id, { status: "done", fraction: 1, result });
      } catch (error) {
        patch(next.id, { status: "failed", error: String(error) });
      }
    }
  } finally {
    running = false;
  }
}

export const useConvertStore = create<ConvertState>((set) => ({
  jobs: [],
  outDir: null,
  add: (paths) => {
    if (!listening) {
      listening = true;
      void onConvertProgress(({ runId, stage, fraction }) => patch(runId, { stage, fraction }));
    }
    set((state) => ({
      jobs: [
        ...state.jobs,
        ...paths
          .filter((path) => !state.jobs.some((job) => job.path === path && job.status !== "done" && job.status !== "failed"))
          .map((path) => ({ id: `convert-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`, path, status: "waiting" as const, stage: "decode" as const, fraction: 0 })),
      ],
    }));
    void pump();
  },
  remove: (id) => set((state) => ({ jobs: state.jobs.filter((job) => job.id !== id || job.status === "running") })),
  clearFinished: () => set((state) => ({ jobs: state.jobs.filter((job) => job.status === "waiting" || job.status === "running") })),
  setOutDir: (outDir) => set({ outDir }),
}));
