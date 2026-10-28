import { useEffect, useState } from "react";
import { Coffee, Pause, Play, RotateCcw, SkipForward, Timer } from "lucide-react";
import { call, on, type FocusState, type Stats } from "../lib/api";
import { duration } from "../lib/format";
import { useNow, useStore } from "../lib/store";
import { Ring } from "../components/ui";

const PHASE_LABEL = { focus: "Focus", short: "Short break", long: "Long break" };

function useTodayFocus() {
  const [s, setS] = useState<Stats | null>(null);
  useEffect(() => {
    const load = () => call<Stats>("stats_get", { rangeDays: 1 }).then(setS);
    void load();
    return on("stats-changed", load);
  }, []);
  return s;
}

export default function FocusPage() {
  const f = useStore((s) => s.focus);
  const now = useNow(500);
  const today = useTodayFocus();
  if (!f) return null;
  const cmd = (action: string, minutes?: number) => call<FocusState>("focus_command", { action, minutes });
  const remaining = f.running ? Math.max(0, f.endsAt - now) : f.remainingMs;
  const progress = f.totalMs ? 1 - remaining / f.totalMs : 0;
  const isBreak = f.phase !== "focus";
  const color = isBreak ? "var(--good)" : "var(--accent)";

  return (
    <div className="flex h-full items-center gap-6">
      <Ring value={f.active ? progress : 0} size={132} stroke={8} color={color}>
        <div className="flex items-center gap-1 text-[11px] font-medium" style={{ color }}>
          {isBreak ? <Coffee size={12} /> : <Timer size={12} />}
          {PHASE_LABEL[f.phase]}
        </div>
        <div className="tabular font-display text-[30px] leading-none font-semibold">{duration(remaining)}</div>
        <div className="mt-1 flex gap-1">
          {Array.from({ length: f.cycles }).map((_, i) => (
            <span key={i} className="h-[5px] w-[5px] rounded-full" style={{ background: i < f.cycle ? color : "var(--faint)" }} />
          ))}
        </div>
      </Ring>

      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <div className="flex flex-wrap items-center gap-1.5 text-[12.5px]">
          {!f.active ? (
            <button className="btn btn-accent px-4" onClick={() => cmd("start")}>
              <Play size={14} fill="currentColor" /> Start {PHASE_LABEL[f.phase].toLowerCase()}
            </button>
          ) : f.running ? (
            <button className="btn btn-accent px-4" onClick={() => cmd("pause")}>
              <Pause size={14} fill="currentColor" /> Pause
            </button>
          ) : (
            <button className="btn btn-accent px-4" onClick={() => cmd("resume")}>
              <Play size={14} fill="currentColor" /> Resume
            </button>
          )}
          <button className="btn" onClick={() => cmd("skip")} title="Skip to next phase">
            <SkipForward size={14} /> Skip
          </button>
          <button className="btn" onClick={() => cmd("reset")} title="Reset cycle">
            <RotateCcw size={14} /> Reset
          </button>
        </div>
        <div className="flex items-center gap-1.5 text-[12px]">
          <span className="text-[var(--muted)]">Quick start</span>
          {[15, 25, 50].map((m) => (
            <button key={m} className="btn h-[26px] px-2.5 text-[12px]" onClick={() => cmd("quick", m)}>
              {m} min
            </button>
          ))}
        </div>
        <div className="flex gap-5 text-[12px]">
          <div>
            <div className="tabular text-[18px] font-semibold">{Math.round(today?.focusMin ?? 0)}</div>
            <div className="text-[var(--muted)]">minutes today</div>
          </div>
          <div>
            <div className="tabular text-[18px] font-semibold">{today?.sessions ?? 0}</div>
            <div className="text-[var(--muted)]">sessions</div>
          </div>
          <div>
            <div className="tabular text-[18px] font-semibold">{today?.focusStreak ?? 0}</div>
            <div className="text-[var(--muted)]">day streak</div>
          </div>
        </div>
      </div>
    </div>
  );
}
