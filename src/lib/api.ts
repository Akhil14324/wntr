import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export async function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

/** Subscribe to a backend event; returns a cleanup function usable in effects. */
export function on<T>(event: string, cb: (payload: T) => void): () => void {
  let un: UnlistenFn | null = null;
  let cancelled = false;
  listen<T>(event, (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else un = u;
  });
  return () => {
    cancelled = true;
    un?.();
  };
}

export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

// ---------------------------------------------------------------------------
// Types mirrored from the Rust side
// ---------------------------------------------------------------------------

export type Settings = Record<string, any>;

export interface SessionInfo {
  id: string;
  app: string;
  playing: boolean;
  title: string;
}

export interface NowPlaying {
  id: string;
  app: string;
  title: string;
  artist: string;
  album: string;
  art: string | null;
  durationMs: number;
  positionMs: number;
  positionAt: number;
  playing: boolean;
  canNext: boolean;
  canPrev: boolean;
  canSeek: boolean;
}

export interface MediaPayload {
  sessions: SessionInfo[];
  current: NowPlaying | null;
}

export interface Lyrics {
  synced: string | null;
  plain: string | null;
  track: string;
  artist: string;
  source: string;
  key: string;
}

export interface ShelfItem {
  id: number;
  kind: "file" | "image" | "text";
  path: string | null;
  text: string | null;
  name: string;
  size: number | null;
  ext: string;
  isDir: boolean;
  missing: boolean;
  addedAt: number;
}

export interface FocusState {
  phase: "focus" | "short" | "long";
  running: boolean;
  active: boolean;
  endsAt: number;
  remainingMs: number;
  totalMs: number;
  cycle: number;
  cycles: number;
}

export interface QuickTimer {
  id: number;
  label: string;
  totalMs: number;
  endsAt: number;
  remainingMs: number;
  running: boolean;
}

export interface Task {
  id: number;
  title: string;
  notes: string;
  color: string;
  priority: number;
  dueAt: number | null;
  repeat: "none" | "daily" | "weekdays" | "weekly";
  done: boolean;
  snoozedUntil: number | null;
  createdAt: number;
}

export interface Completion {
  id: number;
  taskId: number | null;
  title: string;
  at: number;
}

export interface TaskLists {
  open: Task[];
  done: Completion[];
  completedToday: number;
}

export interface WaterSummary {
  today: number;
  goal: number;
  logs: [number, number, number][];
  days: [string, number][];
}

export interface Note {
  id: number;
  body: string;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface ClipItem {
  id: number;
  kind: "text" | "image";
  text: string | null;
  image: string | null;
  pinned: boolean;
  createdAt: number;
}

export interface Habit {
  id: number;
  name: string;
  color: string;
  days: string[];
  streak: number;
  doneToday: boolean;
}

export interface LaunchItem {
  id: number;
  name: string;
  target: string;
  kind: "url" | "folder" | "app";
}

export interface CalEvent {
  id: string;
  calendarId: number;
  title: string;
  start: number;
  end: number;
  allDay: boolean;
  location: string;
  joinUrl: string | null;
  color: string;
}

export interface Calendar {
  id: number;
  name: string;
  source: string;
  color: string;
  enabled: boolean;
  lastFetch: number | null;
  lastError: string | null;
}

export interface DayStat {
  day: string;
  focusMin: number;
  sessions: number;
  waterMl: number;
  tasks: number;
}

export interface Stats {
  days: DayStat[];
  focusMin: number;
  sessions: number;
  bestDay: string | null;
  bestDayMin: number;
  focusStreak: number;
  waterAvg: number;
  waterGoalDays: number;
  waterStreak: number;
  tasksDone: number;
  tasksPerWeek: number;
}

export interface Power {
  hasBattery: boolean;
  charging: boolean;
  percent: number;
}

export interface Network {
  connected: boolean;
  wifi: boolean;
  name: string;
}

export interface StatusSnapshot {
  power: Power;
  network: Network;
  capsLock: boolean;
  layout: string;
  volume: number;
  muted: boolean;
}

export interface Popup {
  kind: string;
  data: any;
  at: number;
}

export interface Alert {
  kind: "task" | "water" | "event" | "focus" | "timer";
  title: string;
  body: string;
  data: any;
  at: number;
}
