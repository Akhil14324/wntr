import { useRef, useState } from "react";
import {
  ChevronDown,
  MessageSquareQuote,
  Music as MusicIcon,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { call, type NowPlaying } from "../lib/api";
import { duration } from "../lib/format";
import { positionAt, useNow, useSetting, useStore } from "../lib/store";
import { Empty } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";
import { LyricsView } from "./Lyrics";

export function control(action: string, positionMs?: number) {
  return call("media_control", { action, positionMs }).catch(bannerError);
}

export function Art({ np, size }: { np: NowPlaying; size: number }) {
  const showArt = useSetting<boolean>("music.showArt", true);
  return np.art && showArt ? (
    <img src={np.art} alt="" className="shrink-0 rounded-[12px] object-cover shadow-lg" style={{ width: size, height: size }} />
  ) : (
    <div
      className="flex shrink-0 items-center justify-center rounded-[12px] bg-[var(--card-hover)] text-[var(--muted)]"
      style={{ width: size, height: size }}
    >
      <MusicIcon size={size * 0.4} />
    </div>
  );
}

export function Progress({ np, compact = false }: { np: NowPlaying; compact?: boolean }) {
  const now = useNow(500);
  const [drag, setDrag] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  const pos = drag ?? positionAt(np, now);
  const dur = np.durationMs;
  const frac = dur > 0 ? pos / dur : 0;

  const fromEvent = (e: React.PointerEvent) => {
    const r = bar.current!.getBoundingClientRect();
    return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * dur;
  };

  return (
    <div className={`flex items-center gap-2.5 ${compact ? "text-[10.5px]" : "text-[12px]"} tabular`}>
      <span className="w-10 text-[var(--muted)]">{duration(pos)}</span>
      <div
        ref={bar}
        className={`group relative flex-1 ${np.canSeek ? "cursor-pointer" : ""}`}
        style={{ height: 14 }}
        onPointerDown={(e) => {
          if (!np.canSeek || !dur) return;
          (e.target as HTMLElement).setPointerCapture(e.pointerId);
          setDrag(fromEvent(e));
        }}
        onPointerMove={(e) => drag != null && setDrag(fromEvent(e))}
        onPointerUp={(e) => {
          if (drag == null) return;
          const target = fromEvent(e);
          setDrag(null);
          void control("seek", Math.round(target));
        }}
      >
        <div className="absolute top-1/2 right-0 left-0 h-[4px] -translate-y-1/2 overflow-hidden rounded-full bg-[var(--card-hover)] transition-[height] group-hover:h-[6px]">
          <div className="h-full rounded-full bg-[var(--fg)]" style={{ width: `${frac * 100}%` }} />
        </div>
      </div>
      <span className="w-11 text-right text-[var(--muted)]">-{duration(Math.max(0, dur - pos))}</span>
    </div>
  );
}

export function Controls({ np, size = 1 }: { np: NowPlaying; size?: number }) {
  return (
    <div className="flex items-center gap-1">
      <button className="icon-btn" style={{ width: 34 * size, height: 34 * size }} disabled={!np.canPrev} onClick={() => control("prev")} aria-label="Previous">
        <SkipBack size={18 * size} fill="currentColor" />
      </button>
      <button className="icon-btn" style={{ width: 40 * size, height: 40 * size }} onClick={() => control("toggle")} aria-label={np.playing ? "Pause" : "Play"}>
        {np.playing ? <Pause size={24 * size} fill="currentColor" /> : <Play size={24 * size} fill="currentColor" />}
      </button>
      <button className="icon-btn" style={{ width: 34 * size, height: 34 * size }} disabled={!np.canNext} onClick={() => control("next")} aria-label="Next">
        <SkipForward size={18 * size} fill="currentColor" />
      </button>
    </div>
  );
}

function VolumeControl() {
  const volume = useStore((s) => s.status.volume);
  const muted = useStore((s) => s.status.muted);
  const [open, setOpen] = useState(false);
  const [local, setLocal] = useState<number | null>(null);
  const v = local ?? volume;
  const Icon = muted || v === 0 ? VolumeX : v < 0.5 ? Volume1 : Volume2;
  return (
    <div className="relative flex items-center" onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)}>
      {open && (
        <div className="absolute right-8 flex h-8 items-center rounded-full bg-[var(--card-hover)] px-3">
          <input
            type="range"
            min={0}
            max={100}
            value={Math.round(v * 100)}
            onChange={(e) => {
              const level = Number(e.target.value) / 100;
              setLocal(level);
              void call("volume_set", { level });
            }}
            onPointerUp={() => setLocal(null)}
            className="w-24 accent-[var(--fg)]"
            aria-label="Volume"
          />
          <span className="ml-2 w-7 text-right text-[11px] tabular text-[var(--muted)]">{Math.round(v * 100)}</span>
        </div>
      )}
      <button className="icon-btn" onClick={() => call("volume_toggle_mute")} aria-label="Mute">
        <Icon size={18} />
      </button>
    </div>
  );
}

function SessionPicker() {
  const sessions = useStore((s) => s.media.sessions);
  const current = useStore((s) => s.media.current);
  const openMenu = useUi((s) => s.openMenu);
  if (!current) return null;
  const many = sessions.length > 1;
  return (
    <button
      className={`flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] text-[var(--muted)] ${many ? "bg-[var(--card)] hover:bg-[var(--card-hover)]" : ""}`}
      disabled={!many}
      onClick={(e) => {
        openMenu(e.clientX, e.clientY + 10, [
          { label: "Automatic (most recent)", run: () => call("media_select", { id: null }) },
          ...sessions.map((s) => ({
            label: `${s.app}${s.title ? ` · ${s.title}` : ""}${s.playing ? "  ▶" : ""}`,
            run: () => call("media_select", { id: s.id }),
          })),
        ]);
      }}
    >
      {current.app}
      {many && <ChevronDown size={12} />}
    </button>
  );
}

export default function MusicPage() {
  const np = useStore((s) => s.media.current);
  const [showLyrics, setShowLyrics] = useState(false);
  if (!np || !np.title) {
    return (
      <Empty
        icon={<MusicIcon size={28} />}
        title="Nothing playing"
        hint="Play something in YouTube Music, Spotify, Apple Music or any app that shows media controls in Windows."
      />
    );
  }
  const lyricsButton = (
    <button
      className="icon-btn"
      style={showLyrics ? { color: "var(--accent)" } : undefined}
      onClick={() => setShowLyrics((v) => !v)}
      aria-label="Lyrics"
      aria-pressed={showLyrics}
      title="Lyrics"
    >
      <MessageSquareQuote size={18} />
    </button>
  );

  if (showLyrics) {
    return (
      <div className="flex h-full flex-col gap-2">
        <div className="flex items-center gap-2.5">
          <Art np={np} size={40} />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] leading-tight font-semibold">{np.title}</div>
            <div className="truncate text-[11.5px] text-[var(--muted)]">{np.artist || np.album}</div>
          </div>
          {lyricsButton}
        </div>
        <LyricsView np={np} />
        <Progress np={np} compact />
        <div className="flex items-center justify-between">
          <Controls np={np} size={0.8} />
          <VolumeControl />
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col justify-center gap-2.5">
      <div className="flex items-center gap-3.5">
        <Art np={np} size={68} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[17px] leading-tight font-semibold">{np.title}</div>
          <div className="truncate text-[13px] text-[var(--muted)]">{np.artist || np.album}</div>
          <div className="mt-1">
            <SessionPicker />
          </div>
        </div>
      </div>
      <Progress np={np} />
      <div className="flex items-center justify-between">
        {lyricsButton}
        <Controls np={np} />
        <VolumeControl />
      </div>
    </div>
  );
}
