import { forwardRef } from "react";

/**
 * The large buttons of the redesigned screens — the app's main actions (record, upload, install)
 * are meant to be found without looking. `Button` stays for the small ones inside rows.
 */
type Variant = "primary" | "secondary" | "ghost" | "danger" | "record";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-[var(--cf-accent)] text-[var(--cf-on-accent)] shadow-[0_6px_16px_-8px_color-mix(in_oklab,var(--cf-accent)_80%,transparent)] hover:bg-[color-mix(in_oklab,var(--cf-accent)_88%,var(--cf-text))]",
  secondary: "bg-[var(--cf-surface)] text-[var(--cf-text)] shadow-[inset_0_0_0_1px_var(--cf-border-strong)] hover:bg-[color-mix(in_oklab,var(--cf-text)_4%,var(--cf-surface))]",
  ghost: "text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]",
  danger: "bg-[var(--cf-danger)] text-[var(--cf-on-accent)] hover:bg-[color-mix(in_oklab,var(--cf-danger)_88%,var(--cf-text))]",
  record: "bg-[var(--cf-record)] text-white shadow-[0_6px_16px_-8px_color-mix(in_oklab,var(--cf-record)_80%,transparent)] hover:bg-[color-mix(in_oklab,var(--cf-record)_88%,black)]",
};

const SIZES = {
  md: "h-9 px-4 text-[13.5px] rounded-xl gap-2",
  lg: "h-11 px-5 text-[14.5px] rounded-xl gap-2",
} as const;

export function bigButtonClass(variant: Variant = "primary", size: keyof typeof SIZES = "md") {
  return `cf-press inline-flex shrink-0 items-center justify-center whitespace-nowrap font-semibold transition-[background-color,color,box-shadow] duration-100 disabled:pointer-events-none disabled:opacity-45 ${VARIANTS[variant]} ${SIZES[size]}`;
}

export const BigButton = forwardRef<HTMLButtonElement, React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: keyof typeof SIZES }>(function BigButton(
  { variant = "primary", size = "md", className = "", type = "button", ...props },
  ref,
) {
  return <button ref={ref} type={type} className={`${bigButtonClass(variant, size)} ${className}`} {...props} />;
});
