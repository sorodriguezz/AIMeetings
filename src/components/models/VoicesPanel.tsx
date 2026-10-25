import { useCallback, useEffect, useState } from "react";
import { Check, Mic, Trash2, X } from "lucide-react";
import { buttonClass } from "../common/Button";
import { ProgressBar, Stat } from "./ModelsView";
import { meetingsDeleteVoice, meetingsRemoveVoices, meetingsRenameVoice, meetingsVoices, type Voice } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useModelsStore } from "../../state/modelsStore";
import { useConfirmStore } from "../../state/confirmStore";
import { useT } from "../../state/languageStore";
import { formatBytes } from "../../lib/format";

/** The voices saved from meetings: renamed in place, forgotten with a click. */
export function VoicesList() {
  const t = useT();
  const [voices, setVoices] = useState<Voice[] | null>(null);
  const reload = useCallback(() => void meetingsVoices().then(setVoices).catch(() => setVoices([])), []);
  useEffect(() => reload(), [reload]);
  if (!voices || voices.length === 0) return <p className="text-[12.5px] text-[var(--cf-text-muted)]">{t("meetings.settings.noVoices")}</p>;
  return (
    <div className="overflow-hidden rounded-xl border border-[var(--cf-border)]">
      {voices.map((voice) => (
        <div key={voice.id} className="flex items-center gap-3 border-b border-[var(--cf-border)] px-3 py-2 text-[13px] last:border-b-0">
          <Mic size={13} className="shrink-0 text-[var(--cf-text-muted)]" />
          <input
            defaultValue={voice.name}
            onBlur={(e) => {
              const name = e.target.value.trim();
              if (name && name !== voice.name) void meetingsRenameVoice(voice.id, name).then(reload);
            }}
            className="min-w-0 flex-1 bg-transparent outline-none"
          />
          {voice.isMe && <span className="rounded-full bg-[var(--cf-accent-soft)] px-2 text-[11px] text-[var(--cf-accent)]">{t("meetings.speaker.me")}</span>}
          <span className="text-[11.5px] text-[var(--cf-text-faint)]">{t("meetings.settings.samples", { n: voice.samples })}</span>
          <button
            type="button"
            aria-label={t("common.delete")}
            className="text-[var(--cf-text-faint)] hover:text-[var(--cf-danger)]"
            onClick={() =>
              void useConfirmStore
                .getState()
                .ask({ message: t("meetings.settings.forgetVoice", { name: voice.name }), confirmLabel: t("common.delete"), danger: true })
                .then((ok) => ok && void meetingsDeleteVoice(voice.id).then(reload))
            }
          >
            <Trash2 size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}

/** Speaker separation: sherpa-onnx, the CAM++ voice model and pyannote's turn detector. */
export function VoicesPanel() {
  const t = useT();
  const status = useMeetingsStore((s) => s.status);
  const download = useMeetingsStore((s) => s.downloads.voices);
  const busy = !!download && download.phase !== "done" && download.phase !== "failed" && download.phase !== "cancelled";
  const refresh = () => {
    void useMeetingsStore.getState().refreshStatus();
    void useModelsStore.getState().load();
  };
  if (!status) return null;
  return (
    <div className="flex flex-col gap-5">
      <div className={`flex flex-col gap-3 rounded-2xl border p-4 ${status.voicesInstalled ? "border-[var(--cf-border)]" : "border-[var(--cf-accent-line)]"}`}>
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span className="text-[14px] font-semibold">3D-Speaker CAM++ · pyannote 3.0</span>
              {status.voicesInstalled && (
                <span className="flex items-center gap-1 text-[11px] font-medium text-[var(--cf-success)]">
                  <Check size={11} />
                  {t("models.installed")}
                </span>
              )}
            </div>
            <p className="mt-0.5 text-[12px] text-[var(--cf-text-muted)]">{t("meetings.settings.voicesHint")}</p>
          </div>
          {!status.voicesSupported ? null : busy ? (
            <button type="button" className={buttonClass({ variant: "ghost", size: "sm" })} onClick={() => void useMeetingsStore.getState().cancelInstall()}>
              <X size={12} />
              {t("models.cancel")}
            </button>
          ) : status.voicesInstalled ? (
            <button
              type="button"
              className={buttonClass({ variant: "danger-ghost", size: "sm" })}
              onClick={() =>
                void useConfirmStore
                  .getState()
                  .ask({ message: t("meetings.settings.voicesRemoveConfirm"), confirmLabel: t("common.delete"), danger: true })
                  .then((ok) => ok && void meetingsRemoveVoices().then(refresh))
              }
            >
              <Trash2 size={12} />
              {t("models.delete")}
            </button>
          ) : (
            <button type="button" className={buttonClass({ variant: "primary", size: "sm" })} onClick={() => void useMeetingsStore.getState().install("voices").then(refresh)}>
              {t("models.download")}
            </button>
          )}
        </div>
        {!status.voicesSupported ? (
          <p className="text-[12.5px] text-[var(--cf-text-muted)]">{t("meetings.settings.voicesUnsupported")}</p>
        ) : (
          <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-4">
            <Stat label={t("models.size")} value={status.voicesInstalled ? "≈ 55 MB" : formatBytes(status.voicesBytes)} />
            <Stat label={t("models.needs")} value={t("models.ramNeed", { size: "≈ 300 MB" })} sub="CPU" />
            <Stat label={t("models.speed")} value={t("models.pace.fast")} tone="text-[var(--cf-success)]" />
            <Stat label={t("models.licence")} value="Apache-2.0 · MIT" />
          </div>
        )}
        {busy && download && <ProgressBar done={download.done} total={download.total} label={download.phase === "unpacking" ? t("meetings.settings.unpacking") : undefined} />}
      </div>
    </div>
  );
}
