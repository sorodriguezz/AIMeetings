import { ArrowLeft } from "lucide-react";

/** A screen of the app: scrolls on its own, with the strip on top that drags the window. */
export function Page({ children, width = 860 }: { children: React.ReactNode; width?: number }) {
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className="cf-drag sticky top-0 z-10 h-4" data-tauri-drag-region />
      <div className="mx-auto flex w-full flex-col gap-6 px-10 pb-16 pt-2" style={{ maxWidth: width }}>
        {children}
      </div>
    </div>
  );
}

/** A screen's title, what it is for in one line, and its main actions on the right. */
export function PageHeader({ title, subtitle, actions, icon: Icon }: { title: React.ReactNode; subtitle?: React.ReactNode; actions?: React.ReactNode; icon?: typeof ArrowLeft }) {
  return (
    <header className="flex flex-wrap items-end gap-4 pt-4">
      <div className="flex min-w-0 flex-1 items-center gap-3.5">
        {Icon && (
          <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]">
            <Icon size={21} />
          </span>
        )}
        <div className="min-w-0">
          <h1 className="truncate text-[26px] font-bold tracking-tight">{title}</h1>
          {subtitle && <p className="mt-0.5 text-[14px] leading-[1.5] text-[var(--cf-text-muted)]">{subtitle}</p>}
        </div>
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}

/** «← Mis grabaciones»: back to where the user came from. */
export function BackLink({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="-ml-2 flex h-8 items-center gap-1.5 self-start rounded-lg px-2 text-[13px] font-medium text-[var(--cf-text-muted)] transition-colors hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]"
    >
      <ArrowLeft size={15} />
      {label}
    </button>
  );
}

/** A plain surface for one topic. */
export function Card({ children, className = "", tone = "plain" }: { children: React.ReactNode; className?: string; tone?: "plain" | "accent" | "sunken" }) {
  const toneClass =
    tone === "accent"
      ? "border-[var(--cf-accent-line)] bg-[color-mix(in_oklab,var(--cf-accent)_4%,var(--cf-surface))]"
      : tone === "sunken"
        ? "border-transparent bg-[var(--cf-surface-2)]"
        : "border-[var(--cf-border)] bg-[var(--cf-surface)]";
  return <section className={`rounded-2xl border p-5 ${toneClass} ${className}`}>{children}</section>;
}

/** A small heading over a group, with an optional line under it. */
export function SectionTitle({ title, hint, action }: { title: string; hint?: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className="flex items-end gap-3">
      <div className="min-w-0 flex-1">
        <h2 className="text-[16px] font-semibold">{title}</h2>
        {hint && <p className="mt-0.5 text-[13px] leading-[1.5] text-[var(--cf-text-muted)]">{hint}</p>}
      </div>
      {action}
    </div>
  );
}
