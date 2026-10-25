import { create } from "zustand";
import { providersList, providersSelect, LOCAL_PROVIDER, type ProvidersList } from "../lib/tauri/providersCommands";
import { pushErrorToast } from "./toastStore";

interface ProvidersState {
  list: ProvidersList | null;
  load: () => Promise<void>;
  select: (id: string) => Promise<void>;
}

export const useProvidersStore = create<ProvidersState>((set, get) => ({
  list: null,
  load: async () => {
    try {
      set({ list: await providersList() });
    } catch (error) {
      pushErrorToast(String(error));
    }
  },
  select: async (id) => {
    try {
      await providersSelect(id);
      await get().load();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },
}));

/** What the selected provider is called on screen. */
export function providerLabel(list: ProvidersList | null, id: string, localLabel: string): string {
  if (id === LOCAL_PROVIDER) return localLabel;
  return list?.builtins.find((b) => b.id === id)?.label ?? list?.custom.find((c) => c.id === id)?.name ?? id;
}
