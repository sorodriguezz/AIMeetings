import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ChevronDown, Download, FileText, Folder, FolderInput, Lock, MessageSquareText, MoreHorizontal, NotebookPen, RotateCcw, Sparkles, Trash2, Unlock, Users, VolumeX } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { ContextMenu, type MenuItem } from "../common/ContextMenu";
import { Spinner } from "../common/Spinner";
import { BigButton } from "../ui/Big";
import { Banner } from "../ui/Banner";
import { BackLink, Page } from "../ui/Page";
import { Tabs } from "../ui/Tabs";
import { MeetingPlayer, type PlayerHandle } from "./MeetingPlayer";
import { MeetingAi } from "./MeetingAi";
import { MeetingNotes } from "./MeetingNotes";
import { RecordBar } from "./RecordBar";
import { TranscriptPanel } from "./TranscriptPanel";
import { PeoplePanel } from "./PeoplePanel";
import { KIND_ICON } from "../home/HomeView";
import { transcriptFile, type TranscriptFormat } from "./transcriptExport";
import { writeFileBytes } from "../../lib/tauri/commands";
import { duration } from "./meetingLabels";
import {
  meetingsAppendNotes,
  meetingsCancelJob,
  meetingsDeleteAudio,
  meetingsExportAudio,
  meetingsMove,
  meetingsSetLocalOnly,
  meetingsSetTitle,
  type Meeting,
  type MeetingDetail,
} from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore, type MeetingTab } from "../../state/uiStore";
import { useLanguageStore, useT } from "../../state/languageStore";
import { useConfirmStore } from "../../state/confirmStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";

const STAGES: Record<string, string> = {
  stopping: "meetings.stage.stopping",
  decode: "meetings.stage.decode",
  detect: "meetings.stage.detect",
  transcribe: "meetings.stage.transcribe",
  voices: "meetings.stage.voices",
  finish: "meetings.stage.finish",
  compress: "meetings.stage.compress",
  store: "meetings.stage.store",
};

/** A recording opened from the list: everything about it on one page, one tab at a time. */
export function MeetingView({ meetingId }: { meetingId: string }) {
  const meeting = useMeetingsStore((s) => s.details[meetingId]?.meeting ?? s.meetings.find((m) => m.id === meetingId));
  const recording = useMeetingsStore((s) => s.status?.recording ?? null);
  useEffect(() => {
    void useMeetingsStore.getState().loadDetail(meetingId);
  }, [meetingId]);
  if (!meeting) return null;
  return <MeetingPage key={meeting.id} meeting={meeting} live={recording?.meetingId === meeting.id} />;
}

function MeetingPage({ meeting, live }: { meeting: Meeting; live: boolean }) {
  const t = useT();
  const language = useLanguageStore((s) => s.language);
  const detail: MeetingDetail | undefined = useMeetingsStore((s) => s.details[meeting.id]);
  const folders = useMeetingsStore((s) => s.folders);
  const tab = useUiStore((s) => s.meetingTab);
  const setTab = useUiStore((s) => s.setMeetingTab);
  const [liveTab, setLiveTab] = useState<"live" | "notes">("live");
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [position, setPosition] = useState(0);
  const [playing, setPlaying] = useState(false);
  const player = useRef<PlayerHandle>(null);
  const lines = detail?.lines ?? [];
  const speakers = detail?.speakers ?? [];
  const reload = () => void useMeetingsStore.getState().loadDetail(meeting.id);
  const busy = meeting.status === "queued" || meeting.status === "processing";

  // Landed here from a search hit: the transcript, at that line.
  const jump = useMeetingsStore((s) => (s.jumpTo?.meetingId === meeting.id ? s.jumpTo : null));
  useEffect(() => {
    if (!jump || !detail) return;
    setTab("transcript");
    if (meeting.audioFile) player.current?.seek(jump.atMs, false);
    const timer = window.setTimeout(() => useMeetingsStore.getState().clearJump(), 300);
    return () => window.clearTimeout(timer);
  }, [jump, detail, meeting.audioFile, setTab]);

  const title = meeting.title || t("meeting.untitled");
  const started = new Date(meeting.startedAt);
  const when = Number.isNaN(started.getTime()) ? "" : started.toLocaleString(language, { weekday: "long", day: "numeric", month: "long", hour: "2-digit", minute: "2-digit" });
  const folder = folders.find((f) => f.id === meeting.folderId);
  const speakerCount = new Set(lines.map((l) => l.speaker)).size;
  const KindIcon = KIND_ICON[meeting.kind] ?? FileText;

  const onInsert = async (markdown: string) => {
    try {
      const notes = await meetingsAppendNotes(meeting.id, markdown);
      useMeetingsStore.getState().patchMeeting({ id: meeting.id, notesMd: notes });
      return true;
    } catch (error) {
      pushErrorToast(String(error));
      return false;
    }
  };

  const safeName = title.replace(/[\\/:*?"<>|]/g, "-");
  const exportTranscript = async (format: TranscriptFormat) => {
    try {
      const path = await save({ defaultPath: `${safeName}.${format}`, filters: [{ name: t(`meetings.exportAs.${format}`), extensions: [format] }] });
      if (!path) return;
      const text = transcriptFile(format, { title, when, lines, speakers, kind: meeting.kind });
      await writeFileBytes(path, new TextEncoder().encode(text));
      useToastStore.getState().pushToast(t("meetings.exported"), "success");
    } catch (error) {
      pushErrorToast(String(error));
    }
  };

  const exportMenu = (x: number, y: number) =>
    setMenu({
      x,
      y,
      items: [
        ...(["md", "txt", "srt"] as TranscriptFormat[]).map((format) => ({
          label: t(`v2.export.${format}`),
          icon: FileText,
          disabled: lines.length === 0,
          onClick: () => void exportTranscript(format),
        })),
        {
          label: t("v2.export.audio"),
          icon: Download,
          separated: true,
          disabled: !meeting.audioFile,
          onClick: () =>
            void save({ defaultPath: `${safeName}.${meeting.audioFile.split(".").pop() ?? "m4a"}` })
              .then((path) => (path ? meetingsExportAudio(meeting.id, path).then(() => useToastStore.getState().pushToast(t("meetings.exported"), "success")) : undefined))
              .catch((e) => pushErrorToast(String(e))),
        },
      ],
    });

  const moreMenu = (x: number, y: number) =>
    setMenu({
      x,
      y,
      items: [
        {
          label: t("v2.meeting.retranscribe"),
          icon: RotateCcw,
          disabled: busy || !meeting.audioFile,
          onClick: () => void useMeetingsStore.getState().reprocess(meeting.id, true, meeting.speakersHint),
        },
        {
          label: t("folder.moveTo"),
          icon: FolderInput,
          onClick: () => {},
          children: [{ id: "", name: t("folder.none") }, ...folders].map((f) => ({
            label: f.name,
            disabled: f.id === meeting.folderId,
            onClick: () => void meetingsMove(meeting.id, f.id).then(reload).then(() => useMeetingsStore.getState().loadLibrary()).catch((e) => pushErrorToast(String(e))),
          })),
        },
        {
          label: meeting.localOnly ? t("v2.meeting.allowCloud") : t("v2.meeting.localOnly"),
          icon: meeting.localOnly ? Unlock : Lock,
          onClick: () => void meetingsSetLocalOnly(meeting.id, !meeting.localOnly).then(reload).catch((e) => pushErrorToast(String(e))),
        },
        // The audio only, now; the transcript, people and notes stay.
        {
          label: t("meetings.deleteAudio"),
          icon: VolumeX,
          danger: true,
          separated: true,
          disabled: busy || !meeting.audioFile || meeting.status === "recorded",
          onClick: () =>
            void useConfirmStore
              .getState()
              .ask({ message: t("meetings.deleteAudioConfirm"), confirmLabel: t("meetings.deleteAudio"), danger: true })
              .then((ok) => (ok ? meetingsDeleteAudio(meeting.id).then(reload) : undefined))
              .catch((e) => pushErrorToast(String(e))),
        },
        {
          label: t("v2.meeting.delete"),
          icon: Trash2,
          danger: true,
          onClick: () =>
            void useConfirmStore
              .getState()
              .ask({ message: t("meetings.deleteConfirm"), confirmLabel: t("meetings.delete"), danger: true })
              .then((ok) => ok && void useMeetingsStore.getState().remove(meeting.id)),
        },
      ],
    });

  const copyText = () => transcriptFile("txt", { title, when, lines, speakers, kind: meeting.kind });

  return (
    <Page width={920}>
      <div className="flex items-center gap-2">
        <BackLink label={folder ? folder.name : t("v2.nav.recordings")} onClick={() => useUiStore.getState().select(null)} />
        <span className="flex-1" />
        {!live && (
          <>
            <BigButton variant="secondary" onClick={(e) => exportMenu(e.clientX, e.clientY)}>
              <Download size={14} />
              {t("v2.meeting.export")}
              <ChevronDown size={13} className="opacity-60" />
            </BigButton>
            <button type="button" aria-label={t("notes.moreActions")} title={t("notes.moreActions")} onClick={(e) => moreMenu(e.clientX, e.clientY)} className="flex h-9 w-9 items-center justify-center rounded-xl text-[var(--cf-text-muted)] shadow-[inset_0_0_0_1px_var(--cf-border-strong)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]">
              <MoreHorizontal size={16} />
            </button>
          </>
        )}
      </div>

      <header className="-mt-2 flex flex-col gap-2">
        <TitleField meetingId={meeting.id} title={meeting.title} />
        <div className="flex flex-wrap items-center gap-2 text-[13px] text-[var(--cf-text-muted)]">
          <span className="flex items-center gap-1.5 rounded-full bg-[var(--cf-sunken)] px-2.5 py-1">
            <KindIcon size={12} />
            {meeting.kind === "import" ? t("v2.kind.import") : meeting.kind === "virtual" ? t("v2.kind.virtual") : t("v2.kind.room")}
          </span>
          {when && <span className="first-letter:uppercase">{when}</span>}
          {meeting.durationMs > 0 && <span>· {duration(meeting.durationMs)}</span>}
          {speakerCount > 0 && <span>· {speakerCount === 1 ? t("v2.people.oneShort") : t("v2.people.manyShort", { n: speakerCount })}</span>}
          {folder && (
            <span className="flex items-center gap-1 rounded-full bg-[var(--cf-sunken)] px-2.5 py-1">
              <Folder size={11} />
              {folder.name}
            </span>
          )}
          {detail?.localOnly && (
            <span className="flex items-center gap-1 rounded-full bg-[var(--cf-sunken)] px-2.5 py-1" title={t("meetings.localOnlyHint")}>
              <Lock size={11} />
              {t("meetings.localOnly")}
            </span>
          )}
        </div>
      </header>

      {live ? (
        <>
          <RecordBar meetingId={meeting.id} />
          <Tabs
            value={liveTab}
            onChange={setLiveTab}
            items={[
              { id: "live", label: t("v2.live.transcript"), icon: MessageSquareText },
              { id: "notes", label: t("meeting.tab.notes"), icon: NotebookPen },
            ]}
          />
          {liveTab === "live" ? (
            meeting.mode === "light" ? (
              <p className="py-8 text-center text-[13.5px] text-[var(--cf-text-muted)]">{t("meetings.recordOnly")}</p>
            ) : lines.length === 0 ? (
              <p className="py-8 text-center text-[13.5px] text-[var(--cf-text-muted)]">{t("meeting.waitingSpeech")}</p>
            ) : (
              <TranscriptPanel meeting={meeting} lines={lines} speakers={speakers} position={0} live />
            )
          ) : (
            <MeetingNotes meetingId={meeting.id} notes={meeting.notesMd} />
          )}
        </>
      ) : (
        <>
          <StatusCard meeting={meeting} />
          {meeting.audioFile && <MeetingPlayer ref={player} meeting={meeting} lines={lines} speakers={speakers} onPosition={setPosition} onPlaying={setPlaying} />}
          <Tabs<MeetingTab>
            value={tab}
            onChange={setTab}
            items={[
              { id: "transcript", label: t("meetings.tab.transcript"), icon: MessageSquareText },
              { id: "people", label: t("v2.tab.people"), icon: Users, count: speakerCount },
              { id: "ai", label: t("v2.tab.ai"), icon: Sparkles },
              { id: "notes", label: t("meeting.tab.notes"), icon: NotebookPen },
            ]}
          />
          {tab === "transcript" &&
            (lines.length === 0 ? (
              <p className="py-10 text-center text-[13.5px] text-[var(--cf-text-muted)]">{busy ? t("v2.meeting.wait") : t("meeting.noTranscript")}</p>
            ) : (
              <TranscriptPanel meeting={meeting} lines={lines} speakers={speakers} position={position} onSeek={meeting.audioFile ? (ms) => player.current?.seek(ms, true) : undefined} copyText={copyText} />
            ))}
          {tab === "people" &&
            (lines.length === 0 ? (
              <p className="py-10 text-center text-[13.5px] text-[var(--cf-text-muted)]">{busy ? t("v2.meeting.wait") : t("meeting.noTranscript")}</p>
            ) : (
              <PeoplePanel meeting={meeting} lines={lines} speakers={speakers} player={player} playing={playing} live={false} />
            ))}
          {tab === "ai" &&
            (detail && lines.length > 0 ? (
              <MeetingAi meeting={meeting} detail={detail} onInsert={onInsert} onSeek={(ms) => player.current?.seek(ms, true)} />
            ) : (
              <p className="py-10 text-center text-[13.5px] text-[var(--cf-text-muted)]">{busy ? t("v2.meeting.wait") : t("meeting.noTranscript")}</p>
            ))}
          {tab === "notes" && <MeetingNotes meetingId={meeting.id} notes={meeting.notesMd} />}
        </>
      )}
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </Page>
  );
}

/** Where the recording is, in a sentence, with the one button that moves it on. */
function StatusCard({ meeting }: { meeting: Meeting }) {
  const t = useT();
  const progress = useMeetingsStore((s) => s.progress[meeting.id]);
  const busy = meeting.status === "queued" || meeting.status === "processing";
  if (busy) {
    const stage = t((STAGES[progress?.stage ?? meeting.stage] ?? "meetings.stage.queued") as Parameters<typeof t>[0]);
    const pct = progress ? Math.round(progress.fraction * 100) : 0;
    return (
      <div className="flex flex-col gap-3 rounded-2xl border border-[var(--cf-accent-line)] bg-[color-mix(in_oklab,var(--cf-accent)_5%,var(--cf-surface))] p-4">
        <div className="flex items-center gap-3">
          <Spinner size={16} />
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-semibold">
              {stage}
              {progress ? ` · ${pct}%` : ""}
            </p>
            <p className="text-[12.5px] text-[var(--cf-text-muted)]">{t("v2.status.workingHint")}</p>
          </div>
          <BigButton variant="ghost" onClick={() => void meetingsCancelJob(meeting.id)}>
            {t("common.cancel")}
          </BigButton>
        </div>
        <div className="h-1.5 overflow-hidden rounded-full bg-[var(--cf-border)]">
          <div className="h-full rounded-full bg-[var(--cf-accent)] transition-[width] duration-300" style={{ width: `${pct}%` }} />
        </div>
      </div>
    );
  }
  if (meeting.status === "recorded") {
    return (
      <Banner tone="info" title={t("v2.status.recordedTitle")} action={<BigButton onClick={() => void useMeetingsStore.getState().transcribe(meeting.id)}>{t("meetings.transcribeNow")}</BigButton>}>
        {t("v2.status.recordedBody")}
      </Banner>
    );
  }
  if (meeting.status === "interrupted") {
    return (
      <Banner tone="warning" title={t("meetings.interrupted")} action={<BigButton onClick={() => void useMeetingsStore.getState().reprocess(meeting.id, true, meeting.speakersHint)}>{t("meetings.processRecorded")}</BigButton>}>
        {t("v2.status.interruptedBody")}
      </Banner>
    );
  }
  if (meeting.status === "failed") {
    const cancelled = meeting.error === "cancelled";
    return (
      <Banner
        tone={cancelled ? "info" : "danger"}
        icon={cancelled ? undefined : AlertTriangle}
        title={cancelled ? t("meetings.cancelled") : t("v2.status.failedTitle")}
        action={
          <BigButton variant="secondary" onClick={() => void useMeetingsStore.getState().reprocess(meeting.id, false, meeting.speakersHint)}>
            <RotateCcw size={14} />
            {cancelled ? t("v2.status.again") : t("meetings.retry")}
          </BigButton>
        }
      >
        {!cancelled && meeting.error && <span className="cf-selectable">{meeting.error}</span>}
      </Banner>
    );
  }
  return null;
}

/** The recording's name, edited in place. */
function TitleField({ meetingId, title }: { meetingId: string; title: string }) {
  const t = useT();
  const [value, setValue] = useState(title);
  useEffect(() => setValue(title), [title]);
  const commit = () => {
    const next = value.trim();
    if (next === title) return;
    useMeetingsStore.getState().patchMeeting({ id: meetingId, title: next });
    void meetingsSetTitle(meetingId, next).catch((e) => pushErrorToast(String(e)));
  };
  return (
    <input
      value={value}
      placeholder={t("meeting.untitled")}
      title={t("v2.meeting.renameHint")}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          setValue(title);
          e.currentTarget.blur();
        }
      }}
      className="-ml-2 min-w-0 rounded-xl border border-transparent bg-transparent px-2 py-1 text-[28px] font-bold tracking-tight outline-none transition-colors placeholder:text-[var(--cf-text-faint)] hover:border-[var(--cf-border)] focus:border-[var(--cf-accent)]"
    />
  );
}
