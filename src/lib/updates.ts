import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import type { TranslationKey } from "./i18n/translations";
import { useMeetingsStore } from "../state/meetingsStore";

type Phase = "idle" | "checking" | "available" | "none" | "downloading" | "installing" | "error";

interface UpdateState {
  current: string;
  phase: Phase;
  update: Update | null;
  downloaded: number;
  total: number;
  bytesPerSec: number;
  etaSeconds: number | null;
  error: TranslationKey | null;
  checkNow: (silent?: boolean) => Promise<void>;
  install: () => Promise<void>;
}

/**
 * Updates from GitHub releases: `latest.json` names a signed bundle per platform (see
 * `.github/workflows/release.yml`), the updater plugin verifies the signature, installs, and the
 * app restarts into the new version.
 */
export const useUpdates = create<UpdateState>((set, get) => ({
  current: "",
  phase: "idle",
  update: null,
  downloaded: 0,
  total: 0,
  bytesPerSec: 0,
  etaSeconds: null,
  error: null,

  checkNow: async (silent = false) => {
    set({ phase: "checking", error: null, current: await getVersion() });
    try {
      const update = await check();
      set({ update, phase: update ? "available" : "none" });
    } catch (e) {
      // No releases yet or offline: stay quiet on the automatic check.
      console.warn("update check failed", e);
      set({ phase: silent ? "idle" : "error", error: updateError(e) });
    }
  },

  install: async () => {
    const { update } = get();
    if (!update) return;
    set({ phase: "downloading", downloaded: 0, total: 0, etaSeconds: null });
    let windowStart = performance.now();
    let windowBytes = 0;
    let ema = 0;
    try {
      await update.download((ev) => {
        if (ev.event === "Started") {
          set({ total: ev.data.contentLength ?? 0 });
        } else if (ev.event === "Progress") {
          windowBytes += ev.data.chunkLength;
          const downloaded = get().downloaded + ev.data.chunkLength;
          const dt = (performance.now() - windowStart) / 1000;
          if (dt >= 1) {
            const inst = windowBytes / dt;
            ema = ema ? 0.8 * ema + 0.2 * inst : inst;
            windowStart = performance.now();
            windowBytes = 0;
          }
          const total = get().total;
          set({ downloaded, bytesPerSec: ema, etaSeconds: ema && total ? (total - downloaded) / ema : null });
        }
      });
      set({ phase: "installing" });
      // The local model server must not hold files the installer replaces.
      await invoke("prepare_update");
      await update.install();
      await relaunch();
    } catch (e) {
      console.warn("update install failed", e);
      set({ phase: "error", error: updateError(e) });
    }
  },
}));

/** Map the updater plugin's raw errors to something a reader can act on. */
function updateError(e: unknown): TranslationKey {
  const msg = String(e).toLowerCase();
  if (msg.includes("being recorded")) return "u.err.busy";
  // A release is published per platform as each build finishes, so for a few minutes latest.json
  // may not list this system yet.
  if (msg.includes("platform") || msg.includes("404")) return "u.err.pending";
  if (msg.includes("signature")) return "u.err.signature";
  if (/network|dns|connect|timed? ?out|sending request|offline/.test(msg)) return "u.err.offline";
  return "u.err.generic";
}

/** True while a meeting is being recorded or transcribed; installing would cut it short. */
export function meetingBusy(): boolean {
  const status = useMeetingsStore.getState().status;
  return !!status?.recording || !!status?.processing || (status?.queued.length ?? 0) > 0;
}
