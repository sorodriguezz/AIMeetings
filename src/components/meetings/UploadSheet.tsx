import { useEffect, useState } from "react";
import { ArrowRight, FileAudio, Film, Upload } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Modal } from "../common/Dialogs";
import { Select } from "../common/Select";
import { BigButton } from "../ui/Big";
import { Banner } from "../ui/Banner";
import { LANGUAGES } from "./NewMeeting";
import { AUDIO_EXTENSIONS, fileName, isVideo, mediaExtensions, videoExtensions } from "../../lib/media";
import { useMeetingsStore } from "../../state/meetingsStore";
import { useUiStore } from "../../state/uiStore";
import { useConvertStore } from "../../state/convertStore";
import { useLanguageStore, useT } from "../../state/languageStore";

/** Opens the system's file picker on the audio and video the app reads. */
export async function pickMedia(label: string, multiple = false): Promise<string[]> {
  const picked = await open({ multiple, filters: [{ name: label, extensions: mediaExtensions() }] });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

/**
 * «Subir audio o video»: a file to transcribe — chosen here, or dropped anywhere on the window. A
 * video is welcome; only its sound is read and kept.
 */
export function UploadSheet({ initialPath }: { initialPath: string | null }) {
  const t = useT();
  const appLanguage = useLanguageStore((s) => s.language);
  const folders = useMeetingsStore((s) => s.folders);
  const canRecord = useMeetingsStore((s) => !!s.status?.canRecord);
  const currentFolder = useUiStore((s) => s.folder);
  const [path, setPath] = useState<string | null>(initialPath);
  const [language, setLanguage] = useState<string>(appLanguage === "en" ? "en" : "es");
  const [folderId, setFolderId] = useState<string>(currentFolder ?? "");
  const [busy, setBusy] = useState(false);
  const close = () => useUiStore.getState().closeUpload();

  // A second file dropped while this is open replaces the first.
  useEffect(() => {
    if (initialPath) setPath(initialPath);
  }, [initialPath]);

  const choose = async () => {
    const [picked] = await pickMedia(t("v2.upload.files"));
    if (picked) setPath(picked);
  };

  const submit = async () => {
    if (!path) return;
    setBusy(true);
    await useMeetingsStore.getState().importFile(folderId, path, language === "auto" ? "" : language);
    setBusy(false);
    close();
  };

  const video = path ? isVideo(path) : false;
  return (
    <Modal onClose={close} width={560}>
      <div className="flex flex-col gap-5 p-7">
        <div>
          <h2 className="text-[20px] font-bold tracking-tight">{t("v2.nav.upload")}</h2>
          <p className="mt-1 text-[13.5px] leading-[1.5] text-[var(--cf-text-muted)]">{t("v2.upload.subtitle")}</p>
        </div>

        {path ? (
          <div className="flex items-center gap-3.5 rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface-2)] p-4">
            <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]">
              {video ? <Film size={20} /> : <FileAudio size={20} />}
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-[14px] font-semibold" title={path}>
                {fileName(path)}
              </p>
              <p className="text-[12.5px] text-[var(--cf-text-muted)]">{video ? t("v2.upload.isVideo") : t("v2.upload.isAudio")}</p>
            </div>
            <BigButton variant="ghost" onClick={() => void choose()}>
              {t("v2.upload.change")}
            </BigButton>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => void choose()}
            className="flex flex-col items-center gap-2 rounded-2xl border-2 border-dashed border-[var(--cf-border-strong)] px-6 py-9 text-center transition-colors hover:border-[var(--cf-accent)] hover:bg-[var(--cf-accent-soft)]"
          >
            <Upload size={26} className="text-[var(--cf-accent)]" />
            <span className="text-[15px] font-semibold">{t("v2.upload.drop")}</span>
            <span className="text-[13px] text-[var(--cf-text-muted)]">{t("v2.upload.orChoose")}</span>
            <span className="mt-2 text-[12px] text-[var(--cf-text-faint)]">
              {t("v2.upload.formats", { audio: AUDIO_EXTENSIONS.slice(0, 5).join(", ").toUpperCase(), video: videoExtensions().join(", ").toUpperCase() })}
            </span>
          </button>
        )}

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-[12.5px] font-medium text-[var(--cf-text-muted)]">{t("v2.new.language")}</span>
            <Select size="field" value={language} onChange={setLanguage} options={LANGUAGES} ariaLabel={t("new.language")} />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-[12.5px] font-medium text-[var(--cf-text-muted)]">{t("new.folder")}</span>
            <Select
              size="field"
              value={folderId}
              onChange={setFolderId}
              ariaLabel={t("new.folder")}
              options={[{ value: "", label: t("folder.none") }, ...folders.map((f) => ({ value: f.id, label: f.name }))]}
            />
          </label>
        </div>

        {!canRecord && (
          <Banner
            tone="warning"
            title={t("new.setupFirst")}
            action={
              <BigButton
                variant="secondary"
                onClick={() => {
                  close();
                  useUiStore.getState().openAi("speech");
                }}
              >
                {t("new.openModels")}
              </BigButton>
            }
          >
            {t("new.setupFirstBody")}
          </Banner>
        )}

        <div className="flex items-center gap-2 pt-1">
          {video && path ? (
            <button
              type="button"
              onClick={() => {
                useConvertStore.getState().add([path]);
                close();
                useUiStore.getState().setView("convert");
              }}
              className="flex items-center gap-1 text-[12.5px] font-medium text-[var(--cf-accent)] hover:underline"
            >
              {t("v2.upload.onlyConvert")}
              <ArrowRight size={12} />
            </button>
          ) : (
            <span />
          )}
          <span className="flex-1" />
          <BigButton variant="ghost" onClick={close}>
            {t("common.cancel")}
          </BigButton>
          <BigButton disabled={!path || busy || !canRecord} onClick={() => void submit()}>
            {t("v2.upload.transcribe")}
          </BigButton>
        </div>
      </div>
    </Modal>
  );
}
