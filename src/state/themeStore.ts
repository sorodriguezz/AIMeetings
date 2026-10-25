import { create } from "zustand";
import { getSetting, setSetting } from "../lib/tauri/commands";

export type Theme = "system" | "light" | "dark";
const KEY = "app_theme";

function apply(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

interface ThemeState {
  theme: Theme;
  init: () => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
}

export const useThemeStore = create<ThemeState>((set) => ({
  theme: "system",
  init: async () => {
    const stored = await getSetting(KEY).catch(() => undefined);
    const theme: Theme = stored === "light" || stored === "dark" ? stored : "system";
    apply(theme);
    set({ theme });
  },
  setTheme: async (theme) => {
    apply(theme);
    set({ theme });
    await setSetting(KEY, theme);
  },
}));
