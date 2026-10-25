import { useState } from "react";
import { AudioLines, Film, Folder, FolderPlus, Inbox, Lock, MoreHorizontal, Pencil, Settings, Sparkles, Trash2, Unlock, Upload, Users, type LucideIcon } from "lucide-react";
import { ContextMenu, type MenuItem } from "../common/ContextMenu";
import { Spinner } from "../common/Spinner";
import { useElapsed } from "../meetings/RecordBar";
import { UpdateNotice } from "./UpdateNotice";
import { clock } from "../meetings/meetingLabels";
import { foldersCreate, foldersDelete, foldersRename, foldersSetLocalOnly, type Folder as FolderRow } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useConfirmStore } from "../../state/confirmStore";
import { promptAction } from "../../state/promptStore";
import { pushErrorToast } from "../../state/toastStore";
import { useT } from "../../state/languageStore";
import { isMac } from "../../lib/platform";

function NavItem({
  icon: Icon,
  label,
  count,
  active,
  onClick,
  onMenu,
  trailing,
  dot,
}: {
  icon: LucideIcon;
  label: string;
  count?: number;
  active: boolean;
  onClick: () => void;
  onMenu?: (x: number, y: number) => void;
  trailing?: React.ReactNode;
  dot?: boolean;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => e.key === "Enter" && onClick()}
      onContextMenu={(e) => {
        if (!onMenu) return;
        e.preventDefault();
        onMenu(e.clientX, e.clientY);
      }}
      className={`group flex h-9 items-center gap-2.5 rounded-xl px-3 text-[13.5px] transition-colors ${
        active ? "bg-[var(--cf-surface)] font-semibold text-[var(--cf-text)] shadow-[var(--cf-shadow-lift)]" : "text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]"
      }`}
    >
      <Icon size={16} className={`shrink-0 ${active ? "text-[var(--cf-accent)]" : ""}`} />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {trailing}
      {dot && <span className="h-2 w-2 shrink-0 rounded-full bg-[var(--cf-warning)]" aria-hidden />}
      {count !== undefined && count > 0 && <span className={`text-[12px] tabular-nums text-[var(--cf-text-faint)] ${onMenu ? "group-hover:hidden" : ""}`}>{count}</span>}
      {onMenu && (
        <button
          type="button"
          aria-label={label}
          className="hidden rounded-md p-0.5 text-[var(--cf-text-faint)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)] group-hover:block"
          onClick={(e) => {
            e.stopPropagation();
            onMenu(e.clientX, e.clientY);
          }}
        >
          <MoreHorizontal size={14} />
        </button>
      )}
    </div>
  );
}

/** What is being recorded or transcribed right now — a click opens it. */
function ActivityPill() {
  const t = useT();
  const status = useMeetingsStore((s) => s.status);
  const progress = useMeetingsStore((s) => s.progress);
  const elapsed = useElapsed();
  const recording = status?.recording;
  const processing = status?.processing;
  if (recording) {
    return (
      <button
        type="button"
        onClick={() => useUiStore.getState().select(recording.meetingId)}
        className="flex items-center gap-2.5 rounded-xl border border-[color-mix(in_oklab,var(--cf-record)_30%,transparent)] bg-[color-mix(in_oklab,var(--cf-record)_8%,var(--cf-surface))] px-3 py-2.5 text-left text-[13px]"
      >
        <span className={`h-2.5 w-2.5 rounded-full bg-[var(--cf-record)] ${recording.paused ? "opacity-40" : "cf-rec-dot"}`} />
        <span className="flex-1 font-semibold">{recording.paused ? t("v2.nav.paused") : t("nav.recordingNow")}</span>
        <span className="font-mono tabular-nums text-[var(--cf-text-muted)]">{clock(elapsed)}</span>
      </button>
    );
  }
  if (processing) {
    const fraction = progress[processing]?.fraction ?? 0;
    return (
      <button type="button" onClick={() => useUiStore.getState().select(processing)} className="flex flex-col gap-2 rounded-xl border border-[var(--cf-border)] bg-[var(--cf-surface)] px-3 py-2.5 text-left text-[13px]">
        <span className="flex items-center gap-2">
          <Spinner size={12} />
          <span className="flex-1 text-[var(--cf-text-muted)]">{t("nav.processingNow", { pct: `${Math.round(fraction * 100)}%` })}</span>
        </span>
        <span className="h-1 overflow-hidden rounded-full bg-[var(--cf-border)]">
          <span className="block h-full rounded-full bg-[var(--cf-accent)] transition-[width] duration-300" style={{ width: `${Math.round(fraction * 100)}%` }} />
        </span>
      </button>
    );
  }
  return null;
}

/**
 * The app's map: the two ways to add a recording at the top, the recordings and their folders in
 * the middle, and the places that are visited less (people, the converter, AI, settings) below.
 */
export function Sidebar() {
  const t = useT();
  const view = useUiStore((s) => s.view);
  const folder = useUiStore((s) => s.folder);
  const meetings = useMeetingsStore((s) => s.meetings);
  const folders = useMeetingsStore((s) => s.folders);
  const recording = useMeetingsStore((s) => !!s.status?.recording);
  const needsSetup = useMeetingsStore((s) => !!s.status && !s.status.canRecord);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const reload = () => void useMeetingsStore.getState().loadLibrary();
  const inLibrary = view === "home" || view === "meeting";

  const newFolder = async () => {
    const name = await promptAction(t("folder.namePrompt"), { confirmLabel: t("folder.create") });
    if (!name) return;
    try {
      const id = await foldersCreate(name);
      await useMeetingsStore.getState().loadLibrary();
      useUiStore.getState().setFolder(id);
    } catch (error) {
      pushErrorToast(String(error));
    }
  };

  const folderMenu = (f: FolderRow, x: number, y: number) =>
    setMenu({
      x,
      y,
      items: [
        {
          label: t("folder.rename"),
          icon: Pencil,
          onClick: () =>
            void promptAction(t("folder.namePrompt"), { initial: f.name, confirmLabel: t("folder.rename") }).then((name) => {
              if (name) void foldersRename(f.id, name).then(reload).catch((e) => pushErrorToast(String(e)));
            }),
        },
        {
          label: f.localOnly ? t("folder.allowCloud") : t("folder.localOnly"),
          icon: f.localOnly ? Unlock : Lock,
          onClick: () => void foldersSetLocalOnly(f.id, !f.localOnly).then(reload).catch((e) => pushErrorToast(String(e))),
        },
        {
          label: t("folder.delete"),
          icon: Trash2,
          danger: true,
          separated: true,
          onClick: () =>
            void useConfirmStore
              .getState()
              .ask({ message: t("folder.deleteConfirm", { name: f.name }), confirmLabel: t("folder.delete"), danger: true })
              .then((ok) => {
                if (!ok) return;
                void foldersDelete(f.id)
                  .then(() => {
                    if (useUiStore.getState().folder === f.id) useUiStore.getState().setFolder(null);
                    reload();
                  })
                  .catch((e) => pushErrorToast(String(e)));
              }),
        },
      ],
    });

  const unfiled = meetings.filter((m) => !m.folderId).length;
  return (
    <aside className="flex h-full w-[236px] shrink-0 flex-col bg-[var(--cf-bg)]">
      {/* macOS draws its traffic lights over this strip; it drags the window. */}
      <div className={`cf-drag shrink-0 ${isMac() ? "h-[44px]" : "h-3"}`} data-tauri-drag-region />
      <div className="flex items-center gap-2 px-5 pb-4">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--cf-accent)] text-[var(--cf-on-accent)]">
          <AudioLines size={15} strokeWidth={2.4} />
        </span>
        <span className="text-[15px] font-bold tracking-tight">AIMeetings</span>
      </div>
      <div className="flex flex-col gap-2 px-3 pb-4">
        <button
          type="button"
          disabled={recording}
          onClick={() => useUiStore.getState().setNewOpen(true)}
          className="cf-press flex h-11 w-full items-center justify-center gap-2.5 rounded-xl bg-[var(--cf-accent)] text-[14px] font-semibold text-[var(--cf-on-accent)] shadow-[0_6px_16px_-6px_color-mix(in_oklab,var(--cf-accent)_70%,transparent)] transition-[background-color] hover:bg-[color-mix(in_oklab,var(--cf-accent)_88%,var(--cf-text))] disabled:opacity-50"
        >
          <span className="h-2.5 w-2.5 rounded-full bg-[var(--cf-record)] ring-2 ring-white/80" aria-hidden />
          {t("v2.nav.record")}
        </button>
        <button
          type="button"
          onClick={() => useUiStore.getState().openUpload()}
          className="cf-press flex h-10 w-full items-center justify-center gap-2 rounded-xl bg-[var(--cf-surface)] text-[13.5px] font-medium shadow-[inset_0_0_0_1px_var(--cf-border-strong)] transition-colors hover:bg-[color-mix(in_oklab,var(--cf-text)_4%,var(--cf-surface))]"
        >
          <Upload size={15} />
          {t("v2.nav.upload")}
        </button>
      </div>
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-3">
        <NavItem icon={AudioLines} label={t("v2.nav.recordings")} count={meetings.length} active={inLibrary && folder === null} onClick={() => useUiStore.getState().setFolder(null)} />
        {folders.length > 0 && unfiled > 0 && (
          <NavItem icon={Inbox} label={t("nav.unfiled")} count={unfiled} active={inLibrary && folder === ""} onClick={() => useUiStore.getState().setFolder("")} />
        )}
        <div className="mt-5 flex items-center justify-between pb-1 pl-3 pr-1.5">
          <span className="text-[11.5px] font-semibold uppercase tracking-wider text-[var(--cf-text-faint)]">{t("nav.folders")}</span>
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--cf-text-faint)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]"
            title={t("nav.newFolder")}
            aria-label={t("nav.newFolder")}
            onClick={() => void newFolder()}
          >
            <FolderPlus size={14} />
          </button>
        </div>
        {folders.map((f) => (
          <NavItem
            key={f.id}
            icon={Folder}
            label={f.name}
            count={f.count}
            active={inLibrary && folder === f.id}
            onClick={() => useUiStore.getState().setFolder(f.id)}
            onMenu={(x, y) => folderMenu(f, x, y)}
            trailing={f.localOnly ? <Lock size={11} className="text-[var(--cf-text-faint)]" /> : undefined}
          />
        ))}
        {folders.length === 0 && (
          <button type="button" onClick={() => void newFolder()} className="mx-3 mt-0.5 text-left text-[12.5px] leading-[1.45] text-[var(--cf-text-faint)] hover:text-[var(--cf-text-muted)]">
            {t("v2.nav.foldersHint")}
          </button>
        )}
      </nav>
      <div className="flex flex-col gap-2 px-3 pb-3 pt-2">
        <UpdateNotice />
        <ActivityPill />
        <div className="flex flex-col gap-0.5 border-t border-[var(--cf-border)] pt-2">
          <NavItem icon={Users} label={t("v2.nav.people")} active={view === "people"} onClick={() => useUiStore.getState().setView("people")} />
          <NavItem icon={Film} label={t("v2.nav.convert")} active={view === "convert"} onClick={() => useUiStore.getState().setView("convert")} />
          <NavItem icon={Sparkles} label={t("nav.models")} active={view === "ai"} dot={needsSetup} onClick={() => useUiStore.getState().setView("ai")} />
          <NavItem icon={Settings} label={t("nav.settings")} active={view === "settings"} onClick={() => useUiStore.getState().setView("settings")} />
        </div>
      </div>
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </aside>
  );
}
