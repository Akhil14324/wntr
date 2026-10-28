import { useEffect, useState } from "react";
import { Download } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { Bar, BarChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { call, on, type Stats } from "../lib/api";
import { dayLabel } from "../lib/format";
import { useSetting } from "../lib/store";
import { Tabs } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

type Area = "focus" | "water" | "tasks";
const RANGES = [
  { value: "1", label: "Today" },
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "0", label: "All" },
];

export default function StatsPage() {
  const [range, setRange] = useState("7");
  const [area, setArea] = useState<Area>("focus");
  const [stats, setStats] = useState<Stats | null>(null);
  const goal = useSetting<number>("water.goal", 1500);
  const hold = useUi((s) => s.hold);

  useEffect(() => {
    const load = () => call<Stats>("stats_get", { rangeDays: Number(range) }).then(setStats);
    void load();
    return on("stats-changed", load);
  }, [range]);

  if (!stats) return null;
  const data = stats.days.map((d) => ({ ...d, label: stats.days.length > 14 ? d.day.slice(5) : dayLabel(d.day) }));
  const key = area === "focus" ? "focusMin" : area === "water" ? "waterMl" : "tasks";
  const color = area === "focus" ? "var(--accent)" : area === "water" ? "#38bdf8" : "#4ade80";
  const unit = area === "focus" ? "min" : area === "water" ? "ml" : "tasks";

  const summary =
    area === "focus"
      ? [
          [`${Math.round(stats.focusMin)}`, "minutes"],
          [`${stats.sessions}`, "sessions"],
          [stats.bestDay ? dayLabel(stats.bestDay) : "—", "best day"],
          [`${stats.focusStreak}`, "day streak"],
        ]
      : area === "water"
        ? [
            [`${Math.round(stats.waterAvg)}`, "ml / day avg"],
            [`${stats.waterGoalDays}`, "goal days"],
            [`${stats.waterStreak}`, "day streak"],
          ]
        : [
            [`${stats.tasksDone}`, "completed"],
            [stats.tasksPerWeek.toFixed(1), "per week"],
          ];

  const exportCsv = async () => {
    const release = hold();
    try {
      const path = await save({ defaultPath: `wntr-${area}-${range === "0" ? "all" : range + "d"}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (path) {
        await call("stats_export_csv", { area, rangeDays: Number(range), path });
        useUi.getState().showBanner("CSV exported", { tone: "good" });
      }
    } catch (e) {
      bannerError(e);
    } finally {
      release();
    }
  };

  const tooltip = {
    cursor: { fill: "rgba(255,255,255,0.05)" },
    contentStyle: { background: "#1b1b20", border: "none", borderRadius: 8, fontSize: 11 },
    formatter: (v: unknown) => [`${v} ${unit}`, area[0].toUpperCase() + area.slice(1)] as [string, string],
  };

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-2">
        <Tabs<Area>
          value={area}
          onChange={setArea}
          options={[
            { value: "focus", label: "Focus" },
            { value: "water", label: "Water" },
            { value: "tasks", label: "Tasks" },
          ]}
        />
        <Tabs value={range} onChange={setRange} options={RANGES} />
        <button className="icon-btn ml-auto h-7 w-7" onClick={exportCsv} title="Export CSV" aria-label="Export CSV">
          <Download size={14} />
        </button>
      </div>
      <div className="flex min-h-0 flex-1 gap-4">
        <div className="flex w-[118px] shrink-0 flex-col justify-center gap-1.5">
          {summary.map(([v, l]) => (
            <div key={l}>
              <div className="text-[17px] leading-tight font-semibold tabular">{v}</div>
              <div className="text-[10.5px] text-[var(--muted)]">{l}</div>
            </div>
          ))}
        </div>
        <div className="min-w-0 flex-1">
          <ResponsiveContainer width="100%" height="100%">
            {data.length > 31 ? (
              <LineChart data={data} margin={{ top: 6, right: 4, bottom: 0, left: -18 }}>
                <CartesianGrid stroke="var(--line)" vertical={false} />
                <XAxis dataKey="label" tickLine={false} axisLine={false} tick={{ fill: "var(--muted)", fontSize: 10 }} minTickGap={24} />
                <YAxis tickLine={false} axisLine={false} tick={{ fill: "var(--muted)", fontSize: 10 }} width={42} />
                <Tooltip {...tooltip} />
                <Line dataKey={key} stroke={color} dot={false} strokeWidth={2} isAnimationActive={false} />
              </LineChart>
            ) : (
              <BarChart data={data} margin={{ top: 6, right: 4, bottom: 0, left: -18 }}>
                <CartesianGrid stroke="var(--line)" vertical={false} />
                <XAxis dataKey="label" tickLine={false} axisLine={false} tick={{ fill: "var(--muted)", fontSize: 10 }} minTickGap={8} />
                <YAxis tickLine={false} axisLine={false} tick={{ fill: "var(--muted)", fontSize: 10 }} width={42} />
                <Tooltip {...tooltip} />
                <Bar dataKey={key} fill={color} radius={[4, 4, 0, 0]} isAnimationActive={false} />
                {area === "water" && <Line dataKey={() => goal} stroke="var(--faint)" dot={false} />}
              </BarChart>
            )}
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
}
