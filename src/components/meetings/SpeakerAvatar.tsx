import { User } from "lucide-react";
import { speakerColor, speakerName } from "./meetingLabels";
import type { Meeting, MeetingSpeaker } from "../../lib/tauri/meetingsCommands";

/** A person's mark: their colour, and the initial of the name they were given. */
export function SpeakerAvatar({ speakerKey, speakers, kind, size = 32 }: { speakerKey: string; speakers: MeetingSpeaker[]; kind: Meeting["kind"]; size?: number }) {
  const color = speakerColor(speakerKey, speakers);
  const named = speakers.find((s) => s.key === speakerKey)?.name.trim();
  const name = speakerName(speakerKey, speakers, kind);
  const letter = (named || name).trim().charAt(0).toUpperCase();
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-full font-semibold"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.42), color, background: `color-mix(in oklab, ${color} 16%, var(--cf-surface))`, boxShadow: `inset 0 0 0 1.5px color-mix(in oklab, ${color} 45%, transparent)` }}
      aria-hidden
    >
      {named || speakerKey === "me" ? letter : <User size={Math.round(size * 0.5)} />}
    </span>
  );
}

/** A small round swatch for menus. */
export function SpeakerDot({ color }: { color: string }) {
  return <span className="h-3 w-3 rounded-full" style={{ background: color }} aria-hidden />;
}
