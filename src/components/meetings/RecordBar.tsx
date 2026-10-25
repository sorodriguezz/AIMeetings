import { useEffect, useState } from "react";
import { MoreHorizontal, Pause, Play, Square, Trash2 } from "lucide-react";
import { ContextMenu } from "../common/ContextMenu";
import { BigButton } from "../ui/Big";
import { Wave } from "../common/Wave";
import { MicCheck } from "./MicCheck";
import { channelsOf, clock } from "./meetingLabels";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useConfirmStore } from "../../state/confirmStore";
import { useT } from "../../state/languageStore";

/** The recording's own clock: the backend's elapsed time, run on locally between reads. */
export function useElapsed(): number {
  const recording = useMeetingsStore((s) => s.status?.recording ?? null);
  const at = useMeetingsStore((s) => s.statusAt);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!recording || recording.paused) return;
    const timer = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [recording]);
  if (!recording) return 0;
  return recording.paused ? recording.elapsedMs : recording.elapsedMs + Math.max(0, now - at);
}

/**
 * The recording in progress: a big clock, how each input sounds (with the words «your microphone»
 * and «the computer's sound», not icons alone), and the two buttons that matter — pause and stop.
 */
export function RecordBar({ meetingId }: { meetingId: string }) {
  const t = useT();
  const recording = useMeetingsStore((s) => s.status?.recording ?? null);
  const levels = useMeetingsStore((s) => s.levels);
  const lag = useMeetingsStore((s) => s.lag);
  const meeting = useMeetingsStore((s) => s.details[meetingId]?.meeting);
  const transcribeOnStop = useMeetingsStore((s) => s.settings.transcribeOnStop);
  const elapsed = useElapsed();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const twoChannels = meeting ? channelsOf(meeting).includes("system") : true;
  const paused = recording?.paused ?? false;
  const liveMode = meeting && meeting.mode !== "light";
  const note = lag?.behind
    ? t("meetings.liveBehind")
    : liveMode && lag && lag.ms > 3000
      ? t("meetings.liveLag", { s: Math.round(lag.ms / 1000) })
      : !liveMode
        ? t("meetings.recordOnly")
        : "";

  return (
    <div className="flex flex-col gap-5 rounded-3xl border border-[color-mix(in_oklab,var(--cf-record)_25%,var(--cf-border))] bg-[color-mix(in_oklab,var(--cf-record)_4%,var(--cf-surface))] p-6">
      <div className="flex flex-wrap items-center gap-5">
        <div className="flex items-center gap-3.5">
          <span className={`h-3.5 w-3.5 shrink-0 rounded-full bg-[var(--cf-record)] ${paused ? "opacity-40" : "cf-rec-dot"}`} aria-hidden />
          <div>
            <p className="text-[12px] font-semibold uppercase tracking-wider" style={{ color: "var(--cf-record)" }}>
              {paused ? t("v2.nav.paused") : t("nav.recordingNow")}
            </p>
            <p className="text-[34px] font-bold leading-none tabular-nums tracking-tight">{clock(elapsed)}</p>
          </div>
        </div>
        <span className="flex-1" />
        <BigButton variant="secondary" size="lg" onClick={() => void useMeetingsStore.getState().pause(meetingId, !paused)}>
          {paused ? <Play size={16} /> : <Pause size={16} />}
          {paused ? t("meetings.resume") : t("meetings.pause")}
        </BigButton>
        <BigButton variant="danger" size="lg" onClick={() => void useMeetingsStore.getState().stop(meetingId)}>
          <Square size={13} fill="currentColor" />
          {t("v2.live.stop")}
        </BigButton>
        <button type="button" className="flex h-11 w-11 items-center justify-center rounded-xl text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]" aria-label={t("notes.moreActions")} onClick={(e) => setMenu({ x: e.clientX, y: e.clientY })}>
          <MoreHorizontal size={18} />
        </button>
      </div>

      <div className="grid gap-3" style={{ gridTemplateColumns: twoChannels ? "1fr 1fr" : "1fr" }}>
        <div className="flex flex-col gap-2 rounded-2xl bg-[var(--cf-surface)] p-3.5 shadow-[inset_0_0_0_1px_var(--cf-border)]">
          <span className="text-[12.5px] font-semibold text-[var(--cf-text-muted)]">{t("v2.new.mic")}</span>
          <div className="h-6">
            <Wave levels={levels.mic} max={24} count={70} />
          </div>
          <MicCheck compact />
        </div>
        {twoChannels && (
          <div className="flex flex-col gap-2 rounded-2xl bg-[var(--cf-surface)] p-3.5 shadow-[inset_0_0_0_1px_var(--cf-border)]">
            <span className="text-[12.5px] font-semibold text-[var(--cf-text-muted)]">{t("v2.new.system")}</span>
            <div className="h-6">
              <Wave levels={levels.system} max={24} count={70} />
            </div>
            <MicCheck compact channel="system" />
          </div>
        )}
      </div>
      {note && <p className="-mt-1 text-[12.5px] text-[var(--cf-text-muted)]">{note}</p>}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            // The other way of stopping: later when the setting transcribes at once, and at once when
            // the setting keeps recordings for later.
            {
              label: transcribeOnStop ? t("meetings.stopKeep") : t("meetings.stopTranscribe"),
              icon: Square,
              onClick: () => void useMeetingsStore.getState().stop(meetingId, !transcribeOnStop),
            },
            {
              label: t("meetings.discard"),
              icon: Trash2,
              danger: true,
              separated: true,
              onClick: () => {
                void useConfirmStore
                  .getState()
                  .ask({ message: t("meetings.discardConfirm"), confirmLabel: t("meetings.discard"), danger: true })
                  .then((ok) => ok && void useMeetingsStore.getState().discard(meetingId));
              },
            },
          ]}
        />
      )}
    </div>
  );
}
