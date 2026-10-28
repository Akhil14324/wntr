import { useEffect, useState } from "react";
import { CalendarDays, GlassWater, ListTodo, Pause, Play, Timer } from "lucide-react";
import { call, on, type TaskLists } from "../lib/api";
import { clock, countdown, duration } from "../lib/format";
import { useNow, useStore } from "../lib/store";
import { useUi } from "../notch/ui";
import { Art, control } from "./Music";
import { nextEvent, useEvents } from "./Calendar";
import { useWater } from "./Water";
import { WeatherIcon, useWeather } from "./Weather";

function Card({ children, onClick, className = "" }: { children: React.ReactNode; onClick?: () => void; className?: string }) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      className={`flex min-w-0 flex-col gap-1 overflow-hidden rounded-2xl bg-[var(--card)] p-2.5 text-left transition-colors hover:bg-[var(--card-hover)] ${className}`}
    >
      {children}
    </div>
  );
}

function Label({ icon, text }: { icon: React.ReactNode; text: string }) {
  return (
    <div className="flex items-center gap-1 text-[10.5px] font-semibold tracking-wide text-[var(--muted)] uppercase">
      {icon}
      {text}
    </div>
  );
}

export default function NowPage() {
  const now = useNow(1000);
  const np = useStore((s) => s.media.current);
  const focus = useStore((s) => s.focus);
  const setPage = useUi((s) => s.setPage);
  const { events } = useEvents();
  const [water] = useWater();
  const weather = useWeather();
  const [tasks, setTasks] = useState<TaskLists | null>(null);

  useEffect(() => {
    const load = () => call<TaskLists>("tasks_list").then(setTasks);
    void load();
    return on("tasks", load);
  }, []);

  const nextTask = tasks?.open[0];
  const ev = nextEvent(events, now);
  const focusRemaining = focus ? (focus.running ? focus.endsAt - now : focus.remainingMs) : 0;

  return (
    <div className="grid h-full grid-cols-[1.35fr_1fr_1fr] grid-rows-2 gap-1.5">
      <Card className="row-span-2 justify-between" onClick={() => setPage("music")}>
        <Label icon={null} text="Now playing" />
        {np?.title ? (
          <>
            <div className="flex items-center gap-2.5">
              <Art np={np} size={50} />
              <div className="min-w-0">
                <div className="truncate text-[13.5px] font-semibold">{np.title}</div>
                <div className="truncate text-[11.5px] text-[var(--muted)]">{np.artist}</div>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                className="icon-btn h-8 w-8 bg-[var(--card-hover)]"
                onClick={(e) => {
                  e.stopPropagation();
                  void control("toggle");
                }}
                aria-label={np.playing ? "Pause" : "Play"}
              >
                {np.playing ? <Pause size={15} fill="currentColor" /> : <Play size={15} fill="currentColor" />}
              </button>
              <span className="text-[11px] text-[var(--muted)]">{np.app}</span>
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center text-[12px] text-[var(--muted)]">Nothing playing</div>
        )}
        {weather.data?.current && (
          <div className="flex items-center gap-1.5 text-[12px]">
            <WeatherIcon code={weather.data.current.weather_code} isDay={weather.data.current.is_day === 1} size={15} />
            <span className="font-semibold tabular">{Math.round(weather.data.current.temperature_2m)}°</span>
            <span className="truncate text-[var(--muted)]">{weather.data.place?.split(",")[0]}</span>
          </div>
        )}
      </Card>

      <Card onClick={() => setPage("todo")}>
        <Label icon={<ListTodo size={11} />} text="Next task" />
        <div className="truncate text-[12.5px] font-medium">{nextTask ? nextTask.title : "All clear"}</div>
        <div className="text-[10.5px] text-[var(--muted)]">
          {nextTask?.dueAt ? `${clock(new Date(nextTask.dueAt))} · ${countdown(nextTask.dueAt, now)}` : `${tasks?.completedToday ?? 0} done today`}
        </div>
      </Card>

      <Card onClick={() => setPage("calendar")}>
        <Label icon={<CalendarDays size={11} />} text="Next event" />
        <div className="truncate text-[12.5px] font-medium">{ev ? ev.title : "No events"}</div>
        <div className="text-[10.5px] text-[var(--muted)]">{ev ? `${clock(new Date(ev.start))} · ${countdown(ev.start, now)}` : "—"}</div>
      </Card>

      <Card onClick={() => setPage("focus")}>
        <Label icon={<Timer size={11} />} text="Focus" />
        <div className="tabular text-[16px] font-semibold" style={{ color: focus?.running ? "var(--accent)" : "var(--fg)" }}>
          {duration(Math.max(0, focusRemaining))}
        </div>
        <div className="text-[10.5px] text-[var(--muted)]">
          {focus?.running ? (focus.phase === "focus" ? "Focusing" : "On a break") : focus?.active ? "Paused" : "Ready"}
        </div>
      </Card>

      <Card onClick={() => setPage("water")}>
        <Label icon={<GlassWater size={11} />} text="Water" />
        <div className="text-[16px] font-semibold tabular">
          {water?.today ?? 0}
          <span className="text-[11px] text-[var(--muted)]"> / {water?.goal ?? 0} ml</span>
        </div>
        <div className="h-[4px] overflow-hidden rounded-full bg-[var(--card-hover)]">
          <div className="h-full rounded-full bg-[#38bdf8]" style={{ width: `${Math.min(100, ((water?.today ?? 0) / (water?.goal || 1)) * 100)}%` }} />
        </div>
      </Card>
    </div>
  );
}
