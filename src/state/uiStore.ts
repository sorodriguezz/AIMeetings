import { create } from "zustand";

/**
 * Where the window is. One screen at a time: the recordings, one recording, the people whose voices
 * are known, the video converter, the AI and its models, and the settings. Opening a recording
 * replaces the list (with a way back) instead of squeezing both side by side.
 */
export type View = "home" | "meeting" | "people" | "convert" | "ai" | "settings";
export type AiTab = "speech" | "voices" | "summaries";
export type SettingsTab = "general" | "recording" | "about";
export type MeetingTab = "transcript" | "people" | "ai" | "notes";

interface UiState {
  view: View;
  aiTab: AiTab;
  settingsTab: SettingsTab;
  /** The recordings' filter: `null` every recording, `""` those outside any folder, an id that folder. */
  folder: string | null;
  /** The recording open in the «meeting» view. */
  selected: string | null;
  /** The open recording's tab — kept while moving between recordings. */
  meetingTab: MeetingTab;
  /** The «Nueva grabación» sheet. */
  newOpen: boolean;
  /** The «Subir audio o video» sheet, with the file already chosen when one was dropped. */
  upload: { path: string | null } | null;
  query: string;
  setView: (view: View) => void;
  openAi: (tab: AiTab) => void;
  openSettings: (tab: SettingsTab) => void;
  setFolder: (folder: string | null) => void;
  /** Opens a recording, or (with `null`) goes back to the list. */
  select: (id: string | null) => void;
  setMeetingTab: (tab: MeetingTab) => void;
  setNewOpen: (open: boolean) => void;
  openUpload: (path?: string | null) => void;
  closeUpload: () => void;
  setQuery: (query: string) => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: "home",
  aiTab: "speech",
  settingsTab: "general",
  folder: null,
  selected: null,
  meetingTab: "transcript",
  newOpen: false,
  upload: null,
  query: "",
  setView: (view) => set({ view }),
  openAi: (aiTab) => set({ view: "ai", aiTab }),
  openSettings: (settingsTab) => set({ view: "settings", settingsTab }),
  setFolder: (folder) => set({ folder, view: "home" }),
  select: (selected) => set(selected ? { selected, view: "meeting" } : { selected: null, view: "home" }),
  setMeetingTab: (meetingTab) => set({ meetingTab }),
  setNewOpen: (newOpen) => set({ newOpen }),
  openUpload: (path = null) => set({ upload: { path } }),
  closeUpload: () => set({ upload: null }),
  setQuery: (query) => set({ query }),
}));
