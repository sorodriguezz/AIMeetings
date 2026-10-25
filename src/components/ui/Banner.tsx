import { AlertTriangle, CheckCircle2, Info, XCircle, type LucideIcon } from "lucide-react";

type Tone = "info" | "warning" | "danger" | "success";

const TONES: Record<Tone, { icon: LucideIcon; color: string }> = {
  info: { icon: Info, color: "var(--cf-accent)" },
  warning: { icon: AlertTriangle, color: "var(--cf-warning)" },
  danger: { icon: XCircle, color: "var(--cf-danger)" },
  success: { icon: CheckCircle2, color: "var(--cf-success)" },
};

/** Something the user should know right here, in plain words, with the one thing to do about it. */
export function Banner({
  tone = "info",
  title,
  children,
  action,
  icon,
}: {
  tone?: Tone;
  title: React.ReactNode;
  children?: React.ReactNode;
  action?: React.ReactNode;
  icon?: LucideIcon;
}) {
  const { icon: ToneIcon, color } = TONES[tone];
  const Icon = icon ?? ToneIcon;
  return (
    <div
      className="flex flex-wrap items-center gap-x-4 gap-y-3 rounded-2xl border px-4 py-3.5"
      style={{ borderColor: `color-mix(in oklab, ${color} 30%, transparent)`, background: `color-mix(in oklab, ${color} 7%, var(--cf-surface))` }}
    >
      <Icon size={18} className="shrink-0" style={{ color }} />
      <div className="min-w-[200px] flex-1">
        <p className="text-[14px] font-semibold">{title}</p>
        {children && <div className="mt-0.5 text-[13px] leading-[1.5] text-[var(--cf-text-muted)]">{children}</div>}
      </div>
      {action && <div className="flex shrink-0 items-center gap-2">{action}</div>}
    </div>
  );
}
