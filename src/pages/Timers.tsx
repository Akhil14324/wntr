import { useState } from "react";
import { Hourglass, Pause, Play, Plus, X } from "lucide-react";
import { call } from "../lib/api";
import { duration } from "../lib/format";
import { useNow, useStore } from "../lib/store";
import { bannerError } from "../notch/ui";

const PRESETS = [1, 3, 5, 10, 15, 30];

/** "5", "5m", "90s", "1h 30m", "1:30" → seconds */
export function parseDuration(s: string): number | null {
  const t = s.trim().toLowerCase();
  if (!t) return null;
  if (/^\d+(\.\d+)?$/.test(t)) return Math.round(Number(t) * 60);
  const colon = t.match(/^(\d+):(\d{1,2})(?::(\d{1,2}))?$/);
  if (colon) {
    const [a, b, c] = [Number(colon[1]), Number(colon[2]), colon[3] ? Number(colon[3]) : null];
    return c == null ? a * 60 + b : a * 3600 + b * 60 + c;
  }
  let total = 0;
  let matched = false;
  for (const m of t.matchAll(/(\d+(?:\.\d+)?)\s*(h|hr|hrs|hours?|m|min|mins|minutes?|s|sec|secs|seconds?)/g)) {
    matched = true;
    const n = Number(m[1]);
    const u = m[2][0];
    total += u === "h" ? n * 3600 : u === "m" ? n * 60 : n;
  }
  return matched ? Math.round(total) : null;
}

export default function TimersPage() {
  const timers = useStore((s) => s.timers);
  const now = useNow(500);
  const [custom, setCustom] = useState("");
  const [label, setLabel] = useState("");

  const add = (seconds: number, l?: string) => call("timer_add", { seconds, label: l || null }).catch(bannerError);

  return (
    <div className="flex h-full gap-4">
      <div className="flex w-[230px] shrink-0 flex-col gap-2">
        <div className="text-[12px] font-semibold">Quick timers</div>
        <div className="grid grid-cols-3 gap-1.5">
          {PRESETS.map((m) => (
            <button key={m} className="btn h-[30px] text-[12.5px]" onClick={() => add(m * 60)}>
              {m} min
            </button>
          ))}
        </div>
        <form
          className="flex gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            const secs = parseDuration(custom);
            if (!secs) return bannerError("Try 7, 90s, 1h 15m or 1:30");
            void add(secs, label);
            setCustom("");
            setLabel("");
          }}
        >
          <input className="field w-[64px] text-[12px]" placeholder="7 min" value={custom} onChange={(e) => setCustom(e.target.value)} />
          <input className="field min-w-0 flex-1 text-[12px]" placeholder="Label" value={label} onChange={(e) => setLabel(e.target.value)} />
          <button className="btn h-[30px] px-2" type="submit" aria-label="Add timer">
            <Plus size={14} />
          </button>
        </form>
      </div>
      <div className="scroll-thin min-w-0 flex-1 overflow-y-auto">
        {timers.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-1 text-[var(--muted)]">
            <Hourglass size={24} />
            <span className="text-[12px]">No timers running</span>
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {timers.map((t) => {
              const rem = t.running ? Math.max(0, t.endsAt - now) : t.remainingMs;
              return (
                <div key={t.id} className="relative overflow-hidden rounded-xl bg-[var(--card)] px-3 py-2">
                  <div
                    className="absolute inset-y-0 left-0 bg-[color-mix(in_srgb,var(--accent)_15%,transparent)]"
                    style={{ width: `${(1 - rem / t.totalMs) * 100}%` }}
                  />
                  <div className="relative flex items-center gap-2">
                    <div className="min-w-0 flex-1">
                      <div className="tabular text-[18px] leading-tight font-semibold">{duration(rem)}</div>
                      <div className="truncate text-[11px] text-[var(--muted)]">{t.label}</div>
                    </div>
                    <button
                      className="icon-btn"
                      onClick={() => call("timer_command", { id: t.id, action: t.running ? "pause" : "resume" })}
                      aria-label={t.running ? "Pause" : "Resume"}
                    >
                      {t.running ? <Pause size={15} /> : <Play size={15} />}
                    </button>
                    <button className="icon-btn" onClick={() => call("timer_command", { id: t.id, action: "cancel" })} aria-label="Cancel">
                      <X size={15} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
