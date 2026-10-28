import { useEffect, useState } from "react";
import { GlassWater, Undo2 } from "lucide-react";
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis } from "recharts";
import { call, on, type WaterSummary } from "../lib/api";
import { dayLabel } from "../lib/format";
import { useSetting } from "../lib/store";
import { Ring } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

export function useWater() {
  const [w, setW] = useState<WaterSummary | null>(null);
  useEffect(() => {
    const load = () => call<WaterSummary>("water_summary").then(setW);
    void load();
    const a = on("water", load);
    const b = on("day-changed", load);
    const c = on("settings", load);
    return () => {
      a();
      b();
      c();
    };
  }, []);
  return [w, setW] as const;
}

export default function WaterPage() {
  const [w, setW] = useWater();
  const bottles = useSetting<number[]>("water.bottles", [250, 500]);
  const reminders = useSetting<boolean>("water.reminders", true);
  const showBanner = useUi((s) => s.showBanner);

  useEffect(
    () =>
      on<{ goal: number }>("water-goal", () =>
        showBanner("Daily water goal reached!", { tone: "good" }),
      ),
    [showBanner],
  );

  if (!w) return null;
  const frac = w.goal ? w.today / w.goal : 0;
  const week = w.days.slice(-7).map(([day, ml]) => ({ day: dayLabel(day).split(" ")[0], ml, goal: ml >= w.goal }));
  const month = w.days.slice(-30);

  return (
    <div className="flex h-full items-center gap-5">
      <Ring value={frac} size={118} stroke={9} color="#38bdf8">
        <GlassWater size={16} className="text-[#38bdf8]" />
        <div className="tabular text-[20px] leading-tight font-semibold">{w.today}</div>
        <div className="text-[10.5px] text-[var(--muted)]">of {w.goal} ml</div>
      </Ring>

      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex flex-wrap items-center gap-1.5 text-[12.5px]">
          {bottles.map((ml) => (
            <button
              key={ml}
              className="btn btn-accent"
              style={{ background: "#38bdf8" }}
              onClick={() => call<WaterSummary>("water_add", { ml }).then(setW).catch(bannerError)}
            >
              + {ml} ml
            </button>
          ))}
          <button
            className="btn"
            disabled={!w.logs.length}
            onClick={() => call<WaterSummary>("water_undo").then(setW).catch(bannerError)}
            title="Undo last entry"
          >
            <Undo2 size={14} /> Undo
          </button>
          <span className="ml-auto text-[11px] text-[var(--muted)]">
            {w.today >= w.goal ? "Goal reached 🎉" : reminders ? "Reminders on" : "Reminders off"}
          </span>
        </div>
        <div className="flex items-end gap-3">
          <div className="h-[86px] min-w-0 flex-1">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={week} margin={{ top: 4, right: 0, bottom: 0, left: 0 }}>
                <XAxis dataKey="day" tickLine={false} axisLine={false} tick={{ fill: "var(--muted)", fontSize: 10 }} />
                <Tooltip
                  cursor={{ fill: "rgba(255,255,255,0.05)" }}
                  contentStyle={{ background: "#1b1b20", border: "none", borderRadius: 8, fontSize: 11 }}
                  formatter={(v) => [`${v} ml`, "Water"]}
                />
                <Bar dataKey="ml" radius={[4, 4, 0, 0]} fill="#38bdf8" isAnimationActive={false} />
              </BarChart>
            </ResponsiveContainer>
          </div>
          <div className="grid shrink-0 grid-flow-col grid-rows-5 gap-[3px]" title="Last 30 days">
            {month.map(([day, ml]) => {
              const f = Math.min(1, ml / (w.goal || 1));
              return (
                <span
                  key={day}
                  title={`${day}: ${ml} ml`}
                  className="h-[12px] w-[12px] rounded-[3px]"
                  style={{ background: ml ? `rgba(56,189,248,${0.2 + f * 0.8})` : "var(--card)" }}
                />
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
