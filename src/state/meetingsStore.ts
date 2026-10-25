import { create } from "zustand";
import { getSettings, setSetting } from "../lib/tauri/commands";
import {
  meetingsCancelInstall,
  meetingsDelete,
  meetingsDetail,
  meetingsDiscard,
  meetingsImport,
  meetingsInstall,
  meetingsList,
  foldersList,
  meetingsPause,
  meetingsReprocess,
  meetingsStart,
  meetingsStatus,
  meetingsStop,
  meetingsTranscribe,
  meetingsSystemAudioSettings,
  MIC_DENIED,
  NO_CLOUD_KEY,
  NO_MODEL,
  onMeetingDetected,
  onMeetingDownload,
  onMeetingLag,
  onMeetingLevel,
  onMeetingMonitor,
  onMeetingLine,
  onMeetingProgress,
  onMeetingState,
  SYSTEM_AUDIO_DENIED,
  type DownloadEvent,
  type Folder,
  type Meeting,
  type MeetingBench,
  type MeetingDetail,
  type MeetingMode,
  type MeetingsStatus,
  type ProgressEvent,
  meetingsSetMicMuted,
} from "../lib/tauri/meetingsCommands";
import { dictationMicSettings } from "../lib/tauri/dictationCommands";
import { watchSettings } from "../lib/settingsSync";
import { translate } from "./languageStore";
import { pushErrorToast, useToastStore } from "./toastStore";
import { useConfirmStore } from "./confirmStore";
import { useUiStore } from "./uiStore";

/**
 * Meetings in the window: what is installed, the one meeting being recorded (its levels, its live
 * lines, how far behind the text is), the library (every meeting and the folders), the meetings
 * opened, and the jobs that finish them.
 *
 * **The backend owns the recording.** This store mirrors it from events — `meetings:level`,
 * `meetings:line`, `meetings:state`, `meetings:progress` — so a reload, or the window coming back
 * from the tray, shows the same meeting in the same state: `refreshStatus` asks again whenever it
 * may have missed something.
 */

export const KEYS = {
  mode: "meetings_mode",
  transcriber: "meetings_transcriber",
  liveModel: "meetings_live_model",
  finalModel: "meetings_final_model",
  cloud: "meetings_cloud",
  threads: "meetings_threads",
  audioDays: "meetings_audio_days",
  detect: "meetings_detect",
  bench: "meetings_bench",
  vocabulary: "meetings_vocabulary",
  consent: "meetings_consent_seen",
  micGain: "meetings_mic_gain",
  afterStop: "meetings_after_stop",
} as const;

export interface MeetingSettings {
  /** `""` = automatic. */
  mode: "" | MeetingMode;
  transcriber: "local" | "cloud";
  liveModel: string;
  finalModel: string;
  cloudUrl: string;
  cloudModel: string;
  threads: number;
  audioDays: number;
  detect: boolean;
  vocabulary: string;
  bench: MeetingBench | null;
  consentSeen: boolean;
  /** The microphone's gain, linear (1 = as the device hears it). */
  micGain: number;
  /** «Detener» transcribes at once; `false` keeps the recording to be transcribed later. */
  transcribeOnStop: boolean;
}

const DEFAULT_SETTINGS: MeetingSettings = {
  mode: "",
  transcriber: "local",
  liveModel: "",
  finalModel: "",
  cloudUrl: "",
  cloudModel: "",
  threads: 0,
  audioDays: 30,
  detect: true,
  vocabulary: "",
  bench: null,
  consentSeen: false,
  micGain: 1,
  transcribeOnStop: true,
};

/** How many levels each channel keeps for the waveform. */
const LEVELS = 120;
export interface MeterBlocks {
  rms: number[];
  peak: number[];
}
const EMPTY_METER: Record<"mic" | "system", MeterBlocks> = { mic: { rms: [], peak: [] }, system: { rms: [], peak: [] } };

/** How many blocks a meter judges by: 2 s at the backend's 25 a second. */
const METER = 50;

interface MeetingsState {
  status: MeetingsStatus | null;
  settings: MeetingSettings;
  /** The recording's levels, newest last. */
  levels: { mic: number[]; system: number[] };
  /** Each input's last blocks, recording or not (the meters before a meeting starts): RMS and peak. */
  meter: Record<"mic" | "system", MeterBlocks>;
  /** Why a meter's input did not open (`MIC_DENIED`, `MIC_NONE`, `SYSTEM_AUDIO_DENIED`…). */
  meterError: Record<"mic" | "system", string | null>;
  /** The microphone silenced in this app only (`meetings_set_mic_muted`) — lifted when a recording stops. */
  micMuted: boolean;
  setMicMuted: (muted: boolean) => void;
  lag: { ms: number; behind: boolean } | null;
  /** Every meeting, newest first — the library filters it on screen. */
  meetings: Meeting[];
  folders: Folder[];
  /** Whether the library was read at least once. */
  libraryLoaded: boolean;
  details: Record<string, MeetingDetail>;
  progress: Record<string, ProgressEvent>;
  downloads: Partial<Record<"vad" | "voices", DownloadEvent>>;
  /** A call app using the microphone, offered for a meeting. */
  suggestion: { name: string } | null;
  /** A search result in a transcript, waiting for its meeting to show the line. */
  jumpTo: { meetingId: string; atMs: number } | null;
  /** When `status` was read — the recording's clock runs on from it. */
  statusAt: number;
  /** AI runs by meeting: what is being written, and an answer waiting to be read or inserted. */
  ai: Record<string, MeetingAiRun>;

  refreshStatus: () => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSetting: (key: keyof typeof KEYS, value: string) => Promise<void>;
  loadLibrary: () => Promise<void>;
  loadDetail: (meetingId: string) => Promise<MeetingDetail | null>;
  start: (args: { folderId?: string; title?: string; kind: "virtual" | "room"; mode?: MeetingMode; language: string }) => Promise<boolean>;
  pause: (meetingId: string, paused: boolean) => Promise<void>;
  /** `transcribe` overrides the «Al detener» setting for this one recording. */
  stop: (meetingId: string, transcribe?: boolean) => Promise<void>;
  /** Transcribes a recording that was kept for later. */
  transcribe: (meetingId: string) => Promise<void>;
  discard: (meetingId: string) => Promise<void>;
  remove: (meetingId: string) => Promise<void>;
  reprocess: (meetingId: string, retranscribe: boolean, speakers: number, cloud?: boolean) => Promise<void>;
  importFile: (folderId: string, path: string, language: string) => Promise<void>;
  install: (item: "vad" | "voices") => Promise<void>;
  cancelInstall: () => Promise<void>;
  dismissSuggestion: () => void;
  /** Opens a transcript at a line: selects its meeting and puts the line in view. */
  jumpToLine: (meetingId: string, atMs: number) => void;
  clearJump: () => void;
  setAi: (meetingId: string, run: MeetingAiRun | null) => void;
  /** Applies a change made here at once, before the backend's event confirms it. */
  patchMeeting: (meeting: Partial<Meeting> & { id: string }) => void;
}

export interface MeetingAiRun {
  runId: string;
  /** What is being written — a recipe's name, or the question. */
  label: string;
  question: boolean;
  status: "running" | "ready" | "failed";
  markdown?: string;
  error?: string;
  /** Already added to the recording's notes. */
  saved?: boolean;
  /** What was asked, to try again after a failure. */
  request?: { recipe?: string; instruction?: string };
}

function parseSettings(raw: Record<string, string>): MeetingSettings {
  let cloud: { url?: string; model?: string } = {};
  try {
    cloud = JSON.parse(raw[KEYS.cloud] || "{}");
  } catch {
    cloud = {};
  }
  let bench: MeetingBench | null = null;
  try {
    bench = raw[KEYS.bench] ? (JSON.parse(raw[KEYS.bench]) as MeetingBench) : null;
  } catch {
    bench = null;
  }
  const mode = raw[KEYS.mode];
  const days = Number.parseInt(raw[KEYS.audioDays] ?? "", 10);
  return {
    mode: mode === "light" || mode === "balanced" || mode === "full" ? mode : "",
    transcriber: raw[KEYS.transcriber] === "cloud" ? "cloud" : "local",
    liveModel: raw[KEYS.liveModel] ?? "",
    finalModel: raw[KEYS.finalModel] ?? "",
    cloudUrl: cloud.url ?? "",
    cloudModel: cloud.model ?? "",
    threads: Number.parseInt(raw[KEYS.threads] ?? "0", 10) || 0,
    audioDays: Number.isFinite(days) ? days : 30,
    detect: raw[KEYS.detect] !== "0",
    vocabulary: raw[KEYS.vocabulary] ?? "",
    bench,
    consentSeen: raw[KEYS.consent] === "1",
    micGain: Number.parseFloat(raw[KEYS.micGain] ?? "") || 1,
    transcribeOnStop: raw[KEYS.afterStop] !== "keep",
  };
}

/** Replaces `meeting` in the library, or adds it at the top. */
function withMeeting(list: Meeting[], meeting: Meeting): Meeting[] {
  return list.some((m) => m.id === meeting.id) ? list.map((m) => (m.id === meeting.id ? meeting : m)) : [meeting, ...list];
}

/** Says what a failed start means, with the way out when there is one. */
async function explainStartError(error: unknown) {
  const text = String(error);
  if (text.includes(MIC_DENIED)) {
    const open = await useConfirmStore.getState().ask({ message: translate("meetings.micDenied"), confirmLabel: translate("dictation.openPrivacy"), danger: false });
    if (open) void dictationMicSettings().catch(() => {});
    return;
  }
  if (text.includes(NO_CLOUD_KEY)) {
    const open = await useConfirmStore.getState().ask({ message: translate("meetings.noCloudKey"), confirmLabel: translate("meetings.openSettings"), danger: false });
    if (open) useUiStore.getState().openSettings("recording");
    return;
  }
  if (text.includes(NO_MODEL)) {
    const open = await useConfirmStore.getState().ask({ message: translate("meetings.noModel"), confirmLabel: translate("meetings.openModels"), danger: false });
    if (open) useUiStore.getState().openAi("speech");
    return;
  }
  pushErrorToast(text);
}

export const useMeetingsStore = create<MeetingsState>((set, get) => ({
  status: null,
  settings: DEFAULT_SETTINGS,
  levels: { mic: [], system: [] },
  meter: EMPTY_METER,
  meterError: { mic: null, system: null },
  micMuted: false,
  setMicMuted: (muted) => {
    set({ micMuted: muted });
    void meetingsSetMicMuted(muted).catch((error) => pushErrorToast(String(error)));
  },
  lag: null,
  meetings: [],
  folders: [],
  libraryLoaded: false,
  details: {},
  progress: {},
  downloads: {},
  suggestion: null,
  jumpTo: null,
  statusAt: 0,
  ai: {},

  refreshStatus: async () => {
    try {
      const status = await meetingsStatus();
      set((state) => ({
        status,
        statusAt: Date.now(),
        // A recording that ended elsewhere takes its waveform with it.
        levels: status.recording ? state.levels : { mic: [], system: [] },
        lag: status.recording ? state.lag : null,
      }));
    } catch {
      // The status is a mirror; the next event or refresh puts it right.
    }
  },

  loadSettings: async () => {
    try {
      const raw = await getSettings(Object.values(KEYS));
      set({ settings: parseSettings(raw) });
    } catch {
      // Defaults stand.
    }
  },

  saveSetting: async (key, value) => {
    await setSetting(KEYS[key], value);
    await get().loadSettings();
  },

  loadLibrary: async () => {
    try {
      const [meetings, folders] = await Promise.all([meetingsList(), foldersList()]);
      set({ meetings, folders, libraryLoaded: true });
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  loadDetail: async (meetingId) => {
    try {
      const detail = await meetingsDetail(meetingId);
      set((state) => ({ details: { ...state.details, [meetingId]: detail }, meetings: withMeeting(state.meetings, detail.meeting) }));
      return detail;
    } catch {
      return null;
    }
  },

  start: async (args) => {
    try {
      const started = await meetingsStart(args);
      if (started.warning === SYSTEM_AUDIO_DENIED) {
        void useConfirmStore
          .getState()
          .ask({ message: translate("meetings.systemDenied"), confirmLabel: translate("dictation.openPrivacy"), danger: false })
          .then((open) => open && void meetingsSystemAudioSettings().catch(() => {}));
      }
      set((state) => ({
        meetings: withMeeting(state.meetings, started.meeting),
        details: { ...state.details, [started.meeting.id]: { meeting: started.meeting, lines: [], speakers: [], localOnly: false } },
        levels: { mic: [], system: [] },
        meterError: { mic: null, system: null },
        lag: null,
      }));
      useUiStore.getState().select(started.meeting.id);
      await get().refreshStatus();
      void get().loadDetail(started.meeting.id);
      return true;
    } catch (error) {
      await explainStartError(error);
      return false;
    }
  },

  pause: async (meetingId, paused) => {
    try {
      await meetingsPause(meetingId, paused);
      await get().refreshStatus();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  stop: async (meetingId, transcribe) => {
    try {
      const meeting = await meetingsStop(meetingId, transcribe);
      set((state) => ({ meetings: withMeeting(state.meetings, meeting), levels: { mic: [], system: [] }, meter: EMPTY_METER, micMuted: false, lag: null }));
      await get().refreshStatus();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  discard: async (meetingId) => {
    try {
      await meetingsDiscard(meetingId);
      set({ micMuted: false });
      await get().refreshStatus();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  remove: async (meetingId) => {
    try {
      await meetingsDelete(meetingId);
      await get().refreshStatus();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  transcribe: async (meetingId) => {
    try {
      await meetingsTranscribe(meetingId);
      await get().refreshStatus();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  reprocess: async (meetingId, retranscribe, speakers, cloud) => {
    try {
      await meetingsReprocess(meetingId, retranscribe, speakers, cloud);
      await get().refreshStatus();
    } catch (error) {
      pushErrorToast(String(error));
    }
  },

  importFile: async (folderId, path, language) => {
    try {
      const meeting = await meetingsImport(folderId, path, language);
      set((state) => ({ meetings: withMeeting(state.meetings, meeting) }));
      useUiStore.getState().select(meeting.id);
      await get().refreshStatus();
    } catch (error) {
      await explainStartError(error);
    }
  },

  install: async (item) => {
    try {
      await meetingsInstall(item);
    } catch (error) {
      if (String(error) !== "cancelled") pushErrorToast(String(error));
    } finally {
      await get().refreshStatus();
    }
  },

  cancelInstall: async () => {
    await meetingsCancelInstall().catch(() => {});
  },

  dismissSuggestion: () => set({ suggestion: null }),

  jumpToLine: (meetingId, atMs) => {
    set({ jumpTo: { meetingId, atMs } });
    useUiStore.getState().select(meetingId);
  },

  clearJump: () => set({ jumpTo: null }),

  patchMeeting: (patch) =>
    set((state) => {
      const meetings = state.meetings.map((m) => (m.id === patch.id ? { ...m, ...patch } : m));
      const detail = state.details[patch.id];
      const details = detail ? { ...state.details, [patch.id]: { ...detail, meeting: { ...detail.meeting, ...patch } } } : state.details;
      return { meetings, details };
    }),

  setAi: (meetingId, run) =>
    set((state) => {
      const ai = { ...state.ai };
      if (run) ai[meetingId] = run;
      else delete ai[meetingId];
      return { ai };
    }),
}));

function pushMeter(meter: MeterBlocks, rms: number, peak: number): MeterBlocks {
  return { rms: [...meter.rms, rms].slice(-METER), peak: [...meter.peak, peak].slice(-METER) };
}

let listening = false;

/** Attaches the event listeners once per window. */
export function ensureMeetingEvents() {
  if (listening) return;
  listening = true;
  const store = useMeetingsStore;
  void onMeetingLevel(({ meetingId, channel, level, peak }) => {
    if (store.getState().status?.recording?.meetingId !== meetingId) return;
    store.setState((state) => {
      const list = [...state.levels[channel], level];
      if (list.length > LEVELS) list.splice(0, list.length - LEVELS);
      // The recording's own level proves the channel works: a monitor's error from before it started
      // (on Windows the meeting's loopback invalidates the monitor's stream) must not stay shown.
      return {
        levels: { ...state.levels, [channel]: list },
        meter: { ...state.meter, [channel]: pushMeter(state.meter[channel], level, peak) },
        meterError: state.meterError[channel] ? { ...state.meterError, [channel]: null } : state.meterError,
      };
    });
  });
  void onMeetingMonitor(({ channel, level, peak, error }) => {
    if (store.getState().status?.recording) return;
    store.setState((state) => ({
      meterError: { ...state.meterError, [channel]: error },
      meter: { ...state.meter, [channel]: error ? { rms: [], peak: [] } : pushMeter(state.meter[channel], level, peak) },
    }));
  });
  void onMeetingLine(({ meetingId, line }) => {
    store.setState((state) => {
      const detail = state.details[meetingId];
      if (!detail) return {};
      const lines = [...detail.lines.filter((l) => l.seq !== line.seq), line].sort((a, b) => a.startMs - b.startMs || a.seq - b.seq);
      const speakers = detail.speakers.some((s) => s.key === line.speaker)
        ? detail.speakers
        : [...detail.speakers, { key: line.speaker, name: "", color: detail.speakers.length, isMe: line.speaker === "me", voiceId: "", talkMs: 0, hasVoice: false }];
      return { details: { ...state.details, [meetingId]: { ...detail, lines, speakers } } };
    });
  });
  void onMeetingLag(({ meetingId, lagMs, behind }) => {
    if (store.getState().status?.recording?.meetingId !== meetingId) return;
    store.setState({ lag: { ms: lagMs, behind } });
  });
  void onMeetingProgress((event) => {
    store.setState((state) => ({ progress: { ...state.progress, [event.meetingId]: event } }));
  });
  void onMeetingDownload((event) => {
    store.setState((state) => ({ downloads: { ...state.downloads, [event.item]: event } }));
    if (event.phase === "done" || event.phase === "failed" || event.phase === "cancelled") {
      void store.getState().refreshStatus();
      window.setTimeout(() => store.setState((state) => ({ downloads: { ...state.downloads, [event.item]: undefined } })), 1500);
    }
  });
  void onMeetingState(({ meetingId, status, error }) => {
    if (status === "deleted") {
      store.setState((state) => {
        const details = { ...state.details };
        delete details[meetingId];
        return { details, meetings: state.meetings.filter((m) => m.id !== meetingId) };
      });
      if (useUiStore.getState().selected === meetingId && useUiStore.getState().view === "meeting") useUiStore.getState().select(null);
      void store.getState().refreshStatus();
      void store.getState().loadLibrary();
      return;
    }
    void store.getState().refreshStatus();
    // The row (and the detail when it is open) re-read: a finished pass rewrites both.
    void store.getState().loadDetail(meetingId);
    void foldersList().then((folders) => store.setState({ folders })).catch(() => {});
    if (status === "ready" || status === "failed") {
      store.setState((state) => {
        const progress = { ...state.progress };
        delete progress[meetingId];
        return { progress };
      });
    }
    if (status === "ready") {
      useToastStore.getState().pushToast(translate("meetings.readyToast"), "success");
    }
    if (status === "failed" && error && error !== "cancelled") {
      pushErrorToast(translate("meetings.failedToast", { error }));
    }
  });
  void onMeetingDetected(({ caller }) => {
    if (store.getState().status?.recording) return;
    store.setState({ suggestion: { name: caller.name } });
  });
  void store.getState().refreshStatus();
  void store.getState().loadSettings();
  void store.getState().loadLibrary();
}

watchSettings(Object.values(KEYS), () => void useMeetingsStore.getState().loadSettings());
