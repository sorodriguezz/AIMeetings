import { useEffect, useState } from "react";
import { FolderOpen } from "lucide-react";
import { Select } from "../common/Select";
import { Segmented } from "../common/Segmented";
import { Button } from "../common/Button";
import { Checkbox } from "../common/Checkbox";
import { Block, Row } from "./Row";
import { dictationInputs, dictationMicPermission, dictationMicRequest, dictationMicSettings, type MicInput, type MicPermission } from "../../lib/tauri/dictationCommands";
import { meetingsRevealRecordings, meetingsSystemAudioSettings } from "../../lib/tauri/meetingsCommands";
import { getSetting, setSetting } from "../../lib/tauri/commands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useLanguageStore, useT } from "../../state/languageStore";
import { useThemeStore, type Theme } from "../../state/themeStore";
import { pushErrorToast } from "../../state/toastStore";
import { isMac } from "../../lib/platform";

const DEVICE_KEY = "dictation_device";

/** Language, appearance and the microphone. */
export function GeneralSettings() {
  const t = useT();
  const chosen = useLanguageStore((s) => s.chosen);
  const theme = useThemeStore((s) => s.theme);
  const [inputs, setInputs] = useState<MicInput[]>([]);
  const [device, setDevice] = useState("");
  const [permission, setPermission] = useState<MicPermission>("unknown");

  useEffect(() => {
    void dictationInputs().then(setInputs).catch(() => setInputs([]));
    void getSetting(DEVICE_KEY).then((v) => setDevice(v ?? ""));
    void dictationMicPermission().then(setPermission).catch(() => {});
  }, []);

  return (
    <div className="flex flex-col gap-8">
      <Block title={t("settings.tab.general")}>
        <Row label={t("settings.language")}>
          <div className="w-[220px]">
            <Select
              size="field"
              value={chosen}
              onChange={(v) => void useLanguageStore.getState().setLanguage(v as "" | "en" | "es")}
              options={[
                { value: "", label: t("settings.languageSystem") },
                { value: "es", label: "Español" },
                { value: "en", label: "English" },
              ]}
            />
          </div>
        </Row>
        <Row label={t("settings.theme")}>
          <Segmented
            layoutId="aim-theme"
            value={theme}
            onChange={(v) => void useThemeStore.getState().setTheme(v as Theme)}
            options={[
              { value: "system", label: t("settings.theme.system") },
              { value: "light", label: t("settings.theme.light") },
              { value: "dark", label: t("settings.theme.dark") },
            ]}
          />
        </Row>
      </Block>

      <Block title={t("settings.microphone")}>
        <Row label={t("settings.microphone")}>
          <div className="w-[320px]">
            <Select
              size="field"
              value={device}
              onChange={(v) => {
                setDevice(v);
                void setSetting(DEVICE_KEY, v).catch((e) => pushErrorToast(String(e)));
              }}
              options={[{ value: "", label: t("settings.microphoneDefault") }, ...inputs.map((i) => ({ value: i.id, label: i.name }))]}
            />
          </div>
        </Row>
        {permission !== "unknown" && (
          <Row label={t("settings.micPermission")}>
            <span className={`text-[12.5px] ${permission === "granted" ? "text-[var(--cf-success)]" : permission === "denied" ? "text-[var(--cf-danger)]" : "text-[var(--cf-text-muted)]"}`}>
              {permission === "granted" ? t("settings.micGranted") : permission === "denied" ? t("settings.micDenied") : "—"}
            </span>
            {permission === "undetermined" && (
              <Button size="md" variant="secondary" onClick={() => void dictationMicRequest().then(setPermission)}>
                {t("settings.micAsk")}
              </Button>
            )}
            {permission === "denied" && (
              <Button size="md" variant="secondary" onClick={() => void dictationMicSettings().catch(() => {})}>
                {t("settings.openPrivacy")}
              </Button>
            )}
          </Row>
        )}
        {isMac() && (
          <Row label={t("settings.systemAudio")} hint={t("settings.systemAudioHint")}>
            <Button size="md" variant="secondary" onClick={() => void meetingsSystemAudioSettings().catch(() => {})}>
              {t("settings.openPrivacy")}
            </Button>
          </Row>
        )}
      </Block>
    </div>
  );
}

/** What happens to recordings: how long the audio is kept, where, call detection and battery. */
export function RecordingSettings() {
  const t = useT();
  const status = useMeetingsStore((s) => s.status);
  const settings = useMeetingsStore((s) => s.settings);
  const save = useMeetingsStore((s) => s.saveSetting);
  const [saving, setSaving] = useState("0");
  useEffect(() => {
    void getSetting("power_saving").then((v) => setSaving(v === "2" ? "2" : "0"));
  }, []);
  return (
    <Block title={t("meetings.settings.audio")} hint={t("meetings.settings.audioHint")}>
        <Row label={t("meetings.settings.keep")}>
          <div className="w-[260px]">
            <Select
              size="field"
              value={String(settings.audioDays)}
              onChange={(v) => void save("audioDays", v)}
              options={[
                { value: "7", label: t("meetings.settings.days", { n: 7 }) },
                { value: "30", label: t("meetings.settings.days", { n: 30 }) },
                { value: "90", label: t("meetings.settings.days", { n: 90 }) },
                { value: "0", label: t("meetings.settings.forever") },
                { value: "-1", label: t("meetings.settings.deleteAfter") },
              ]}
            />
          </div>
        </Row>
        <Row label={t("meetings.settings.folder")}>
          <code title={status?.recordingsDir} className="min-w-0 max-w-[360px] truncate font-mono text-[11.5px] text-[var(--cf-text-muted)]">
            {status?.recordingsDir}
          </code>
          <Button size="md" variant="secondary" onClick={() => void meetingsRevealRecordings().catch((e) => pushErrorToast(String(e)))}>
            <FolderOpen size={13} />
            {t("meetings.settings.openFolder")}
          </Button>
        </Row>
        <Row label={t("meetings.suggest.title")}>
          <label className="flex cursor-pointer items-center gap-2 text-[12.5px]">
            <Checkbox checked={settings.detect} onChange={(on) => void save("detect", on ? "1" : "0")} />
            {t("meetings.settings.detect")}
          </label>
        </Row>
        <Row label={t("settings.battery")}>
          <div className="w-[300px]">
            <Select
              size="field"
              value={saving}
              onChange={(v) => {
                setSaving(v);
                void setSetting("power_saving", v);
              }}
              options={[
                { value: "0", label: t("settings.battery.off") },
                { value: "2", label: t("settings.battery.on") },
              ]}
            />
          </div>
        </Row>
    </Block>
  );
}
