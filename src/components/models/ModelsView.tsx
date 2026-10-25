import { useEffect, useState } from "react";
import { ArrowRight, BrainCircuit, Check, Cpu, Gauge, HardDrive, MemoryStick, Mic, Monitor, Sparkles, Trash2, Users, Wand2, X } from "lucide-react";
import { buttonClass } from "../common/Button";
import { Spinner } from "../common/Spinner";
import { VoicesPanel } from "./VoicesPanel";
import { AnalysisPanel } from "./AnalysisPanel";
import { ProvidersSettings } from "../settings/ProvidersSettings";
import { useModelsStore, ensureModelEvents } from "../../state/modelsStore";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore, type AiTab } from "../../state/uiStore";
import { Page, PageHeader } from "../ui/Page";
import { Tabs } from "../ui/Tabs";
import { useConfirmStore } from "../../state/confirmStore";
import { useT, type Translate } from "../../state/languageStore";
import { formatBytes } from "../../lib/format";
import type { DictationModel } from "../../lib/tauri/dictationCommands";
import { llmSetConfig, type ListenFit, type MachineInfo, type ModelsOverview } from "../../lib/tauri/modelsCommands";


export const PACE_TONE: Record<string, string> = {
  fast: "text-[var(--cf-success)]",
  ok: "text-[var(--cf-success)]",
  usable: "text-[var(--cf-success)]",
  slow: "text-[var(--cf-warning)]",
  "too-slow": "text-[var(--cf-danger)]",
  crawl: "text-[var(--cf-danger)]",
  unknown: "text-[var(--cf-text-muted)]",
};

/** A label over a value, the unit every model card is built from. */
export function Stat({ label, value, sub, tone = "" }: { label: string; value: React.ReactNode; sub?: React.ReactNode; tone?: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="text-[10.5px] font-medium uppercase tracking-wide text-[var(--cf-text-faint)]">{label}</span>
      <span className={`truncate text-[13px] font-medium ${tone}`}>{value}</span>
      {sub && <span className="truncate text-[11px] text-[var(--cf-text-muted)]">{sub}</span>}
    </div>
  );
}

/** A download in progress: its bar, and how far it got. */
export function ProgressBar({ done, total, label }: { done: number; total: number; label?: string }) {
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  return (
    <div className="flex items-center gap-3">
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-[var(--cf-border)]">
        <div className="h-full rounded-full bg-[var(--cf-accent)] transition-[width] duration-200" style={{ width: `${pct}%` }} />
      </div>
      <span className="w-[120px] shrink-0 text-right font-mono text-[11px] tabular-nums text-[var(--cf-text-muted)]">{label ?? `${formatBytes(done)} / ${formatBytes(total)}`}</span>
    </div>
  );
}

function gpuText(machine: MachineInfo, t: Translate): string {
  const name = machine.gpuName ?? "GPU";
  switch (machine.gpu) {
    case "unified":
      return t("models.gpuUnified", { name, size: formatBytes(machine.gpuBytes) });
    case "discrete":
      return t("models.gpuDiscrete", { name, size: formatBytes(machine.gpuBytes) });
    case "integrated":
      return t("models.gpuIntegrated", { name });
    default:
      return t("models.gpuNone");
  }
}

function MachineCard({ machine }: { machine: MachineInfo }) {
  const t = useT();
  const os = machine.os === "macos" ? "macOS" : machine.os === "windows" ? "Windows" : machine.os;
  const arch = machine.arch === "aarch64" ? "ARM64" : machine.arch === "x86_64" ? "x64" : machine.arch;
  const items = [
    { icon: Cpu, label: t("models.cpu"), value: machine.cpu || "—", sub: t("models.cores", { cores: machine.cores, threads: machine.threads }) },
    { icon: MemoryStick, label: t("models.ram"), value: formatBytes(machine.ramBytes), sub: machine.ramBandwidth ? `${Math.round(machine.ramBandwidth / 1e9)} GB/s` : undefined },
    { icon: Monitor, label: t("models.gpu"), value: gpuText(machine, t) },
    { icon: HardDrive, label: t("models.onDisk"), value: formatBytes(machine.modelsBytes), sub: `${os} · ${arch}` },
  ];
  return (
    <div className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-border)] lg:grid-cols-4">
      {items.map(({ icon: Icon, label, value, sub }) => (
        <div key={label} className="flex items-start gap-3 bg-[var(--cf-surface)] p-4">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[var(--cf-sunken)]">
            <Icon size={15} className="text-[var(--cf-text-muted)]" />
          </span>
          <Stat label={label} value={value} sub={sub} />
        </div>
      ))}
    </div>
  );
}

function SpeechCard({ model, fit, live, final, onUse }: { model: DictationModel; fit?: ListenFit; live: boolean; final: boolean; onUse: (slot: "live" | "final") => void }) {
  const t = useT();
  const progress = useModelsStore((s) => s.speechProgress[model.id] ?? (s.speechProgress.engine && !model.installed ? s.speechProgress.engine : undefined));
  const measuring = useModelsStore((s) => s.measuring === model.id);
  const busy = !!progress && progress.phase !== "done" && progress.phase !== "failed" && progress.phase !== "cancelled";
  const recommended = fit?.recommendedFor ?? [];
  const canFinal = model.features.includes("meetingsFinal");
  const languages = model.languages.length === 0 ? t("models.languagesAll") : model.languages.map((l) => l.toUpperCase()).join(" · ");
  return (
    <div className={`flex flex-col gap-3 rounded-2xl border p-4 transition-colors ${recommended.length > 0 ? "border-[var(--cf-accent-line)]" : "border-[var(--cf-border)]"}`}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[14px] font-semibold">{model.label}</span>
            <span className="rounded-full bg-[var(--cf-sunken)] px-2 py-[1px] text-[10.5px] font-medium uppercase tracking-wide text-[var(--cf-text-muted)]">{model.family}</span>
            {recommended.length > 0 && (
              <span className="flex items-center gap-1 rounded-full bg-[var(--cf-accent-soft)] px-2 py-[1px] text-[11px] font-medium text-[var(--cf-accent)]">
                <Sparkles size={10} />
                {t("models.recommendedFor", { what: recommended.map((r) => (r === "meetingsLive" ? t("models.forLive") : t("models.forFinal"))).join(" · ") })}
              </span>
            )}
            {model.installed && (
              <span className="flex items-center gap-1 text-[11px] font-medium text-[var(--cf-success)]">
                <Check size={11} />
                {t("models.installed")}
              </span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {busy ? (
            <button type="button" className={buttonClass({ variant: "ghost", size: "sm" })} onClick={() => void useModelsStore.getState().cancelSpeech()}>
              <X size={12} />
              {t("models.cancel")}
            </button>
          ) : model.installed ? (
            <>
              <button type="button" className={buttonClass({ variant: live ? "primary" : "secondary", size: "sm" })} onClick={() => onUse("live")} title={t("meetings.settings.liveModel")}>
                {live && <Check size={11} />}
                {t("models.forLive")}
              </button>
              {canFinal && (
                <button type="button" className={buttonClass({ variant: final ? "primary" : "secondary", size: "sm" })} onClick={() => onUse("final")} title={t("meetings.settings.finalModel")}>
                  {final && <Check size={11} />}
                  {t("models.forFinal")}
                </button>
              )}
              <button type="button" disabled={measuring} className={buttonClass({ variant: "ghost", size: "sm" })} onClick={() => void useModelsStore.getState().measureSpeech(model.id)}>
                {measuring ? <Spinner size={11} /> : <Gauge size={12} />}
                {measuring ? t("models.measuring") : t("models.measure")}
              </button>
              <button
                type="button"
                className={buttonClass({ variant: "danger-ghost", size: "sm" })}
                aria-label={t("models.delete")}
                onClick={() =>
                  void useConfirmStore
                    .getState()
                    .ask({ message: t("models.deleteConfirm", { name: model.label }), confirmLabel: t("models.delete"), danger: true })
                    .then((ok) => ok && void useModelsStore.getState().removeSpeech(model.id))
                }
              >
                <Trash2 size={12} />
              </button>
            </>
          ) : (
            <button type="button" className={buttonClass({ variant: recommended.length > 0 ? "primary" : "secondary", size: "sm" })} onClick={() => void useModelsStore.getState().installSpeech(model.id)}>
              {t("models.download")}
            </button>
          )}
        </div>
      </div>
      <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3 xl:grid-cols-6">
        <Stat label={t("models.size")} value={formatBytes(model.sizeBytes)} sub={model.engineBytes > 0 && !model.installed ? t("models.engine", { size: formatBytes(model.engineBytes) }) : undefined} />
        <Stat label={t("models.needs")} value={fit ? t("models.ramNeed", { size: formatBytes(fit.ramMb * 1e6) }) : "—"} sub="CPU" />
        <Stat
          label={t("models.speed")}
          value={fit ? t(`models.pace.${fit.pace}`) : "—"}
          tone={fit ? PACE_TONE[fit.pace] : ""}
          sub={fit ? `${t("models.speedPerPhrase", { s: (fit.ms / 1000).toFixed(fit.ms < 1000 ? 2 : 1) })} · ${fit.measured ? t("models.measured") : t("models.estimated")}` : undefined}
        />
        <Stat label={t("models.accuracy")} value={fit ? t(`models.accuracy.${fit.accuracy}`) : "—"} />
        <Stat label={t("models.languages")} value={languages} />
        <Stat label={t("models.licence")} value={model.licence} />
      </div>
      {busy && progress && <ProgressBar done={progress.done} total={progress.total} label={progress.phase === "unpacking" ? t("meetings.settings.unpacking") : undefined} />}
    </div>
  );
}

/** One thing the user wants done, with the model recommended for it and a single button. */
function TaskCard({
  icon: Icon,
  title,
  description,
  optional,
  done,
  children,
  detailsLabel,
  details,
}: {
  icon: typeof Cpu;
  title: string;
  description: string;
  optional?: string;
  done: boolean;
  children: React.ReactNode;
  detailsLabel?: string;
  details?: React.ReactNode;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <section className="flex flex-col gap-4 rounded-3xl border border-[var(--cf-border)] bg-[var(--cf-surface)] p-5">
      <div className="flex items-start gap-4">
        <span className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl ${done ? "bg-[color-mix(in_oklab,var(--cf-success)_14%,transparent)] text-[var(--cf-success)]" : "bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]"}`}>
          {done ? <Check size={20} /> : <Icon size={20} />}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-[16px] font-semibold">{title}</h2>
            {optional && <span className="rounded-full bg-[var(--cf-sunken)] px-2 py-[1px] text-[11px] text-[var(--cf-text-muted)]">{optional}</span>}
          </div>
          <p className="mt-0.5 text-[13px] leading-[1.5] text-[var(--cf-text-muted)]">{description}</p>
        </div>
      </div>
      {children}
      {details && (
        <>
          <button type="button" onClick={() => setOpen((v) => !v)} className="self-start rounded-full px-2 py-1 text-[12.5px] font-medium text-[var(--cf-accent)] hover:bg-[var(--cf-accent-soft)]">
            {open ? t("models.hideAll") : detailsLabel}
          </button>
          {open && <div className="cf-fade-in flex flex-col gap-3">{details}</div>}
        </>
      )}
    </section>
  );
}

/** The recommended model of a task, in plain words: what it weighs, needs, and how it runs here. */
function Pick({ name, facts, action }: { name: string; facts: string[]; action: React.ReactNode }) {
  const t = useT();
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-2xl bg-[var(--cf-surface-2)] px-4 py-3">
      <Sparkles size={15} className="shrink-0 text-[var(--cf-accent)]" />
      <div className="min-w-0 flex-1">
        <p className="text-[13.5px] font-semibold">
          {name} <span className="ml-1 text-[11.5px] font-medium text-[var(--cf-accent)]">{t("models.recommended")}</span>
        </p>
        <p className="text-[12px] text-[var(--cf-text-muted)]">{facts.filter(Boolean).join(" · ")}</p>
      </div>
      {action}
    </div>
  );
}

/**
 * «IA y modelos», one job per tab: turning speech into text, telling people apart, and writing
 * summaries. Each tab says whether that job is ready, recommends the model that suits this computer
 * with one button, and keeps the full list (with every measurement) a click away.
 */
export function ModelsView() {
  const t = useT();
  const overview = useModelsStore((s) => s.overview);
  const status = useMeetingsStore((s) => s.status);
  const tab = useUiStore((s) => s.aiTab);
  const [machineOpen, setMachineOpen] = useState(false);

  useEffect(() => {
    ensureModelEvents();
    void useModelsStore.getState().load();
  }, []);

  const machine = overview?.machine;
  const machineLine = machine ? [machine.cpu, formatBytes(machine.ramBytes) + " RAM", machine.gpuName && machine.gpuName !== machine.cpu ? machine.gpuName : ""].filter(Boolean).join(" · ") : "";
  const speechReady = !!overview?.speech.models.some((m) => m.installed);

  return (
    <Page width={880}>
      <PageHeader icon={Sparkles} title={t("nav.models")} subtitle={t("v2.models.subtitle")} />
      {machine && (
        <div className="-mt-2 flex flex-col gap-3">
          <button type="button" onClick={() => setMachineOpen((v) => !v)} className="flex items-center gap-2 self-start rounded-full bg-[var(--cf-sunken)] px-3.5 py-1.5 text-[12.5px] text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
            <Cpu size={13} />
            {t("v2.models.yourComputer")} {machineLine}
            <span className="font-medium text-[var(--cf-accent)]">{machineOpen ? t("models.hideAll") : t("models.details")}</span>
          </button>
          {machineOpen && <MachineCard machine={machine} />}
        </div>
      )}
      <Tabs<AiTab>
        value={tab}
        onChange={(id) => useUiStore.getState().openAi(id)}
        items={[
          { id: "speech", label: t("v2.models.tabSpeech"), icon: Mic, dot: !!overview && !speechReady },
          { id: "voices", label: t("v2.models.tabVoices"), icon: Users, dot: !!status && !status.voicesInstalled && status.voicesSupported },
          { id: "summaries", label: t("v2.models.tabSummaries"), icon: Wand2 },
        ]}
      />
      {!overview ? (
        <div className="flex flex-col gap-3">
          {[0, 1].map((i) => (
            <div key={i} className="cf-skeleton h-[140px] rounded-3xl" />
          ))}
        </div>
      ) : tab === "speech" ? (
        <SpeechTab overview={overview} />
      ) : tab === "voices" ? (
        <VoicesTab />
      ) : (
        <SummariesTab overview={overview} />
      )}
    </Page>
  );
}

function SpeechTab({ overview }: { overview: ModelsOverview }) {
  const t = useT();
  const settings = useMeetingsStore((s) => s.settings);
  const speechProgress = useModelsStore((s) => s.speechProgress);
  const fitOf = (id: string) => overview.fit.listen.find((f) => f.id === id);
  const speechModels = overview.speech.models;
  const installedSpeech = speechModels.filter((m) => m.installed);
  const pick = speechModels.find((m) => m.id === overview.fit.picks.meetingsFinal);
  const pickFit = pick ? fitOf(pick.id) : undefined;
  const speechBusy = Object.values(speechProgress).find((p) => p.phase === "downloading" || p.phase === "unpacking");
  const useSpeech = (id: string, slot: "live" | "final") => void useMeetingsStore.getState().saveSetting(slot === "live" ? "liveModel" : "finalModel", id);
  return (
    <TaskCard
      icon={Mic}
      title={t("models.task.speech")}
      description={t("models.task.speechBody")}
      done={installedSpeech.length > 0}
      detailsLabel={t("models.seeAll", { n: speechModels.length })}
      details={
        !overview.speech.supported ? (
          <p className="text-[13px] text-[var(--cf-text-muted)]">{t("models.unsupported")}</p>
        ) : (
          speechModels.map((model) => (
            <SpeechCard key={model.id} model={model} fit={fitOf(model.id)} live={settings.liveModel === model.id} final={settings.finalModel === model.id} onUse={(slot) => useSpeech(model.id, slot)} />
          ))
        )
      }
    >
      {installedSpeech.length > 0 && <p className="text-[14px]">{t("models.readyWith", { names: installedSpeech.map((m) => m.label).join(", ") })}</p>}
      {pick && !pick.installed && (
        <Pick
          name={pick.label}
          facts={[
            t("models.weighs", { size: formatBytes(pick.sizeBytes + (pick.engineBytes || 0)) }),
            pickFit ? t("models.ramNeed", { size: formatBytes(pickFit.ramMb * 1e6) }) : "",
            pickFit ? `${t(`models.pace.${pickFit.pace}`)} · ${t(`models.accuracy.${pickFit.accuracy}`).toLowerCase()} ${t("models.accuracyWord")}` : "",
          ]}
          action={
            <button type="button" disabled={!!speechBusy} className={buttonClass({ variant: "primary", size: "lg" })} onClick={() => void useModelsStore.getState().installSpeech(pick.id)}>
              {t("models.install")}
            </button>
          }
        />
      )}
      {speechBusy && <ProgressBar done={speechBusy.done} total={speechBusy.total} label={speechBusy.phase === "unpacking" ? t("meetings.settings.unpacking") : undefined} />}
    </TaskCard>
  );
}

function VoicesTab() {
  const t = useT();
  const status = useMeetingsStore((s) => s.status);
  const voicesDownload = useMeetingsStore((s) => s.downloads.voices);
  const voicesBusy = voicesDownload && (voicesDownload.phase === "downloading" || voicesDownload.phase === "unpacking");
  return (
    <TaskCard icon={Users} title={t("models.task.voices")} description={t("models.task.voicesBody")} done={!!status?.voicesInstalled} detailsLabel={t("models.manage")} details={<VoicesPanel />}>
      {status && !status.voicesInstalled && status.voicesSupported && (
        <Pick
          name="CAM++ · pyannote"
          facts={[t("models.weighs", { size: formatBytes(status.voicesBytes) }), t("models.ramNeed", { size: "≈ 300 MB" }), t("models.pace.fast")]}
          action={
            <button type="button" disabled={!!voicesBusy} className={buttonClass({ variant: "primary", size: "lg" })} onClick={() => void useMeetingsStore.getState().install("voices").then(() => useModelsStore.getState().load())}>
              {t("models.install")}
            </button>
          }
        />
      )}
      {status?.voicesInstalled && <p className="text-[14px]">{t("models.voicesInstalled")}</p>}
      {voicesBusy && voicesDownload && <ProgressBar done={voicesDownload.done} total={voicesDownload.total} />}
      <button type="button" onClick={() => useUiStore.getState().setView("people")} className="flex items-center gap-1.5 self-start text-[13px] font-medium text-[var(--cf-accent)] hover:underline">
        {t("v2.people.seeSaved")}
        <ArrowRight size={13} />
      </button>
    </TaskCard>
  );
}

function SummariesTab({ overview }: { overview: ModelsOverview }) {
  const t = useT();
  const llmProgress = useModelsStore((s) => s.llmProgress);
  const llmPick = overview.llm.find((r) => r.recommended);
  const llmActive = overview.llm.find((r) => r.active && r.installed);
  const llmBusy = Object.values(llmProgress).find((p) => p.phase === "downloading");
  const use = (id: string) => llmSetConfig({ backend: "bundled", model: id, ctx: overview.llmConfig.ctx, url: overview.llmConfig.url }).then(() => useModelsStore.getState().load());
  return (
    <div className="flex flex-col gap-5">
      <section className="rounded-3xl border border-[var(--cf-border)] bg-[var(--cf-surface)] p-5">
        <ProvidersSettings />
      </section>
      <div id="local-ai">
        <TaskCard
          icon={BrainCircuit}
          title={t("models.task.analysis")}
          description={t("models.task.analysisBody")}
          optional={t("models.optional")}
          done={!!llmActive}
          detailsLabel={t("models.seeAllAdvanced")}
          details={<AnalysisPanel overview={overview} />}
        >
          {llmActive && <p className="text-[14px]">{t("models.readyWith", { names: llmActive.label })}</p>}
          {!llmActive && llmPick && overview.llmEngine && (
            <Pick
              name={llmPick.label}
              facts={[t("models.weighs", { size: formatBytes(llmPick.sizeBytes) }), t("models.ramNeed", { size: formatBytes(llmPick.needBytes) }), t(`models.pace.${llmPick.pace}`)]}
              action={
                llmPick.installed ? (
                  <button type="button" className={buttonClass({ variant: "primary", size: "lg" })} onClick={() => void use(llmPick.id)}>
                    {t("models.use")}
                  </button>
                ) : (
                  <button type="button" disabled={!!llmBusy} className={buttonClass({ variant: "primary", size: "lg" })} onClick={() => void useModelsStore.getState().installLlm(llmPick.id).then(() => use(llmPick.id))}>
                    {t("models.install")}
                  </button>
                )
              }
            />
          )}
          {llmBusy && <ProgressBar done={llmBusy.done} total={llmBusy.total} />}
        </TaskCard>
      </div>
    </div>
  );
}
