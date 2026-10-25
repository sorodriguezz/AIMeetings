import { useEffect, useState } from "react";
import {
  ArrowUp,
  BookOpen,
  BookmarkPlus,
  CalendarClock,
  Check,
  CheckSquare,
  ChevronDown,
  ClipboardList,
  Copy,
  GraduationCap,
  Lightbulb,
  ListChecks,
  Mail,
  NotebookPen,
  RotateCcw,
  Route,
  Settings2,
  ShieldAlert,
  Sparkles,
  Trash2,
  Wand2,
  X,
  type LucideIcon,
} from "lucide-react";
import { Spinner } from "../common/Spinner";
import { Markdown } from "../common/Markdown";
import { BigButton } from "../ui/Big";
import { ProviderPicker } from "./ProviderPicker";
import { BUILTIN_RECIPES, PRIMARY_RECIPES, factsOf, namesOf } from "./meetingLabels";
import {
  LOCAL_UNAVAILABLE,
  meetingsAi,
  meetingsDeleteRecipe,
  meetingsRecipes,
  meetingsSaveRecipe,
  type Meeting,
  type MeetingDetail,
  type MeetingRecipe,
} from "../../lib/tauri/meetingsCommands";
import { providersCancelRun } from "../../lib/tauri/providersCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useLanguageStore, useT } from "../../state/languageStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";

const RECIPE_ICON: Record<(typeof BUILTIN_RECIPES)[number], LucideIcon> = {
  summary: Sparkles,
  study: BookOpen,
  concepts: Lightbulb,
  quiz: GraduationCap,
  tasks: CheckSquare,
  minutes: ClipboardList,
  decisions: ListChecks,
  plan: Route,
  agenda: CalendarClock,
  risks: ShieldAlert,
  followup: Mail,
};

function newRunId(): string {
  return `meeting-ai-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function isCancellation(error: unknown): boolean {
  return String(error).includes("cancelled");
}

/**
 * «Resumen con IA»: pick what you want from the recording (a summary, study notes, review
 * questions…) or ask it anything. The answer is shown right here — and a chosen recipe is also
 * added to «Mis notas» at once, so nothing is lost when the user moves on. Which AI writes it is
 * said at the top, with the way to change it.
 *
 * **The run lives in the store**, by recording, so opening another one does not lose it.
 */
export function MeetingAi({
  meeting,
  detail,
  onInsert,
  onSeek,
}: {
  meeting: Meeting;
  detail: MeetingDetail;
  /** Appends Markdown to the recording's notes. */
  onInsert: (markdown: string) => Promise<boolean>;
  onSeek: (ms: number) => void;
}) {
  const t = useT();
  const language = useLanguageStore((s) => s.language);
  const run = useMeetingsStore((s) => s.ai[meeting.id]);
  const [recipes, setRecipes] = useState<MeetingRecipe[]>([]);
  const [question, setQuestion] = useState("");
  const [instruction, setInstruction] = useState("");
  const [showAll, setShowAll] = useState(false);
  const [own, setOwn] = useState(false);

  useEffect(() => {
    void meetingsRecipes().then(setRecipes).catch(() => setRecipes([]));
  }, []);

  const running = run?.status === "running";

  const ask = async (request: { recipe?: string; instruction?: string; question: boolean; label: string }) => {
    if (running) return;
    const runId = newRunId();
    const store = useMeetingsStore.getState();
    const base = { runId, label: request.label, question: request.question, request: { recipe: request.recipe, instruction: request.instruction } };
    store.setAi(meeting.id, { ...base, status: "running" });
    try {
      const facts = factsOf(meeting, meeting.title, detail.speakers, detail.lines, language);
      const answer = await meetingsAi({
        meetingId: meeting.id,
        recipe: request.recipe,
        instruction: request.instruction,
        question: request.question,
        facts,
        names: namesOf(detail.speakers, detail.lines, meeting.kind),
        runId,
      });
      const markdown = answer.markdown.trim();
      if (!markdown) {
        store.setAi(meeting.id, null);
        return;
      }
      const saved = !request.question && (await onInsert(markdown));
      store.setAi(meeting.id, { ...base, status: "ready", markdown, saved });
    } catch (error) {
      if (isCancellation(error)) {
        store.setAi(meeting.id, null);
        return;
      }
      const text = String(error);
      store.setAi(meeting.id, { ...base, status: "failed", error: text.includes(LOCAL_UNAVAILABLE) ? t("meetings.ai.localUnavailable") : text });
    }
  };

  const recipeLabel = (id: (typeof BUILTIN_RECIPES)[number]) => t(`meetings.recipe.${id}` as Parameters<typeof t>[0]);
  const hintOf = (id: (typeof BUILTIN_RECIPES)[number]) => ((PRIMARY_RECIPES as readonly string[]).includes(id) ? t(`recipe.hint.${id}` as Parameters<typeof t>[0]) : "");

  const saveInstruction = async () => {
    const text = instruction.trim();
    if (!text) return;
    const name = text.length > 32 ? `${text.slice(0, 30)}…` : text;
    try {
      const saved = await meetingsSaveRecipe(null, name, text);
      setRecipes((current) => [...current, saved]);
      useToastStore.getState().pushToast(t("meetings.ai.recipeSaved"), "success");
    } catch (error) {
      pushErrorToast(String(error));
    }
  };

  // `[12:34]` in an answer plays that moment: made into links the preview keeps.
  const linked = (markdown: string) =>
    markdown.replace(/\[(?:(\d{1,2}):)?(\d{1,2}):(\d{2})\](?!\()/g, (whole, h, m, s) => {
      const ms = ((Number(h ?? 0) * 60 + Number(m)) * 60 + Number(s)) * 1000;
      return `[${whole.slice(1, -1)}](#t=${ms})`;
    });

  const shownRecipes = showAll ? BUILTIN_RECIPES : PRIMARY_RECIPES;
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-3 rounded-2xl bg-[var(--cf-surface-2)] px-4 py-3">
        <Wand2 size={16} className="shrink-0 text-[var(--cf-accent)]" />
        <span className="text-[13.5px] text-[var(--cf-text-muted)]">{t("v2.ai.using")}</span>
        <ProviderPicker forcedLocal={detail.localOnly} />
        <span className="flex-1" />
        <button type="button" onClick={() => useUiStore.getState().openAi("summaries")} className="flex items-center gap-1.5 text-[12.5px] font-medium text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
          <Settings2 size={13} />
          {t("v2.ai.configure")}
        </button>
      </div>

      {run && <ResultCard meetingId={meeting.id} onInsert={onInsert} onSeek={onSeek} linked={linked} onRetry={() => run.request && void ask({ ...run.request, question: run.question, label: run.label })} />}

      <section className="flex flex-col gap-3">
        <h3 className="text-[16px] font-semibold">{t("ai.title")}</h3>
        <div className="grid grid-cols-2 gap-2.5 lg:grid-cols-3">
          {shownRecipes.map((id) => {
            const Icon = RECIPE_ICON[id];
            const hint = hintOf(id);
            return (
              <button
                key={id}
                type="button"
                disabled={running}
                onClick={() => void ask({ recipe: id, question: false, label: recipeLabel(id) })}
                className="group flex items-start gap-3 rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface)] p-3.5 text-left transition-all hover:-translate-y-[1px] hover:border-[var(--cf-accent-line)] hover:shadow-[var(--cf-shadow-lift)] disabled:opacity-50"
              >
                <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]">
                  <Icon size={17} />
                </span>
                <span className="min-w-0">
                  <span className="block text-[14px] font-semibold">{recipeLabel(id)}</span>
                  {hint && <span className="mt-0.5 block text-[12.5px] leading-[1.4] text-[var(--cf-text-muted)]">{hint}</span>}
                </span>
              </button>
            );
          })}
          {showAll &&
            recipes.map((recipe) => (
              <div key={recipe.id} className="group relative flex">
                <button
                  type="button"
                  disabled={running}
                  title={recipe.prompt}
                  onClick={() => void ask({ recipe: recipe.id, question: false, label: recipe.name })}
                  className="flex flex-1 items-start gap-3 rounded-2xl border border-dashed border-[var(--cf-border-strong)] p-3.5 text-left transition-colors hover:border-[var(--cf-accent-line)] disabled:opacity-50"
                >
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-[var(--cf-sunken)] text-[var(--cf-text-muted)]">
                    <BookmarkPlus size={16} />
                  </span>
                  <span className="min-w-0 pr-5 text-[14px] font-semibold">{recipe.name}</span>
                </button>
                <button
                  type="button"
                  aria-label={t("meetings.ai.deleteRecipe")}
                  className="absolute right-2 top-2 hidden rounded-md p-1 text-[var(--cf-text-faint)] hover:text-[var(--cf-danger)] group-hover:block"
                  onClick={() => void meetingsDeleteRecipe(recipe.id).then(() => setRecipes((c) => c.filter((r) => r.id !== recipe.id)))}
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
        </div>
        <button type="button" onClick={() => setShowAll((v) => !v)} className="flex items-center gap-1 self-start text-[13px] font-medium text-[var(--cf-accent)] hover:underline">
          <ChevronDown size={14} className={`transition-transform ${showAll ? "rotate-180" : ""}`} />
          {showAll ? t("v2.ai.fewer") : t("v2.ai.more", { n: BUILTIN_RECIPES.length - PRIMARY_RECIPES.length + recipes.length })}
        </button>
      </section>

      <section className="flex flex-col gap-2.5">
        <h3 className="text-[16px] font-semibold">{t("v2.ai.askTitle")}</h3>
        <form
          className="flex items-center gap-2 rounded-2xl border border-[var(--cf-field-border)] bg-[var(--cf-field)] p-1.5 pl-4 focus-within:border-[var(--cf-accent)]"
          onSubmit={(e) => {
            e.preventDefault();
            const text = question.trim();
            if (!text) return;
            void ask({ instruction: text, question: true, label: text });
            setQuestion("");
          }}
        >
          <input
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            disabled={running}
            placeholder={t("v2.ai.askPlaceholder")}
            className="h-9 min-w-0 flex-1 bg-transparent text-[14px] outline-none placeholder:text-[var(--cf-text-faint)]"
          />
          <button type="submit" aria-label={t("meetings.ai.ask")} disabled={running || !question.trim()} className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-[var(--cf-accent)] text-[var(--cf-on-accent)] disabled:opacity-40">
            <ArrowUp size={17} />
          </button>
        </form>
        <p className="text-[12.5px] text-[var(--cf-text-faint)]">{t("v2.ai.askHint")}</p>
      </section>

      <section className="flex flex-col gap-2.5">
        <button type="button" onClick={() => setOwn((v) => !v)} className="flex items-center gap-1.5 self-start text-[13.5px] font-medium text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]">
          <ChevronDown size={14} className={`transition-transform ${own ? "" : "-rotate-90"}`} />
          {t("v2.ai.ownTitle")}
        </button>
        {own && (
          <form
            className="cf-fade-in flex flex-col gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const text = instruction.trim();
              if (!text) return;
              void ask({ instruction: text, question: false, label: text.length > 40 ? `${text.slice(0, 38)}…` : text });
            }}
          >
            <textarea
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
              disabled={running}
              rows={3}
              placeholder={t("v2.ai.ownPlaceholder")}
              className="resize-none rounded-2xl border border-[var(--cf-field-border)] bg-[var(--cf-field)] px-4 py-3 text-[14px] leading-[1.5] outline-none focus:border-[var(--cf-accent)]"
            />
            <div className="flex items-center justify-end gap-2">
              <BigButton variant="ghost" disabled={!instruction.trim()} onClick={() => void saveInstruction()}>
                <BookmarkPlus size={14} />
                {t("v2.ai.saveRecipe")}
              </BigButton>
              <BigButton type="submit" disabled={running || !instruction.trim()}>
                {t("meetings.ai.write")}
              </BigButton>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}

/** What the AI is writing, or wrote: shown in full, with where it went and what else to do with it. */
function ResultCard({
  meetingId,
  onInsert,
  onSeek,
  linked,
  onRetry,
}: {
  meetingId: string;
  onInsert: (markdown: string) => Promise<boolean>;
  onSeek: (ms: number) => void;
  linked: (markdown: string) => string;
  onRetry: () => void;
}) {
  const t = useT();
  const run = useMeetingsStore((s) => s.ai[meetingId]);
  if (!run) return null;
  const close = () => useMeetingsStore.getState().setAi(meetingId, null);
  return (
    <div className="cf-fade-in flex flex-col gap-3 rounded-2xl border border-[var(--cf-accent-line)] bg-[var(--cf-surface)] p-5 shadow-[var(--cf-shadow-lift)]">
      <div className="flex items-center gap-2.5">
        {run.status === "running" ? <Spinner size={15} /> : <Sparkles size={16} className="text-[var(--cf-accent)]" />}
        <p className="min-w-0 flex-1 truncate text-[14.5px] font-semibold">
          {run.status === "running" ? t("meetings.ai.writing", { what: run.label }) : run.status === "failed" ? t("meetings.ai.failed", { what: run.label }) : run.label}
        </p>
        {run.status === "running" ? (
          <BigButton variant="ghost" onClick={() => void providersCancelRun(run.runId).catch(() => {})}>
            {t("common.cancel")}
          </BigButton>
        ) : (
          <button type="button" aria-label={t("common.close")} onClick={close} className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--cf-text-faint)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]">
            <X size={15} />
          </button>
        )}
      </div>
      {run.status === "running" && <p className="text-[13px] text-[var(--cf-text-muted)]">{t("v2.ai.runningHint")}</p>}
      {run.status === "failed" && (
        <div className="flex flex-col gap-3">
          <p className="cf-selectable rounded-xl bg-[color-mix(in_oklab,var(--cf-danger)_8%,transparent)] px-3 py-2 text-[12.5px] text-[var(--cf-danger)]">{run.error}</p>
          <div className="flex gap-2">
            <BigButton variant="secondary" onClick={onRetry}>
              <RotateCcw size={14} />
              {t("meetings.retry")}
            </BigButton>
            <BigButton variant="ghost" onClick={() => useUiStore.getState().openAi("summaries")}>
              <Settings2 size={14} />
              {t("v2.ai.checkSetup")}
            </BigButton>
          </div>
        </div>
      )}
      {run.markdown && (
        <>
          <div
            className="max-h-[55vh] overflow-y-auto rounded-xl bg-[var(--cf-surface-2)] px-4 py-3"
            onClick={(e) => {
              const anchor = (e.target as HTMLElement).closest("a");
              const href = anchor?.getAttribute("href") ?? "";
              if (href.startsWith("#t=")) {
                e.preventDefault();
                onSeek(Number(href.slice(3)));
              }
            }}
          >
            <Markdown source={linked(run.markdown)} />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {run.saved ? (
              <>
                <span className="flex items-center gap-1.5 text-[13px] font-medium text-[var(--cf-success)]">
                  <Check size={14} />
                  {t("v2.ai.saved")}
                </span>
                <BigButton variant="secondary" onClick={() => useUiStore.getState().setMeetingTab("notes")}>
                  <NotebookPen size={14} />
                  {t("v2.ai.openNotes")}
                </BigButton>
              </>
            ) : (
              <BigButton
                variant="secondary"
                onClick={() =>
                  void onInsert(run.markdown ?? "").then((ok) => {
                    if (ok) useMeetingsStore.getState().setAi(meetingId, { ...run, saved: true });
                  })
                }
              >
                <NotebookPen size={14} />
                {t("v2.ai.saveToNotes")}
              </BigButton>
            )}
            <BigButton
              variant="ghost"
              onClick={() =>
                void navigator.clipboard
                  .writeText(run.markdown ?? "")
                  .then(() => useToastStore.getState().pushToast(t("meeting.copied"), "success"))
                  .catch(() => {})
              }
            >
              <Copy size={14} />
              {t("v2.transcript.copy")}
            </BigButton>
          </div>
        </>
      )}
    </div>
  );
}
