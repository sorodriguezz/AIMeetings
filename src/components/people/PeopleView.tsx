import { useCallback, useEffect, useState } from "react";
import { AudioLines, Check, Trash2, UserCheck, Users } from "lucide-react";
import { Page, PageHeader } from "../ui/Page";
import { EmptyState } from "../ui/EmptyState";
import { Banner } from "../ui/Banner";
import { BigButton } from "../ui/Big";
import { meetingsDeleteVoice, meetingsRenameVoice, meetingsVoices, type Voice } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useConfirmStore } from "../../state/confirmStore";
import { pushErrorToast } from "../../state/toastStore";
import { useLanguageStore, useT } from "../../state/languageStore";

/**
 * «Personas»: the voices the app knows. Each one was saved from a recording; from then on, a new
 * recording where that voice speaks gets its name by itself. Renamed or forgotten here.
 */
export function PeopleView() {
  const t = useT();
  const language = useLanguageStore((s) => s.language);
  const status = useMeetingsStore((s) => s.status);
  const [voices, setVoices] = useState<Voice[] | null>(null);
  const reload = useCallback(() => void meetingsVoices().then(setVoices).catch(() => setVoices([])), []);
  useEffect(() => reload(), [reload]);

  return (
    <Page width={820}>
      <PageHeader icon={Users} title={t("v2.nav.people")} subtitle={t("v2.peoplePage.subtitle")} />

      {status && !status.voicesInstalled && status.voicesSupported && (
        <Banner tone="info" icon={AudioLines} title={t("v2.people.installTitle")} action={<BigButton onClick={() => useUiStore.getState().openAi("voices")}>{t("new.openModels")}</BigButton>}>
          {t("v2.people.installBody")}
        </Banner>
      )}

      <div className="grid grid-cols-3 gap-3">
        {[t("v2.peoplePage.step1"), t("v2.peoplePage.step2"), t("v2.peoplePage.step3")].map((text, i) => (
          <div key={text} className="flex gap-3 rounded-2xl bg-[var(--cf-surface-2)] p-4">
            <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[var(--cf-accent)] text-[13px] font-bold text-[var(--cf-on-accent)]">{i + 1}</span>
            <p className="text-[13px] leading-[1.5] text-[var(--cf-text-muted)]">{text}</p>
          </div>
        ))}
      </div>

      {voices === null ? (
        <div className="cf-skeleton h-20 rounded-2xl" />
      ) : voices.length === 0 ? (
        <EmptyState icon={Users} title={t("v2.peoplePage.emptyTitle")} body={t("v2.peoplePage.emptyBody")} />
      ) : (
        <section className="flex flex-col gap-2">
          <h2 className="text-[16px] font-semibold">{voices.length === 1 ? t("v2.peoplePage.countOne") : t("v2.peoplePage.count", { n: voices.length })}</h2>
          <div className="flex flex-col divide-y divide-[var(--cf-border)] rounded-2xl border border-[var(--cf-border)]">
            {voices.map((voice) => (
              <div key={voice.id} className="flex items-center gap-4 px-4 py-3">
                <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-[var(--cf-accent-soft)] text-[15px] font-semibold text-[var(--cf-accent)]">{voice.name.trim().charAt(0).toUpperCase() || "?"}</span>
                <div className="min-w-0 flex-1">
                  <input
                    defaultValue={voice.name}
                    aria-label={t("v2.peoplePage.rename")}
                    onBlur={(e) => {
                      const name = e.target.value.trim();
                      if (name && name !== voice.name) void meetingsRenameVoice(voice.id, name).then(reload).catch((err) => pushErrorToast(String(err)));
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                    className="-ml-2 h-8 w-full max-w-[320px] rounded-lg border border-transparent bg-transparent px-2 text-[15px] font-semibold outline-none hover:border-[var(--cf-border)] focus:border-[var(--cf-accent)] focus:bg-[var(--cf-field)]"
                  />
                  <p className="flex items-center gap-1.5 text-[12.5px] text-[var(--cf-text-muted)]">
                    <Check size={12} className="text-[var(--cf-success)]" />
                    {voice.samples === 1 ? t("v2.peoplePage.fromOne") : t("v2.peoplePage.fromMany", { n: voice.samples })}
                    <span>· {t("v2.peoplePage.since", { date: new Date(voice.createdAt).toLocaleDateString(language, { day: "numeric", month: "short", year: "numeric" }) })}</span>
                  </p>
                </div>
                {voice.isMe && (
                  <span className="flex items-center gap-1.5 rounded-lg bg-[var(--cf-accent-soft)] px-2.5 py-1 text-[12px] font-semibold text-[var(--cf-accent)]">
                    <UserCheck size={12} />
                    {t("v2.people.you")}
                  </span>
                )}
                <BigButton
                  variant="ghost"
                  onClick={() =>
                    void useConfirmStore
                      .getState()
                      .ask({ message: t("meetings.settings.forgetVoice", { name: voice.name }), confirmLabel: t("v2.peoplePage.forget"), danger: true })
                      .then((ok) => ok && void meetingsDeleteVoice(voice.id).then(reload))
                  }
                >
                  <Trash2 size={14} />
                  {t("v2.peoplePage.forget")}
                </BigButton>
              </div>
            ))}
          </div>
        </section>
      )}
    </Page>
  );
}
