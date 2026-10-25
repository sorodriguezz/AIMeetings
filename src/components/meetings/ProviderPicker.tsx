import { useEffect } from "react";
import { Cpu, Lock, Terminal } from "lucide-react";
import { Select, type SelectItems } from "../common/Select";
import { useProvidersStore } from "../../state/providersStore";
import { useUiStore } from "../../state/uiStore";
import { useT } from "../../state/languageStore";
import { LOCAL_PROVIDER } from "../../lib/tauri/providersCommands";

const MANAGE = "__manage__";

/**
 * Which engine writes the analysis: an AI CLI (claude, codex, gemini…, or one the user added) or the
 * local model. The choice is the app's (`ai_provider`), shared with Settings › AI providers. A
 * local-only meeting has no choice to make, and says so.
 */
export function ProviderPicker({ forcedLocal }: { forcedLocal: boolean }) {
  const t = useT();
  const list = useProvidersStore((s) => s.list);
  useEffect(() => {
    if (!list) void useProvidersStore.getState().load();
  }, [list]);

  if (forcedLocal) {
    return (
      <span className="flex items-center gap-1.5 text-[12px] text-[var(--cf-text-muted)]" title={t("ai.forcedLocal")}>
        <Lock size={12} />
        {t("ai.local")}
      </span>
    );
  }

  const options: SelectItems = [
    {
      label: t("providers.builtin"),
      options: (list?.builtins ?? []).map((b) => ({
        value: b.id,
        label: b.found ? b.label : `${b.label} · ${t("ai.notFound")}`,
        icon: Terminal,
      })),
    },
    ...(list && list.custom.length > 0
      ? [{ label: t("providers.custom"), options: list.custom.map((c) => ({ value: c.id, label: c.name, icon: Terminal })) }]
      : []),
    { label: t("ai.local"), options: [{ value: LOCAL_PROVIDER, label: t("ai.local"), icon: Cpu }] },
    { label: "", options: [{ value: MANAGE, label: t("ai.manageProviders") }] },
  ];
  return (
    <div className="w-[220px]">
      <Select
        size="field"
        value={list?.selected ?? "claude"}
        options={options}
        ariaLabel={t("ai.provider")}
        loading={!list}
        onChange={(value) => {
          if (value === MANAGE) useUiStore.getState().openAi("summaries");
          else void useProvidersStore.getState().select(value);
        }}
      />
    </div>
  );
}
