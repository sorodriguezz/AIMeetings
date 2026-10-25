import { useEffect, useMemo, useState } from "react";
import { AudioLines, Check, Cpu, Mic, Sparkles, Terminal } from "lucide-react";
import { Button } from "../common/Button";
import { Checkbox } from "../common/Checkbox";
import { Spinner } from "../common/Spinner";
import { ProgressBar } from "../models/ModelsView";
import { dictationMicPermission, dictationMicRequest, type MicPermission } from "../../lib/tauri/dictationCommands";
import { meetingsSystemAudioSettings } from "../../lib/tauri/meetingsCommands";
import { setSetting } from "../../lib/tauri/commands";
import { LOCAL_PROVIDER } from "../../lib/tauri/providersCommands";
import { ensureModelEvents, useModelsStore } from "../../state/modelsStore";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useProvidersStore } from "../../state/providersStore";
import { useT } from "../../state/languageStore";
import { formatBytes } from "../../lib/format";
import { isMac } from "../../lib/platform";

type Step = "welcome" | "permissions" | "models" | "providers";
const STEPS: Step[] = ["welcome", "permissions", "models", "providers"];

function Shell({ step, children, footer }: { step: Step; children: React.ReactNode; footer: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col bg-[var(--cf-bg)]">
      <div className="cf-drag h-10 shrink-0" data-tauri-drag-region />
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-6 pb-6">
        <div className="cf-fade-in flex w-full max-w-[560px] flex-col gap-6 rounded-3xl border border-[var(--cf-border)] bg-[var(--cf-surface)] p-8 shadow-[var(--cf-shadow)]" key={step}>
          <div className="flex gap-1.5">
            {STEPS.map((s) => (
              <span key={s} className={`h-1 flex-1 rounded-full ${STEPS.indexOf(s) <= STEPS.indexOf(step) ? "bg-[var(--cf-accent)]" : "bg-[var(--cf-border)]"}`} />
            ))}
          </div>
          {children}
          <div className="flex items-center justify-end gap-2">{footer}</div>
        </div>
      </div>
    </div>
  );
}

/** The first run: what the app does, the microphone, the models that fit this computer, and the AI. */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const t = useT();
  const [step, setStep] = useState<Step>("welcome");
  const [permission, setPermission] = useState<MicPermission>("unknown");
  const overview = useModelsStore((s) => s.overview);
  const speechProgress = useModelsStore((s) => s.speechProgress);
  const llmProgress = useModelsStore((s) => s.llmProgress);
  const voicesDownload = useMeetingsStore((s) => s.downloads.voices);
  const providers = useProvidersStore((s) => s.list);
  const [withLlm, setWithLlm] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [installed, setInstalled] = useState(false);

  useEffect(() => {
    ensureModelEvents();
    void useModelsStore.getState().load();
    void useProvidersStore.getState().load();
    void dictationMicPermission().then(setPermission).catch(() => {});
  }, []);

  const finish = async () => {
    await setSetting("onboarding_done", "1").catch(() => {});
    onDone();
  };

  const pack = useMemo(() => {
    if (!overview) return null;
    const ids = [...new Set([overview.fit.picks.meetingsFinal, overview.fit.picks.meetingsLive])];
    const speech = ids.map((id) => overview.speech.models.find((m) => m.id === id)).filter((m): m is NonNullable<typeof m> => !!m);
    const engine = speech.some((m) => !m.installed) ? Math.max(...speech.filter((m) => !m.installed).map((m) => m.engineBytes), 0) : 0;
    const speechBytes = speech.filter((m) => !m.installed).reduce((sum, m) => sum + m.sizeBytes, 0) + engine;
    const voicesBytes = overview.voices.voicesSupported && !overview.voices.voicesInstalled ? overview.voices.voicesBytes : 0;
    const llm = overview.llmEngine ? overview.llm.find((r) => r.recommended) : undefined;
    return { speech, speechBytes, voicesBytes, llm };
  }, [overview]);

  const total = pack ? pack.speechBytes + pack.voicesBytes + (withLlm && pack.llm && !pack.llm.installed ? pack.llm.sizeBytes : 0) : 0;

  const install = async () => {
    if (!pack) return;
    setInstalling(true);
    for (const model of pack.speech) if (!model.installed) await useModelsStore.getState().installSpeech(model.id);
    if (pack.voicesBytes > 0) await useMeetingsStore.getState().install("voices");
    if (withLlm && pack.llm && !pack.llm.installed) await useModelsStore.getState().installLlm(pack.llm.id);
    await useModelsStore.getState().load();
    await useMeetingsStore.getState().refreshStatus();
    setInstalling(false);
    setInstalled(true);
  };

  const next = (to: Step) => () => setStep(to);
  const skip = (
    <Button variant="ghost" onClick={() => void finish()}>
      {t("onboarding.skip")}
    </Button>
  );

  if (step === "welcome") {
    return (
      <Shell step={step} footer={<Button variant="primary" size="lg" onClick={next("permissions")}>{t("onboarding.start")}</Button>}>
        <span className="flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-[#12A383] to-[#0B6E5C] text-white shadow-[0_10px_24px_-10px_#12A383]">
          <AudioLines size={26} />
        </span>
        <div>
          <h1 className="text-[24px] font-semibold tracking-tight">{t("onboarding.welcome")}</h1>
          <p className="mt-2 text-[14px] leading-[1.55] text-[var(--cf-text-muted)]">{t("onboarding.welcomeBody")}</p>
        </div>
      </Shell>
    );
  }

  if (step === "permissions") {
    return (
      <Shell
        step={step}
        footer={
          <>
            {skip}
            <Button variant="primary" size="lg" onClick={next("models")}>
              {t("onboarding.next")}
            </Button>
          </>
        }
      >
        <div>
          <h1 className="text-[20px] font-semibold tracking-tight">{t("onboarding.permissions")}</h1>
          <p className="mt-2 text-[13.5px] leading-[1.55] text-[var(--cf-text-muted)]">{t("onboarding.permissionsBody")}</p>
        </div>
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3 rounded-xl border border-[var(--cf-border)] p-3">
            <Mic size={16} className="text-[var(--cf-text-muted)]" />
            <span className="flex-1 text-[13px] font-medium">{t("settings.microphone")}</span>
            {permission === "granted" ? (
              <span className="flex items-center gap-1 text-[12.5px] text-[var(--cf-success)]">
                <Check size={13} />
                {t("onboarding.micOk")}
              </span>
            ) : (
              <Button variant="secondary" onClick={() => void dictationMicRequest().then(setPermission)}>
                {t("onboarding.allowMic")}
              </Button>
            )}
          </div>
          {isMac() && (
            <div className="flex items-center gap-3 rounded-xl border border-[var(--cf-border)] p-3">
              <AudioLines size={16} className="text-[var(--cf-text-muted)]" />
              <div className="min-w-0 flex-1">
                <p className="text-[13px] font-medium">{t("settings.systemAudio")}</p>
                <p className="text-[11.5px] text-[var(--cf-text-muted)]">{t("settings.systemAudioHint")}</p>
              </div>
              <Button variant="secondary" onClick={() => void meetingsSystemAudioSettings().catch(() => {})}>
                {t("settings.openPrivacy")}
              </Button>
            </div>
          )}
        </div>
      </Shell>
    );
  }

  if (step === "models") {
    const speechBusy = Object.values(speechProgress).find((p) => p.phase === "downloading" || p.phase === "unpacking");
    const llmBusy = Object.values(llmProgress).find((p) => p.phase === "downloading");
    const voicesBusy = voicesDownload && (voicesDownload.phase === "downloading" || voicesDownload.phase === "unpacking") ? voicesDownload : undefined;
    const busy = speechBusy ?? voicesBusy ?? llmBusy;
    return (
      <Shell
        step={step}
        footer={
          <>
            {!installing && skip}
            {installed || total === 0 ? (
              <Button variant="primary" size="lg" onClick={next("providers")}>
                {t("onboarding.next")}
              </Button>
            ) : (
              <Button variant="primary" size="lg" disabled={installing || !pack} onClick={() => void install()}>
                {installing && <Spinner size={12} />}
                {installing ? t("onboarding.installing") : t("onboarding.installPack", { size: formatBytes(total) })}
              </Button>
            )}
          </>
        }
      >
        <div>
          <h1 className="text-[20px] font-semibold tracking-tight">{t("onboarding.models")}</h1>
          <p className="mt-2 text-[13.5px] leading-[1.55] text-[var(--cf-text-muted)]">{t("onboarding.modelsBody")}</p>
        </div>
        {!pack ? (
          <div className="cf-skeleton h-[120px] rounded-xl" />
        ) : (
          <div className="flex flex-col gap-2">
            {overview && (
              <p className="text-[12px] text-[var(--cf-text-muted)]">
                {overview.machine.cpu} · {formatBytes(overview.machine.ramBytes)} RAM{overview.machine.gpuName && overview.machine.gpuName !== overview.machine.cpu ? ` · ${overview.machine.gpuName}` : ""}
              </p>
            )}
            {pack.speech.map((model) => (
              <div key={model.id} className="flex items-center gap-3 rounded-xl border border-[var(--cf-border)] p-3">
                <Sparkles size={15} className="text-[var(--cf-accent)]" />
                <span className="flex-1 text-[13px]">{t("onboarding.packSpeech", { name: model.label })}</span>
                {model.installed ? <Check size={14} className="text-[var(--cf-success)]" /> : <span className="text-[12px] text-[var(--cf-text-muted)]">{formatBytes(model.sizeBytes)}</span>}
              </div>
            ))}
            {overview?.voices.voicesSupported && (
              <div className="flex items-center gap-3 rounded-xl border border-[var(--cf-border)] p-3">
                <Sparkles size={15} className="text-[var(--cf-accent)]" />
                <span className="flex-1 text-[13px]">{t("onboarding.packVoices")}</span>
                {overview.voices.voicesInstalled ? <Check size={14} className="text-[var(--cf-success)]" /> : <span className="text-[12px] text-[var(--cf-text-muted)]">{formatBytes(pack.voicesBytes)}</span>}
              </div>
            )}
            {pack.llm && (
              <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-[var(--cf-border)] p-3">
                <Checkbox checked={withLlm || pack.llm.installed} disabled={pack.llm.installed || installing} onChange={setWithLlm} />
                <Cpu size={15} className="text-[var(--cf-text-muted)]" />
                <span className="flex-1 text-[13px]">{t("onboarding.packAnalysis", { name: pack.llm.label })}</span>
                {pack.llm.installed ? <Check size={14} className="text-[var(--cf-success)]" /> : <span className="text-[12px] text-[var(--cf-text-muted)]">{formatBytes(pack.llm.sizeBytes)}</span>}
              </label>
            )}
            {busy && <ProgressBar done={busy.done} total={busy.total} />}
            {installed && <p className="flex items-center gap-1.5 text-[13px] text-[var(--cf-success)]"><Check size={14} />{t("onboarding.installed")}</p>}
          </div>
        )}
      </Shell>
    );
  }

  const found = providers?.builtins.filter((b) => b.found) ?? [];
  return (
    <Shell
      step={step}
      footer={
        <Button variant="primary" size="lg" onClick={() => void finish()}>
          {t("onboarding.done")}
        </Button>
      }
    >
      <div>
        <h1 className="text-[20px] font-semibold tracking-tight">{t("onboarding.providers")}</h1>
        <p className="mt-2 text-[13.5px] leading-[1.55] text-[var(--cf-text-muted)]">{t("onboarding.providersBody")}</p>
      </div>
      <div className="flex flex-col gap-2">
        {found.map((provider) => (
          <button
            key={provider.id}
            type="button"
            onClick={() => void useProvidersStore.getState().select(provider.id)}
            className={`flex items-center gap-3 rounded-xl border p-3 text-left ${providers?.selected === provider.id ? "border-[var(--cf-accent)] bg-[var(--cf-accent-soft)]" : "border-[var(--cf-border)] hover:bg-[var(--cf-hover)]"}`}
          >
            <Terminal size={15} className="text-[var(--cf-text-muted)]" />
            <span className="flex-1 text-[13px] font-medium">{provider.label}</span>
            {providers?.selected === provider.id && <Check size={14} className="text-[var(--cf-accent)]" />}
          </button>
        ))}
        <button
          type="button"
          onClick={() => void useProvidersStore.getState().select(LOCAL_PROVIDER)}
          className={`flex items-center gap-3 rounded-xl border p-3 text-left ${providers?.selected === LOCAL_PROVIDER ? "border-[var(--cf-accent)] bg-[var(--cf-accent-soft)]" : "border-[var(--cf-border)] hover:bg-[var(--cf-hover)]"}`}
        >
          <Cpu size={15} className="text-[var(--cf-text-muted)]" />
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-medium">{t("ai.local")}</p>
            <p className="text-[11.5px] text-[var(--cf-text-muted)]">{t("ai.localHint")}</p>
          </div>
          {providers?.selected === LOCAL_PROVIDER && <Check size={14} className="text-[var(--cf-accent)]" />}
        </button>
      </div>
    </Shell>
  );
}
