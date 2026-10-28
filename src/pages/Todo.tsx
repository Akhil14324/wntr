import { useEffect, useState } from "react";
import { Check, Flag, ListTodo, Pencil, Repeat, Trash2, X } from "lucide-react";
import { call, on, type Task, type TaskLists } from "../lib/api";
import { clock, fromLocalInput, relativeDateTime, toLocalInput } from "../lib/format";
import { Empty, TASK_COLORS, Tabs } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

type View = "today" | "upcoming" | "done";

function endOfToday() {
  const d = new Date();
  d.setHours(23, 59, 59, 999);
  return d.getTime();
}

/** "Call mom @17:30 !" → title, due today 17:30, high priority. */
export function parseQuickAdd(input: string) {
  let title = input.trim();
  let priority = 0;
  let dueAt: number | null = null;
  if (/\s!$/.test(title) || title.endsWith("!!")) {
    priority = 2;
    title = title.replace(/\s*!+$/, "");
  }
  const m = title.match(/\s@(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$/i);
  if (m) {
    let h = Number(m[1]);
    const min = m[2] ? Number(m[2]) : 0;
    const ap = m[3]?.toLowerCase();
    if (ap === "pm" && h < 12) h += 12;
    if (ap === "am" && h === 12) h = 0;
    if (h < 24 && min < 60) {
      const d = new Date();
      d.setHours(h, min, 0, 0);
      if (d.getTime() < Date.now()) d.setDate(d.getDate() + 1);
      dueAt = d.getTime();
      title = title.slice(0, m.index).trim();
    }
  }
  return { title, priority, dueAt };
}

function Editor({ task, onClose }: { task: Partial<Task>; onClose: () => void }) {
  const [t, setT] = useState<Partial<Task>>({ color: "blue", priority: 0, repeat: "none", notes: "", ...task });
  const hold = useUi((s) => s.hold);
  useEffect(() => hold(), [hold]);
  const save = async () => {
    try {
      if (t.repeat !== "none" && !t.dueAt) throw new Error("Repeating tasks need a due date and time");
      await call("task_save", { task: t });
      onClose();
    } catch (e) {
      bannerError(e);
    }
  };
  return (
    <div className="absolute inset-0 z-10 flex flex-col gap-1.5 bg-[var(--notch-bg)] text-[12px]">
      <div className="flex items-center gap-2">
        <input
          autoFocus
          className="field flex-1 text-[13px] font-medium"
          placeholder="Task title"
          value={t.title ?? ""}
          onChange={(e) => setT({ ...t, title: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && save()}
        />
        <button className="btn btn-accent" onClick={save}>
          <Check size={14} /> Save
        </button>
        <button className="icon-btn" onClick={onClose} aria-label="Cancel">
          <X size={15} />
        </button>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="datetime-local"
          className="field text-[12px]"
          value={toLocalInput(t.dueAt ?? null)}
          onChange={(e) => setT({ ...t, dueAt: fromLocalInput(e.target.value) })}
        />
        <select className="field text-[12px]" value={t.repeat} onChange={(e) => setT({ ...t, repeat: e.target.value as Task["repeat"] })}>
          <option value="none">No repeat</option>
          <option value="daily">Daily</option>
          <option value="weekdays">Weekdays</option>
          <option value="weekly">Weekly</option>
        </select>
        <select className="field text-[12px]" value={t.priority} onChange={(e) => setT({ ...t, priority: Number(e.target.value) })}>
          <option value={0}>No priority</option>
          <option value={1}>Low</option>
          <option value={2}>High</option>
        </select>
        <div className="flex gap-1">
          {Object.entries(TASK_COLORS).map(([name, hex]) => (
            <button
              key={name}
              onClick={() => setT({ ...t, color: name })}
              aria-label={name}
              className="h-[18px] w-[18px] rounded-full"
              style={{ background: hex, outline: t.color === name ? "2px solid var(--fg)" : "none", outlineOffset: 1 }}
            />
          ))}
        </div>
      </div>
      <textarea
        className="field min-h-0 flex-1 resize-none text-[12px]"
        placeholder="Notes"
        value={t.notes ?? ""}
        onChange={(e) => setT({ ...t, notes: e.target.value })}
      />
    </div>
  );
}

export default function TodoPage() {
  const [lists, setLists] = useState<TaskLists | null>(null);
  const [view, setView] = useState<View>("today");
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState<Partial<Task> | null>(null);
  const showBanner = useUi((s) => s.showBanner);

  useEffect(() => {
    const load = () => call<TaskLists>("tasks_list").then(setLists);
    void load();
    const a = on("tasks", load);
    const b = on("day-changed", load);
    return () => {
      a();
      b();
    };
  }, []);

  if (!lists) return null;
  const eod = endOfToday();
  const today = lists.open.filter((t) => !t.dueAt || t.dueAt <= eod);
  const upcoming = lists.open.filter((t) => t.dueAt && t.dueAt > eod);
  const shown = view === "today" ? today : view === "upcoming" ? upcoming : [];

  const add = async () => {
    const q = parseQuickAdd(draft);
    if (!q.title) return;
    try {
      await call("task_save", { task: { title: q.title, dueAt: q.dueAt, priority: q.priority } });
      setDraft("");
    } catch (e) {
      bannerError(e);
    }
  };

  const complete = async (t: Task) => {
    try {
      const completionId = await call<number>("task_complete", { id: t.id });
      showBanner(t.repeat !== "none" ? `Done — next one scheduled` : `Completed “${t.title}”`, {
        tone: "good",
        action: { label: "Undo", run: () => call("task_undo_complete", { completionId }) },
      });
    } catch (e) {
      bannerError(e);
    }
  };

  return (
    <div className="relative flex h-full flex-col gap-2">
      <div className="flex items-center gap-2">
        <Tabs<View>
          value={view}
          onChange={setView}
          options={[
            { value: "today", label: `Today ${today.length || ""}`.trim() },
            { value: "upcoming", label: `Upcoming ${upcoming.length || ""}`.trim() },
            { value: "done", label: "Done" },
          ]}
        />
        <span className="text-[11px] text-[var(--good)]">{lists.completedToday ? `✓ ${lists.completedToday} today` : ""}</span>
        <input
          className="field ml-auto w-[230px] text-[12px]"
          placeholder="Add a task… (@17:30, ! for high)"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
      </div>

      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto">
        {view === "done" ? (
          lists.done.length ? (
            lists.done.map((c) => (
              <div key={c.id} className="flex items-center gap-2 rounded-lg px-2 py-1 text-[12.5px] hover:bg-[var(--card)]">
                <Check size={14} className="text-[var(--good)]" />
                <span className="min-w-0 flex-1 truncate text-[var(--muted)] line-through">{c.title}</span>
                <span className="text-[11px] text-[var(--faint)]">{relativeDateTime(c.at)}</span>
                <button className="text-[11px] text-[var(--accent)] hover:underline" onClick={() => call("task_undo_complete", { completionId: c.id })}>
                  Undo
                </button>
              </div>
            ))
          ) : (
            <Empty icon={<ListTodo size={24} />} title="Nothing completed yet" />
          )
        ) : shown.length ? (
          shown.map((t) => {
            const overdue = t.dueAt != null && t.dueAt < Date.now();
            return (
              <div key={t.id} className="group flex items-center gap-2 rounded-lg px-1.5 py-1 hover:bg-[var(--card)]">
                <button
                  onClick={() => complete(t)}
                  aria-label="Complete"
                  className="flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full border-2 transition-colors hover:bg-[var(--card-hover)]"
                  style={{ borderColor: TASK_COLORS[t.color] ?? TASK_COLORS.blue }}
                >
                  <Check size={10} className="opacity-0 group-hover:opacity-60" strokeWidth={3} />
                </button>
                <button className="min-w-0 flex-1 text-left" onClick={() => setEditing(t)}>
                  <div className="flex items-center gap-1.5 text-[12.5px]">
                    <span className="truncate">{t.title}</span>
                    {t.priority === 2 && <Flag size={11} className="shrink-0 text-[var(--bad)]" fill="currentColor" />}
                    {t.priority === 1 && <Flag size={11} className="shrink-0 text-[var(--muted)]" />}
                    {t.repeat !== "none" && <Repeat size={11} className="shrink-0 text-[var(--muted)]" />}
                  </div>
                </button>
                {t.dueAt && (
                  <span className="shrink-0 text-[11px] tabular" style={{ color: overdue ? "var(--bad)" : "var(--muted)" }}>
                    {view === "today" && t.dueAt > Date.now() - 86400000 ? clock(new Date(t.dueAt)) : relativeDateTime(t.dueAt)}
                  </span>
                )}
                <button className="icon-btn h-6 w-6 opacity-0 group-hover:opacity-100" onClick={() => setEditing(t)} aria-label="Edit">
                  <Pencil size={12} />
                </button>
                <button className="icon-btn h-6 w-6 opacity-0 group-hover:opacity-100" onClick={() => call("task_delete", { id: t.id })} aria-label="Delete">
                  <Trash2 size={12} />
                </button>
              </div>
            );
          })
        ) : (
          <Empty
            icon={<ListTodo size={24} />}
            title={view === "today" ? "All clear for today" : "Nothing upcoming"}
            hint="Type a task above and press Enter."
          />
        )}
      </div>
      {editing && <Editor task={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
