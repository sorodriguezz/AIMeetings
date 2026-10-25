import { useEffect, useState } from "react";
import { Check, ChevronDown, Copy, Monitor, Users } from "lucide-react";
import { Modal } from "../common/Dialogs";
import { Select } from "../common/Select";
import { BigButton } from "../ui/Big";
import { Banner } from "../ui/Banner";
import { MicCheck } from "./MicCheck";
import { meetingsMonitor, type MeetingMode } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useLanguageStore, useT } from "../../state/languageStore";
import { useToastStore } from "../../state/toastStore";

export const LANGUAGES = [
  { value: "es", label: "Español" },
  { value: "en", label: "English" },
  { value: "pt", label: "Português" },
  { value: "fr", label: "Français" },
  { value: "de", label: "Deutsch" },
  { value: "it", label: "Italiano" },
  { value: "auto", label: "Auto" },
];

/**
 * «Nueva grabación», one question at a time: what is being recorded (a call, or something in the
 * room), an optional name, and whether the microphone is heard — with the language, folder and
 * quality folded away until someone wants them.
 */
export function NewMeeting() {
  const t = useT();
  const appLanguage = useLanguageStore((s) => s.language);
  const settings = useMeetingsStore((s) => s.settings);
  const status = useMeetingsStore((s) => s.status);
  const folders = useMeetingsStore((s) => s.folders);
  const currentFolder = useUiStore((s) => s.folder);
  const [kind, setKind] = useState<"virtual" | "room">("virtual");
  const [mode, setMode] = useState<"" | MeetingMode>(settings.mode);
  const [language, setLanguage] = useState<string>(appLanguage === "en" ? "en" : "es");
  const [title, setTitle] = useState("");
  const [folderId, setFolderId] = useState<string>(currentFolder ?? "");
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const recommended = settings.bench?.recommended ?? "balanced";
  const effective: MeetingMode = mode || recommended;
  const close = () => useUiStore.getState().setNewOpen(false);
  const modeHint: Record<MeetingMode, string> = {
    light: t("meetings.mode.lightHint"),
    balanced: t("meetings.mode.balancedHint"),
    full: t("meetings.mode.fullHint"),
  };

  // The inputs are heard while this is open, so their levels can be checked before starting.
  useEffect(() => {
    void meetingsMonitor(true, kind === "virtual").catch(() => {});
  }, [kind]);
  useEffect(() => () => void meetingsMonitor(false).catch(() => {}), []);

  const start = async () => {
    setBusy(true);
    const ok = await useMeetingsStore.getState().start({
      folderId,
      title: title.trim() || undefined,
      kind,
      mode: mode || undefined,
      language: language === "auto" ? "" : language,
    });
    setBusy(false);
    if (ok) {
      close();
      if (!settings.consentSeen) void useMeetingsStore.getState().saveSetting("consent", "1");
    }
  };

  const canRecord = !!status?.canRecord;
  const options = [
    { value: "virtual" as const, icon: Monitor, label: t("v2.new.call"), hint: t("v2.new.callHint") },
    { value: "room" as const, icon: Users, label: t("v2.new.room"), hint: t("v2.new.roomHint") },
  ];
  return (
    <Modal onClose={close} width={580}>
      <div className="flex flex-col gap-6 p-7">
        <h2 className="text-[20px] font-bold tracking-tight">{t("new.title")}</h2>

        <section className="flex flex-col gap-2.5">
          <p className="text-[14px] font-semibold">{t("v2.new.what")}</p>
          <div className="grid grid-cols-2 gap-3">
            {options.map((option) => {
              const Icon = option.icon;
              const active = kind === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => setKind(option.value)}
                  aria-pressed={active}
                  className={`relative flex flex-col items-start gap-2 rounded-2xl border-2 p-4 text-left transition-colors ${
                    active ? "border-[var(--cf-accent)] bg-[var(--cf-accent-soft)]" : "border-[var(--cf-border)] hover:border-[var(--cf-border-strong)]"
                  }`}
                >
                  {active && (
                    <span className="absolute right-3 top-3 flex h-5 w-5 items-center justify-center rounded-full bg-[var(--cf-accent)] text-[var(--cf-on-accent)]">
                      <Check size={12} strokeWidth={3} />
                    </span>
                  )}
                  <span className={`flex h-10 w-10 items-center justify-center rounded-xl ${active ? "bg-[var(--cf-accent)] text-[var(--cf-on-accent)]" : "bg-[var(--cf-sunken)] text-[var(--cf-text-muted)]"}`}>
                    <Icon size={19} />
                  </span>
                  <span className="text-[14.5px] font-semibold">{option.label}</span>
                  <span className="text-[12.5px] leading-[1.45] text-[var(--cf-text-muted)]">{option.hint}</span>
                </button>
              );
            })}
          </div>
        </section>

        <label className="flex flex-col gap-1.5">
          <span className="text-[14px] font-semibold">
            {t("v2.new.name")} <span className="font-normal text-[var(--cf-text-faint)]">{t("v2.new.optional")}</span>
          </span>
          <input
            autoFocus
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder={t("v2.new.namePlaceholder")}
            onKeyDown={(e) => e.key === "Enter" && canRecord && void start()}
            className="h-11 rounded-xl border border-[var(--cf-field-border)] bg-[var(--cf-field)] px-3.5 text-[14px] outline-none focus:border-[var(--cf-accent)]"
          />
        </label>

        <section className="flex flex-col gap-3 rounded-2xl bg-[var(--cf-surface-2)] p-4">
          <p className="text-[14px] font-semibold">{t("v2.new.check")}</p>
          <div className="flex flex-col gap-1.5">
            <span className="text-[13px] text-[var(--cf-text-muted)]">{t("v2.new.mic")}</span>
            <MicCheck />
          </div>
          {kind === "virtual" && (
            <div className="flex flex-col gap-1.5">
              <span className="text-[13px] text-[var(--cf-text-muted)]">{t("v2.new.system")}</span>
              <MicCheck channel="system" />
            </div>
          )}
          <p className="text-[12px] leading-[1.45] text-[var(--cf-text-faint)]">{kind === "virtual" ? t("v2.new.checkHintCall") : t("v2.new.checkHintRoom")}</p>
        </section>

        <section className="flex flex-col gap-3">
          <button type="button" onClick={() => setMore((v) => !v)} className="flex items-center gap-1.5 self-start text-[13px] font-medium text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
            <ChevronDown size={14} className={`transition-transform ${more ? "" : "-rotate-90"}`} />
            {t("v2.new.more")}
            {!more && (
              <span className="font-normal text-[var(--cf-text-faint)]">
                · {LANGUAGES.find((l) => l.value === language)?.label} · {t(`meetings.mode.${effective}`)}
              </span>
            )}
          </button>
          {more && (
            <div className="cf-fade-in flex flex-col gap-3">
              <div className="grid grid-cols-3 gap-3">
                <label className="flex flex-col gap-1.5">
                  <span className="text-[12.5px] font-medium text-[var(--cf-text-muted)]">{t("v2.new.language")}</span>
                  <Select size="field" value={language} onChange={setLanguage} options={LANGUAGES} ariaLabel={t("new.language")} />
                </label>
                <label className="flex flex-col gap-1.5">
                  <span className="text-[12.5px] font-medium text-[var(--cf-text-muted)]">{t("new.folder")}</span>
                  <Select
                    size="field"
                    value={folderId}
                    onChange={setFolderId}
                    ariaLabel={t("new.folder")}
                    options={[{ value: "", label: t("folder.none") }, ...folders.map((f) => ({ value: f.id, label: f.name }))]}
                  />
                </label>
                <label className="flex flex-col gap-1.5">
                  <span className="text-[12.5px] font-medium text-[var(--cf-text-muted)]">{t("v2.new.quality")}</span>
                  <Select
                    size="field"
                    value={mode}
                    onChange={(next) => setMode(next as "" | MeetingMode)}
                    ariaLabel={t("meetings.mode.label")}
                    options={[
                      { value: "", label: t("meetings.mode.auto", { mode: t(`meetings.mode.${recommended}`) }) },
                      { value: "light", label: t("meetings.mode.light") },
                      { value: "balanced", label: t("meetings.mode.balanced") },
                      { value: "full", label: t("meetings.mode.full") },
                    ]}
                  />
                </label>
              </div>
              <p className="text-[12.5px] leading-[1.45] text-[var(--cf-text-muted)]">{modeHint[effective]}</p>
            </div>
          )}
        </section>

        {!canRecord && (
          <Banner
            tone="warning"
            title={t("new.setupFirst")}
            action={
              <BigButton
                variant="secondary"
                onClick={() => {
                  close();
                  useUiStore.getState().openAi("speech");
                }}
              >
                {t("new.openModels")}
              </BigButton>
            }
          >
            {t("new.setupFirstBody")}
          </Banner>
        )}

        {!settings.consentSeen && (
          <div className="flex items-center gap-2 rounded-xl bg-[var(--cf-surface-2)] px-3.5 py-2.5 text-[12.5px] text-[var(--cf-text-muted)]">
            <span className="min-w-0 flex-1">{t("meetings.consentHint")}</span>
            <button
              type="button"
              className="flex shrink-0 items-center gap-1 font-medium text-[var(--cf-accent)] hover:underline"
              onClick={() =>
                void navigator.clipboard
                  .writeText(t("meetings.consentMessage"))
                  .then(() => useToastStore.getState().pushToast(t("meetings.consentCopied"), "success"))
                  .catch(() => {})
              }
            >
              <Copy size={11} />
              {t("meetings.consentCopy")}
            </button>
          </div>
        )}

        <div className="flex items-center justify-end gap-2">
          <BigButton variant="ghost" size="lg" onClick={close}>
            {t("common.cancel")}
          </BigButton>
          <BigButton variant="record" size="lg" disabled={busy || !canRecord} onClick={() => void start()}>
            <span className="h-2.5 w-2.5 rounded-full bg-white" aria-hidden />
            {t("v2.new.start")}
          </BigButton>
        </div>
      </div>
    </Modal>
  );
}
