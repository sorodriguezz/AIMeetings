import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** What may pick a model that listens. A meeting's final pass needs per-word times, which only
 *  whisper gives; everything else takes either engine. */
export type ListenFeature = "dictation" | "codi" | "meetingsLive" | "meetingsFinal" | "flows";

/** `dictation::ModelStatus`: one model that listens — whisper.cpp's or sherpa-onnx's (Parakeet,
 *  Canary). One setting (`dictation_model`, `meetings_live_model`…) holds either id. */
export interface DictationModel {
  id: string;
  family: "whisper" | "parakeet" | "canary";
  label: string;
  sizeBytes: number;
  installed: boolean;
  /** Model and the library that runs it are both in. */
  usable: boolean;
  /** The library's download, when this model would bring it. */
  engineBytes: number;
  /** Codes it hears; empty = whisper's ninety-nine. */
  languages: string[];
  translates: boolean;
  licence: string;
  features: ListenFeature[];
}

/** `dictation::Status`. */
export interface DictationStatus {
  /** Whether this platform has an engine to install at all. */
  supported: boolean;
  engineInstalled: boolean;
  engineBytes: number;
  /** sherpa-onnx's library, shared with meetings' voices and the reading voices. */
  sherpaInstalled: boolean;
  sherpaBytes: number;
  models: DictationModel[];
}

/** `dictation:download` — `item` is `"engine"` or a model's id. */
export interface DictationProgress {
  item: string;
  phase: "downloading" | "unpacking" | "done" | "failed" | "cancelled";
  done: number;
  total: number;
  error?: string;
}

export const dictationStatus = () => invoke<DictationStatus>("dictation_status");

/** Resolves once the engine (if missing) and the model are on disk and verified. */
export const dictationInstall = (modelId: string) => invoke<void>("dictation_install", { modelId });

export const dictationCancelInstall = () => invoke<void>("dictation_cancel_install");

export const dictationRemove = (modelId: string) => invoke<void>("dictation_remove", { modelId });

/** `dictation::capture::InputDevice`: one microphone. `id` is what `dictation_device` stores. */
export interface MicInput {
  id: string;
  name: string;
  isDefault: boolean;
}

/** `dictation::permission::Permission`. `unknown` where the system has nothing to read (Windows):
 *  the input itself refuses, with [`MIC_DENIED`]. */
export type MicPermission = "granted" | "denied" | "undetermined" | "unknown";

/** What a recording that the system refused fails with — and one with no input at all. */
export const MIC_DENIED = "MIC_DENIED";
export const MIC_NONE = "MIC_NONE";

export const dictationInputs = () => invoke<MicInput[]>("dictation_inputs");

export const dictationMicPermission = () => invoke<MicPermission>("dictation_mic_permission");

/** Asks the system's own question when it has not been asked yet (macOS); the state either way. */
export const dictationMicRequest = () => invoke<MicPermission>("dictation_mic_request");

/** Opens the system's microphone privacy page. */
export const dictationMicSettings = () => invoke<void>("dictation_mic_settings");

export const onDictationProgress = (handler: (event: DictationProgress) => void): Promise<UnlistenFn> =>
  listen<DictationProgress>("dictation:download", (event) => handler(event.payload));
