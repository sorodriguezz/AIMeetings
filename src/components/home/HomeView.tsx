import { useEffect, useMemo, useState } from "react";
import { AudioLines, FileAudio, Folder, FolderInput, Lock, Monitor, MoreHorizontal, Search, Trash2, Upload, Users, X } from "lucide-react";
import { Spinner } from "../common/Spinner";
import { ContextMenu, type MenuItem } from "../common/ContextMenu";
import { Page, PageHeader } from "../ui/Page";
import { Banner } from "../ui/Banner";
import { EmptyState } from "../ui/EmptyState";
import { BigButton } from "../ui/Big";
import { duration } from "../meetings/meetingLabels";
import { meetingsList, meetingsMove, meetingsSearchLines, type LineHit, type Meeting } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useConfirmStore } from "../../state/confirmStore";
import { pushErrorToast } from "../../state/toastStore";
import { useLanguageStore, useT, type Translate } from "../../state/languageStore";

type Group = "today" | "yesterday" | "week" | "earlier";

function groupOf(iso: string, now: Date): Group {
  const at = new Date(iso).getTime();
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (at >= start) return "today";
  if (at >= start - 86_400_000) return "yesterday";
  if (at >= start - 6 * 86_400_000) return "week";
  return "earlier";
}

export const KIND_ICON = { virtual: Monitor, room: Users, import: FileAudio } as const;

/** Where the recording stands, said as a word when it is not simply ready. */
function StatusBadge({ meeting, t }: { meeting: Meeting; t: Translate }) {
  const map: Partial<Record<Meeting["status"], { label: string; color: string }>> = {
    recording: { label: t("library.status.recording"), color: "var(--cf-record)" },
    paused: { label: t("v2.nav.paused"), color: "var(--cf-record)" },
    processing: { label: t("library.status.processing"), color: "var(--cf-accent)" },
    queued: { label: t("library.status.queued"), color: "var(--cf-text-muted)" },
    failed: { label: t("library.status.failed"), color: "var(--cf-danger)" },
    recorded: { label: t("v2.home.notTranscribed"), color: "var(--cf-warning)" },
    interrupted: { label: t("library.status.interrupted"), color: "var(--cf-warning)" },
  };
  const badge = map[meeting.status];
  if (!badge || (meeting.status === "failed" && meeting.error === "cancelled")) return null;
  return (
    <span
      className="flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-1 text-[12px] font-medium"
      style={{ color: badge.color, background: `color-mix(in oklab, ${badge.color} 10%, transparent)` }}
    >
      {(meeting.status === "processing" || meeting.status === "queued") && <Spinner size={10} />}
      {(meeting.status === "recording" || meeting.status === "paused") && <span className="h-1.5 w-1.5 rounded-full bg-[var(--cf-record)]" />}
      {badge.label}
    </span>
  );
}

function Row({ meeting, hit, language, folderName, onMenu }: { meeting: Meeting; hit?: LineHit; language: string; folderName?: string; onMenu: (x: number, y: number) => void }) {
  const t = useT();
  const started = new Date(meeting.startedAt);
  const time = Number.isNaN(started.getTime()) ? "" : started.toLocaleString(language, { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const Icon = KIND_ICON[meeting.kind] ?? FileAudio;
  const preview = hit?.text ?? meeting.notesMd.replace(/[#*_>`[\]-]/g, "").trim().split("\n").find((l) => l.trim()) ?? "";
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => {
        if (hit) useMeetingsStore.getState().jumpToLine(meeting.id, hit.startMs);
        else useUiStore.getState().select(meeting.id);
      }}
      onKeyDown={(e) => e.key === "Enter" && useUiStore.getState().select(meeting.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(e.clientX, e.clientY);
      }}
      className="group flex items-center gap-4 rounded-2xl px-4 py-3.5 transition-colors hover:bg-[var(--cf-hover)]"
    >
      <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-[var(--cf-sunken)] text-[var(--cf-text-muted)] group-hover:bg-[var(--cf-accent-soft)] group-hover:text-[var(--cf-accent)]">
        <Icon size={19} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[15px] font-semibold">{meeting.title || t("meeting.untitled")}</span>
          {meeting.localOnly && <Lock size={12} className="shrink-0 text-[var(--cf-text-faint)]" />}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-2 text-[13px] text-[var(--cf-text-muted)]">
          <span>{time}</span>
          {meeting.durationMs > 0 && <span>· {duration(meeting.durationMs)}</span>}
          {folderName && (
            <span className="flex items-center gap-1">
              · <Folder size={11} /> {folderName}
            </span>
          )}
        </div>
        {preview && <p className="mt-1 line-clamp-1 text-[13px] text-[var(--cf-text-faint)]">{hit ? `“${preview}”` : preview}</p>}
      </div>
      <StatusBadge meeting={meeting} t={t} />
      <button
        type="button"
        aria-label={t("notes.moreActions")}
        onClick={(e) => {
          e.stopPropagation();
          onMenu(e.clientX, e.clientY);
        }}
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-[var(--cf-text-faint)] opacity-0 transition-opacity hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)] focus:opacity-100 group-hover:opacity-100"
      >
        <MoreHorizontal size={16} />
      </button>
    </div>
  );
}

/**
 * The recordings: the folder chosen in the sidebar (or all of them), newest first, grouped by day,
 * with one search over titles, notes and everything that was said.
 */
export function HomeView() {
  const t = useT();
  const language = useLanguageStore((s) => s.language);
  const all = useMeetingsStore((s) => s.meetings);
  const folders = useMeetingsStore((s) => s.folders);
  const loaded = useMeetingsStore((s) => s.libraryLoaded);
  const status = useMeetingsStore((s) => s.status);
  const folder = useUiStore((s) => s.folder);
  const query = useUiStore((s) => s.query);
  const [found, setFound] = useState<{ ids: Set<string>; hits: Map<string, LineHit> } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);

  // A search asks the backend: the transcripts are not in memory.
  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) {
      setFound(null);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void Promise.all([meetingsList(undefined, q), meetingsSearchLines(q)]).then(([meetings, lines]) => {
        if (cancelled) return;
        const hits = new Map<string, LineHit>();
        for (const line of lines) if (!hits.has(line.meetingId)) hits.set(line.meetingId, line);
        setFound({ ids: new Set([...meetings.map((m) => m.id), ...hits.keys()]), hits });
      });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query]);

  const shown = useMemo(() => {
    const inFolder = all.filter((m) => folder === null || m.folderId === folder);
    return found ? inFolder.filter((m) => found.ids.has(m.id)) : inFolder;
  }, [all, folder, found]);

  const groups = useMemo(() => {
    const now = new Date();
    const order: Group[] = ["today", "yesterday", "week", "earlier"];
    const map = new Map<Group, Meeting[]>();
    for (const meeting of shown) {
      const group = groupOf(meeting.startedAt, now);
      map.set(group, [...(map.get(group) ?? []), meeting]);
    }
    return order.filter((g) => map.has(g)).map((g) => ({ group: g, meetings: map.get(g)! }));
  }, [shown]);

  const folderRow = folders.find((f) => f.id === folder);
  const title = folder === null ? t("v2.nav.recordings") : folder === "" ? t("nav.unfiled") : (folderRow?.name ?? t("v2.nav.recordings"));

  const rowMenu = (meeting: Meeting, x: number, y: number) =>
    setMenu({
      x,
      y,
      items: [
        {
          label: t("folder.moveTo"),
          icon: FolderInput,
          onClick: () => {},
          children: [{ id: "", name: t("folder.none") }, ...folders].map((f) => ({
            label: f.name,
            disabled: f.id === meeting.folderId,
            onClick: () =>
              void meetingsMove(meeting.id, f.id)
                .then(() => useMeetingsStore.getState().loadLibrary())
                .catch((e) => pushErrorToast(String(e))),
          })),
        },
        {
          label: t("meetings.delete"),
          icon: Trash2,
          danger: true,
          separated: true,
          onClick: () =>
            void useConfirmStore
              .getState()
              .ask({ message: t("meetings.deleteConfirm"), confirmLabel: t("meetings.delete"), danger: true })
              .then((ok) => ok && void useMeetingsStore.getState().remove(meeting.id)),
        },
      ],
    });

  return (
    <Page width={900}>
      <PageHeader
        title={title}
        subtitle={loaded ? (shown.length === 1 ? t("v2.home.countOne") : t("v2.home.count", { n: shown.length })) : undefined}
      />

      {status && !status.canRecord && (
        <Banner
          tone="warning"
          title={t("v2.home.setupTitle")}
          action={
            <BigButton onClick={() => useUiStore.getState().openAi("speech")}>{t("v2.home.setupAction")}</BigButton>
          }
        >
          {t("v2.home.setupBody")}
        </Banner>
      )}

      {(all.length > 0 || query) && (
        <label className="flex h-12 items-center gap-3 rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface-2)] px-4 transition-colors focus-within:border-[var(--cf-accent)] focus-within:bg-[var(--cf-surface)]">
          <Search size={17} className="shrink-0 text-[var(--cf-text-faint)]" />
          <input
            value={query}
            onChange={(e) => useUiStore.getState().setQuery(e.target.value)}
            placeholder={t("library.search")}
            className="min-w-0 flex-1 bg-transparent text-[14.5px] outline-none placeholder:text-[var(--cf-text-faint)]"
          />
          {query && (
            <button type="button" aria-label={t("common.close")} onClick={() => useUiStore.getState().setQuery("")} className="rounded-md p-1 text-[var(--cf-text-faint)] hover:text-[var(--cf-text)]">
              <X size={15} />
            </button>
          )}
        </label>
      )}

      {!loaded ? (
        <div className="flex flex-col gap-2">
          {[0, 1, 2].map((i) => (
            <div key={i} className="cf-skeleton h-[72px] rounded-2xl" />
          ))}
        </div>
      ) : shown.length === 0 ? (
        query.trim() ? (
          <EmptyState icon={Search} title={t("library.noResults", { query: query.trim() })} body={t("v2.home.noResultsBody")} />
        ) : (
          <EmptyState icon={AudioLines} title={folder === null ? t("library.empty.title") : t("v2.home.emptyFolder")} body={t("v2.home.emptyBody")}>
            <BigButton size="lg" onClick={() => useUiStore.getState().setNewOpen(true)} disabled={!!status?.recording}>
              <span className="h-2.5 w-2.5 rounded-full bg-[var(--cf-record)] ring-2 ring-white/80" aria-hidden />
              {t("v2.nav.record")}
            </BigButton>
            <BigButton size="lg" variant="secondary" onClick={() => useUiStore.getState().openUpload()}>
              <Upload size={16} />
              {t("v2.nav.upload")}
            </BigButton>
          </EmptyState>
        )
      ) : (
        <div className="-mx-4 flex flex-col gap-5">
          {groups.map(({ group, meetings }) => (
            <section key={group} className="flex flex-col gap-0.5">
              <h3 className="px-4 pb-1 text-[12px] font-semibold uppercase tracking-wider text-[var(--cf-text-faint)]">{t(`library.group.${group}`)}</h3>
              {meetings.map((meeting) => (
                <Row
                  key={meeting.id}
                  meeting={meeting}
                  hit={found?.hits.get(meeting.id)}
                  language={language}
                  folderName={folder === null ? folders.find((f) => f.id === meeting.folderId)?.name : undefined}
                  onMenu={(x, y) => rowMenu(meeting, x, y)}
                />
              ))}
            </section>
          ))}
        </div>
      )}
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </Page>
  );
}
