import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, Copy, MousePointerClick, Pencil, Play, Search, UserPlus, UserRound, X } from "lucide-react";
import { ContextMenu, type MenuItem } from "../common/ContextMenu";
import { SpeakerAvatar, SpeakerDot } from "./SpeakerAvatar";
import { assignLines, assignLinesToNew } from "./speakerActions";
import { clock, speakerColor, speakerName } from "./meetingLabels";
import { meetingsUpdateLine, type Meeting, type MeetingLine, type MeetingSpeaker } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";
import { useT } from "../../state/languageStore";

/** Consecutive lines of one person: read as one paragraph, reassigned as one. */
interface Turn {
  speaker: string;
  lines: MeetingLine[];
}

export function turnsOf(lines: MeetingLine[]): Turn[] {
  const turns: Turn[] = [];
  for (const line of lines) {
    const last = turns[turns.length - 1];
    if (last && last.speaker === line.speaker) last.lines.push(line);
    else turns.push({ speaker: line.speaker, lines: [line] });
  }
  return turns;
}

/** `text` with every `query` in it marked. */
function Highlighted({ text, query }: { text: string; query: string }) {
  if (!query) return <>{text}</>;
  const lower = text.toLowerCase();
  const parts: React.ReactNode[] = [];
  let at = 0;
  for (;;) {
    const found = lower.indexOf(query, at);
    if (found < 0) break;
    parts.push(text.slice(at, found));
    parts.push(
      <mark key={found} className="rounded-sm bg-[color-mix(in_oklab,var(--cf-warning)_35%,transparent)] text-inherit">
        {text.slice(found, found + query.length)}
      </mark>,
    );
    at = found + query.length;
  }
  parts.push(text.slice(at));
  return <>{parts}</>;
}

/**
 * The transcript, as people talking: one paragraph per turn under the speaker's name. A click on a
 * sentence selects it and says, in words, what can be done with it — listen from there, correct
 * it, or give it to somebody else. A click on the name changes who spoke the whole turn.
 */
export function TranscriptPanel({
  meeting,
  lines,
  speakers,
  position,
  live = false,
  onSeek,
  copyText,
}: {
  meeting: Meeting;
  lines: MeetingLine[];
  speakers: MeetingSpeaker[];
  /** Where the player is, to mark the line being heard. */
  position: number;
  /** While recording: read-only, following the newest line. */
  live?: boolean;
  onSeek?: (ms: number) => void;
  copyText?: () => string;
}) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [only, setOnly] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const end = useRef<HTMLDivElement>(null);
  const q = query.trim().toLowerCase();

  // Landed here from a search hit: the line comes into view.
  const jump = useMeetingsStore((s) => (s.jumpTo?.meetingId === meeting.id ? s.jumpTo : null));
  useEffect(() => {
    if (!jump || lines.length === 0) return;
    const line = lines.find((l) => l.startMs === jump.atMs);
    if (line) setSelected(line.seq);
    requestAnimationFrame(() => listRef.current?.querySelector(`[data-start-ms="${jump.atMs}"]`)?.scrollIntoView({ block: "center" }));
  }, [jump, lines]);

  useEffect(() => {
    if (live) end.current?.scrollIntoView({ block: "nearest" });
  }, [live, lines.length]);

  const activeSeq = useMemo(() => {
    if (position <= 0) return null;
    let found: number | null = null;
    for (const line of lines) {
      if (line.startMs <= position) found = line.seq;
      else break;
    }
    return found;
  }, [lines, position]);

  const people = useMemo(() => {
    const keys = [...new Set(lines.map((l) => l.speaker))];
    return keys.map((key) => ({ key, name: speakerName(key, speakers, meeting.kind), color: speakerColor(key, speakers) }));
  }, [lines, speakers, meeting.kind]);

  const turns = useMemo(() => {
    const filtered = lines.filter((l) => (!only || l.speaker === only) && (!q || l.text.toLowerCase().includes(q)));
    return turnsOf(filtered);
  }, [lines, only, q]);

  /** «Who said this?» — every person, then a new one. */
  const pickSpeaker = (seqs: number[], current: string, x: number, y: number) => {
    const items: MenuItem[] = people
      .filter((p) => p.key !== current)
      .map((p) => ({ label: p.name, leading: <SpeakerDot color={p.color} />, onClick: () => void assignLines(meeting.id, seqs, p.key) }));
    for (const s of speakers) {
      if (s.key !== current && !people.some((p) => p.key === s.key)) {
        items.push({ label: speakerName(s.key, speakers, meeting.kind), leading: <SpeakerDot color={speakerColor(s.key, speakers)} />, onClick: () => void assignLines(meeting.id, seqs, s.key) });
      }
    }
    items.push({ label: t("v2.transcript.newPerson"), icon: UserPlus, separated: items.length > 0, onClick: () => void assignLinesToNew(meeting.id, seqs) });
    setMenu({ x, y, items });
  };

  const saveEdit = (line: MeetingLine, text: string) => {
    setEditing(null);
    const next = text.trim();
    if (!next || next === line.text) return;
    void meetingsUpdateLine(meeting.id, line.seq, next)
      .then(() => useMeetingsStore.getState().loadDetail(meeting.id))
      .catch((e) => pushErrorToast(String(e)));
  };

  const selectedLine = selected === null ? null : lines.find((l) => l.seq === selected) ?? null;

  return (
    <div className="flex flex-col gap-4">
      {!live && (
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <label className="flex h-10 min-w-0 flex-1 items-center gap-2.5 rounded-xl border border-[var(--cf-border)] bg-[var(--cf-surface)] px-3 focus-within:border-[var(--cf-accent)]">
              <Search size={15} className="shrink-0 text-[var(--cf-text-faint)]" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("v2.transcript.search")}
                className="min-w-0 flex-1 bg-transparent text-[13.5px] outline-none placeholder:text-[var(--cf-text-faint)]"
              />
              {query && (
                <button type="button" aria-label={t("common.close")} onClick={() => setQuery("")} className="text-[var(--cf-text-faint)] hover:text-[var(--cf-text)]">
                  <X size={14} />
                </button>
              )}
            </label>
            {copyText && (
              <button
                type="button"
                onClick={() =>
                  void navigator.clipboard.writeText(copyText()).then(() => useToastStore.getState().pushToast(t("meeting.copied"), "success"))
                }
                className="flex h-10 shrink-0 items-center gap-2 rounded-xl border border-[var(--cf-border)] px-3.5 text-[13px] font-medium text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]"
              >
                <Copy size={14} />
                {t("v2.transcript.copy")}
              </button>
            )}
          </div>
          {people.length > 1 && (
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="mr-1 text-[12.5px] text-[var(--cf-text-muted)]">{t("v2.transcript.show")}</span>
              <button
                type="button"
                onClick={() => setOnly(null)}
                className={`rounded-full px-3 py-1 text-[12.5px] font-medium transition-colors ${only === null ? "bg-[var(--cf-text)] text-[var(--cf-surface)]" : "bg-[var(--cf-sunken)] text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]"}`}
              >
                {t("v2.transcript.everyone")}
              </button>
              {people.map((p) => (
                <button
                  key={p.key}
                  type="button"
                  onClick={() => setOnly(only === p.key ? null : p.key)}
                  className={`flex items-center gap-1.5 rounded-full py-1 pl-2 pr-3 text-[12.5px] font-medium transition-colors ${only === p.key ? "bg-[var(--cf-text)] text-[var(--cf-surface)]" : "bg-[var(--cf-sunken)] text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]"}`}
                >
                  <span className="h-2.5 w-2.5 rounded-full" style={{ background: p.color }} />
                  {p.name}
                </button>
              ))}
            </div>
          )}
          <p className="flex items-center gap-2 text-[12.5px] text-[var(--cf-text-faint)]">
            <MousePointerClick size={13} />
            {t("v2.transcript.hint")}
          </p>
        </div>
      )}

      {turns.length === 0 ? (
        <p className="py-10 text-center text-[13.5px] text-[var(--cf-text-muted)]">{q ? t("v2.transcript.noMatch") : t("meeting.noTranscript")}</p>
      ) : (
        <div ref={listRef} className="flex flex-col">
          {turns.map((turn) => {
            const first = turn.lines[0];
            const color = speakerColor(turn.speaker, speakers);
            const inTurn = selectedLine && turn.lines.some((l) => l.seq === selectedLine.seq) ? selectedLine : null;
            return (
              <div key={first.seq} className="flex gap-3.5 border-b border-[var(--cf-border)] py-4 last:border-b-0">
                <SpeakerAvatar speakerKey={turn.speaker} speakers={speakers} kind={meeting.kind} size={34} />
                <div className="min-w-0 flex-1">
                  <div className="mb-1 flex items-center gap-2">
                    {live ? (
                      <span className="text-[14px] font-semibold" style={{ color }}>
                        {speakerName(turn.speaker, speakers, meeting.kind)}
                      </span>
                    ) : (
                      <button
                        type="button"
                        title={t("v2.transcript.whoTurn")}
                        onClick={(e) =>
                          pickSpeaker(
                            turn.lines.map((l) => l.seq),
                            turn.speaker,
                            e.clientX,
                            e.clientY,
                          )
                        }
                        className="-ml-1.5 flex items-center gap-1 rounded-lg px-1.5 py-0.5 text-[14px] font-semibold transition-colors hover:bg-[var(--cf-hover)]"
                        style={{ color }}
                      >
                        {speakerName(turn.speaker, speakers, meeting.kind)}
                        <ChevronDown size={13} className="opacity-60" />
                      </button>
                    )}
                    <button
                      type="button"
                      disabled={!onSeek}
                      onClick={() => onSeek?.(first.startMs)}
                      className="flex items-center gap-1 rounded-md px-1 font-mono text-[12px] tabular-nums text-[var(--cf-text-faint)] hover:text-[var(--cf-accent)] disabled:hover:text-[var(--cf-text-faint)]"
                      title={onSeek ? t("v2.transcript.listen") : undefined}
                    >
                      {clock(first.startMs)}
                    </button>
                  </div>
                  <p className="text-[14.5px] leading-[1.7]">
                    {turn.lines.map((line) => (
                      <Fragment key={line.seq}>
                        {editing === line.seq ? (
                          <EditLine line={line} onDone={(text) => (text === null ? setEditing(null) : saveEdit(line, text))} />
                        ) : (
                          <span
                            data-start-ms={line.startMs}
                            onClick={live ? undefined : () => setSelected(selected === line.seq ? null : line.seq)}
                            onDoubleClick={live ? undefined : () => setEditing(line.seq)}
                            className={`rounded-md px-0.5 py-0.5 transition-colors [box-decoration-break:clone] ${live ? "" : "cursor-pointer"} ${
                              selected === line.seq
                                ? "bg-[color-mix(in_oklab,var(--cf-accent)_20%,transparent)] ring-1 ring-[var(--cf-accent-line)]"
                                : line.seq === activeSeq
                                  ? "bg-[var(--cf-accent-soft)]"
                                  : live
                                    ? ""
                                    : "hover:bg-[var(--cf-hover)]"
                            } ${line.pass === "live" && live ? "text-[var(--cf-text-muted)]" : ""}`}
                          >
                            <Highlighted text={line.text} query={q} />
                          </span>
                        )}{" "}
                      </Fragment>
                    ))}
                  </p>
                  {inTurn && editing === null && (
                    <div className="cf-fade-in mt-2 flex flex-wrap items-center gap-1.5 rounded-xl bg-[var(--cf-surface-2)] p-1.5">
                      {onSeek && (
                        <LineAction icon={Play} label={t("v2.transcript.listen")} onClick={() => onSeek(inTurn.startMs)} />
                      )}
                      <LineAction icon={Pencil} label={t("v2.transcript.correct")} onClick={() => setEditing(inTurn.seq)} />
                      <LineAction icon={UserRound} label={t("v2.transcript.otherPerson")} onClick={(x, y) => pickSpeaker([inTurn.seq], inTurn.speaker, x, y)} />
                      <span className="flex-1" />
                      <button type="button" aria-label={t("common.close")} onClick={() => setSelected(null)} className="flex h-7 w-7 items-center justify-center rounded-lg text-[var(--cf-text-faint)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]">
                        <X size={14} />
                      </button>
                    </div>
                  )}
                </div>
              </div>
            );
          })}
          <div ref={end} />
        </div>
      )}
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

function LineAction({ icon: Icon, label, onClick }: { icon: typeof Play; label: string; onClick: (x: number, y: number) => void }) {
  return (
    <button
      type="button"
      onClick={(e) => onClick(e.clientX, e.clientY)}
      className="flex h-8 items-center gap-1.5 rounded-lg bg-[var(--cf-surface)] px-3 text-[13px] font-medium shadow-[inset_0_0_0_1px_var(--cf-border)] transition-colors hover:bg-[var(--cf-hover)]"
    >
      <Icon size={13} className="text-[var(--cf-accent)]" />
      {label}
    </button>
  );
}

/** A sentence being corrected: Enter keeps it, Escape leaves it as it was. */
function EditLine({ line, onDone }: { line: MeetingLine; onDone: (text: string | null) => void }) {
  const [draft, setDraft] = useState(line.text);
  const done = useRef(false);
  const finish = (text: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(text);
  };
  return (
    <textarea
      autoFocus
      value={draft}
      rows={Math.max(2, Math.ceil(draft.length / 80))}
      onChange={(e) => setDraft(e.target.value)}
      onFocus={(e) => e.currentTarget.setSelectionRange(e.currentTarget.value.length, e.currentTarget.value.length)}
      onBlur={() => finish(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          finish(draft);
        }
        if (e.key === "Escape") finish(null);
      }}
      className="my-1 block w-full resize-none rounded-xl border-2 border-[var(--cf-accent)] bg-[var(--cf-field)] px-3 py-2 text-[14.5px] leading-[1.6] outline-none"
    />
  );
}
