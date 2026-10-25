/** Whether nothing should move — the system's reduce-motion setting. */
export function motionOff(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

/** The arrival curve panels use — fast out, long settle. */
export const EASE_OUT = "cubic-bezier(0.22, 1, 0.36, 1)";

export type MotionLevel = "off" | "medium" | "full";

export function motionLevel(): MotionLevel {
  return motionOff() ? "off" : "medium";
}

/** For a component; the system setting rarely changes while the app runs. */
export function useMotionLevel(): MotionLevel {
  return motionLevel();
}

/** How a programmatic scroll should travel: smoothly, unless nothing is to move. */
export function scrollBehavior(): ScrollBehavior {
  return motionOff() ? "auto" : "smooth";
}
