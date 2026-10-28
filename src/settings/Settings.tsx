import { useEffect, useLayoutEffect, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Bell,
  CalendarDays,
  ClipboardList,
  CloudSun,
  Database,
  GlassWater,
  Info,
  Keyboard,
  ListTodo,
  Moon,
  Music,
  Palette,
  Power,
  Settings2,
  StickyNote,
  Timer,
  Trash2,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { call, errText, on, type Calendar } from "../lib/api";
import { relativeDateTime } from "../lib/format";
import { useSetting, useStore } from "../lib/store";
import { Toggle } from "../components/ui";
import { PAGES } from "../notch/pages";

const SECTIONS = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "music", label: "Music & lyrics", icon: Music },
  { id: "focus", label: "Focus", icon: Timer },
  { id: "water", label: "Water", icon: GlassWater },
  { id: "tasks", label: "Tasks", icon: ListTodo },
  { id: "calendar", label: "Calendar", icon: CalendarDays },
  { id: "status", label: "Status popups", icon: Bell },
  { id: "clipboard", label: "Clipboard", icon: ClipboardList },
  { id: "notes", label: "Notes", icon: StickyNote },
  { id: "weather", label: "Weather", icon: CloudSun },
  { id: "dnd", label: "Do Not Disturb", icon: Moon },
  { id: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { id: "data", label: "Data", icon: Database },
  { id: "about", label: "About", icon: Info },
];

function patchAt(path: string, value: unknown) {
  const parts = path.split(".");
  const root: Record<string, unknown> = {};
  let cur = root;
  parts.forEach((p, i) => {
    if (i === parts.length - 1) cur[p] = value;
    else {
      cur[p] = {};
      cur = cur[p] as Record<string, unknown>;
    }
  });
  return root;
}

function useSet() {
  const setSettings = useStore((s) => s.setSettings);
  return (path: string, value: unknown) => setSettings(patchAt(path, value));
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-white/5 py-3 last:border-0">
      <div className="min-w-0">
        <div className="text-[13.5px]">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] opacity-55">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

function Group({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      {title && <div className="mb-1 text-[11.5px] font-semibold tracking-wide uppercase opacity-50">{title}</div>}
      <div className="rounded-2xl bg-white/[0.04] px-4">{children}</div>
    </div>
  );
}

function SwitchRow({ path, label, hint, fallback = true }: { path: string; label: string; hint?: string; fallback?: boolean }) {
  const v = useSetting<boolean>(path, fallback);
  const set = useSet();
  return (
    <Row label={label} hint={hint}>
      <Toggle on={v} onChange={(x) => set(path, x)} label={label} />
    </Row>
  );
}

function NumberRow({
  path,
  label,
  hint,
  min,
  max,
  step = 1,
  suffix,
  fallback,
}: {
  path: string;
  label: string;
  hint?: string;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  fallback: number;
}) {
  const v = useSetting<number>(path, fallback);
  const set = useSet();
  const [draft, setDraft] = useState(String(v));
  useEffect(() => setDraft(String(v)), [v]);
  const commit = () => {
    const n = Number(draft);
    if (Number.isFinite(n)) set(path, Math.max(min, Math.min(max, n)));
    else setDraft(String(v));
  };
  return (
    <Row label={label} hint={hint}>
      <input
        type="number"
        className="field w-[90px] text-right"
        value={draft}
        min={min}
        max={max}
        step={step}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
      />
      {suffix && <span className="w-10 text-[12.5px] opacity-60">{suffix}</span>}
    </Row>
  );
}

function ListRow({ path, label, hint, fallback, suffix }: { path: string; label: string; hint?: string; fallback: number[]; suffix?: string }) {
  const v = useSetting<number[]>(path, fallback);
  const set = useSet();
  const [draft, setDraft] = useState(v.join(", "));
  useEffect(() => setDraft(v.join(", ")), [v]);
  const commit = () => {
    const nums = draft
      .split(/[,\s]+/)
      .map(Number)
      .filter((n) => Number.isFinite(n) && n > 0);
    if (nums.length) set(path, nums.slice(0, 6));
    else setDraft(v.join(", "));
  };
  return (
    <Row label={label} hint={hint}>
      <input className="field w-[160px] text-right" value={draft} onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
      {suffix && <span className="w-10 text-[12.5px] opacity-60">{suffix}</span>}
    </Row>
  );
}

function Select({ path, options, fallback }: { path: string; options: { value: string; label: string }[]; fallback: string }) {
  const v = useSetting<string>(path, fallback);
  const set = useSet();
  return (
    <select className="field min-w-[150px]" value={v} onChange={(e) => set(path, e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

function HotkeyInput() {
  const v = useSetting<string>("general.hotkey", "Ctrl+Alt+N");
  const set = useSet();
  const [recording, setRecording] = useState(false);
  return (
    <button
      className="field min-w-[150px] text-center font-mono text-[12.5px]"
      style={recording ? { borderColor: "var(--accent)" } : {}}
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        if (e.key === "Escape") return setRecording(false);
        if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
        const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
        if (!mods.length) return;
        let key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.key.length === 1 ? e.key.toUpperCase() : e.code;
        if (key === " ") key = "Space";
        set("general.hotkey", [...mods, key].join("+"));
        setRecording(false);
      }}
    >
      {recording ? "Press keys…" : v || "None"}
    </button>
  );
}

function PagesEditor() {
  const pages = useSetting<string[]>("general.pages", PAGES.map((p) => p.id));
  const set = useSet();
  const ordered = [...pages.filter((id) => PAGES.some((p) => p.id === id)), ...PAGES.map((p) => p.id).filter((id) => !pages.includes(id))];
  const move = (id: string, d: number) => {
    const list = pages.slice();
    const i = list.indexOf(id);
    const j = i + d;
    if (i < 0 || j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    set("general.pages", list);
  };
  return (
    <div className="py-2">
      {ordered.map((id) => {
        const p = PAGES.find((x) => x.id === id)!;
        const on = pages.includes(id);
        const Icon = p.icon;
        return (
          <div key={id} className="flex items-center gap-3 py-1.5">
            <Toggle
              on={on}
              label={p.label}
              onChange={(v) => {
                const next = v ? [...pages, id] : pages.filter((x) => x !== id);
                if (next.length) set("general.pages", next);
              }}
            />
            <Icon size={15} className="opacity-70" />
            <span className={`flex-1 text-[13px] ${on ? "" : "opacity-45"}`}>{p.label}</span>
            {on && (
              <>
                <button className="icon-btn h-7 w-7" onClick={() => move(id, -1)} aria-label="Move up">
                  <ArrowUp size={14} />
                </button>
                <button className="icon-btn h-7 w-7" onClick={() => move(id, 1)} aria-label="Move down">
                  <ArrowDown size={14} />
                </button>
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}

function General() {
  const [monitors, setMonitors] = useState<string[]>([]);
  const size = useSetting<string>("general.notchSize", "medium");
  useEffect(() => {
    void call<{ monitors: string[] }>("notch_info").then((i) => setMonitors(i.monitors));
  }, []);
  return (
    <>
      <Group>
        <SwitchRow path="general.autostart" label="Start with Windows" hint="Adds WNTR to your startup apps (no admin needed)." />
        <Row label="Show notch on">
          <Select path="general.monitor" fallback="primary" options={[{ value: "primary", label: "Primary monitor" }, ...monitors.map((m) => ({ value: m, label: m.replace(/^\\\\\.\\/, "") }))]} />
        </Row>
        <Row label="Notch size">
          <Select
            path="general.notchSize"
            fallback="medium"
            options={[
              { value: "small", label: "Small" },
              { value: "medium", label: "Medium" },
              { value: "large", label: "Large" },
              { value: "custom", label: "Custom" },
            ]}
          />
        </Row>
        {size === "custom" && (
          <>
            <NumberRow path="general.customWidth" label="Custom width" min={140} max={360} fallback={216} suffix="px" />
            <NumberRow path="general.customHeight" label="Custom height" min={24} max={56} fallback={34} suffix="px" />
          </>
        )}
        <SwitchRow path="general.autoLarger" label="Larger on wide monitors" hint="One size up on screens wider than 1920 px." />
        <SwitchRow path="general.minimalMode" label="Minimal mode" hint="Collapsed notch becomes a thin line (for thick laptop bezels)." fallback={false} />
        <SwitchRow path="general.hideInFullscreen" label="Hide in fullscreen apps" hint="Games, videos and presentations." />
        <Row label="Open / close hotkey" hint="Global shortcut. Click, then press the new combination.">
          <HotkeyInput />
        </Row>
        <NumberRow path="general.expandDelayMs" label="Expand delay" min={0} max={1500} step={10} fallback={120} suffix="ms" />
        <NumberRow path="general.collapseDelayMs" label="Collapse delay" min={100} max={3000} step={50} fallback={450} suffix="ms" />
      </Group>
      <Group title="Pages">
        <PagesEditor />
      </Group>
    </>
  );
}

const ACCENTS = ["#7dd3fc", "#a78bfa", "#f472b6", "#fb923c", "#4ade80", "#facc15", "#f87171", "#e5e7eb"];

function Appearance() {
  const accent = useSetting<string>("appearance.accent", "#7dd3fc");
  const color = useSetting<string>("appearance.notchColor", "#000000");
  const opacity = useSetting<number>("appearance.notchOpacity", 1);
  const set = useSet();
  return (
    <Group>
      <Row label="Theme">
        <Select path="appearance.theme" fallback="dark" options={[{ value: "dark", label: "Dark" }, { value: "light", label: "Light" }]} />
      </Row>
      <Row label="Accent color">
        {ACCENTS.map((c) => (
          <button
            key={c}
            aria-label={c}
            onClick={() => set("appearance.accent", c)}
            className="h-6 w-6 rounded-full"
            style={{ background: c, outline: accent === c ? "2px solid white" : "none", outlineOffset: 2 }}
          />
        ))}
        <input type="color" value={accent} onChange={(e) => set("appearance.accent", e.target.value)} className="h-7 w-9 cursor-pointer rounded bg-transparent" aria-label="Custom accent" />
      </Row>
      <Row label="Notch color">
        <input type="color" value={color} onChange={(e) => set("appearance.notchColor", e.target.value)} className="h-7 w-9 cursor-pointer rounded bg-transparent" aria-label="Notch color" />
        <button className="btn text-[12px]" onClick={() => set("appearance.notchColor", "#000000")}>
          Black
        </button>
      </Row>
      <Row label="Notch opacity">
        <input type="range" min={30} max={100} value={Math.round(opacity * 100)} onChange={(e) => set("appearance.notchOpacity", Number(e.target.value) / 100)} className="w-40" />
        <span className="w-10 text-right text-[12.5px] tabular opacity-70">{Math.round(opacity * 100)}%</span>
      </Row>
      <SwitchRow path="appearance.mascot" label="Idle mascot" hint="A pair of eyes appears when the notch has been quiet for a while." />
      <NumberRow path="appearance.mascotIdleMin" label="Mascot appears after" min={1} max={120} fallback={5} suffix="min" />
    </Group>
  );
}

function MusicSection() {
  const set = useSet();
  return (
    <>
      <Group>
        <SwitchRow path="music.lyricsInPill" label="Lyrics in the collapsed notch" hint="The pill shows the current line and resizes with it." />
        <SwitchRow path="music.showArt" label="Show album art" />
        <Row label="Lyric offsets" hint="Per-song timing fixes you set on the lyrics page.">
          <button className="btn text-[12px]" onClick={() => set("music.lyricOffsets", {})}>
            Reset all
          </button>
        </Row>
      </Group>
      <Group title="Media sources">
        <SwitchRow path="music.sources.chrome" label="Google Chrome" hint="Includes YouTube Music in a Chrome tab." />
        <SwitchRow path="music.sources.edge" label="Microsoft Edge" />
        <SwitchRow path="music.sources.firefox" label="Firefox" />
        <SwitchRow path="music.sources.spotify" label="Spotify" />
        <SwitchRow path="music.sources.other" label="Other apps" hint="Apple Music, Media Player, YouTube Music desktop, VLC, …" />
      </Group>
    </>
  );
}

function FocusSection() {
  return (
    <Group>
      <NumberRow path="focus.focusMin" label="Focus length" min={1} max={180} fallback={25} suffix="min" />
      <NumberRow path="focus.shortMin" label="Short break" min={1} max={60} fallback={5} suffix="min" />
      <NumberRow path="focus.longMin" label="Long break" min={1} max={90} fallback={15} suffix="min" />
      <NumberRow path="focus.cycles" label="Focus sessions before a long break" min={1} max={12} fallback={4} />
      <SwitchRow path="focus.chime" label="Chime" hint="Plays when a focus session or break ends." />
      <SwitchRow path="focus.autoStartBreaks" label="Start breaks automatically" />
    </Group>
  );
}

function WaterSection() {
  const start = useSetting<string>("water.start", "09:00");
  const end = useSetting<string>("water.end", "22:00");
  const set = useSet();
  return (
    <Group>
      <SwitchRow path="water.reminders" label="Reminders" hint="Turning this off keeps your log." />
      <ListRow path="water.bottles" label="Bottle sizes" hint="One quick-add button each, comma separated." fallback={[250, 500]} suffix="ml" />
      <NumberRow path="water.goal" label="Daily goal" min={250} max={6000} step={50} fallback={1500} suffix="ml" />
      <NumberRow path="water.intervalMin" label="Remind every" min={5} max={360} fallback={60} suffix="min" />
      <Row label="Active hours">
        <input type="time" className="field" value={start} onChange={(e) => set("water.start", e.target.value)} />
        <span className="opacity-50">to</span>
        <input type="time" className="field" value={end} onChange={(e) => set("water.end", e.target.value)} />
      </Row>
    </Group>
  );
}

function TasksSection() {
  return (
    <Group>
      <NumberRow path="tasks.defaultReminderMin" label="Remind before due time" hint="0 = at the due time." min={0} max={1440} fallback={0} suffix="min" />
      <ListRow path="tasks.snooze" label="Snooze options" fallback={[5, 10, 30]} suffix="min" />
    </Group>
  );
}

function CalendarSection() {
  const [cals, setCals] = useState<Calendar[]>([]);
  const [name, setName] = useState("");
  const [source, setSource] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const load = () => call<Calendar[]>("calendar_list").then(setCals);
  useEffect(() => {
    void load();
    return on("calendar", load);
  }, []);
  const add = async () => {
    setBusy(true);
    setError("");
    try {
      await call("calendar_add", { name, source, color: ACCENTS[cals.length % ACCENTS.length] });
      setName("");
      setSource("");
      await load();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Group title="Calendars">
        {cals.map((c) => (
          <Row key={c.id} label={c.name} hint={c.lastError ? `Error: ${c.lastError}` : c.lastFetch ? `Updated ${relativeDateTime(c.lastFetch)}` : undefined}>
            <span className="h-3 w-3 rounded-full" style={{ background: c.color }} />
            <Toggle on={c.enabled} onChange={(v) => call("calendar_toggle", { id: c.id, enabled: v }).then(load)} label={c.name} />
            <button className="icon-btn" onClick={() => call("calendar_remove", { id: c.id }).then(load)} aria-label="Remove">
              <Trash2 size={15} />
            </button>
          </Row>
        ))}
        {!cals.length && <div className="py-3 text-[13px] opacity-55">No calendars yet.</div>}
      </Group>
      <Group title="Add a calendar">
        <div className="flex flex-col gap-2 py-3">
          <input className="field" placeholder="Name (e.g. Work)" value={name} onChange={(e) => setName(e.target.value)} />
          <div className="flex gap-2">
            <input className="field flex-1" placeholder="Secret iCal address (https://… .ics) or file path" value={source} onChange={(e) => setSource(e.target.value)} />
            <button
              className="btn"
              onClick={async () => {
                const p = await open({ multiple: false, filters: [{ name: "iCalendar", extensions: ["ics"] }] });
                if (typeof p === "string") setSource(p);
              }}
            >
              Browse…
            </button>
            <button className="btn btn-accent" disabled={busy || !source.trim()} onClick={add}>
              {busy ? "Checking…" : "Add"}
            </button>
          </div>
          {error && <div className="text-[12.5px] text-[#f87171]">{error}</div>}
          <div className="text-[12px] leading-relaxed opacity-55">
            Google Calendar: Settings → your calendar → Integrate calendar → “Secret address in iCal format”. Outlook: Settings → Calendar → Shared calendars → Publish a calendar → ICS link. Nothing is sent anywhere except a request to this address.
          </div>
        </div>
      </Group>
      <Group title="Alerts">
        <NumberRow path="calendar.alertMin" label="Alert before events" hint="0 turns alerts off." min={0} max={240} fallback={10} suffix="min" />
        <NumberRow path="calendar.refreshMin" label="Refresh every" min={5} max={1440} fallback={15} suffix="min" />
      </Group>
    </>
  );
}

function StatusSection() {
  return (
    <>
      <Group title="Popups">
        <SwitchRow path="status.battery" label="Battery" hint="Charger plugged in or removed, low battery." />
        <SwitchRow path="status.wifi" label="Wi-Fi" hint="Connect and disconnect." />
        <SwitchRow path="status.bluetooth" label="Bluetooth" hint="Paired devices connecting and disconnecting." />
        <SwitchRow path="status.capsLock" label="Caps Lock" />
        <SwitchRow path="status.keyboardLang" label="Keyboard language" hint="WNTR reads only the active layout and Caps Lock state, never typed keys." />
      </Group>
      <Group title="Overlays">
        <SwitchRow path="status.volume" label="Volume HUD" />
        <SwitchRow path="status.brightness" label="Brightness HUD" hint="Laptop screens only." />
      </Group>
      <Group title="Low battery">
        <ListRow path="status.lowBattery" label="Alert at" fallback={[20, 10]} suffix="%" />
      </Group>
    </>
  );
}

function ClipboardSection() {
  const size = useSetting<number>("clipboard.size", 50);
  const set = useSet();
  const [msg, setMsg] = useState("");
  return (
    <Group>
      <SwitchRow path="clipboard.paused" label="Pause recording" fallback={false} hint="Content marked private by password managers is never recorded." />
      <Row label="History size" hint="Pinned items are never removed automatically.">
        <input type="range" min={20} max={200} step={10} value={size} onChange={(e) => set("clipboard.size", Number(e.target.value))} className="w-40" />
        <span className="w-10 text-right text-[12.5px] tabular opacity-70">{size}</span>
      </Row>
      <Row label="Clear history" hint={msg || "Keeps pinned items."}>
        <button className="btn" onClick={() => call("clipboard_clear").then(() => setMsg("Cleared."))}>
          Clear
        </button>
      </Row>
    </Group>
  );
}

function NotesSection() {
  const dir = useSetting<string>("notes.exportDir", "");
  const set = useSet();
  const [msg, setMsg] = useState("");
  return (
    <Group>
      <Row label="Export folder" hint={dir || "Documents\\WNTR Notes"}>
        <button
          className="btn"
          onClick={async () => {
            const p = await open({ directory: true });
            if (typeof p === "string") set("notes.exportDir", p);
          }}
        >
          Choose…
        </button>
      </Row>
      <Row label="Export all notes" hint={msg || "One .md file per note."}>
        <button
          className="btn"
          onClick={() =>
            call<string[]>("notes_export", {})
              .then((p) => setMsg(`Exported ${p.length} notes.`))
              .catch((e) => setMsg(errText(e)))
          }
        >
          Export
        </button>
      </Row>
    </Group>
  );
}

function WeatherSection() {
  const city = useSetting<string>("weather.city", "");
  const place = useSetting<string>("weather.place", "");
  const [draft, setDraft] = useState(city);
  const [msg, setMsg] = useState("");
  useEffect(() => setDraft(city), [city]);
  return (
    <Group>
      <Row label="City" hint={msg || (place ? `Using ${place}` : "Only the city's coordinates are sent to Open-Meteo.")}>
        <input className="field w-[200px]" value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="City" />
        <button
          className="btn"
          onClick={() =>
            call("weather_set_city", { city: draft })
              .then(() => setMsg(""))
              .catch((e) => setMsg(errText(e)))
          }
        >
          Save
        </button>
      </Row>
      <Row label="Units">
        <Select path="weather.unit" fallback="celsius" options={[{ value: "celsius", label: "Celsius" }, { value: "fahrenheit", label: "Fahrenheit" }]} />
      </Row>
    </Group>
  );
}

function DndSection() {
  const until = useSetting<number>("dnd.until", 0);
  const active = until > Date.now();
  const presets = useSetting<number[]>("dnd.presets", [30, 60]);
  return (
    <>
      <Group>
        <Row label="Status" hint={active ? `Popups and reminders paused until ${new Date(until).toLocaleString()}` : "Off"}>
          {active && (
            <button className="btn" onClick={() => call("dnd_set", { minutes: null })}>
              Turn off
            </button>
          )}
        </Row>
        <Row label="Pause for">
          {presets.map((m) => (
            <button key={m} className="btn" onClick={() => call("dnd_set", { minutes: m })}>
              {m >= 60 ? `${m / 60} h` : `${m} min`}
            </button>
          ))}
          <button className="btn" onClick={() => call("dnd_set", { minutes: -1 })}>
            Until tomorrow
          </button>
        </Row>
      </Group>
      <Group>
        <ListRow path="dnd.presets" label="Duration presets" fallback={[30, 60]} suffix="min" />
      </Group>
    </>
  );
}

function ShortcutsSection() {
  const hotkey = useSetting<string>("general.hotkey", "Ctrl+Alt+N");
  return (
    <>
      <Group title="Global">
        <Row label="Open / close the notch">
          <kbd className="field font-mono text-[12.5px]">{hotkey}</kbd>
        </Row>
      </Group>
      <Group title="While the notch is open">
        <Row label="Previous / next page">
          <kbd className="field font-mono text-[12.5px]">← / →</kbd>
          <span className="text-[12px] opacity-55">or mouse wheel</span>
        </Row>
        <Row label="Close">
          <kbd className="field font-mono text-[12.5px]">Esc</kbd>
        </Row>
        {PAGES.map((p) => (
          <Row key={p.id} label={p.label}>
            <kbd className="field font-mono text-[12.5px]">{p.key}</kbd>
          </Row>
        ))}
      </Group>
    </>
  );
}

function DataSection() {
  const [msg, setMsg] = useState("");
  const run = (p: Promise<unknown>, ok: string) => p.then(() => setMsg(ok)).catch((e) => setMsg(errText(e)));
  return (
    <Group>
      <Row label="Export everything" hint="Tasks, notes, logs, shelf, settings… in one JSON file.">
        <button
          className="btn"
          onClick={async () => {
            const path = await save({ defaultPath: `wntr-backup-${new Date().toISOString().slice(0, 10)}.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
            if (path) run(call("data_export", { path }), `Exported to ${path}`);
          }}
        >
          Export…
        </button>
      </Row>
      <Row label="Import" hint="Replaces all current data with the file's contents.">
        <button
          className="btn"
          onClick={async () => {
            const path = await open({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
            if (typeof path === "string" && window.confirm("Replace all WNTR data with this backup?")) run(call("data_import", { path }), "Imported.");
          }}
        >
          Import…
        </button>
      </Row>
      <Row label="Data folder" hint="%APPDATA%\\WNTR">
        <button className="btn" onClick={() => call("open_data_folder")}>
          Open
        </button>
      </Row>
      <Row label="Reset" hint="Deletes all WNTR data and restores default settings.">
        <button
          className="btn"
          style={{ color: "#fca5a5" }}
          onClick={() => window.confirm("Delete all WNTR data? This can't be undone.") && run(call("data_reset"), "Reset done.")}
        >
          Reset…
        </button>
      </Row>
      {msg && <div className="pb-3 text-[12.5px] opacity-70">{msg}</div>}
    </Group>
  );
}

function AboutSection() {
  const [version, setVersion] = useState("");
  const [update, setUpdate] = useState<{ available: boolean; version?: string; notes?: string } | null>(null);
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void call<string>("app_version").then(setVersion);
  }, []);
  const check = async () => {
    setBusy(true);
    setMsg("");
    try {
      setUpdate(await call("update_check"));
    } catch (e) {
      setMsg(errText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Group>
        <Row label="WNTR" hint={`Version ${version}`}>
          <button className="btn" onClick={() => call("open_target", { target: "https://github.com/Akhil14324/wntr" })}>
            GitHub
          </button>
        </Row>
        <Row
          label="Updates"
          hint={msg || (update ? (update.available ? `Version ${update.version} is available.` : "You're up to date.") : "Checks GitHub Releases. Never installs without asking.")}
        >
          {update?.available ? (
            <button
              className="btn btn-accent"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                setMsg("Downloading…");
                try {
                  await call("update_install");
                } catch (e) {
                  setMsg(errText(e));
                  setBusy(false);
                }
              }}
            >
              Install and restart
            </button>
          ) : (
            <button className="btn" disabled={busy} onClick={check}>
              {busy ? "Checking…" : "Check now"}
            </button>
          )}
        </Row>
        <Row label="Licenses" hint="WNTR is MIT licensed. Built with Tauri, React, Framer Motion, Recharts and Lucide (MIT/ISC). Lyrics by LRCLIB, weather by Open-Meteo (CC BY 4.0)." >
          <span />
        </Row>
      </Group>
      <Group>
        <Row label="Exit WNTR" hint="Closes the notch and all reminders.">
          <button className="btn" style={{ color: "#fca5a5" }} onClick={() => call("app_quit")}>
            <Power size={14} /> Exit
          </button>
        </Row>
      </Group>
    </>
  );
}

const CONTENT: Record<string, () => React.ReactNode> = {
  general: General,
  appearance: Appearance,
  music: MusicSection,
  focus: FocusSection,
  water: WaterSection,
  tasks: TasksSection,
  calendar: CalendarSection,
  status: StatusSection,
  clipboard: ClipboardSection,
  notes: NotesSection,
  weather: WeatherSection,
  dnd: DndSection,
  shortcuts: ShortcutsSection,
  data: DataSection,
  about: AboutSection,
};

export default function SettingsApp() {
  const initial = window.location.hash.split("/")[1] || "general";
  const [section, setSection] = useState(SECTIONS.some((s) => s.id === initial) ? initial : "general");
  const settings = useStore((s) => s.settings);
  const accent = useSetting<string>("appearance.accent", "#7dd3fc");

  // The settings window always uses the dark palette; only the accent follows the theme.
  useLayoutEffect(() => {
    document.documentElement.style.setProperty("--accent", accent);
  }, [accent]);

  useEffect(() => on<string>("settings-section", (s) => SECTIONS.some((x) => x.id === s) && setSection(s)), []);

  const Content = CONTENT[section];
  const current = SECTIONS.find((s) => s.id === section)!;
  return (
    <div className="flex h-full">
      <nav className="scroll-thin flex w-[220px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-white/5 p-3">
        <div className="mb-3 px-2 pt-1 font-display text-[20px] font-bold tracking-tight">WNTR</div>
        {SECTIONS.map((s) => {
          const Icon = s.icon;
          return (
            <button
              key={s.id}
              onClick={() => setSection(s.id)}
              className={`flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition-colors ${
                section === s.id ? "bg-white/10 font-semibold" : "opacity-75 hover:bg-white/5 hover:opacity-100"
              }`}
            >
              <Icon size={15} />
              {s.label}
            </button>
          );
        })}
      </nav>
      <main className="scroll-thin flex-1 overflow-y-auto px-8 py-6">
        <h1 className="mb-4 font-display text-[22px] font-semibold">{current.label}</h1>
        {settings ? <Content /> : <div className="opacity-50">Loading…</div>}
      </main>
    </div>
  );
}
