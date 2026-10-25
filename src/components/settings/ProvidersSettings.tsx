import { useEffect, useState } from "react";
import { Check, ChevronDown, Cpu, ExternalLink, Plus, Settings2, Terminal, Trash2 } from "lucide-react";
import { Button, buttonClass } from "../common/Button";
import { Modal } from "../common/Dialogs";
import { Select } from "../common/Select";
import { Spinner } from "../common/Spinner";
import { inputClass } from "./Row";
import {
  LOCAL_PROVIDER,
  providersDeleteCustom,
  providersSaveCustom,
  providersSetBuiltin,
  providersTest,
  type BuiltinProvider,
  type CustomProvider,
} from "../../lib/tauri/providersCommands";
import { openExternalUrl } from "../../lib/tauri/commands";
import { useProvidersStore } from "../../state/providersStore";
import { useConfirmStore } from "../../state/confirmStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";
import { useT } from "../../state/languageStore";

/** Command-line AI tools people already have, with the arguments that make them answer once. */
const PRESETS = [
  { id: "ollama", name: "Ollama", binary: "ollama", argsTemplate: "run {model}", inputMode: "stdin", model: "llama3.2" },
  { id: "llm", name: "llm", binary: "llm", argsTemplate: "-m {model}", inputMode: "stdin", model: "" },
  { id: "aichat", name: "aichat", binary: "aichat", argsTemplate: "-m {model}", inputMode: "stdin", model: "" },
  { id: "other", name: "", binary: "", argsTemplate: "{prompt}", inputMode: "stdin", model: "" },
] as const;

async function runTest(id: string, okText: (answer: string) => string) {
  try {
    const answer = await providersTest(id);
    useToastStore.getState().pushToast(okText(answer.slice(0, 80)), "success");
  } catch (error) {
    pushErrorToast(String(error));
  }
}

/** One choice: a tile that says what it is, whether it is ready, and selects it on click. */
function Tile({
  icon: Icon,
  title,
  subtitle,
  selected,
  disabled,
  onSelect,
  onConfigure,
}: {
  icon: typeof Terminal;
  title: string;
  subtitle: string;
  selected: boolean;
  disabled?: boolean;
  onSelect: () => void;
  onConfigure?: () => void;
}) {
  const t = useT();
  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      aria-disabled={disabled}
      onClick={() => !disabled && onSelect()}
      onKeyDown={(e) => e.key === "Enter" && !disabled && onSelect()}
      className={`group relative flex items-center gap-3 rounded-2xl border p-4 transition-all ${
        selected ? "border-[var(--cf-accent)] bg-[var(--cf-accent-soft)]" : disabled ? "border-[var(--cf-border)] opacity-60" : "border-[var(--cf-border)] hover:-translate-y-[1px] hover:border-[var(--cf-accent-line)] hover:shadow-[var(--cf-shadow-lift)]"
      }`}
    >
      <span className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-xl ${selected ? "bg-[var(--cf-accent)] text-[var(--cf-on-accent)]" : "bg-[var(--cf-sunken)] text-[var(--cf-text-muted)]"}`}>
        {selected ? <Check size={18} /> : <Icon size={18} />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-semibold">{title}</span>
        <span className="block truncate text-[12px] text-[var(--cf-text-muted)]">{subtitle}</span>
      </span>
      {onConfigure && (
        <button
          type="button"
          aria-label={t("v2.ai.configure")}
          onClick={(e) => {
            e.stopPropagation();
            onConfigure();
          }}
          className="rounded-lg p-1.5 text-[var(--cf-text-faint)] opacity-0 transition-opacity hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)] group-hover:opacity-100"
        >
          <Settings2 size={15} />
        </button>
      )}
    </div>
  );
}

/** A known tool's program path and model, and a test — all optional. */
function BuiltinEditor({ provider, onClose }: { provider: BuiltinProvider; onClose: () => void }) {
  const t = useT();
  const [binary, setBinary] = useState(provider.binaryPath);
  const [model, setModel] = useState(provider.model);
  const [testing, setTesting] = useState(false);
  const save = async () => {
    try {
      await providersSetBuiltin(provider.id, binary.trim(), model.trim());
      await useProvidersStore.getState().load();
      onClose();
    } catch (error) {
      pushErrorToast(String(error));
    }
  };
  return (
    <Modal onClose={onClose} width={480}>
      <div className="flex flex-col gap-4 p-6">
        <h2 className="text-[17px] font-semibold">{provider.label}</h2>
        <p className="font-mono text-[11.5px] text-[var(--cf-text-muted)]">{provider.found ?? provider.installCommand}</p>
        <label className="flex flex-col gap-1">
          <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.model")}</span>
          <input className={`${inputClass} font-mono text-[12px]`} value={model} placeholder={t("providers.modelPlaceholder")} onChange={(e) => setModel(e.target.value)} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.binary")}</span>
          <input className={`${inputClass} font-mono text-[12px]`} value={binary} placeholder={t("providers.binaryPlaceholder", { name: provider.defaultBinary })} onChange={(e) => setBinary(e.target.value)} />
        </label>
        <div className="mt-2 flex items-center gap-2">
          <Button
            variant="ghost"
            disabled={testing || !provider.found}
            onClick={() => {
              setTesting(true);
              void runTest(provider.id, (answer) => t("providers.testOk", { answer })).finally(() => setTesting(false));
            }}
          >
            {testing && <Spinner size={11} />}
            {testing ? t("providers.testing") : t("providers.test")}
          </Button>
          <span className="flex-1" />
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={() => void save()}>
            {t("providers.save")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

/** Adding (or editing) a tool of the user's own: pick a template, name the program, done. */
function CustomEditor({ initial, onClose }: { initial: CustomProvider | null; onClose: () => void }) {
  const t = useT();
  const [form, setForm] = useState<{ name: string; binary: string; argsTemplate: string; inputMode: string; model: string }>(
    initial ? { name: initial.name, binary: initial.binary, argsTemplate: initial.argsTemplate, inputMode: initial.inputMode, model: initial.model } : { ...PRESETS[0] },
  );
  const [preset, setPreset] = useState<string>(initial ? "other" : "ollama");
  const [advanced, setAdvanced] = useState(!!initial);
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<typeof form>) => setForm((f) => ({ ...f, ...patch }));
  const valid = form.name.trim() && form.binary.trim();
  const submit = async () => {
    if (!valid) return;
    setBusy(true);
    try {
      const saved = await providersSaveCustom({ id: initial?.id, ...form });
      if (!initial) await useProvidersStore.getState().select(saved.id);
      await useProvidersStore.getState().load();
      onClose();
    } catch (error) {
      pushErrorToast(String(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal onClose={onClose} width={520}>
      <form
        className="flex flex-col gap-4 p-6"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <div>
          <h2 className="text-[18px] font-semibold">{initial ? t("providers.edit") : t("providers.add")}</h2>
          <p className="mt-1 text-[12.5px] text-[var(--cf-text-muted)]">{t("providers.addHint")}</p>
        </div>
        {!initial && (
          <div className="grid grid-cols-4 gap-2">
            {PRESETS.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => {
                  setPreset(p.id);
                  setForm({ name: p.name, binary: p.binary, argsTemplate: p.argsTemplate, inputMode: p.inputMode, model: p.model });
                  if (p.id === "other") setAdvanced(true);
                }}
                className={`rounded-xl border px-2 py-2.5 text-[12.5px] font-medium transition-colors ${preset === p.id ? "border-[var(--cf-accent)] bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]" : "border-[var(--cf-border)] hover:bg-[var(--cf-hover)]"}`}
              >
                {p.id === "other" ? t("providers.presetOther") : p.name}
              </button>
            ))}
          </div>
        )}
        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.name")}</span>
            <input autoFocus className={inputClass} value={form.name} placeholder={t("providers.namePlaceholder")} onChange={(e) => set({ name: e.target.value })} />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.model")}</span>
            <input className={`${inputClass} font-mono text-[12px]`} value={form.model} placeholder={t("providers.modelPlaceholder")} onChange={(e) => set({ model: e.target.value })} />
          </label>
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.command")}</span>
          <input className={`${inputClass} font-mono text-[12px]`} value={form.binary} placeholder={t("providers.commandPlaceholder")} onChange={(e) => set({ binary: e.target.value })} />
        </label>
        <button type="button" onClick={() => setAdvanced((v) => !v)} className="flex items-center gap-1 self-start text-[12.5px] font-medium text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
          <ChevronDown size={14} className={`transition-transform ${advanced ? "rotate-180" : ""}`} />
          {t("providers.advanced")}
        </button>
        {advanced && (
          <div className="cf-fade-in flex flex-col gap-3 rounded-2xl bg-[var(--cf-surface-2)] p-3">
            <label className="flex flex-col gap-1">
              <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.args")}</span>
              <input className={`${inputClass} font-mono text-[12px]`} value={form.argsTemplate} placeholder="run {model} {prompt}" onChange={(e) => set({ argsTemplate: e.target.value })} />
              <span className="text-[11.5px] text-[var(--cf-text-muted)]">{t("providers.argsHint")}</span>
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-[12px] font-medium text-[var(--cf-text-muted)]">{t("providers.input")}</span>
              <Select
                size="field"
                value={form.inputMode}
                onChange={(v) => set({ inputMode: v })}
                options={[
                  { value: "stdin", label: t("providers.input.stdin") },
                  { value: "file", label: t("providers.input.file") },
                  { value: "arg", label: t("providers.input.arg") },
                ]}
              />
            </label>
          </div>
        )}
        <div className="mt-1 flex items-center justify-end gap-2">
          {initial && (
            <Button
              variant="danger-ghost"
              onClick={() =>
                void useConfirmStore
                  .getState()
                  .ask({ message: t("providers.deleteConfirm", { name: initial.name }), confirmLabel: t("providers.delete"), danger: true })
                  .then((ok) => {
                    if (!ok) return;
                    void providersDeleteCustom(initial.id)
                      .then(() => useProvidersStore.getState().load())
                      .then(onClose)
                      .catch((e) => pushErrorToast(String(e)));
                  })
              }
            >
              <Trash2 size={12} />
              {t("providers.delete")}
            </Button>
          )}
          <span className="flex-1" />
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" variant="primary" disabled={!valid || busy}>
            {initial ? t("providers.save") : t("providers.addAndUse")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

/** «¿Con qué IA quieres resumir?» — the tools found, the local model, and the user's own. */
export function ProvidersSettings() {
  const t = useT();
  const list = useProvidersStore((s) => s.list);
  const [editingBuiltin, setEditingBuiltin] = useState<BuiltinProvider | null>(null);
  const [editingCustom, setEditingCustom] = useState<CustomProvider | null | "new">(null);
  const [showMissing, setShowMissing] = useState(false);
  useEffect(() => {
    void useProvidersStore.getState().load();
  }, []);
  if (!list) return <Spinner />;
  const found = list.builtins.filter((b) => b.found);
  const missing = list.builtins.filter((b) => !b.found);
  const select = (id: string) => void useProvidersStore.getState().select(id);

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-[16px] font-semibold">{t("providers.question")}</h2>
        <p className="mt-1 max-w-[62ch] text-[13px] leading-[1.5] text-[var(--cf-text-muted)]">{t("providers.hint")}</p>
      </div>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {found.map((provider) => (
          <Tile
            key={provider.id}
            icon={Terminal}
            title={provider.label}
            subtitle={provider.model ? `${t("providers.found")} · ${provider.model}` : t("providers.found")}
            selected={list.selected === provider.id}
            onSelect={() => select(provider.id)}
            onConfigure={() => setEditingBuiltin(provider)}
          />
        ))}
        <Tile icon={Cpu} title={t("providers.localTitle")} subtitle={t("providers.localShort")} selected={list.selected === LOCAL_PROVIDER} onSelect={() => select(LOCAL_PROVIDER)} onConfigure={() => document.getElementById("local-ai")?.scrollIntoView({ behavior: "smooth", block: "start" })} />
        {list.custom.map((provider) => (
          <Tile
            key={provider.id}
            icon={Terminal}
            title={provider.name}
            subtitle={provider.binary}
            selected={list.selected === provider.id}
            onSelect={() => select(provider.id)}
            onConfigure={() => setEditingCustom(provider)}
          />
        ))}
        <button
          type="button"
          onClick={() => setEditingCustom("new")}
          className="flex items-center gap-3 rounded-2xl border border-dashed border-[var(--cf-border-strong)] p-4 text-left text-[var(--cf-text-muted)] transition-colors hover:border-[var(--cf-accent)] hover:text-[var(--cf-accent)]"
        >
          <span className="flex h-10 w-10 items-center justify-center rounded-xl bg-[var(--cf-sunken)]">
            <Plus size={18} />
          </span>
          <span className="text-[14px] font-semibold">{t("providers.add")}</span>
        </button>
      </div>

      {missing.length > 0 && (
        <section className="flex flex-col gap-2">
          <button type="button" onClick={() => setShowMissing((v) => !v)} className="flex items-center gap-1 self-start text-[13px] font-medium text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
            <ChevronDown size={14} className={`transition-transform ${showMissing ? "rotate-180" : ""}`} />
            {t("providers.notInstalled", { n: missing.length })}
          </button>
          {showMissing && (
            <div className="cf-fade-in flex flex-col divide-y divide-[var(--cf-border)] rounded-2xl border border-[var(--cf-border)]">
              {missing.map((provider) => (
                <div key={provider.id} className="flex items-center gap-3 px-4 py-3">
                  <span className="min-w-0 flex-1">
                    <span className="block text-[13.5px] font-medium">{provider.label}</span>
                    <code className="block truncate text-[11.5px] text-[var(--cf-text-muted)]">{provider.installCommand}</code>
                  </span>
                  {provider.setupUrl && (
                    <button type="button" className={buttonClass({ variant: "ghost", size: "md" })} onClick={() => void openExternalUrl(provider.setupUrl)}>
                      <ExternalLink size={12} />
                      {t("providers.install")}
                    </button>
                  )}
                  <button type="button" className={buttonClass({ variant: "ghost", size: "md" })} onClick={() => setEditingBuiltin(provider)}>
                    <Settings2 size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}
      {editingBuiltin && <BuiltinEditor provider={editingBuiltin} onClose={() => setEditingBuiltin(null)} />}
      {editingCustom && <CustomEditor initial={editingCustom === "new" ? null : editingCustom} onClose={() => setEditingCustom(null)} />}
    </div>
  );
}
