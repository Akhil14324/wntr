import { useEffect, useState } from "react";
import { CalendarDays, MapPin, RefreshCw, Video } from "lucide-react";
import { call, on, type CalEvent } from "../lib/api";
import { clock, countdown, isSameDay, shortDate } from "../lib/format";
import { useNow } from "../lib/store";
import { Empty } from "../components/ui";
import { bannerError } from "../notch/ui";

export function useEvents() {
  const [events, setEvents] = useState<CalEvent[]>([]);
  const [hasCalendars, setHas] = useState(true);
  useEffect(() => {
    const load = () => {
      void call<CalEvent[]>("calendar_events").then(setEvents);
      void call<unknown[]>("calendar_list").then((l) => setHas(l.length > 0));
    };
    load();
    return on("calendar", load);
  }, []);
  return { events, hasCalendars };
}

export function nextEvent(events: CalEvent[], now: number) {
  return events.find((e) => !e.allDay && e.end > now) ?? null;
}

function timeRange(e: CalEvent) {
  if (e.allDay) return "All day";
  return `${clock(new Date(e.start))}–${clock(new Date(e.end))}`;
}

export default function CalendarPage() {
  const { events, hasCalendars } = useEvents();
  const now = useNow(15000);
  const [busy, setBusy] = useState(false);

  if (!hasCalendars) {
    return (
      <Empty
        icon={<CalendarDays size={26} />}
        title="No calendars yet"
        hint="Add a secret iCal address (Google, Outlook, Apple) or an .ics file in Settings."
      >
        <button className="btn mt-1 text-[12px]" onClick={() => call("open_settings", { section: "calendar" })}>
          Open calendar settings
        </button>
      </Empty>
    );
  }

  const today = new Date(now);
  const next = nextEvent(events, now);
  const todays = events.filter((e) => isSameDay(new Date(e.start), today) || (e.allDay && e.start <= now && e.end > now));
  const live = next && next.start <= now;

  return (
    <div className="flex h-full gap-4">
      <div className="flex w-[250px] shrink-0 flex-col justify-center gap-2">
        {next ? (
          <div className="rounded-2xl bg-[var(--card)] p-3" style={{ borderLeft: `3px solid ${next.color}` }}>
            <div className="text-[11px] font-semibold" style={{ color: live ? "var(--good)" : "var(--accent)" }}>
              {live ? "Happening now" : `Next · ${countdown(next.start, now)}`}
            </div>
            <div className="mt-0.5 truncate text-[15px] font-semibold">{next.title}</div>
            <div className="text-[12px] text-[var(--muted)]">
              {isSameDay(new Date(next.start), today) ? "" : `${shortDate(new Date(next.start))} · `}
              {timeRange(next)}
            </div>
            {next.location && (
              <div className="mt-0.5 flex items-center gap-1 truncate text-[11px] text-[var(--muted)]">
                <MapPin size={11} /> <span className="truncate">{next.location}</span>
              </div>
            )}
            {next.joinUrl && (
              <button className="btn btn-accent mt-2 h-[28px] text-[12px]" onClick={() => call("open_target", { target: next.joinUrl })}>
                <Video size={14} /> Join
              </button>
            )}
          </div>
        ) : (
          <div className="rounded-2xl bg-[var(--card)] p-3 text-[12.5px] text-[var(--muted)]">No upcoming events in the next two weeks.</div>
        )}
        <button
          className="btn h-[26px] self-start text-[11.5px]"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            await call("calendar_refresh").catch(bannerError);
            setBusy(false);
          }}
        >
          <RefreshCw size={12} className={busy ? "animate-spin" : ""} /> Refresh
        </button>
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="mb-1 text-[12px] font-semibold">Today</div>
        <div className="scroll-thin min-h-0 flex-1 overflow-y-auto">
          {todays.length ? (
            todays.map((e) => {
              const past = e.end < now;
              return (
                <div key={e.id} className="flex items-center gap-2 rounded-lg px-1.5 py-1 text-[12.5px]" style={{ opacity: past ? 0.45 : 1 }}>
                  <span className="h-[26px] w-[3px] shrink-0 rounded-full" style={{ background: e.color }} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{e.title}</div>
                    <div className="text-[11px] text-[var(--muted)] tabular">{timeRange(e)}</div>
                  </div>
                  {e.joinUrl && !past && (
                    <button className="icon-btn h-7 w-7" onClick={() => call("open_target", { target: e.joinUrl })} aria-label="Join">
                      <Video size={14} />
                    </button>
                  )}
                </div>
              );
            })
          ) : (
            <div className="pt-3 text-[12px] text-[var(--muted)]">Nothing else today.</div>
          )}
        </div>
      </div>
    </div>
  );
}
