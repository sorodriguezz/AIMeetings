import { currentPlatform } from "./platform";

/**
 * The files the app takes. A video is read for its audio track only (`meetings::encode`), so the
 * app never keeps the picture: what is stored is a light `.m4a`.
 */
export const AUDIO_EXTENSIONS = ["m4a", "mp3", "wav", "aac", "flac", "ogg", "oga", "opus", "aiff"];

/** MP4/MOV/M4V and MKV/WebM everywhere; AVI and WMV through Windows' own decoders. */
export function videoExtensions(): string[] {
  const common = ["mp4", "mov", "m4v", "mkv", "webm", "3gp"];
  return currentPlatform() === "windows" ? [...common, "avi", "wmv"] : common;
}

export function mediaExtensions(): string[] {
  return [...AUDIO_EXTENSIONS, ...videoExtensions()];
}

export function extensionOf(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function isVideo(path: string): boolean {
  return videoExtensions().includes(extensionOf(path));
}

export function isMedia(path: string): boolean {
  return mediaExtensions().includes(extensionOf(path));
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** The name without its extension. */
export function baseName(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

export function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut > 0 ? path.slice(0, cut) : path;
}

/** `dir` + `name` with the platform's separator. */
export function joinPath(dir: string, name: string): string {
  const separator = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return dir.endsWith(separator) ? `${dir}${name}` : `${dir}${separator}${name}`;
}

/** Roughly what an hour of the converted audio weighs: AAC at 48 kbps (96 kbps on Windows). */
export function convertedMbPerHour(): number {
  return currentPlatform() === "windows" ? 43 : 22;
}
