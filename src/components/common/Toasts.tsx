import { useEffect } from "react";
import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import { useToastStore, type Toast } from "../../state/toastStore";
import { useT } from "../../state/languageStore";

const ICON = { error: AlertCircle, success: CheckCircle2, info: Info };
const COLOR = { error: "text-[var(--cf-danger)]", success: "text-[var(--cf-success)]", info: "text-[var(--cf-accent)]" };

function ToastRow({ toast }: { toast: Toast }) {
  const t = useT();
  const dismiss = useToastStore((s) => s.dismissToast);
  useEffect(() => {
    const timer = window.setTimeout(() => dismiss(toast.id), toast.type === "error" ? 8000 : 3500);
    return () => window.clearTimeout(timer);
  }, [toast, dismiss]);
  const Icon = ICON[toast.type];
  return (
    <div className="cf-fade-in pointer-events-auto flex max-w-[380px] items-start gap-2.5 rounded-xl border border-[var(--cf-border)] bg-[var(--cf-surface-raised)] px-3.5 py-2.5 text-[12.5px] shadow-[var(--cf-shadow)]">
      <Icon size={15} className={`mt-[1px] shrink-0 ${COLOR[toast.type]}`} />
      <span className="cf-selectable min-w-0 flex-1 break-words leading-[1.45]">{toast.message}</span>
      <button type="button" onClick={() => dismiss(toast.id)} className="shrink-0 text-[var(--cf-text-faint)] hover:text-[var(--cf-text)]" aria-label={t("common.close")}>
        <X size={13} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useToastStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[80] flex flex-col items-end gap-2">
      {toasts.slice(-4).map((toast) => (
        <ToastRow key={toast.id} toast={toast} />
      ))}
    </div>
  );
}
