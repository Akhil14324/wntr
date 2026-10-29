import { useSetting } from "./store";

export type WidgetId = "now" | "task" | "event" | "focus" | "water";

export interface WidgetSize {
  /** Columns out of COLUMNS. */
  w: number;
  /** Rows out of ROWS. */
  h: number;
}

export const COLUMNS = 10;
export const ROWS = 2;

export const WIDGETS: { id: WidgetId; label: string }[] = [
  { id: "now", label: "Now playing" },
  { id: "task", label: "Next task" },
  { id: "event", label: "Next event" },
  { id: "focus", label: "Focus" },
  { id: "water", label: "Water" },
];

export const OVERVIEW_DEFAULTS: Record<WidgetId, WidgetSize> = {
  now: { w: 4, h: 2 },
  task: { w: 3, h: 1 },
  event: { w: 3, h: 1 },
  focus: { w: 3, h: 1 },
  water: { w: 3, h: 1 },
};

export function clampSize(s: WidgetSize): WidgetSize {
  return {
    w: Math.min(COLUMNS, Math.max(2, Math.round(s.w))),
    h: Math.min(ROWS, Math.max(1, Math.round(s.h))),
  };
}

/** Saved sizes merged over the defaults, clamped to the grid. */
export function useOverviewLayout(): Record<WidgetId, WidgetSize> {
  const saved = useSetting<Partial<Record<WidgetId, Partial<WidgetSize>>>>("overview.widgets", {});
  const out = {} as Record<WidgetId, WidgetSize>;
  for (const w of WIDGETS) out[w.id] = clampSize({ ...OVERVIEW_DEFAULTS[w.id], ...saved[w.id] });
  return out;
}
