import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { Pause, Play, RotateCcw, RotateCw } from "lucide-react";
import { meetingsAudio, meetingsPeaks, type Meeting, type MeetingLine, type MeetingSpeaker } from "../../lib/tauri/meetingsCommands";
import { clock, speakerColor } from "./meetingLabels";
import { useT } from "../../state/languageStore";
import { pushErrorToast } from "../../state/toastStore";
import { currentPlatform } from "../../lib/platform";

/**
 * The recording read straight from disk through the `aim-audio:` scheme (`meetings_cmd::serve_audio`),
 * a range at a time — instead of the whole file copied over IPC and again into a blob, hundreds of
 * megabytes for an hour of WAV. `null` when the element cannot load it, so the player falls back.
 */
function streamed(meetingId: string, file: string): Promise<HTMLAudioElement | null> {
  // WebView2 serves a custom scheme as `http://<scheme>.localhost`; WKWebView as the scheme itself.
  const base = currentPlatform() === "windows" ? "http://aim-audio.localhost" : "aim-audio://localhost";
  const element = new Audio();
  // Web Audio's mono mix reads the samples only of an element loaded with CORS.
  element.crossOrigin = "anonymous";
  element.preload = "metadata";
  return new Promise((resolve) => {
    const done = (ok: boolean) => {
      element.removeEventListener("loadedmetadata", onLoaded);
      element.removeEventListener("error", onError);
      clearTimeout(timer);
      if (!ok) element.removeAttribute("src");
      resolve(ok ? element : null);
    };
    const onLoaded = () => done(true);
    const onError = () => done(false);
    const timer = setTimeout(() => done(false), 5000);
    element.addEventListener("loadedmetadata", onLoaded);
    element.addEventListener("error", onError);
    element.src = `${base}/${encodeURIComponent(meetingId)}?f=${encodeURIComponent(file)}`;
  });
}

export interface PlayerHandle {
  /** Jumps to `ms`, and starts playing with `play`. */
  seek: (ms: number, play?: boolean) => void;
  /** Plays `[from, to)` and stops there — a person's voice, a line. */
  playRange: (from: number, to: number) => void;
  pause: () => void;
}

/** Bars the waveform draws. */
const BARS = 160;
const SPEEDS = [1, 1.25, 1.5, 2];

/**
 * The meeting's audio: a waveform coloured by who was speaking, click to jump, and the speed.
 *
 * **Loaded on the first play**, not on open: a two-hour meeting is tens of megabytes, and most
 * visits to a note never press play. The bytes come over IPC into a Blob URL. A meeting kept as
 * stereo (microphone left, the call right — `meetings::encode`) is played through Web Audio mixed
 * to mono, so neither side of the call is in one ear only.
 */
export const MeetingPlayer = forwardRef<PlayerHandle, {
  meeting: Meeting;
  lines: MeetingLine[];
  speakers: MeetingSpeaker[];
  onPosition: (ms: number) => void;
  /** Hears play and pause, so a «listen» button elsewhere can show which it is. */
  onPlaying?: (playing: boolean) => void;
}>(function MeetingPlayer({ meeting, lines, speakers, onPosition, onPlaying }, ref) {
  const t = useT();
  const audio = useRef<HTMLAudioElement | null>(null);
  const url = useRef<string | null>(null);
  const context = useRef<AudioContext | null>(null);
  const [loading, setLoading] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [speed, setSpeed] = useState(1);
  const [peaks, setPeaks] = useState<number[]>([]);
  const stopAt = useRef<number | null>(null);
  const playingListener = useRef(onPlaying);
  playingListener.current = onPlaying;
  const length = Math.max(1, meeting.durationMs);

  useEffect(() => {
    let alive = true;
    void meetingsPeaks(meeting.id)
      .then((byChannel) => {
        if (!alive) return;
        const channels = Object.values(byChannel);
        const width = Math.max(0, ...channels.map((c) => c.length));
        setPeaks(Array.from({ length: width }, (_, i) => Math.max(0, ...channels.map((c) => c[i] ?? 0)) / 255));
      })
      .catch(() => setPeaks([]));
    return () => {
      alive = false;
    };
  }, [meeting.id, meeting.audioFile]);

  // Gone with the panel: the element, the context and the Blob URL.
  useEffect(
    () => () => {
      audio.current?.pause();
      void context.current?.close().catch(() => {});
      if (url.current) URL.revokeObjectURL(url.current);
      audio.current = null;
      url.current = null;
    },
    [meeting.id],
  );

  const ensure = async (): Promise<HTMLAudioElement | null> => {
    if (audio.current) return audio.current;
    setLoading(true);
    try {
      let element = await streamed(meeting.id, meeting.audioFile);
      if (!element) {
        // The old way, kept for a webview that refuses the scheme: the whole file over IPC.
        const bytes = await meetingsAudio(meeting.id);
        const type = meeting.audioFile.endsWith(".wav") ? "audio/wav" : "audio/mp4";
        url.current = URL.createObjectURL(new Blob([bytes], { type }));
        element = new Audio(url.current);
        element.preload = "auto";
      }
      element.playbackRate = speed;
      element.addEventListener("timeupdate", () => {
        const ms = element.currentTime * 1000;
        setPosition(ms);
        onPosition(ms);
        if (stopAt.current !== null && ms >= stopAt.current) {
          stopAt.current = null;
          element.pause();
        }
      });
      const playingNow = (on: boolean) => {
        setPlaying(on);
        playingListener.current?.(on);
      };
      element.addEventListener("play", () => playingNow(true));
      element.addEventListener("pause", () => playingNow(false));
      element.addEventListener("ended", () => playingNow(false));
      try {
        // Down to mono and back out to both ears: a GainNode with one explicit channel mixes
        // whatever comes in to it (speakers interpretation, ½·(L+R)).
        const ctx = new AudioContext();
        const source = ctx.createMediaElementSource(element);
        const mono = ctx.createGain();
        mono.channelCount = 1;
        mono.channelCountMode = "explicit";
        mono.channelInterpretation = "speakers";
        source.connect(mono).connect(ctx.destination);
        context.current = ctx;
      } catch {
        // Without Web Audio it simply plays as recorded.
      }
      audio.current = element;
      return element;
    } catch (error) {
      pushErrorToast(String(error));
      return null;
    } finally {
      setLoading(false);
    }
  };

  const seek = async (ms: number, play = false) => {
    const element = await ensure();
    if (!element) return;
    element.currentTime = Math.max(0, ms / 1000);
    setPosition(ms);
    onPosition(ms);
    if (play) {
      await context.current?.resume().catch(() => {});
      await element.play().catch(() => {});
    }
  };

  useImperativeHandle(ref, () => ({
    seek: (ms, play) => {
      stopAt.current = null;
      void seek(ms, play);
    },
    playRange: (from, to) => {
      void seek(from, true).then(() => {
        stopAt.current = to;
      });
    },
    pause: () => {
      stopAt.current = null;
      audio.current?.pause();
    },
  }));

  const toggle = async () => {
    stopAt.current = null;
    const element = await ensure();
    if (!element) return;
    if (element.paused) {
      await context.current?.resume().catch(() => {});
      await element.play().catch((e) => pushErrorToast(String(e)));
    } else {
      element.pause();
    }
  };

  // Each bar's colour: whoever spoke longest within its slice of time.
  const colours = useMemo(() => {
    const out: string[] = [];
    const slice = length / BARS;
    let cursor = 0;
    for (let bar = 0; bar < BARS; bar += 1) {
      const from = bar * slice;
      const to = from + slice;
      while (cursor < lines.length && lines[cursor].endMs < from) cursor += 1;
      const talk = new Map<string, number>();
      for (let i = cursor; i < lines.length && lines[i].startMs < to; i += 1) {
        const shared = Math.min(to, lines[i].endMs) - Math.max(from, lines[i].startMs);
        if (shared > 0) talk.set(lines[i].speaker, (talk.get(lines[i].speaker) ?? 0) + shared);
      }
      const best = [...talk.entries()].sort((a, b) => b[1] - a[1])[0];
      out.push(best ? speakerColor(best[0], speakers) : "var(--cf-text-faint)");
    }
    return out;
  }, [lines, speakers, length]);

  const heights = useMemo(() => {
    if (peaks.length === 0) return Array<number>(BARS).fill(0.15);
    return Array.from({ length: BARS }, (_, bar) => {
      const from = Math.floor((bar / BARS) * peaks.length);
      const to = Math.max(from + 1, Math.floor(((bar + 1) / BARS) * peaks.length));
      return Math.max(0.08, ...peaks.slice(from, to));
    });
  }, [peaks]);

  const played = position / length;
  return (
    <div className="flex items-center gap-3 rounded-2xl border border-[var(--cf-border)] bg-[var(--cf-surface-2)] px-3 py-2.5">
      <button
        type="button"
        onClick={() => void toggle()}
        disabled={loading}
        aria-label={playing ? t("meetings.pausePlayback") : t("meetings.play")}
        className="cf-press flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-[var(--cf-accent)] text-[var(--cf-on-accent)] transition-colors hover:bg-[color-mix(in_oklab,var(--cf-accent)_88%,var(--cf-text))] disabled:opacity-50"
      >
        {playing ? <Pause size={16} fill="currentColor" /> : <Play size={16} fill="currentColor" className="ml-0.5" />}
      </button>
      <button type="button" title={t("v2.player.back")} aria-label={t("v2.player.back")} onClick={() => void seek(Math.max(0, position - 10_000))} className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]">
        <RotateCcw size={15} />
      </button>
      <div
        role="slider"
        aria-label={t("meetings.position")}
        aria-valuemin={0}
        aria-valuemax={Math.round(length / 1000)}
        aria-valuenow={Math.round(position / 1000)}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "ArrowRight") void seek(position + 5000);
          if (e.key === "ArrowLeft") void seek(position - 5000);
          if (e.key === " ") {
            e.preventDefault();
            void toggle();
          }
        }}
        onClick={(e) => {
          const box = e.currentTarget.getBoundingClientRect();
          stopAt.current = null;
          void seek(((e.clientX - box.left) / box.width) * length, true);
        }}
        className="flex h-10 min-w-0 flex-1 cursor-pointer items-center gap-[1.5px] rounded outline-none focus-visible:ring-1 focus-visible:ring-[var(--cf-accent)]"
      >
        {heights.map((h, i) => (
          <span
            key={i}
            className="min-w-0 flex-1 rounded-full"
            style={{ height: `${Math.round(h * 100)}%`, background: colours[i], opacity: i / BARS <= played ? 1 : 0.35 }}
          />
        ))}
      </div>
      <button type="button" title={t("v2.player.forward")} aria-label={t("v2.player.forward")} onClick={() => void seek(Math.min(length, position + 10_000))} className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]">
        <RotateCw size={15} />
      </button>
      <span className="shrink-0 font-mono text-[12px] tabular-nums text-[var(--cf-text-muted)]">
        {clock(position)} / {clock(length)}
      </span>
      <button
        type="button"
        className="shrink-0 rounded-lg px-2 py-1 font-mono text-[12px] font-semibold text-[var(--cf-text-muted)] hover:bg-[var(--cf-hover)] hover:text-[var(--cf-text)]"
        title={t("meetings.speed")}
        onClick={() => {
          const next = SPEEDS[(SPEEDS.indexOf(speed) + 1) % SPEEDS.length];
          setSpeed(next);
          if (audio.current) audio.current.playbackRate = next;
        }}
      >
        {speed}×
      </button>
    </div>
  );
});
