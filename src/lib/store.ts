import { useEffect, useState } from "react";
import { create } from "zustand";
import {
  call,
  on,
  type FocusState,
  type Lyrics,
  type MediaPayload,
  type NowPlaying,
  type QuickTimer,
  type Settings,
  type StatusSnapshot,
  type TaskLists,
} from "./api";
import { parseLrc, type LrcLine } from "./lrc";

export interface LyricsState {
  key: string;
  status: "idle" | "loading" | "ready" | "none" | "error";
  lines: LrcLine[];
  plain: string | null;
  data: Lyrics | null;
}

interface Store {
  settings: Settings | null;
  media: MediaPayload;
  status: StatusSnapshot;
  focus: FocusState | null;
  timers: QuickTimer[];
  completedToday: number;
  idleSecs: number;
  monitorWidth: number;
  lyrics: LyricsState;
  setSettings: (patch: Settings) => Promise<void>;
  setLyrics: (l: Partial<LyricsState>) => void;
}

const emptyLyrics: LyricsState = { key: "", status: "idle", lines: [], plain: null, data: null };

export const useStore = create<Store>((set) => ({
  settings: null,
  media: { sessions: [], current: null },
  status: {
    power: { hasBattery: false, charging: false, percent: -1 },
    network: { connected: false, wifi: false, name: "" },
    capsLock: false,
    layout: "",
    volume: 0,
    muted: false,
  },
  focus: null,
  timers: [],
  completedToday: 0,
  idleSecs: 0,
  monitorWidth: 1920,
  lyrics: emptyLyrics,
  setSettings: async (patch) => {
    const next = await call<Settings>("settings_set", { patch });
    set({ settings: next });
  },
  setLyrics: (l) => set((s) => ({ lyrics: { ...s.lyrics, ...l } })),
}));

/** Read a dotted settings path with a fallback. */
export function useSetting<T>(path: string, fallback: T): T {
  return useStore((s) => {
    let cur: any = s.settings;
    for (const p of path.split(".")) cur = cur?.[p];
    return (cur ?? fallback) as T;
  });
}

export function trackKey(np: NowPlaying | null) {
  return np ? `${np.title}|${np.artist}` : "";
}

/** Playback position projected to `now`. */
export function positionAt(np: NowPlaying | null, now: number) {
  if (!np) return 0;
  const p = np.playing ? np.positionMs + (now - np.positionAt) : np.positionMs;
  return np.durationMs > 0 ? Math.min(Math.max(p, 0), np.durationMs) : Math.max(p, 0);
}

/** Re-render every `ms` milliseconds; returns Date.now(). */
export function useNow(ms = 1000) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(id);
  }, [ms]);
  return now;
}

export async function loadLyrics(np: NowPlaying, force = false) {
  const key = trackKey(np);
  const { lyrics, setLyrics } = useStore.getState();
  if (!force && lyrics.key === key && lyrics.status !== "idle") return;
  setLyrics({ key, status: "loading", lines: [], plain: null, data: null });
  try {
    const l = await call<Lyrics>("lyrics_get", {
      title: np.title,
      artist: np.artist,
      album: np.album,
      durationMs: np.durationMs,
    });
    applyLyrics(key, l);
  } catch {
    if (useStore.getState().lyrics.key === key) setLyrics({ status: "error" });
  }
}

export function applyLyrics(key: string, l: Lyrics) {
  const { setLyrics } = useStore.getState();
  if (useStore.getState().lyrics.key !== key) return;
  const lines = l.synced ? parseLrc(l.synced) : [];
  setLyrics({
    status: lines.length || l.plain ? "ready" : "none",
    lines,
    plain: l.plain,
    data: l,
  });
}

let started = false;

/** Load initial state and subscribe to backend events (once per window). */
export function initStore() {
  if (started) return;
  started = true;
  const set = useStore.setState;
  call<Settings>("settings_get").then((s) => set({ settings: s }));
  call<MediaPayload>("media_state").then((m) => set({ media: m }));
  call<StatusSnapshot>("status_snapshot").then((s) => set({ status: s }));
  call<FocusState>("focus_state").then((f) => set({ focus: f }));
  call<QuickTimer[]>("timers_list").then((t) => set({ timers: t }));
  const refreshTasks = () =>
    call<TaskLists>("tasks_list").then((t) => set({ completedToday: t.completedToday }));
  refreshTasks();
  call<{ monitorWidth: number; idleSecs: number }>("notch_info").then((i) =>
    set({ monitorWidth: i.monitorWidth, idleSecs: i.idleSecs }),
  );

  on<Settings>("settings", (s) => set({ settings: s }));
  on<MediaPayload>("media", (m) => {
    set({ media: m });
    const np = m.current;
    const lyr = useStore.getState().lyrics;
    if (np && np.title && trackKey(np) !== lyr.key) void loadLyrics(np);
    if (!np && lyr.key) useStore.getState().setLyrics(emptyLyrics);
  });
  on<any>("power", (p) => set((s) => ({ status: { ...s.status, power: p } })));
  on<any>("network", (n) => set((s) => ({ status: { ...s.status, network: n } })));
  on<any>("caps", (c) => set((s) => ({ status: { ...s.status, capsLock: c.on } })));
  on<any>("volume", (v) => set((s) => ({ status: { ...s.status, volume: v.level, muted: v.muted } })));
  on<FocusState>("focus", (f) => set({ focus: f }));
  on<QuickTimer[]>("timers", (t) => set({ timers: t }));
  on("tasks", refreshTasks);
  on("day-changed", refreshTasks);
  on<{ secs: number }>("idle", (i) => set({ idleSecs: i.secs }));
  on<{ width: number }>("notch-monitor", (m) => set({ monitorWidth: m.width }));
  on("data-imported", refreshTasks);

  // A track may already be playing when the UI starts.
  setTimeout(() => {
    const np = useStore.getState().media.current;
    if (np?.title) void loadLyrics(np);
  }, 500);
}
