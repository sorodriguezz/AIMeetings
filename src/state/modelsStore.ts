import { create } from "zustand";
import { dictationCancelInstall, dictationInstall, dictationRemove, onDictationProgress, type DictationProgress } from "../lib/tauri/dictationCommands";
import { llmCancelInstall, llmDelete, llmInstall, modelsOverview, onLlmDownload, speechMeasure, type LlmProgress, type ModelsOverview } from "../lib/tauri/modelsCommands";
import { pushErrorToast } from "./toastStore";
import { useMeetingsStore } from "./meetingsStore";

interface ModelsState {
  overview: ModelsOverview | null;
  loading: boolean;
  /** Speech downloads by item (`engine` or a model id). */
  speechProgress: Record<string, DictationProgress>;
  llmProgress: Record<string, LlmProgress>;
  measuring: string | null;
  load: () => Promise<void>;
  installSpeech: (id: string) => Promise<void>;
  cancelSpeech: () => Promise<void>;
  removeSpeech: (id: string) => Promise<void>;
  measureSpeech: (id: string) => Promise<void>;
  installLlm: (id: string) => Promise<void>;
  cancelLlm: () => Promise<void>;
  removeLlm: (id: string) => Promise<void>;
}

export const useModelsStore = create<ModelsState>((set, get) => ({
  overview: null,
  loading: false,
  speechProgress: {},
  llmProgress: {},
  measuring: null,
  load: async () => {
    set({ loading: true });
    try {
      set({ overview: await modelsOverview() });
    } catch (error) {
      pushErrorToast(String(error));
    } finally {
      set({ loading: false });
    }
  },
  installSpeech: async (id) => {
    try {
      await dictationInstall(id);
    } catch (error) {
      if (!String(error).includes("cancelled")) pushErrorToast(String(error));
    } finally {
      await get().load();
      void useMeetingsStore.getState().refreshStatus();
    }
  },
  cancelSpeech: async () => {
    await dictationCancelInstall().catch(() => {});
  },
  removeSpeech: async (id) => {
    try {
      await dictationRemove(id);
    } catch (error) {
      pushErrorToast(String(error));
    }
    await get().load();
    void useMeetingsStore.getState().refreshStatus();
  },
  measureSpeech: async (id) => {
    set({ measuring: id });
    try {
      await speechMeasure(id);
    } catch (error) {
      pushErrorToast(String(error));
    } finally {
      set({ measuring: null });
      await get().load();
    }
  },
  installLlm: async (id) => {
    try {
      await llmInstall(id);
    } catch (error) {
      if (!String(error).includes("cancelled")) pushErrorToast(String(error));
    } finally {
      await get().load();
    }
  },
  cancelLlm: async () => {
    await llmCancelInstall().catch(() => {});
  },
  removeLlm: async (id) => {
    try {
      await llmDelete(id);
    } catch (error) {
      pushErrorToast(String(error));
    }
    await get().load();
  },
}));

let listening = false;
export function ensureModelEvents() {
  if (listening) return;
  listening = true;
  void onDictationProgress((event) => {
    useModelsStore.setState((s) => ({ speechProgress: { ...s.speechProgress, [event.item]: event } }));
    if (event.phase === "done" || event.phase === "failed" || event.phase === "cancelled") {
      window.setTimeout(() => {
        useModelsStore.setState((s) => {
          const next = { ...s.speechProgress };
          delete next[event.item];
          return { speechProgress: next };
        });
      }, 800);
    }
  });
  void onLlmDownload((event) => {
    useModelsStore.setState((s) => ({ llmProgress: { ...s.llmProgress, [event.id]: event } }));
    if (event.phase !== "downloading") {
      window.setTimeout(() => {
        useModelsStore.setState((s) => {
          const next = { ...s.llmProgress };
          delete next[event.id];
          return { llmProgress: next };
        });
      }, 800);
    }
  });
}
