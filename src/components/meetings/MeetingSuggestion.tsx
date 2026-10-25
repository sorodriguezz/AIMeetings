import { useEffect } from "react";
import { Mic, X } from "lucide-react";
import { buttonClass } from "../common/Button";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useLanguageStore, useT, translate } from "../../state/languageStore";

/** How long the card waits for an answer before it goes away by itself. */
const SHOWN_MS = 60_000;

/**
 * «¿Grabar esta reunión?» — the card the call detector raises when Teams, Zoom or a browser starts
 * using the microphone (`meetings::detect`). It only ever asks: «Grabar» starts recording a call
 * into the library, titled after the app; anything else lets it go.
 */
export function MeetingSuggestion() {
  const t = useT();
  const suggestion = useMeetingsStore((s) => s.suggestion);

  useEffect(() => {
    if (!suggestion) return;
    const timer = window.setTimeout(() => useMeetingsStore.getState().dismissSuggestion(), SHOWN_MS);
    return () => window.clearTimeout(timer);
  }, [suggestion]);

  if (!suggestion) return null;
  return (
    <div className="cf-fade-in fixed bottom-4 left-1/2 z-[60] flex w-[360px] -translate-x-1/2 flex-col gap-3 rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface-raised)] p-4 text-[13px] shadow-[var(--cf-shadow-modal)]" role="dialog" aria-label={t("meetings.suggest.title")}>
      <div className="flex items-start gap-3">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]">
          <Mic size={15} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="font-semibold">{t("meetings.suggest.title")}</p>
          <p className="mt-0.5 text-[12.5px] text-[var(--cf-text-muted)]">{t("meetings.suggest.body", { app: suggestion.name })}</p>
        </div>
        <button type="button" aria-label={t("common.close")} className="text-[var(--cf-text-faint)] hover:text-[var(--cf-text)]" onClick={() => useMeetingsStore.getState().dismissSuggestion()}>
          <X size={14} />
        </button>
      </div>
      <div className="flex justify-end gap-2">
        <button type="button" className={buttonClass({ variant: "ghost", size: "md" })} onClick={() => useMeetingsStore.getState().dismissSuggestion()}>
          {t("meetings.suggest.no")}
        </button>
        <button type="button" className={buttonClass({ variant: "primary", size: "md" })} onClick={() => void recordCall(suggestion.name)}>
          <span className="h-2 w-2 rounded-full bg-[var(--cf-record)]" aria-hidden />
          {t("meetings.suggest.yes")}
        </button>
      </div>
    </div>
  );
}

/** Records the call that was detected, titled after the app that is using the microphone. */
async function recordCall(app: string) {
  useMeetingsStore.getState().dismissSuggestion();
  const language = useLanguageStore.getState().language;
  const when = new Date().toLocaleString(language, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  await useMeetingsStore.getState().start({ kind: "virtual", title: translate("meetings.suggest.noteTitle", { app, when }), language: language === "en" ? "en" : "es" });
}
