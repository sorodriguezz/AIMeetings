import { useEffect, useState } from "react";
import { Copy, FolderOpen, Info, Mic, Settings, SlidersHorizontal } from "lucide-react";
import { Button } from "../common/Button";
import { Block, Row } from "./Row";
import { GeneralSettings, RecordingSettings } from "./GeneralSettings";
import { Page, PageHeader } from "../ui/Page";
import { Tabs } from "../ui/Tabs";
import { TranscriptionSettings } from "./TranscriptionSettings";
import { appDiagnostics, revealPath, type Diagnostics } from "../../lib/tauri/commands";
import { useUiStore, type SettingsTab } from "../../state/uiStore";
import { useToastStore } from "../../state/toastStore";
import { meetingBusy, useUpdates } from "../../lib/updates";
import { Spinner } from "../common/Spinner";
import { useT } from "../../state/languageStore";

function About() {
  const t = useT();
  const [info, setInfo] = useState<Diagnostics | null>(null);
  useEffect(() => {
    void appDiagnostics().then(setInfo).catch(() => {});
  }, []);
  if (!info) return null;
  return (
    <Block title="AIMeetings" hint={t("settings.about.licences")}>
      <Row label={t("settings.about.version", { v: info.version })}>
        <span className="text-[12.5px] text-[var(--cf-text-muted)]">
          {info.os} · {info.arch}
        </span>
      </Row>
      <UpdatesRow />
      <Row label={t("settings.about.data")}>
        <code className="min-w-0 max-w-[360px] truncate font-mono text-[11.5px] text-[var(--cf-text-muted)]">{info.stateDir}</code>
        <Button size="md" variant="secondary" onClick={() => void revealPath(info.stateDir)}>
          <FolderOpen size={13} />
          {t("meetings.settings.openFolder")}
        </Button>
      </Row>
      <Row label={t("settings.about.copyDiagnostics")}>
        <Button
          size="md"
          variant="secondary"
          onClick={() =>
            void navigator.clipboard
              .writeText(`AIMeetings ${info.version} · ${info.os} ${info.arch}\n\n${info.logTail}`)
              .then(() => useToastStore.getState().pushToast(t("settings.about.copied"), "success"))
          }
        >
          <Copy size={13} />
          {t("settings.about.copyDiagnostics")}
        </Button>
      </Row>
    </Block>
  );
}

function UpdatesRow() {
  const t = useT();
  const up = useUpdates();
  return (
    <Row label={t("u.title")}>
      <Button size="md" variant="secondary" disabled={up.phase === "checking" || up.phase === "downloading" || up.phase === "installing"} onClick={() => void up.checkNow()}>
        {up.phase === "checking" && <Spinner size={11} />}
        {up.phase === "checking" ? t("u.checking") : t("u.check")}
      </Button>
      {up.phase === "none" && <span className="text-[12.5px] text-[var(--cf-success)]">{t("u.none")}</span>}
      {up.phase === "available" && up.update && (
        <>
          <span className="text-[12.5px]">{t("u.available", { v: up.update.version })}</span>
          <Button size="md" variant="primary" disabled={meetingBusy()} onClick={() => void up.install()}>
            {t("u.install")}
          </Button>
        </>
      )}
      {up.phase === "downloading" && <span className="text-[12.5px] text-[var(--cf-text-muted)]">{Math.round(up.total ? (up.downloaded / up.total) * 100 : 0)}%</span>}
      {up.phase === "installing" && <span className="text-[12.5px] text-[var(--cf-text-muted)]">{t("u.installing")}</span>}
      {up.phase === "error" && up.error && <span className="text-[12.5px] text-[var(--cf-danger)]">{t(up.error)}</span>}
    </Row>
  );
}

export function SettingsView() {
  const t = useT();
  const tab = useUiStore((s) => s.settingsTab);
  return (
    <Page width={860}>
      <PageHeader icon={Settings} title={t("settings.title")} />
      <Tabs<SettingsTab>
        value={tab}
        onChange={(id) => useUiStore.getState().openSettings(id)}
        items={[
          { id: "general", label: t("settings.tab.general"), icon: SlidersHorizontal },
          { id: "recording", label: t("v2.settings.recording"), icon: Mic },
          { id: "about", label: t("settings.tab.about"), icon: Info },
        ]}
      />
      {tab === "general" && <GeneralSettings />}
      {tab === "recording" && (
        <div className="flex flex-col gap-8">
          <RecordingSettings />
          <TranscriptionSettings />
        </div>
      )}
      {tab === "about" && <About />}
    </Page>
  );
}
