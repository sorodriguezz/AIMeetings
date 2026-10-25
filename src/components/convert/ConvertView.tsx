import { ArrowRight, Check, Clock, FileAudio, Film, FolderOpen, Gauge, Mic, Upload, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Select } from "../common/Select";
import { Spinner } from "../common/Spinner";
import { Page, PageHeader } from "../ui/Page";
import { BigButton } from "../ui/Big";
import { duration } from "../meetings/meetingLabels";
import { convertedMbPerHour, fileName, videoExtensions } from "../../lib/media";
import { formatBytes } from "../../lib/format";
import { revealPath } from "../../lib/tauri/commands";
import { useConvertStore, type ConvertJob } from "../../state/convertStore";
import { useUiStore } from "../../state/uiStore";
import { useT } from "../../state/languageStore";

const PICK_FOLDER = "__pick__";

/**
 * «Convertir video a audio»: meetings are often downloaded as videos, and this app only needs their
 * sound. The audio comes out as speech-sized AAC — one channel at the 16 kHz the models read, so
 * nothing they would use is thrown away — at a small part of the video's weight.
 */
export function ConvertView() {
  const t = useT();
  const jobs = useConvertStore((s) => s.jobs);
  const outDir = useConvertStore((s) => s.outDir);

  const choose = async () => {
    const picked = await open({ multiple: true, filters: [{ name: t("v2.convert.videos"), extensions: videoExtensions() }] });
    if (!picked) return;
    useConvertStore.getState().add(Array.isArray(picked) ? picked : [picked]);
  };

  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") useConvertStore.getState().setOutDir(dir);
  };

  const finished = jobs.some((j) => j.status === "done" || j.status === "failed");
  return (
    <Page width={820}>
      <PageHeader icon={Film} title={t("v2.nav.convert")} subtitle={t("v2.convert.subtitle")} />

      <button
        type="button"
        onClick={() => void choose()}
        className="flex flex-col items-center gap-2.5 rounded-3xl border-2 border-dashed border-[var(--cf-border-strong)] px-6 py-12 text-center transition-colors hover:border-[var(--cf-accent)] hover:bg-[var(--cf-accent-soft)]"
      >
        <span className="flex h-14 w-14 items-center justify-center rounded-2xl bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]">
          <Upload size={24} />
        </span>
        <span className="text-[16px] font-semibold">{t("v2.convert.drop")}</span>
        <span className="text-[13.5px] text-[var(--cf-text-muted)]">{t("v2.upload.orChoose")}</span>
        <span className="mt-1 text-[12px] text-[var(--cf-text-faint)]">{videoExtensions().join(" · ").toUpperCase()}</span>
      </button>

      <div className="grid grid-cols-3 gap-3">
        {[
          { icon: FileAudio, title: t("v2.convert.factFormat"), body: t("v2.convert.factFormatBody") },
          { icon: Mic, title: t("v2.convert.factQuality"), body: t("v2.convert.factQualityBody") },
          { icon: Gauge, title: t("v2.convert.factSize", { mb: convertedMbPerHour() }), body: t("v2.convert.factSizeBody") },
        ].map(({ icon: Icon, title, body }) => (
          <div key={title} className="flex flex-col gap-1.5 rounded-2xl bg-[var(--cf-surface-2)] p-4">
            <Icon size={17} className="text-[var(--cf-accent)]" />
            <p className="text-[13.5px] font-semibold">{title}</p>
            <p className="text-[12.5px] leading-[1.45] text-[var(--cf-text-muted)]">{body}</p>
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <span className="text-[13.5px] font-medium">{t("v2.convert.saveIn")}</span>
        <div className="w-[320px]">
          <Select
            size="field"
            value={outDir ?? ""}
            ariaLabel={t("v2.convert.saveIn")}
            onChange={(value) => {
              if (value === PICK_FOLDER) void chooseFolder();
              else useConvertStore.getState().setOutDir(value || null);
            }}
            options={[
              { value: "", label: t("v2.convert.sameFolder") },
              ...(outDir ? [{ value: outDir, label: fileName(outDir) || outDir }] : []),
              { value: PICK_FOLDER, label: t("v2.convert.pickFolder") },
            ]}
          />
        </div>
      </div>

      {jobs.length > 0 && (
        <section className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <h2 className="text-[16px] font-semibold">{t("v2.convert.list")}</h2>
            {finished && (
              <button type="button" onClick={() => useConvertStore.getState().clearFinished()} className="text-[13px] font-medium text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
                {t("v2.convert.clear")}
              </button>
            )}
          </div>
          <div className="flex flex-col divide-y divide-[var(--cf-border)] rounded-2xl border border-[var(--cf-border)]">
            {jobs.map((job) => (
              <JobRow key={job.id} job={job} />
            ))}
          </div>
        </section>
      )}
    </Page>
  );
}

function JobRow({ job }: { job: ConvertJob }) {
  const t = useT();
  const pct = Math.round(job.fraction * 100);
  return (
    <div className="flex items-center gap-4 px-4 py-3.5">
      <span
        className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-xl ${
          job.status === "done" ? "bg-[color-mix(in_oklab,var(--cf-success)_14%,transparent)] text-[var(--cf-success)]" : job.status === "failed" ? "bg-[color-mix(in_oklab,var(--cf-danger)_12%,transparent)] text-[var(--cf-danger)]" : "bg-[var(--cf-sunken)] text-[var(--cf-text-muted)]"
        }`}
      >
        {job.status === "done" ? <Check size={18} /> : job.status === "running" ? <Spinner size={16} /> : job.status === "failed" ? <X size={18} /> : <Clock size={17} />}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[14px] font-semibold" title={job.path}>
          {fileName(job.path)}
        </p>
        {job.status === "waiting" && <p className="text-[12.5px] text-[var(--cf-text-muted)]">{t("v2.convert.waiting")}</p>}
        {job.status === "running" && (
          <div className="mt-1.5 flex items-center gap-3">
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-[var(--cf-border)]">
              <div className="h-full rounded-full bg-[var(--cf-accent)] transition-[width] duration-200" style={{ width: `${job.stage === "encode" ? 100 : pct}%` }} />
            </div>
            <span className="w-[150px] shrink-0 text-right text-[12px] text-[var(--cf-text-muted)]">{job.stage === "encode" ? t("v2.convert.saving") : t("v2.convert.reading", { pct })}</span>
          </div>
        )}
        {job.status === "done" && job.result && (
          <p className="text-[12.5px] text-[var(--cf-text-muted)]">
            {t("v2.convert.done", { before: formatBytes(job.result.sourceBytes), after: formatBytes(job.result.bytes) })} · {duration(job.result.durationMs)} · {fileName(job.result.path)}
          </p>
        )}
        {job.status === "failed" && <p className="cf-selectable text-[12.5px] text-[var(--cf-danger)]">{job.error}</p>}
      </div>
      {job.status === "done" && job.result && (
        <>
          <BigButton variant="ghost" onClick={() => void revealPath(job.result!.path)}>
            <FolderOpen size={14} />
            {t("v2.convert.show")}
          </BigButton>
          <BigButton variant="secondary" onClick={() => useUiStore.getState().openUpload(job.result!.path)}>
            {t("v2.upload.transcribe")}
            <ArrowRight size={14} />
          </BigButton>
        </>
      )}
      {(job.status === "waiting" || job.status === "failed") && (
        <button type="button" aria-label={t("common.delete")} onClick={() => useConvertStore.getState().remove(job.id)} className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--cf-text-faint)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]">
          <X size={15} />
        </button>
      )}
    </div>
  );
}
