import type { LucideIcon } from "lucide-react";

/** What a screen shows with nothing in it: what goes here, and how to put something. */
export function EmptyState({ icon: Icon, title, body, children }: { icon: LucideIcon; title: string; body?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-3 px-6 py-14 text-center">
      <span className="flex h-16 w-16 items-center justify-center rounded-3xl bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]">
        <Icon size={28} />
      </span>
      <p className="mt-1 text-[17px] font-semibold">{title}</p>
      {body && <p className="max-w-[420px] text-[13.5px] leading-[1.55] text-[var(--cf-text-muted)]">{body}</p>}
      {children && <div className="mt-2 flex flex-wrap items-center justify-center gap-2">{children}</div>}
    </div>
  );
}
