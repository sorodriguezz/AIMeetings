/** One setting: its label on the left, the control on the right, a hint under both. */
export function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5 py-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <span className="w-[180px] shrink-0 text-[13px] font-medium">{label}</span>
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">{children}</div>
      </div>
      {hint && <p className="max-w-[64ch] text-[12px] leading-[1.45] text-[var(--cf-text-muted)] sm:pl-[196px]">{hint}</p>}
    </div>
  );
}

/** A titled group of settings. */
export function Block({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col">
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {hint && <p className="mt-1 max-w-[70ch] text-[12.5px] leading-[1.5] text-[var(--cf-text-muted)]">{hint}</p>}
      <div className="mt-2 divide-y divide-[var(--cf-border)]">{children}</div>
    </section>
  );
}

export const inputClass =
  "h-[30px] rounded-lg border border-[var(--cf-field-border)] bg-[var(--cf-field)] px-2.5 text-[13px] outline-none focus:border-[var(--cf-accent)]";
