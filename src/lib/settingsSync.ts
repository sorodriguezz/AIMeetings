import { listen } from "@tauri-apps/api/event";

/** Calls `onChange` whenever one of `keys` is saved — from this pane or another. */
export function watchSettings(keys: readonly string[], onChange: (key: string, value: string) => void) {
  // Outside a webview (unit tests) there is no event bus to listen on.
  if (typeof window === "undefined") return;
  const wanted = new Set(keys);
  void listen<{ key: string; value: string }>("settings:changed", (event) => {
    if (wanted.has(event.payload.key)) onChange(event.payload.key, event.payload.value);
  });
}
