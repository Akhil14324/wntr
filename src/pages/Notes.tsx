import { useEffect, useRef, useState } from "react";
import { ArrowLeft, Download, Pin, PinOff, Search, StickyNote, Trash2 } from "lucide-react";
import { call, on, type Note } from "../lib/api";
import { relativeDateTime } from "../lib/format";
import { Empty } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

/** Very light Markdown preview: headings, bold, italics, code, bullets, checkboxes. */
export function Preview({ text, lines = 2 }: { text: string; lines?: number }) {
  const rows = text.split(/\r?\n/).filter((l) => l.trim()).slice(0, lines);
  return (
    <>
      {rows.map((row, i) => {
        const heading = /^#{1,6}\s/.test(row);
        const bullet = row.match(/^\s*[-*]\s+(\[( |x)\]\s+)?/i);
        const content = row.replace(/^#{1,6}\s/, "").replace(/^\s*[-*]\s+(\[( |x)\]\s+)?/i, "");
        const parts = content.split(/(\*\*[^*]+\*\*|`[^`]+`|_[^_]+_|\*[^*]+\*)/g);
        return (
          <div key={i} className={`truncate ${heading ? "font-semibold" : ""}`}>
            {bullet && <span className="mr-1 text-[var(--muted)]">{bullet[2] ? (bullet[2].toLowerCase() === "x" ? "☑" : "☐") : "•"}</span>}
            {parts.map((p, j) =>
              p.startsWith("**") ? (
                <b key={j}>{p.slice(2, -2)}</b>
              ) : p.startsWith("`") ? (
                <code key={j} className="rounded bg-[var(--card-hover)] px-1 text-[0.92em]">
                  {p.slice(1, -1)}
                </code>
              ) : (p.startsWith("_") || p.startsWith("*")) && p.length > 2 ? (
                <i key={j}>{p.slice(1, -1)}</i>
              ) : (
                <span key={j}>{p}</span>
              ),
            )}
          </div>
        );
      })}
    </>
  );
}

function Editor({ note, onBack }: { note: Note; onBack: () => void }) {
  const [body, setBody] = useState(note.body);
  const hold = useUi((s) => s.hold);
  const saved = useRef(note.body);
  useEffect(() => hold(), [hold]);
  useEffect(() => {
    const t = window.setTimeout(() => {
      if (body !== saved.current && body.trim()) {
        saved.current = body;
        void call("note_update", { id: note.id, body });
      }
    }, 400);
    return () => window.clearTimeout(t);
  }, [body, note.id]);
  const exportOne = async () => {
    try {
      const paths = await call<string[]>("notes_export", { ids: [note.id] });
      useUi.getState().showBanner(`Exported to ${paths[0]}`, { tone: "good", ms: 4000 });
    } catch (e) {
      bannerError(e);
    }
  };
  return (
    <div className="absolute inset-0 z-10 flex flex-col gap-1.5 bg-[var(--notch-bg)]">
      <div className="flex items-center gap-1">
        <button className="btn h-[26px] text-[12px]" onClick={onBack}>
          <ArrowLeft size={13} /> Notes
        </button>
        <span className="ml-1 text-[11px] text-[var(--muted)]">Edited {relativeDateTime(note.updatedAt)}</span>
        <button className="icon-btn ml-auto h-7 w-7" onClick={exportOne} title="Export as .md" aria-label="Export">
          <Download size={14} />
        </button>
        <button
          className="icon-btn h-7 w-7"
          onClick={() => call("note_delete", { id: note.id }).then(onBack)}
          title="Delete"
          aria-label="Delete"
        >
          <Trash2 size={14} />
        </button>
      </div>
      <textarea
        autoFocus
        className="field scroll-thin min-h-0 flex-1 resize-none font-mono text-[12.5px] leading-relaxed"
        value={body}
        onChange={(e) => setBody(e.target.value)}
        onBlur={() => {
          if (body !== saved.current && body.trim()) {
            saved.current = body;
            void call("note_update", { id: note.id, body });
          }
        }}
      />
    </div>
  );
}

export default function NotesPage() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState("");
  const [open, setOpen] = useState<Note | null>(null);

  useEffect(() => {
    const load = () => call<Note[]>("notes_list", { query }).then(setNotes);
    void load();
    return on("notes", load);
  }, [query]);

  const add = async () => {
    if (!draft.trim()) return;
    try {
      await call("note_add", { body: draft });
      setDraft("");
    } catch (e) {
      bannerError(e);
    }
  };

  const exportAll = async () => {
    try {
      const paths = await call<string[]>("notes_export", {});
      useUi.getState().showBanner(`Exported ${paths.length} note${paths.length === 1 ? "" : "s"}`, { tone: "good" });
    } catch (e) {
      bannerError(e);
    }
  };

  return (
    <div className="relative flex h-full flex-col gap-2">
      <div className="flex items-center gap-1.5">
        <input
          className="field min-w-0 flex-1 text-[12.5px]"
          placeholder="Quick note… press Enter to save"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && add()}
        />
        <div className="relative">
          <Search size={12} className="absolute top-1/2 left-2 -translate-y-1/2 text-[var(--faint)]" />
          <input className="field w-[150px] pl-6 text-[12px]" placeholder="Search" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <button className="icon-btn" onClick={exportAll} title="Export all notes as .md" aria-label="Export all">
          <Download size={15} />
        </button>
      </div>
      <div className="scroll-thin grid min-h-0 flex-1 auto-rows-min grid-cols-3 gap-1.5 overflow-y-auto">
        {notes.map((n) => (
          <div
            key={n.id}
            role="button"
            tabIndex={0}
            onClick={() => setOpen(n)}
            className="group relative h-[64px] overflow-hidden rounded-xl bg-[var(--card)] p-2 text-left text-[11.5px] leading-snug transition-colors hover:bg-[var(--card-hover)]"
          >
            <Preview text={n.body} lines={3} />
            <button
              className="absolute top-1 right-1 rounded-full p-1 opacity-0 transition-opacity group-hover:opacity-100 hover:bg-[var(--card-hover)]"
              style={n.pinned ? { opacity: 1, color: "var(--accent)" } : {}}
              onClick={(e) => {
                e.stopPropagation();
                void call("note_pin", { id: n.id, pinned: !n.pinned });
              }}
              aria-label={n.pinned ? "Unpin" : "Pin"}
            >
              {n.pinned ? <Pin size={11} fill="currentColor" /> : <PinOff size={11} />}
            </button>
          </div>
        ))}
        {!notes.length && (
          <div className="col-span-3 h-[120px]">
            <Empty icon={<StickyNote size={24} />} title={query ? "No matching notes" : "No notes yet"} hint={query ? undefined : "Type above and press Enter."} />
          </div>
        )}
      </div>
      {open && <Editor note={open} onBack={() => setOpen(null)} />}
    </div>
  );
}
