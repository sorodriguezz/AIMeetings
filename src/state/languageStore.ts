import { useMemo } from "react";
import { create } from "zustand";
import { locale } from "@tauri-apps/plugin-os";
import { getSetting, setSetting } from "../lib/tauri/commands";
import { translations, type Language, type TranslationKey } from "../lib/i18n/translations";
import { watchSettings } from "../lib/settingsSync";

const KEY = "app_language";

/** Spanish for any `es*` locale, English for everything else. */
export function languageFromLocale(value: string | null | undefined): Language {
  return value?.trim().toLowerCase().startsWith("es") ? "es" : "en";
}

interface LanguageState {
  language: Language;
  /** `""` = follow the system. */
  chosen: "" | Language;
  init: () => Promise<void>;
  setLanguage: (lang: "" | Language) => Promise<void>;
}

async function systemLanguage(): Promise<Language> {
  const fromOs = await locale().catch(() => null);
  return languageFromLocale(fromOs ?? (typeof navigator !== "undefined" ? navigator.language : "en"));
}

export const useLanguageStore = create<LanguageState>((set) => ({
  language: "en",
  chosen: "",
  init: async () => {
    const stored = await getSetting(KEY).catch(() => undefined);
    const chosen = stored === "en" || stored === "es" ? stored : "";
    const language = chosen || (await systemLanguage());
    document.documentElement.lang = language;
    set({ language, chosen });
  },
  setLanguage: async (chosen) => {
    const language = chosen || (await systemLanguage());
    document.documentElement.lang = language;
    set({ language, chosen });
    await setSetting(KEY, chosen);
  },
}));

function render(language: Language, key: TranslationKey, params?: Record<string, string | number>): string {
  const raw = translations[language][key] ?? translations.en[key] ?? key;
  if (!params) return raw;
  return Object.entries(params).reduce<string>((acc, [name, value]) => acc.split(`{${name}}`).join(String(value)), raw);
}

export type Translate = (key: TranslationKey, params?: Record<string, string | number>) => string;

/** The translator, stable for as long as the language is. */
export function useT(): Translate {
  const language = useLanguageStore((s) => s.language);
  return useMemo(() => (key: TranslationKey, params?: Record<string, string | number>) => render(language, key, params), [language]);
}

/** For code that can't call a hook. Doesn't subscribe: use `useT` in components. */
export function translate(key: TranslationKey, params?: Record<string, string | number>): string {
  return render(useLanguageStore.getState().language, key, params);
}

watchSettings([KEY], () => void useLanguageStore.getState().init());
