import { useEffect } from "react";
import { Download } from "lucide-react";
import { ProgressBar } from "../models/ModelsView";
import { meetingBusy, useUpdates } from "../../lib/updates";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useT } from "../../state/languageStore";

/** A small card at the bottom of the sidebar when a new version is out. */
export function UpdateNotice() {
  const t = useT();
  const { phase, update, downloaded, total, checkNow, install } = useUpdates();
  // Re-render when the recording or the queue changes: that is what blocks installing.
  useMeetingsStore((s) => s.status);

  useEffect(() => {
    const first = window.setTimeout(() => void checkNow(true), 4000);
    const every = window.setInterval(() => void checkNow(true), 6 * 3600 * 1000);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
    };
  }, [checkNow]);

  if (!update || !["available", "downloading", "installing"].includes(phase)) return null;
  const busy = meetingBusy();
  return (
    <div className="cf-fade-in rounded-2xl border border-[var(--cf-accent-line)] bg-[var(--cf-accent-soft)] p-3">
      <div className="flex items-center gap-2 text-[12.5px] font-semibold">
        <Download size={13} className="text-[var(--cf-accent)]" />
        {t("u.available", { v: update.version })}
      </div>
      {phase === "available" && (
        <>
          {busy && <p className="mt-1 text-[11.5px] leading-snug text-[var(--cf-text-muted)]">{t("u.busy")}</p>}
          <button
            type="button"
            disabled={busy}
            onClick={() => void install()}
            className="mt-2 h-8 w-full rounded-xl bg-[var(--cf-accent)] text-[12.5px] font-semibold text-[var(--cf-on-accent)] transition-[filter] hover:brightness-110 disabled:opacity-40"
          >
            {t("u.install")}
          </button>
        </>
      )}
      {phase === "downloading" && (
        <div className="mt-2">
          <ProgressBar done={downloaded} total={total} />
        </div>
      )}
      {phase === "installing" && <p className="mt-1 text-[11.5px] text-[var(--cf-text-muted)]">{t("u.installing")}</p>}
    </div>
  );
}
