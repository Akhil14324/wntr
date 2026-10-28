import { useEffect, useState } from "react";
import { Check, Flame, Plus, Trash2 } from "lucide-react";
import { call, on, type Habit } from "../lib/api";
import { todayIso } from "../lib/format";
import { Empty, TASK_COLORS } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

function lastDays(n: number) {
  const out: string[] = [];
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    out.push(todayIso(d));
  }
  return out;
}

export default function HabitsPage() {
  const [habits, setHabits] = useState<Habit[]>([]);
  const [name, setName] = useState("");
  const [color, setColor] = useState("green");
  const openMenu = useUi((s) => s.openMenu);

  useEffect(() => {
    const load = () => call<Habit[]>("habits_list").then(setHabits);
    void load();
    const a = on("habits", load);
    const b = on("day-changed", load);
    return () => {
      a();
      b();
    };
  }, []);

  const days = lastDays(7);
  const add = async () => {
    if (!name.trim()) return;
    await call("habit_save", { id: null, name, color: TASK_COLORS[color] }).catch(bannerError);
    setName("");
  };

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-1.5">
        <input
          className="field min-w-0 flex-1 text-[12.5px]"
          placeholder="New habit (e.g. Read 20 minutes)"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <div className="flex gap-1">
          {Object.entries(TASK_COLORS).map(([n, hex]) => (
            <button
              key={n}
              aria-label={n}
              onClick={() => setColor(n)}
              className="h-[16px] w-[16px] rounded-full"
              style={{ background: hex, outline: color === n ? "2px solid var(--fg)" : "none", outlineOffset: 1 }}
            />
          ))}
        </div>
        <button className="btn h-[28px] px-2" onClick={add} aria-label="Add habit">
          <Plus size={14} />
        </button>
      </div>
      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto">
        {habits.length ? (
          <table className="w-full text-[12.5px]">
            <thead>
              <tr className="text-[10px] text-[var(--faint)]">
                <th />
                {days.map((d) => (
                  <th key={d} className="w-[24px] font-normal">
                    {["S", "M", "T", "W", "T", "F", "S"][new Date(d + "T12:00").getDay()]}
                  </th>
                ))}
                <th className="w-[46px]" />
              </tr>
            </thead>
            <tbody>
              {habits.map((h) => (
                <tr
                  key={h.id}
                  className="group"
                  onContextMenu={(e) =>
                    openMenu(e.clientX, e.clientY, [
                      { label: "Delete habit", danger: true, run: () => call("habit_delete", { id: h.id }) },
                    ])
                  }
                >
                  <td className="max-w-0 truncate py-0.5 pr-2">
                    <span className="mr-1.5 inline-block h-[8px] w-[8px] rounded-full" style={{ background: h.color }} />
                    {h.name}
                  </td>
                  {days.map((d) => {
                    const done = h.days.includes(d);
                    return (
                      <td key={d} className="text-center">
                        <button
                          onClick={() => call("habit_toggle", { id: h.id, day: d })}
                          aria-label={`${h.name} ${d}`}
                          className="inline-flex h-[20px] w-[20px] items-center justify-center rounded-md transition-colors"
                          style={{ background: done ? h.color : "var(--card)" }}
                        >
                          {done && <Check size={12} className="text-black" strokeWidth={3} />}
                        </button>
                      </td>
                    );
                  })}
                  <td className="text-right whitespace-nowrap">
                    <span className="inline-flex items-center gap-0.5 text-[11.5px] font-semibold" style={{ color: h.streak ? "#fb923c" : "var(--faint)" }}>
                      <Flame size={12} />
                      {h.streak}
                    </span>
                    <button
                      className="icon-btn ml-0.5 h-5 w-5 opacity-0 group-hover:opacity-100"
                      onClick={() => call("habit_delete", { id: h.id })}
                      aria-label="Delete"
                    >
                      <Trash2 size={11} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <Empty icon={<Flame size={24} />} title="No habits yet" hint="Add one above and tick it off each day to build a streak." />
        )}
      </div>
    </div>
  );
}
