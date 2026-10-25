import { useEffect, useRef, useState } from "react";
import { useConfirmStore } from "../../state/confirmStore";
import { usePromptStore } from "../../state/promptStore";
import { useT } from "../../state/languageStore";
import { Button } from "./Button";

/** A centred sheet over a dimmed window. Escape and a click outside both cancel. */
export function Modal({ onClose, children, width = 420 }: { onClose: () => void; children: React.ReactNode; width?: number }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/30 p-6 backdrop-blur-[2px]" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="cf-fade-in max-h-full w-full overflow-y-auto rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface-raised)] shadow-[var(--cf-shadow-modal)]" style={{ maxWidth: width }} role="dialog" aria-modal>
        {children}
      </div>
    </div>
  );
}

export function ConfirmHost() {
  const t = useT();
  const request = useConfirmStore((s) => s.request);
  const respond = useConfirmStore((s) => s.respond);
  const pick = useConfirmStore((s) => s.pick);
  const confirmRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (request) requestAnimationFrame(() => confirmRef.current?.focus());
  }, [request]);
  if (!request) return null;
  return (
    <Modal onClose={() => respond(false)}>
      <div className="p-5">
        <p className="cf-selectable whitespace-pre-line text-[13.5px] leading-[1.5]">{request.message}</p>
        {request.items && request.items.length > 0 && (
          <ul className="mt-2 list-disc pl-5 text-[12.5px] text-[var(--cf-text-muted)]">
            {request.items.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => respond(false)}>
            {t("common.cancel")}
          </Button>
          {request.choices ? (
            request.choices.map((choice) => (
              <Button key={choice.id} variant={choice.variant === "danger" ? "danger" : choice.variant === "primary" ? "primary" : "secondary"} onClick={() => pick(choice.id)}>
                {choice.label}
              </Button>
            ))
          ) : (
            <Button ref={confirmRef} variant={request.danger ? "danger" : "primary"} onClick={() => respond(true)}>
              {request.confirmLabel ?? t("common.confirm")}
            </Button>
          )}
        </div>
      </div>
    </Modal>
  );
}

export function PromptHost() {
  const t = useT();
  const request = usePromptStore((s) => s.request);
  const respond = usePromptStore((s) => s.respond);
  const [value, setValue] = useState("");
  useEffect(() => setValue(request?.initial ?? ""), [request]);
  if (!request) return null;
  const problem = request.validate?.(value) ?? null;
  const empty = !request.allowEmpty && !value.trim();
  const submit = () => {
    if (problem || empty) return;
    respond(value.trim());
  };
  return (
    <Modal onClose={() => respond(null)}>
      <form
        className="p-5"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label className="block text-[13.5px] font-medium">{request.message}</label>
        <input
          autoFocus
          value={value}
          placeholder={request.placeholder}
          onChange={(e) => setValue(e.target.value)}
          onFocus={(e) => e.currentTarget.select()}
          className="mt-3 h-9 w-full rounded-lg border border-[var(--cf-field-border)] bg-[var(--cf-field)] px-3 text-[13px] outline-none focus:border-[var(--cf-accent)]"
        />
        {problem && value && <p className="mt-1.5 text-[12px] text-[var(--cf-danger)]">{problem}</p>}
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => respond(null)}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" variant="primary" disabled={!!problem || empty}>
            {request.confirmLabel ?? t("common.confirm")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
