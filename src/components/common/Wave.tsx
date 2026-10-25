function barHeight(level: number, max: number): number {
  return Math.max(3, Math.min(max, 3 + Math.sqrt(level) * max * 1.6));
}

/** The level history as dots that rise into bars where there was sound — newest on the right. */
export function Wave({ levels, max = 18, count = 120 }: { levels: number[]; max?: number; count?: number }) {
  const shown = levels.slice(-count);
  const padded = [...Array<number>(Math.max(0, count - shown.length)).fill(0), ...shown];
  return (
    <div className="flex h-full min-w-0 flex-1 items-center justify-end gap-[3px] overflow-hidden" aria-hidden>
      {padded.map((level, index) => {
        const height = barHeight(level, max);
        return (
          <span
            key={index}
            className={`w-[3px] shrink-0 rounded-full ${height > 4 ? "bg-[var(--cf-text-muted)]" : "bg-[var(--cf-text-faint)]/60"}`}
            style={{ height }}
          />
        );
      })}
    </div>
  );
}
