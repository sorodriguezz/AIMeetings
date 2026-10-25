import { invoke } from "@tauri-apps/api/core";

export interface BuiltinProvider {
  id: string;
  label: string;
  defaultBinary: string;
  /** The user's override, `""` = the default. */
  binaryPath: string;
  model: string;
  /** Where it resolved, or `null` when it is not installed. */
  found: string | null;
  setupUrl: string;
  installCommand: string;
}

/** `db::provider_queries::CliProviderRow`. */
export interface CustomProvider {
  id: string;
  name: string;
  binary: string;
  argsTemplate: string;
  inputMode: "stdin" | "file" | "arg";
  model: string;
  sortOrder: number;
}

export interface ProvidersList {
  builtins: BuiltinProvider[];
  custom: CustomProvider[];
  /** A builtin id, a custom id, or `local`. */
  selected: string;
}

export const LOCAL_PROVIDER = "local";

export const providersList = () => invoke<ProvidersList>("providers_list");
export const providersSetBuiltin = (id: string, binaryPath: string, model: string) => invoke<void>("providers_set_builtin", { id, binaryPath, model });
export const providersSaveCustom = (input: { id?: string; name: string; binary: string; argsTemplate: string; inputMode: string; model: string }) =>
  invoke<CustomProvider>("providers_save_custom", { input });
export const providersDeleteCustom = (id: string) => invoke<void>("providers_delete_custom", { id });
export const providersSelect = (id: string) => invoke<void>("providers_select", { id });
export const providersTest = (id: string) => invoke<string>("providers_test", { id });
export const providersCancelRun = (runId: string) => invoke<void>("providers_cancel_run", { runId });
