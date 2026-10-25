import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, AudioLines, Check, ChevronDown, Merge, Minus, Pause, Play, Plus, RefreshCw, UserCheck } from "lucide-react";
import { ContextMenu, type MenuItem } from "../common/ContextMenu";
import { BigButton } from "../ui/Big";
import { Banner } from "../ui/Banner";
import { Card } from "../ui/Page";
import { SpeakerAvatar, SpeakerDot } from "./SpeakerAvatar";
import { mergeSpeakers, rememberVoice, renameSpeaker } from "./speakerActions";
import { duration, speakerColor, speakerName } from "./meetingLabels";
import { meetingsVoices, type Meeting, type MeetingLine, type MeetingSpeaker, type Voice } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useConfirmStore } from "../../state/confirmStore";
import { useT } from "../../state/languageStore";
import { formatBytes } from "../../lib/format";
import type { PlayerHandle } from "./MeetingPlayer";

const MAX_SPEAKERS = 99;

/**
 * «Personas»: everyone the recording heard, one card each — their name typed straight in, a button
 * to hear them, and the fixes in words: this is me, remember this voice, these two are the same
 * person. Below, the way to separate the voices again when the count came out wrong.
 */
export function PeoplePanel({
  meeting,
  lines,
  speakers,
  player,
  playing,
  live,
}: {
  meeting: Meeting;
  lines: MeetingLine[];
  speakers: MeetingSpeaker[];
  player: React.RefObject<PlayerHandle | null>;
  /** The player is playing — a card's «listen» shows «stop» while its voice is heard. */
  playing: boolean;
  live: boolean;
}) {
  const t = useT();
  const status = useMeetingsStore((s) => s.status);
  const download = useMeetingsStore((s) => s.downloads.voices);
  const [voices, setVoices] = useState<Voice[]>([]);
  const [listening, setListening] = useState<string | null>(null);
  const [count, setCount] = useState<number>(0);

  useEffect(() => {
    void meetingsVoices().then(setVoices).catch(() => setVoices([]));
  }, [speakers]);
  useEffect(() => {
    if (!playing) setListening(null);
  }, [playing]);

  const people = useMemo(() => {
    const talk = new Map<string, { ms: number; lines: number; longest: MeetingLine | null }>();
    for (const line of lines) {
      const entry = talk.get(line.speaker) ?? { ms: 0, lines: 0, longest: null };
      entry.ms += Math.max(0, line.endMs - line.startMs);
      entry.lines += 1;
      if (!entry.longest || line.endMs - line.startMs > entry.longest.endMs - entry.longest.startMs) entry.longest = line;
      talk.set(line.speaker, entry);
    }
    const total = [...talk.values()].reduce((sum, e) => sum + e.ms, 0) || 1;
    return [...talk.entries()]
      .map(([key, entry]) => ({ key, speaker: speakers.find((s) => s.key === key), ...entry, share: entry.ms / total }))
      .sort((a, b) => b.ms - a.ms);
  }, [lines, speakers]);

  useEffect(() => {
    if (count === 0 && people.length > 0) setCount(meeting.speakersHint > 0 ? meeting.speakersHint : people.length);
  }, [people.length, meeting.speakersHint, count]);

  const busy = meeting.status === "queued" || meeting.status === "processing";
  const canSeparate = !!meeting.audioFile && !!status?.voicesInstalled && !busy && !live && meeting.status !== "recorded";
  const installing = !!download && (download.phase === "downloading" || download.phase === "unpacking");

  const listen = (key: string, line: MeetingLine | null) => {
    if (!line) return;
    if (listening === key) {
      player.current?.pause();
      setListening(null);
      return;
    }
    setListening(key);
    // A few seconds is enough to recognise somebody.
    player.current?.playRange(line.startMs, Math.min(line.endMs, line.startMs + 12_000));
  };

  return (
    <div className="flex flex-col gap-5">
      <div>
        <p className="text-[15px] font-semibold">
          {people.length === 0 ? t("v2.people.none") : people.length === 1 ? t("v2.people.foundOne") : t("v2.people.found", { n: people.length })}
        </p>
        <p className="mt-0.5 text-[13.5px] leading-[1.5] text-[var(--cf-text-muted)]">{t("v2.people.why")}</p>
      </div>

      {status && !status.voicesInstalled && status.voicesSupported && (
        <Banner
          tone="info"
          icon={AudioLines}
          title={t("v2.people.installTitle")}
          action={
            <BigButton disabled={installing} onClick={() => void useMeetingsStore.getState().install("voices")}>
              {installing && download ? `${Math.round((download.done / Math.max(1, download.total)) * 100)}%` : t("v2.people.install", { size: formatBytes(status.voicesBytes) })}
            </BigButton>
          }
        >
          {t("v2.people.installBody")}
        </Banner>
      )}

      <div className="flex flex-col gap-3">
        {people.map((person) => (
          <PersonCard
            key={person.key}
            meeting={meeting}
            speakerKey={person.key}
            speaker={person.speaker}
            speakers={speakers}
            others={people.filter((p) => p.key !== person.key).map((p) => p.key)}
            voices={voices}
            talkMs={person.ms}
            share={person.share}
            lineCount={person.lines}
            canListen={!!meeting.audioFile && !!person.longest && !live}
            listening={listening === person.key}
            onListen={() => listen(person.key, person.longest)}
            live={live}
          />
        ))}
      </div>

      {!live && people.length > 0 && (
        <Card tone="sunken">
          <div className="flex flex-wrap items-center gap-4">
            <div className="min-w-[240px] flex-1">
              <p className="text-[14px] font-semibold">{t("v2.people.wrongCount")}</p>
              <p className="mt-0.5 text-[12.5px] leading-[1.5] text-[var(--cf-text-muted)]">
                {!meeting.audioFile ? t("v2.people.noAudio") : meeting.status === "recorded" ? t("v2.people.transcribeFirst") : !status?.voicesInstalled ? t("v2.people.needsModel") : t("v2.people.wrongCountHint")}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[13px] text-[var(--cf-text-muted)]">{t("v2.people.there")}</span>
              <div className="flex items-center rounded-xl border border-[var(--cf-border)] bg-[var(--cf-surface)]">
                <button type="button" aria-label="−" disabled={!canSeparate || count <= 1} onClick={() => setCount((c) => Math.max(1, c - 1))} className="flex h-9 w-9 items-center justify-center text-[var(--cf-text-muted)] hover:text-[var(--cf-text)] disabled:opacity-40">
                  <Minus size={14} />
                </button>
                <span className="w-8 text-center text-[15px] font-semibold tabular-nums">{count}</span>
                <button type="button" aria-label="+" disabled={!canSeparate || count >= MAX_SPEAKERS} onClick={() => setCount((c) => Math.min(MAX_SPEAKERS, c + 1))} className="flex h-9 w-9 items-center justify-center text-[var(--cf-text-muted)] hover:text-[var(--cf-text)] disabled:opacity-40">
                  <Plus size={14} />
                </button>
              </div>
              <span className="text-[13px] text-[var(--cf-text-muted)]">{count === 1 ? t("v2.people.personWord") : t("v2.people.peopleWord")}</span>
            </div>
            <div className="flex items-center gap-2">
              <BigButton variant="ghost" disabled={!canSeparate} onClick={() => void useMeetingsStore.getState().reprocess(meeting.id, false, 0)}>
                {t("v2.people.dontKnow")}
              </BigButton>
              <BigButton variant="secondary" disabled={!canSeparate} onClick={() => void useMeetingsStore.getState().reprocess(meeting.id, false, count)}>
                <RefreshCw size={14} />
                {t("meetings.reseparate")}
              </BigButton>
            </div>
          </div>
        </Card>
      )}

      <button type="button" onClick={() => useUiStore.getState().setView("people")} className="flex items-center gap-1.5 self-start text-[13px] font-medium text-[var(--cf-accent)] hover:underline">
        {t("v2.people.seeSaved")}
        <ArrowRight size={13} />
      </button>
    </div>
  );
}

function PersonCard({
  meeting,
  speakerKey,
  speaker,
  speakers,
  others,
  voices,
  talkMs,
  share,
  lineCount,
  canListen,
  listening,
  onListen,
  live,
}: {
  meeting: Meeting;
  speakerKey: string;
  speaker: MeetingSpeaker | undefined;
  speakers: MeetingSpeaker[];
  others: string[];
  voices: Voice[];
  talkMs: number;
  share: number;
  lineCount: number;
  canListen: boolean;
  listening: boolean;
  onListen: () => void;
  live: boolean;
}) {
  const t = useT();
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const isMe = speakerKey === "me" || !!speaker?.isMe;
  const fallback = speakerName(speakerKey, speakers.map((s) => (s.key === speakerKey ? { ...s, name: "" } : s)), meeting.kind);
  const name = speaker?.name.trim() ?? "";
  const shown = name || fallback;
  const color = speakerColor(speakerKey, speakers);
  const inputRef = useRef<HTMLInputElement>(null);
  const savedVoice = speaker?.voiceId ? voices.find((v) => v.id === speaker.voiceId) : undefined;
  // `others` (the mic's other side) is everyone on the call at once: no single voice to keep.
  const canRemember = !!speaker?.hasVoice && speakerKey !== "others" && !live;

  const remember = (x: number, y: number) => {
    if (!name && !isMe) {
      inputRef.current?.focus();
      return;
    }
    const items: MenuItem[] = [
      { label: t("v2.people.saveAsNew", { name: shown }), icon: Plus, onClick: () => void rememberVoice(meeting.id, speakerKey, isMe ? t("meetings.speaker.me") : name, isMe) },
      ...voices
        .filter((v) => v.id !== speaker?.voiceId)
        .map((v, i) => ({ label: t("v2.people.isSaved", { name: v.name }), separated: i === 0, leading: <SpeakerDot color="var(--cf-text-faint)" />, onClick: () => void rememberVoice(meeting.id, speakerKey, v.name, v.isMe, v) })),
    ];
    if (items.length === 1) {
      items[0].onClick();
      return;
    }
    setMenu({ x, y, items });
  };

  const merge = (x: number, y: number) =>
    setMenu({
      x,
      y,
      items: others.map((key) => ({
        label: speakerName(key, speakers, meeting.kind),
        leading: <SpeakerDot color={speakerColor(key, speakers)} />,
        onClick: () => {
          const into = speakerName(key, speakers, meeting.kind);
          void useConfirmStore
            .getState()
            .ask({ message: t("v2.people.mergeConfirm", { from: shown, into }), confirmLabel: t("v2.people.mergeAction"), danger: false })
            .then((ok) => ok && void mergeSpeakers(meeting.id, speakerKey, key));
        },
      })),
    });

  return (
    <div className="flex flex-col gap-3.5 rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface)] p-4">
      <div className="flex items-center gap-4">
        <SpeakerAvatar speakerKey={speakerKey} speakers={speakers} kind={meeting.kind} size={46} />
        <div className="min-w-0 flex-1">
          <NameField key={`${speakerKey}:${name}`} inputRef={inputRef} initial={name} placeholder={isMe ? t("meetings.speaker.me") : speakerKey === "others" ? fallback : t("v2.people.namePlaceholder", { who: fallback })} disabled={speakerKey === "others"} onCommit={(next) => void renameSpeaker(meeting.id, speakerKey, next)} />
          <div className="mt-1.5 flex items-center gap-2.5 text-[12.5px] text-[var(--cf-text-muted)]">
            <span className="h-1.5 w-24 overflow-hidden rounded-full bg-[var(--cf-sunken)]">
              <span className="block h-full rounded-full" style={{ width: `${Math.max(4, Math.round(share * 100))}%`, background: color }} />
            </span>
            <span>{t("v2.people.talked", { time: duration(talkMs || speaker?.talkMs || 0), pct: Math.round(share * 100) })}</span>
            <span>· {lineCount === 1 ? t("v2.people.lineOne") : t("v2.people.lines", { n: lineCount })}</span>
          </div>
        </div>
        {canListen && (
          <BigButton variant={listening ? "primary" : "secondary"} onClick={onListen}>
            {listening ? <Pause size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
            {listening ? t("v2.people.stop") : t("v2.people.listen")}
          </BigButton>
        )}
      </div>

      {speakerKey === "others" ? (
        <p className="border-t border-[var(--cf-border)] pt-3 text-[12.5px] leading-[1.5] text-[var(--cf-text-muted)]">{t("v2.people.othersHint")}</p>
      ) : (isMe || canRemember || (others.length > 0 && !live)) && (
      <div className="flex flex-wrap items-center gap-2 border-t border-[var(--cf-border)] pt-3">
        {isMe && (
          <span className="flex h-8 items-center gap-1.5 rounded-lg bg-[var(--cf-accent-soft)] px-3 text-[12.5px] font-semibold text-[var(--cf-accent)]">
            <UserCheck size={13} />
            {speakerKey === "me" && meeting.kind === "virtual" ? t("v2.people.youMic") : t("v2.people.you")}
          </span>
        )}
        {savedVoice || speaker?.voiceId ? (
          <span className="flex h-8 items-center gap-1.5 rounded-lg bg-[color-mix(in_oklab,var(--cf-success)_12%,transparent)] px-3 text-[12.5px] font-semibold text-[var(--cf-success)]" title={t("meetings.knownVoice")}>
            <Check size={13} />
            {t("v2.people.voiceKnown")}
          </span>
        ) : (
          canRemember && (
            <PersonAction onClick={remember} title={!name && !isMe ? t("v2.people.nameFirst") : t("v2.people.rememberHint")} chevron={voices.length > 0}>
              <AudioLines size={13} />
              {t("v2.people.remember")}
            </PersonAction>
          )
        )}
        {!isMe && canRemember && !speaker?.voiceId && meeting.kind !== "virtual" && (
          <PersonAction onClick={() => void rememberVoice(meeting.id, speakerKey, t("meetings.speaker.me"), true)} title={t("v2.people.thisIsMeHint")}>
            <UserCheck size={13} />
            {t("meetings.thisIsMe")}
          </PersonAction>
        )}
        {others.length > 0 && !live && (
          <PersonAction onClick={merge} chevron title={t("v2.people.sameHint")}>
            <Merge size={13} />
            {t("v2.people.same")}
          </PersonAction>
        )}
        {!name && !isMe && speakerKey !== "others" && canRemember && <span className="text-[12px] text-[var(--cf-text-faint)]">{t("v2.people.nameFirst")}</span>}
      </div>
      )}
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

function PersonAction({ children, onClick, title, chevron }: { children: React.ReactNode; onClick: (x: number, y: number) => void; title?: string; chevron?: boolean }) {
  return (
    <button
      type="button"
      title={title}
      onClick={(e) => onClick(e.clientX, e.clientY)}
      className="flex h-8 items-center gap-1.5 rounded-lg px-3 text-[12.5px] font-medium text-[var(--cf-text)] shadow-[inset_0_0_0_1px_var(--cf-border-strong)] transition-colors hover:bg-[var(--cf-hover)]"
    >
      {children}
      {chevron && <ChevronDown size={12} className="opacity-60" />}
    </button>
  );
}

/** The name, edited where it is shown: saved on Enter or leaving the field. */
function NameField({
  initial,
  placeholder,
  disabled,
  onCommit,
  inputRef,
}: {
  initial: string;
  placeholder: string;
  disabled?: boolean;
  onCommit: (name: string) => void;
  inputRef: React.RefObject<HTMLInputElement | null>;
}) {
  const [value, setValue] = useState(initial);
  const commit = () => {
    const next = value.trim();
    if (next !== initial) onCommit(next);
  };
  return (
    <input
      ref={inputRef}
      value={value}
      disabled={disabled}
      placeholder={placeholder}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          setValue(initial);
          e.currentTarget.blur();
        }
      }}
      className="-ml-2 h-9 w-full max-w-[360px] rounded-lg border border-transparent bg-transparent px-2 text-[16px] font-semibold outline-none transition-colors placeholder:font-medium placeholder:text-[var(--cf-text-faint)] hover:border-[var(--cf-border)] focus:border-[var(--cf-accent)] focus:bg-[var(--cf-field)] disabled:hover:border-transparent"
    />
  );
}
