import { invoke } from "@tauri-apps/api/core";

export const getSettings = (keys: string[]) => invoke<Record<string, string>>("get_settings", { keys });
export const getSetting = async (key: string) => (await getSettings([key]))[key];
export const setSetting = (key: string, value: string) => invoke<void>("set_setting", { key, value });
export const writeFileBytes = (path: string, bytes: Uint8Array | number[]) => invoke<void>("write_file_bytes", { path, bytes: Array.from(bytes) });
export const revealPath = (path: string) => invoke<void>("reveal_path", { path });
export const openExternalUrl = (url: string) => invoke<void>("open_external", { url });

export interface Diagnostics {
  version: string;
  os: string;
  arch: string;
  stateDir: string;
  logTail: string;
}
export const appDiagnostics = () => invoke<Diagnostics>("app_diagnostics");
