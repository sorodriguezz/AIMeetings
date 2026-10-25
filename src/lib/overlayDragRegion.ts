/**
 * Whether a press is the window's rather than the app's. AIMeetings keeps the native title bar (and
 * on macOS the overlay one, whose controls AppKit draws above the webview), so no press inside the
 * page ever belongs to the window.
 */
export function pressBelongsToWindow(_event: MouseEvent): boolean {
  return false;
}
