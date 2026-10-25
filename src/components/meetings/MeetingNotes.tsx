import { useEffect, useRef, useState } from "react";
import { Check, Eye, Pencil } from "lucide-react";
import { Markdown } from "../common/Markdown";
import { meetingsSetNotes } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { pushErrorToast } from "../../state/toastStore";
import { useT } from "../../state/languageStore";

/** How long typing must pause before the notes are written. */
const SAVE_AFTER_MS = 600;

/**
 * The recording's own page of notes: what the user writes (also while it records), and where the
 * AI's summaries and study notes land. Saved as it is typed; «Ver» shows it formatted.
 */
export function MeetingNotes({ meetingId, notes }: { meetingId: string; notes: string }) {
  const t = useT();
  const [draft, setDraft] = useState(notes);
  const [preview, setPreview] = useState(notes.trim().length > 0);
  const [saved, setSaved] = useState(false);
  const timer = useRef<number | null>(null);
  const dirty = useRef(false);
  const draftRef = useRef(draft);
  draftRef.current = draft;

  // Text written elsewhere (an AI result) replaces the draft unless the user is mid-edit.
  useEffect(() => {
    if (!dirty.current) setDraft(notes);
  }, [notes]);

  // Leaving with a pending save writes it at once.
  useEffect(
    () => () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        if (dirty.current) void meetingsSetNotes(meetingId, draftRef.current).catch(() => {});
      }
    },
    [meetingId],
  );

  const change = (value: string) => {
    setDraft(value);
    dirty.current = true;
    setSaved(false);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      void meetingsSetNotes(meetingId, value)
        .then(() => {
          dirty.current = false;
          setSaved(true);
          useMeetingsStore.getState().patchMeeting({ id: meetingId, notesMd: value });
        })
        .catch((error) => pushErrorToast(String(error)));
    }, SAVE_AFTER_MS);
  };

  const mode = (on: boolean, icon: typeof Eye, label: string, onClick: () => void) => {
    const Icon = icon;
    return (
      <button
        type="button"
        aria-pressed={on}
        onClick={onClick}
        className={`flex h-8 items-center gap-1.5 rounded-lg px-3 text-[13px] font-medium transition-colors ${on ? "bg-[var(--cf-surface)] text-[var(--cf-text)] shadow-[var(--cf-shadow-lift)]" : "text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]"}`}
      >
        <Icon size={13} />
        {label}
      </button>
    );
  };

  return (
    <div className="flex min-h-[360px] flex-1 flex-col gap-3">
      <div className="flex items-center gap-3">
        <div className="flex rounded-xl bg-[var(--cf-sunken)] p-1">
          {mode(!preview, Pencil, t("v2.notes.write"), () => setPreview(false))}
          {mode(preview, Eye, t("v2.notes.read"), () => setPreview(true))}
        </div>
        <span className="flex items-center gap-1 text-[12.5px] text-[var(--cf-text-faint)]">
          {saved && <Check size={12} />}
          {saved ? t("meeting.notesSaved") : t("v2.notes.autosave")}
        </span>
      </div>
      {preview ? (
        <div className="min-h-[320px] flex-1 cursor-text rounded-2xl border border-[var(--cf-border)] px-6 py-5" onDoubleClick={() => setPreview(false)}>
          {draft.trim() ? <Markdown source={draft} /> : <p className="text-[14px] text-[var(--cf-text-faint)]">{t("v2.notes.empty")}</p>}
        </div>
      ) : (
        <textarea
          autoFocus
          value={draft}
          onChange={(e) => change(e.target.value)}
          placeholder={t("v2.notes.placeholder")}
          spellCheck
          className="min-h-[320px] flex-1 resize-none rounded-2xl border border-[var(--cf-field-border)] bg-[var(--cf-field)] px-6 py-5 text-[14.5px] leading-[1.7] outline-none focus:border-[var(--cf-accent)]"
        />
      )}
    </div>
  );
}
