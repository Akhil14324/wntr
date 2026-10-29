import { useEffect, useState } from "react";
import { call, on, type CalEvent } from "./api";

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
