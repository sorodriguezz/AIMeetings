/**
 * The app's text. `base.*.json` is the meetings vocabulary carried over from code-flow (rarely
 * edited); `app.ts` is what this app adds, and overrides any base string that spoke of notes, books
 * or another app. Both languages are compiled in — the whole dictionary is a few dozen kilobytes.
 */
import baseEn from "./base.en.json";
import baseEs from "./base.es.json";
import { appEn, appEs } from "./app";
import { v2En, v2Es } from "./v2";

export type Language = "en" | "es";
export type TranslationKey = keyof typeof baseEn | keyof typeof appEn | keyof typeof v2En;

const en: Record<TranslationKey, string> = { ...baseEn, ...appEn, ...v2En };
const es: Record<TranslationKey, string> = { ...baseEn, ...baseEs, ...appEn, ...appEs, ...v2En, ...v2Es } as Record<TranslationKey, string>;

export const translations: Record<Language, Record<TranslationKey, string>> = { en, es };
