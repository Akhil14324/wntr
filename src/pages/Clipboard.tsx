import { useEffect, useState } from "react";
import { ClipboardList, Pause, Pin, Play, Search, Trash2, X } from "lucide-react";
import { call, on, type ClipItem } from "../lib/api";
import { relativeDateTime } from "../lib/format";
import { useSetting, useStore } from "../lib/store";
import { Empty } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

export default function ClipboardPage() {
  const [items, setItems] = useState<ClipItem[]>([]);
  const [query, setQuery] = useState("");
  const paused = useSetting<boolean>("clipboard.paused", false);
  const setSettings = useStore((s) => s.setSettings);
  const showBanner = useUi((s) => s.showBanner);

  useEffect(() => {
    const load = () => call<ClipItem[]>("clipboard_list", { query }).then(setItems);
    void load();
    return on("clipboard", load);
  }, [query]);

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-1.5">
        <div className="relative flex-1">
          <Search size={12} className="absolute top-1/2 left-2 -translate-y-1/2 text-[var(--faint)]" />
          <input className="field w-full pl-6 text-[12px]" placeholder="Search clipboard history" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <button
          className="btn h-[28px] text-[12px]"
          style={paused ? { color: "var(--warn)" } : {}}
          onClick={() => setSettings({ clipboard: { paused: !paused } })}
          title={paused ? "Resume recording" : "Pause recording (e.g. before copying passwords)"}
        >
          {paused ? <Play size={13} /> : <Pause size={13} />} {paused ? "Paused" : "Pause"}
        </button>
        <button
          className="btn h-[28px] text-[12px]"
          onClick={() => call("clipboard_clear").then(() => showBanner("History cleared (pinned items kept)"))}
          title="Clear history"
        >
          <Trash2 size={13} /> Clear
        </button>
      </div>
      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto">
        {items.length ? (
          items.map((it) => (
            <div
              key={it.id}
              role="button"
              tabIndex={0}
              onClick={() =>
                call<boolean>("clipboard_paste", { id: it.id })
                  .then((pasted) => showBanner(pasted ? "Pasted" : "Copied — paste with Ctrl+V", { tone: "good" }))
                  .catch(bannerError)
              }
              className="group flex items-center gap-2 rounded-lg px-2 py-1 transition-colors hover:bg-[var(--card)]"
            >
              {it.kind === "image" && it.image ? (
                <img src={it.image} alt="" className="h-[34px] max-w-[90px] rounded-md object-cover" />
              ) : (
                <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{it.text?.replace(/\s+/g, " ")}</span>
              )}
              {it.kind === "image" && <span className="flex-1 text-[11.5px] text-[var(--muted)]">Image</span>}
              <span className="shrink-0 text-[10.5px] text-[var(--faint)]">{relativeDateTime(it.createdAt)}</span>
              <button
                className="icon-btn h-6 w-6 shrink-0"
                style={{ opacity: it.pinned ? 1 : undefined, color: it.pinned ? "var(--accent)" : undefined }}
                onClick={(e) => {
                  e.stopPropagation();
                  void call("clipboard_pin", { id: it.id, pinned: !it.pinned });
                }}
                aria-label={it.pinned ? "Unpin" : "Pin"}
              >
                <Pin size={12} fill={it.pinned ? "currentColor" : "none"} />
              </button>
              <button
                className="icon-btn h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100"
                onClick={(e) => {
                  e.stopPropagation();
                  void call("clipboard_delete", { id: it.id });
                }}
                aria-label="Delete"
              >
                <X size={12} />
              </button>
            </div>
          ))
        ) : (
          <Empty
            icon={<ClipboardList size={24} />}
            title={query ? "No matches" : paused ? "Recording paused" : "Nothing copied yet"}
            hint={query ? undefined : "Text and images you copy show up here. Stored only on this PC."}
          />
        )}
      </div>
    </div>
  );
}
