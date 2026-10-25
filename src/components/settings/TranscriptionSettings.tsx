import { useCallback, useEffect, useState } from "react";
import { Gauge } from "lucide-react";
import { Select } from "../common/Select";
import { Segmented } from "../common/Segmented";
import { Button } from "../common/Button";
import { Block, Row, inputClass } from "./Row";
import { meetingsBench, meetingsSetCloudKey, meetingsTestCloud } from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useT } from "../../state/languageStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";

/** OpenAI-compatible transcription services, by the base URL their docs give. */
const PRESETS = [
  { id: "openai", url: "https://api.openai.com/v1", model: "whisper-1" },
  { id: "groq", url: "https://api.groq.com/openai/v1", model: "whisper-large-v3-turbo" },
] as const;

function VocabularyField() {
  const t = useT();
  const stored = useMeetingsStore((s) => s.settings.vocabulary);
  const [value, setValue] = useState(stored);
  useEffect(() => setValue(stored), [stored]);
  return (
    <input
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => value !== stored && void useMeetingsStore.getState().saveSetting("vocabulary", value.trim())}
      placeholder={t("meetings.settings.vocabularyPlaceholder")}
      className={`${inputClass} w-full max-w-[420px]`}
    />
  );
}

function CloudFields() {
  const t = useT();
  const settings = useMeetingsStore((s) => s.settings);
  const hasKey = useMeetingsStore((s) => s.status?.cloudKey ?? false);
  const [url, setUrl] = useState(settings.cloudUrl);
  const [model, setModel] = useState(settings.cloudModel);
  const [key, setKey] = useState("");
  const [testing, setTesting] = useState(false);
  useEffect(() => {
    setUrl(settings.cloudUrl);
    setModel(settings.cloudModel);
  }, [settings.cloudUrl, settings.cloudModel]);
  const preset = PRESETS.find((p) => p.url === (url || PRESETS[0].url))?.id ?? "custom";
  const persist = (next: { url: string; model: string }) => void useMeetingsStore.getState().saveSetting("cloud", JSON.stringify(next));
  return (
    <>
      <Row label={t("meetings.settings.service")}>
        <div className="w-[260px]">
          <Select
            size="field"
            value={preset}
            onChange={(id) => {
              const found = PRESETS.find((p) => p.id === id);
              const next = found ? { url: found.url, model: found.model } : { url: "", model };
              setUrl(next.url);
              setModel(next.model);
              persist(next);
            }}
            options={[
              { value: "openai", label: "OpenAI" },
              { value: "groq", label: "Groq" },
              { value: "custom", label: t("meetings.settings.custom") },
            ]}
          />
        </div>
      </Row>
      <Row label="URL">
        <input value={url} onChange={(e) => setUrl(e.target.value)} onBlur={() => persist({ url: url.trim(), model })} placeholder="https://api.openai.com/v1" className={`${inputClass} w-full max-w-[420px] font-mono text-[12px]`} />
      </Row>
      <Row label={t("meetings.settings.cloudModel")}>
        <input value={model} onChange={(e) => setModel(e.target.value)} onBlur={() => persist({ url, model: model.trim() })} placeholder="whisper-1" className={`${inputClass} w-[260px] font-mono text-[12px]`} />
      </Row>
      <Row label={t("meetings.settings.key")} hint={t("meetings.settings.cloudHint")}>
        <input type="password" value={key} onChange={(e) => setKey(e.target.value)} placeholder={hasKey ? t("meetings.settings.keySaved") : "sk-…"} className={`${inputClass} w-[260px] font-mono text-[12px]`} />
        <Button
          size="md"
          variant="secondary"
          disabled={!key.trim()}
          onClick={() =>
            void meetingsSetCloudKey(key)
              .then(() => {
                setKey("");
                return useMeetingsStore.getState().refreshStatus();
              })
              .catch((e) => pushErrorToast(String(e)))
          }
        >
          {t("common.save")}
        </Button>
        <Button
          size="md"
          variant="ghost"
          disabled={testing}
          onClick={() => {
            setTesting(true);
            void meetingsTestCloud()
              .then(() => useToastStore.getState().pushToast(t("meetings.settings.cloudOk"), "success"))
              .catch((e) => pushErrorToast(String(e)))
              .finally(() => setTesting(false));
          }}
        >
          {t("meetings.settings.test")}
        </Button>
      </Row>
    </>
  );
}

function Bench() {
  const t = useT();
  const bench = useMeetingsStore((s) => s.settings.bench);
  const [running, setRunning] = useState(false);
  const run = useCallback(async () => {
    setRunning(true);
    try {
      await meetingsBench();
      await useMeetingsStore.getState().loadSettings();
    } catch (error) {
      pushErrorToast(String(error));
    } finally {
      setRunning(false);
    }
  }, []);
  return (
    <>
      <Button size="md" variant="secondary" disabled={running} onClick={() => void run()}>
        <Gauge size={13} />
        {running ? t("meetings.settings.measuring") : t("meetings.settings.measure")}
      </Button>
      {bench && (
        <span className="text-[12px] text-[var(--cf-text-muted)]">
          {t("meetings.settings.perPhrase", { times: bench.perUtteranceMs.map(([id, ms]) => `${id} ${(ms / 1000).toFixed(1)} s`).join(" · ") })}
          {" → "}
          {t(`meetings.mode.${bench.recommended}`)}
        </span>
      )}
    </>
  );
}

/** Where and how meetings become text. */
export function TranscriptionSettings() {
  const t = useT();
  const status = useMeetingsStore((s) => s.status);
  const settings = useMeetingsStore((s) => s.settings);
  const save = useMeetingsStore((s) => s.saveSetting);
  if (!status) return null;
  if (!status.supported) return <p className="text-[13px] text-[var(--cf-text-muted)]">{t("meetings.settings.unsupported")}</p>;
  const installed = status.models.filter((m) => m.installed);
  const modelOptions = (auto: string, feature: "meetingsLive" | "meetingsFinal") => [
    { value: "", label: auto },
    ...installed.filter((m) => m.features.includes(feature)).map((m) => ({ value: m.id, label: m.label })),
  ];
  const cores = navigator.hardwareConcurrency || 8;
  return (
    <div className="flex flex-col gap-8">
      <Block title={t("meetings.settings.transcription")} hint={t("meetings.settings.transcriptionHint")}>
        <Row label={t("meetings.settings.service")}>
          <Segmented
            layoutId="aim-transcriber"
            value={settings.transcriber}
            onChange={(next) => void save("transcriber", next)}
            options={[
              { value: "local", label: t("meetings.settings.local") },
              { value: "cloud", label: t("meetings.settings.cloud") },
            ]}
          />
        </Row>
        {settings.transcriber === "local" ? (
          installed.length === 0 ? (
            <Row label={t("meetings.settings.liveModel")}>
              <span className="text-[12.5px] text-[var(--cf-text-muted)]">{t("meetings.settings.noModels")}</span>
              <Button size="md" variant="secondary" onClick={() => useUiStore.getState().openAi("speech")}>
                {t("meetings.openModels")}
              </Button>
            </Row>
          ) : (
            <>
              <Row label={t("meetings.settings.liveModel")}>
                <div className="w-[280px]">
                  <Select size="field" value={settings.liveModel} onChange={(v) => void save("liveModel", v)} options={modelOptions(t("meetings.settings.autoFast"), "meetingsLive")} />
                </div>
              </Row>
              <Row label={t("meetings.settings.finalModel")} hint={t("meetings.settings.moreModels")}>
                <div className="w-[280px]">
                  <Select size="field" value={settings.finalModel} onChange={(v) => void save("finalModel", v)} options={modelOptions(t("meetings.settings.autoBest"), "meetingsFinal")} />
                </div>
              </Row>
            </>
          )
        ) : (
          <CloudFields />
        )}
        <Row label={t("meetings.settings.vocabulary")} hint={t("meetings.settings.vocabularyHint")}>
          <VocabularyField />
        </Row>
      </Block>

      <Block title={t("meetings.settings.mode")} hint={t("meetings.settings.modeHint")}>
        <Row label={t("meetings.mode.label")} hint={t(`meetings.mode.${settings.mode || settings.bench?.recommended || "balanced"}Hint` as "meetings.mode.balancedHint")}>
          <div className="w-[280px]">
            <Select
              size="field"
              value={settings.mode}
              onChange={(v) => void save("mode", v)}
              options={[
                { value: "", label: t("meetings.mode.auto", { mode: t(`meetings.mode.${settings.bench?.recommended ?? "balanced"}`) }) },
                { value: "light", label: t("meetings.mode.light") },
                { value: "balanced", label: t("meetings.mode.balanced") },
                { value: "full", label: t("meetings.mode.full") },
              ]}
            />
          </div>
        </Row>
        <Row label={t("meetings.settings.measure")}>
          <Bench />
        </Row>
        <Row label={t("meetings.settings.afterStop")} hint={t("meetings.settings.afterStopHint")}>
          <div className="w-[280px]">
            <Select
              size="field"
              value={settings.transcribeOnStop ? "transcribe" : "keep"}
              onChange={(v) => void save("afterStop", v)}
              options={[
                { value: "transcribe", label: t("meetings.settings.afterStopTranscribe") },
                { value: "keep", label: t("meetings.settings.afterStopKeep") },
              ]}
            />
          </div>
        </Row>
        <Row label={t("meetings.settings.threads")} hint={t("meetings.settings.threadsHint")}>
          <div className="w-[280px]">
            <Select
              size="field"
              value={String(settings.threads)}
              onChange={(v) => void save("threads", v)}
              options={[
                { value: "0", label: t("meetings.settings.threadsAuto", { n: Math.max(1, Math.floor(cores / 2)) }) },
                ...Array.from({ length: Math.min(16, cores) }, (_, i) => ({ value: String(i + 1), label: String(i + 1) })),
              ]}
            />
          </div>
        </Row>
      </Block>
    </div>
  );
}
