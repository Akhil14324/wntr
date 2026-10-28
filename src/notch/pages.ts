import type { ComponentType } from "react";
import {
  CalendarDays,
  ChartColumn,
  ClipboardList,
  CloudSun,
  Flame,
  FolderOpen,
  GlassWater,
  Hourglass,
  LayoutDashboard,
  ListTodo,
  MicVocal,
  Music,
  Rocket,
  StickyNote,
  Timer,
  type LucideIcon,
} from "lucide-react";
import NowPage from "../pages/Now";
import MusicPage from "../pages/Music";
import LyricsPage from "../pages/Lyrics";
import ShelfPage from "../pages/Shelf";
import FocusPage from "../pages/Focus";
import WaterPage from "../pages/Water";
import TodoPage from "../pages/Todo";
import CalendarPage from "../pages/Calendar";
import NotesPage from "../pages/Notes";
import ClipboardPage from "../pages/Clipboard";
import HabitsPage from "../pages/Habits";
import LauncherPage from "../pages/Launcher";
import WeatherPage from "../pages/Weather";
import TimersPage from "../pages/Timers";
import StatsPage from "../pages/Stats";

export interface PageDef {
  id: string;
  label: string;
  icon: LucideIcon;
  /** Shortcut key while the notch has focus. */
  key: string;
  component: ComponentType;
}

export const PAGES: PageDef[] = [
  { id: "now", label: "Now", icon: LayoutDashboard, key: "N", component: NowPage },
  { id: "music", label: "Music", icon: Music, key: "M", component: MusicPage },
  { id: "lyrics", label: "Lyrics", icon: MicVocal, key: "L", component: LyricsPage },
  { id: "shelf", label: "Shelf", icon: FolderOpen, key: "S", component: ShelfPage },
  { id: "focus", label: "Focus", icon: Timer, key: "F", component: FocusPage },
  { id: "water", label: "Water", icon: GlassWater, key: "W", component: WaterPage },
  { id: "todo", label: "To-do", icon: ListTodo, key: "T", component: TodoPage },
  { id: "calendar", label: "Calendar", icon: CalendarDays, key: "C", component: CalendarPage },
  { id: "notes", label: "Notes", icon: StickyNote, key: "O", component: NotesPage },
  { id: "clipboard", label: "Clipboard", icon: ClipboardList, key: "V", component: ClipboardPage },
  { id: "habits", label: "Habits", icon: Flame, key: "H", component: HabitsPage },
  { id: "launcher", label: "Launcher", icon: Rocket, key: "Q", component: LauncherPage },
  { id: "weather", label: "Weather", icon: CloudSun, key: "E", component: WeatherPage },
  { id: "timers", label: "Timers", icon: Hourglass, key: "R", component: TimersPage },
  { id: "stats", label: "Stats", icon: ChartColumn, key: "A", component: StatsPage },
];

export function enabledPages(ids: string[] | undefined): PageDef[] {
  if (!ids || !ids.length) return PAGES;
  const list = ids.map((id) => PAGES.find((p) => p.id === id)).filter(Boolean) as PageDef[];
  return list.length ? list : PAGES;
}
