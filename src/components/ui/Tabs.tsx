import type { LucideIcon } from "lucide-react";

export interface TabItem<T extends string> {
  id: T;
  label: string;
  icon?: LucideIcon;
  /** A small number after the label (people, models…). */
  count?: number;
  /** A dot that asks for attention (something to set up). */
  dot?: boolean;
}

/** The tabs of a screen: big enough to read, the chosen one filled. */
export function Tabs<T extends string>({ items, value, onChange }: { items: TabItem<T>[]; value: T; onChange: (id: T) => void }) {
  return (
    <div role="tablist" className="flex flex-wrap gap-1 rounded-2xl bg-[var(--cf-sunken)] p-1">
      {items.map(({ id, label, icon: Icon, count, dot }) => {
        const active = id === value;
        return (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(id)}
            className={`relative flex h-9 flex-1 items-center justify-center gap-2 whitespace-nowrap rounded-xl px-3.5 text-[13.5px] font-medium transition-colors ${
              active ? "bg-[var(--cf-surface)] text-[var(--cf-text)] shadow-[var(--cf-shadow-lift)]" : "text-[var(--cf-text-muted)] hover:text-[var(--cf-text)]"
            }`}
          >
            {Icon && <Icon size={15} className={active ? "text-[var(--cf-accent)]" : ""} />}
            {label}
            {count !== undefined && count > 0 && (
              <span className={`rounded-full px-1.5 text-[11.5px] tabular-nums ${active ? "bg-[var(--cf-accent-soft)] text-[var(--cf-accent)]" : "bg-[var(--cf-hover)]"}`}>{count}</span>
            )}
            {dot && <span className="h-1.5 w-1.5 rounded-full bg-[var(--cf-warning)]" aria-hidden />}
          </button>
        );
      })}
    </div>
  );
}
