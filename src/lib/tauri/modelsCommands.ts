import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DictationStatus } from "./dictationCommands";

export type GpuKind = "unified" | "discrete" | "integrated" | "unknown";
export type Fit = "comfortable" | "tight" | "spills-to-cpu" | "does-not-fit" | "unknown";
export type LlmPace = "fast" | "usable" | "slow" | "crawl" | "unknown";
export type ListenPace = "fast" | "ok" | "slow" | "too-slow";

export interface MachineInfo {
  class: "apple" | "pc" | "modest";
  cpu: string;
  cores: number;
  threads: number;
  ramBytes: number;
  ramBandwidth: number | null;
  gpu: GpuKind;
  gpuName: string | null;
  gpuBytes: number | null;
  os: string;
  arch: string;
  modelsBytes: number;
}

export interface ListenFit {
  id: string;
  ms: number;
  measured: boolean;
  pace: ListenPace;
  ramMb: number;
  accuracy: "max" | "high" | "medium" | "basic";
  recommendedFor: ("meetingsLive" | "meetingsFinal")[];
}

export interface SpeechFit {
  machine: { class: string; cpu: string; cores: number; ramGb: number; gpu: string | null };
  listen: ListenFit[];
  picks: { meetingsLive: string; meetingsFinal: string };
}

export interface LlmRow {
  id: string;
  label: string;
  params: string;
  licence: string;
  licenceUrl: string;
  sizeBytes: number;
  installed: boolean;
  partialBytes: number | null;
  needBytes: number;
  ctx: number;
  maxCtx: number;
  fit: Fit;
  writeTps: number | null;
  pace: LlmPace;
  gpuShare: number;
  recommended: boolean;
  cudaOnly: boolean;
  active: boolean;
}

export type LlmBackend = "bundled" | "ollama" | "openai";

export interface LlmConfig {
  backend: LlmBackend;
  model: string;
  ctx: number;
  url: string;
}

export type LlmStatus = { kind: "off" } | { kind: "starting"; model_id: string } | { kind: "ready"; model_id: string } | { kind: "failed"; message: string };

export interface ModelsOverview {
  machine: MachineInfo;
  speech: DictationStatus;
  fit: SpeechFit;
  voices: { supported: boolean; voicesInstalled: boolean; voicesSupported: boolean; voicesBytes: number; vadInstalled: boolean; canRecord: boolean };
  llm: LlmRow[];
  llmConfig: LlmConfig;
  llmEngine: boolean;
  llmStatus: LlmStatus;
}

export interface LlmProgress {
  id: string;
  phase: "downloading" | "done" | "failed" | "cancelled";
  done: number;
  total: number;
  error?: string;
}

export const modelsOverview = () => invoke<ModelsOverview>("models_overview");
export const speechMeasure = (modelId: string) => invoke<{ ms: number; measuredAt: string }>("speech_measure", { modelId });
export const llmInstall = (modelId: string) => invoke<void>("llm_install", { modelId });
export const llmCancelInstall = () => invoke<void>("llm_cancel_install");
export const llmDelete = (modelId: string) => invoke<void>("llm_delete", { modelId });
export const llmSetConfig = (config: LlmConfig & { apiKey?: string }) => invoke<void>("llm_set_config", { config });
export const llmServerModels = (backend: LlmBackend, url: string) => invoke<string[]>("llm_server_models", { backend, url });
export const llmProbe = () => invoke<{ model: string; writeTps: number; seconds: number }>("llm_probe");
export const onLlmDownload = (f: (e: LlmProgress) => void): Promise<UnlistenFn> => listen<LlmProgress>("llm:download", (e) => f(e.payload));
