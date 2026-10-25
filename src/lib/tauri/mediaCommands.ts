import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** `media_cmd::Converted`. */
export interface Converted {
  /** The `.m4a` written — or a `.wav` beside it when the system has no AAC encoder. */
  path: string;
  bytes: number;
  sourceBytes: number;
  durationMs: number;
}

export interface ConvertProgress {
  runId: string;
  stage: "decode" | "encode";
  fraction: number;
}

/** A video (or any audio file) made into the light audio the app keeps: mono, 16 kHz, AAC. */
export const mediaExtractAudio = (path: string, target: string, runId: string) => invoke<Converted>("media_extract_audio", { path, target, runId });

export const onConvertProgress = (f: (e: ConvertProgress) => void): Promise<UnlistenFn> => listen<ConvertProgress>("media:convert", (e) => f(e.payload));
