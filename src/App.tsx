import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Upload } from "lucide-react";
import { Sidebar } from "./components/layout/Sidebar";
import { HomeView } from "./components/home/HomeView";
import { MeetingView } from "./components/meetings/MeetingView";
import { NewMeeting } from "./components/meetings/NewMeeting";
import { UploadSheet } from "./components/meetings/UploadSheet";
import { MeetingSuggestion } from "./components/meetings/MeetingSuggestion";
import { PeopleView } from "./components/people/PeopleView";
import { ConvertView } from "./components/convert/ConvertView";
import { ModelsView } from "./components/models/ModelsView";
import { SettingsView } from "./components/settings/SettingsView";
import { Onboarding } from "./components/onboarding/Onboarding";
import { Toasts } from "./components/common/Toasts";
import { ConfirmHost, PromptHost } from "./components/common/Dialogs";
import { ensureMeetingEvents, useMeetingsStore } from "./state/meetingsStore";
import { useUiStore } from "./state/uiStore";
import { useLanguageStore, useT } from "./state/languageStore";
import { useThemeStore } from "./state/themeStore";
import { getSetting } from "./lib/tauri/commands";
import { isMedia } from "./lib/media";
import { useConvertStore } from "./state/convertStore";

/**
 * A file dragged over the window: a recording to transcribe, from anywhere in the app. On the
 * converter's screen the files go to its list instead.
 */
function useFileDrop(): boolean {
  const [over, setOver] = useState(false);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") setOver(true);
        else if (payload.type === "leave") setOver(false);
        else if (payload.type === "drop") {
          setOver(false);
          const files = payload.paths.filter(isMedia);
          if (files.length === 0) return;
          if (useUiStore.getState().view === "convert") useConvertStore.getState().add(files);
          else useUiStore.getState().openUpload(files[0]);
        }
      })
      .then((f) => (unlisten = f))
      .catch(() => {});
    return () => unlisten?.();
  }, []);
  return over;
}

function DropOverlay() {
  const t = useT();
  const view = useUiStore((s) => s.view);
  return (
    <div className="pointer-events-none fixed inset-0 z-[80] flex items-center justify-center bg-[color-mix(in_oklab,var(--cf-accent)_10%,transparent)] p-8 backdrop-blur-[2px]">
      <div className="flex flex-col items-center gap-3 rounded-3xl border-2 border-dashed border-[var(--cf-accent)] bg-[var(--cf-surface)] px-14 py-10 shadow-[var(--cf-shadow-modal)]">
        <Upload size={30} className="text-[var(--cf-accent)]" />
        <p className="text-[17px] font-semibold">{view === "convert" ? t("v2.drop.convert") : t("v2.drop.title")}</p>
        <p className="text-[13px] text-[var(--cf-text-muted)]">{t("v2.drop.body")}</p>
      </div>
    </div>
  );
}

export function App() {
  const view = useUiStore((s) => s.view);
  const selected = useUiStore((s) => s.selected);
  const newOpen = useUiStore((s) => s.newOpen);
  const upload = useUiStore((s) => s.upload);
  const [ready, setReady] = useState(false);
  const [onboarding, setOnboarding] = useState(false);
  const dragging = useFileDrop();

  useEffect(() => {
    void Promise.all([useLanguageStore.getState().init(), useThemeStore.getState().init()]).then(async () => {
      ensureMeetingEvents();
      const done = await getSetting("onboarding_done").catch(() => "1");
      setOnboarding(done !== "1");
      setReady(true);
    });
    // The tray's "New meeting" / "Stop recording".
    const unlisten = listen<{ action: string }>("tray:action", (event) => {
      const recording = useMeetingsStore.getState().status?.recording;
      if (event.payload.action === "start" && !recording) useUiStore.getState().setNewOpen(true);
      if (event.payload.action === "stop" && recording) void useMeetingsStore.getState().stop(recording.meetingId);
    });
    return () => void unlisten.then((f) => f());
  }, []);

  if (!ready) return <div className="h-full bg-[var(--cf-bg)]" />;
  if (onboarding) return <Onboarding onDone={() => setOnboarding(false)} />;

  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="my-2 mr-2 flex min-w-0 flex-1 overflow-hidden rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface)] shadow-[var(--cf-shadow-lift)]">
        {view === "home" && <HomeView />}
        {view === "meeting" && selected && <MeetingView meetingId={selected} />}
        {view === "people" && <PeopleView />}
        {view === "convert" && <ConvertView />}
        {view === "ai" && <ModelsView />}
        {view === "settings" && <SettingsView />}
      </div>
      {newOpen && <NewMeeting />}
      {upload && <UploadSheet initialPath={upload.path} />}
      {dragging && <DropOverlay />}
      <MeetingSuggestion />
      <Toasts />
      <ConfirmHost />
      <PromptHost />
    </div>
  );
}
