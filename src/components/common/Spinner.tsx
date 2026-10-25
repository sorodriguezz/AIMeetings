/** A small ring that turns — what something is doing while it is working. */
export function Spinner({ size = 14, className = "" }: { size?: number; className?: string }) {
  return (
    <span
      role="status"
      aria-hidden
      className={`cf-spin inline-block shrink-0 rounded-full border-2 border-[var(--cf-border-strong)] border-t-[var(--cf-accent)] ${className}`}
      style={{ width: size, height: size }}
    />
  );
}
