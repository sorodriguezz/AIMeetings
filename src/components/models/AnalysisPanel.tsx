import { useEffect, useState } from "react";
import { Check, ExternalLink, Gauge, RefreshCw, Sparkles, Trash2, X } from "lucide-react";
import { buttonClass } from "../common/Button";
import { Select } from "../common/Select";
import { Spinner } from "../common/Spinner";
import { PACE_TONE, ProgressBar, Stat } from "./ModelsView";
import { llmProbe, llmServerModels, llmSetConfig, type LlmBackend, type LlmRow, type ModelsOverview } from "../../lib/tauri/modelsCommands";
import { openExternalUrl } from "../../lib/tauri/commands";
import { useModelsStore } from "../../state/modelsStore";
import { useConfirmStore } from "../../state/confirmStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";
import { useT } from "../../state/languageStore";
import { formatBytes, formatCtx } from "../../lib/format";

const FIT_TONE: Record<string, string> = {
  comfortable: "text-[var(--cf-success)]",
  tight: "text-[var(--cf-warning)]",
  "spills-to-cpu": "text-[var(--cf-warning)]",
  "does-not-fit": "text-[var(--cf-danger)]",
  unknown: "text-[var(--cf-text-muted)]",
};

function LlmCard({ row, onUse }: { row: LlmRow; onUse: () => void }) {
  const t = useT();
  const progress = useModelsStore((s) => s.llmProgress[row.id]);
  const busy = progress?.phase === "downloading";
  const unusable = row.cudaOnly || row.fit === "does-not-fit";
  return (
    <div className={`flex flex-col gap-3 rounded-2xl border p-4 ${row.recommended ? "border-[var(--cf-accent-line)]" : "border-[var(--cf-border)]"} ${unusable && !row.installed ? "opacity-70" : ""}`}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[14px] font-semibold">{row.label}</span>
            <span className="rounded-full bg-[var(--cf-sunken)] px-2 py-[1px] text-[10.5px] font-medium text-[var(--cf-text-muted)]">{row.params}</span>
            {row.recommended && (
              <span className="flex items-center gap-1 rounded-full bg-[var(--cf-accent-soft)] px-2 py-[1px] text-[11px] font-medium text-[var(--cf-accent)]">
                <Sparkles size={10} />
                {t("models.recommended")}
              </span>
            )}
            {row.installed && (
              <span className="flex items-center gap-1 text-[11px] font-medium text-[var(--cf-success)]">
                <Check size={11} />
                {t("models.installed")}
              </span>
            )}
          </div>
          {row.cudaOnly && <p className="mt-0.5 text-[12px] text-[var(--cf-warning)]">{t("models.cudaOnly")}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {busy ? (
            <button type="button" className={buttonClass({ variant: "ghost", size: "sm" })} onClick={() => void useModelsStore.getState().cancelLlm()}>
              <X size={12} />
              {t("models.cancel")}
            </button>
          ) : row.installed ? (
            <>
              <button type="button" disabled={row.active} className={buttonClass({ variant: row.active ? "primary" : "secondary", size: "sm" })} onClick={onUse}>
                {row.active && <Check size={11} />}
                {row.active ? t("models.inUse") : t("models.use")}
              </button>
              <button
                type="button"
                className={buttonClass({ variant: "danger-ghost", size: "sm" })}
                aria-label={t("models.delete")}
                onClick={() =>
                  void useConfirmStore
                    .getState()
                    .ask({ message: t("models.deleteConfirm", { name: row.label }), confirmLabel: t("models.delete"), danger: true })
                    .then((ok) => ok && void useModelsStore.getState().removeLlm(row.id))
                }
              >
                <Trash2 size={12} />
              </button>
            </>
          ) : (
            <button type="button" disabled={row.cudaOnly} className={buttonClass({ variant: row.recommended ? "primary" : "secondary", size: "sm" })} onClick={() => void useModelsStore.getState().installLlm(row.id)}>
              {row.partialBytes ? t("models.resume", { done: formatBytes(row.partialBytes), total: formatBytes(row.sizeBytes) }) : t("models.download")}
            </button>
          )}
        </div>
      </div>
      <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3 xl:grid-cols-6">
        <Stat label={t("models.size")} value={formatBytes(row.sizeBytes)} />
        <Stat label={t("models.needs")} value={formatBytes(row.needBytes)} sub={t("models.context", { ctx: formatCtx(row.ctx) })} />
        <Stat label="RAM / VRAM" value={t(`models.fit.${row.fit}`)} tone={FIT_TONE[row.fit]} sub={row.gpuShare > 0 ? t("models.gpuShare", { pct: `${Math.round(row.gpuShare * 100)}%` }) : "CPU"} />
        <Stat
          label={t("models.speed")}
          value={t(`models.pace.${row.pace}`)}
          tone={PACE_TONE[row.pace]}
          sub={row.writeTps ? `${t("models.tps", { tps: Math.round(row.writeTps * 0.75) })} · ${t("models.estimated")}` : undefined}
        />
        <Stat label={t("models.contextLabel")} value={formatCtx(row.maxCtx)} sub="max" />
        <Stat
          label={t("models.licence")}
          value={
            row.licenceUrl ? (
              <button type="button" className="flex items-center gap-1 text-[var(--cf-accent)] hover:underline" onClick={() => void openExternalUrl(row.licenceUrl)}>
                {row.licence}
                <ExternalLink size={10} />
              </button>
            ) : (
              row.licence
            )
          }
        />
      </div>
      {busy && progress && <ProgressBar done={progress.done} total={progress.total} />}
    </div>
  );
}

/** The local language model: which backend runs it, which model, how much window — and the catalogue. */
export function AnalysisPanel({ overview }: { overview: ModelsOverview }) {
  const t = useT();
  const config = overview.llmConfig;
  const [backend, setBackend] = useState<LlmBackend>(config.backend);
  const [url, setUrl] = useState(config.url);
  const [model, setModel] = useState(config.model);
  const [key, setKey] = useState("");
  const [served, setServed] = useState<string[] | null>(null);
  const [probing, setProbing] = useState(false);
  const [listing, setListing] = useState(false);

  useEffect(() => {
    setBackend(config.backend);
    setUrl(config.url);
    setModel(config.model);
  }, [config.backend, config.url, config.model]);

  const save = async (patch: Partial<{ backend: LlmBackend; model: string; ctx: number; url: string; apiKey: string }>) => {
    try {
      await llmSetConfig({ backend, model, ctx: config.ctx, url, ...patch });
      await useModelsStore.getState().load();
    } catch (error) {
      pushErrorToast(String(error));
    }
  };

  const list = async () => {
    setListing(true);
    try {
      setServed(await llmServerModels(backend, url));
    } catch (error) {
      pushErrorToast(String(error));
      setServed([]);
    } finally {
      setListing(false);
    }
  };

  const probe = async () => {
    setProbing(true);
    try {
      const result = await llmProbe();
      useToastStore.getState().pushToast(t("models.probeResult", { tps: result.writeTps.toFixed(1), model: result.model }), "success");
    } catch (error) {
      pushErrorToast(String(error));
    } finally {
      setProbing(false);
    }
  };

  const field = "h-[30px] w-full rounded-lg border border-[var(--cf-field-border)] bg-[var(--cf-field)] px-2.5 text-[13px] outline-none focus:border-[var(--cf-accent)]";
  const active = overview.llm.find((r) => r.active);
  const ctxOptions = [0, 8192, 16384, 32768, 65536].filter((c) => c === 0 || !active || c <= active.maxCtx);

  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-1 gap-3 rounded-2xl bg-[var(--cf-surface-2)] p-4 md:grid-cols-[200px_minmax(0,1fr)_auto]">
        <label className="flex flex-col gap-1">
          <span className="text-[11.5px] font-medium text-[var(--cf-text-muted)]">{t("models.backend")}</span>
          <Select
            size="field"
            value={backend}
            onChange={(value) => {
              const next = value as LlmBackend;
              setBackend(next);
              setServed(null);
              void save({ backend: next, model: next === "bundled" ? "" : model });
            }}
            options={[
              { value: "bundled", label: t("models.backend.bundled") },
              { value: "ollama", label: t("models.backend.ollama") },
              { value: "openai", label: t("models.backend.openai") },
            ]}
          />
        </label>
        {backend === "bundled" ? (
          <label className="flex flex-col gap-1">
            <span className="text-[11.5px] font-medium text-[var(--cf-text-muted)]">{t("models.contextLabel")}</span>
            <Select
              size="field"
              value={String(config.ctx)}
              onChange={(value) => void save({ ctx: Number(value) })}
              options={ctxOptions.map((c) => ({ value: String(c), label: c === 0 ? t("models.contextAuto") : formatCtx(c) }))}
            />
          </label>
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <label className="flex flex-col gap-1">
              <span className="text-[11.5px] font-medium text-[var(--cf-text-muted)]">{t("models.serverUrl")}</span>
              <input className={field} value={url} placeholder={backend === "ollama" ? "http://127.0.0.1:11434" : "https://…/v1"} onChange={(e) => setUrl(e.target.value)} onBlur={() => void save({})} />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-[11.5px] font-medium text-[var(--cf-text-muted)]">{t("models.serverModel")}</span>
              <div className="flex gap-1.5">
                {served && served.length > 0 ? (
                  <Select size="field" className="flex-1" value={model} onChange={(value) => { setModel(value); void save({ model: value }); }} options={served.map((m) => ({ value: m, label: m }))} />
                ) : (
                  <input className={field} value={model} placeholder="qwen3:4b" onChange={(e) => setModel(e.target.value)} onBlur={() => void save({})} />
                )}
                <button type="button" className={buttonClass({ variant: "secondary", size: "md" })} title={t("models.serverRefresh")} onClick={() => void list()}>
                  {listing ? <Spinner size={12} /> : <RefreshCw size={13} />}
                </button>
              </div>
            </label>
            {backend === "openai" && (
              <label className="flex flex-col gap-1 sm:col-span-2">
                <span className="text-[11.5px] font-medium text-[var(--cf-text-muted)]">{t("models.serverKey")}</span>
                <input className={field} type="password" value={key} placeholder="sk-…" onChange={(e) => setKey(e.target.value)} onBlur={() => key && void save({ apiKey: key }).then(() => setKey(""))} />
              </label>
            )}
          </div>
        )}
        <div className="flex items-end">
          <button type="button" disabled={probing} className={buttonClass({ variant: "secondary", size: "md" })} onClick={() => void probe()}>
            {probing ? <Spinner size={12} /> : <Gauge size={13} />}
            {t("models.probe")}
          </button>
        </div>
      </div>
      {backend === "bundled" && (
        <>
          {!overview.llmEngine && <p className="rounded-xl bg-[color-mix(in_oklab,var(--cf-warning)_10%,transparent)] px-3 py-2 text-[12.5px] text-[var(--cf-warning)]">{t("models.engineMissing")}</p>}
          <div className="flex flex-col gap-3">
            {overview.llm.map((row) => (
              <LlmCard key={row.id} row={row} onUse={() => void save({ backend: "bundled", model: row.id })} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
