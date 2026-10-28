import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  ClipboardPaste,
  Copy,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileSpreadsheet,
  FileText,
  FileVideo,
  Folder,
  FolderOpen,
  Plus,
  SquareCheck,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, errText, on, type ShelfItem } from "../lib/api";
import { bytes } from "../lib/format";
import { Empty } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

const thumbs = new Map<number, string | null>();

function useThumb(item: ShelfItem) {
  const isImage = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"].includes(item.ext);
  const [src, setSrc] = useState<string | null>(thumbs.get(item.id) ?? null);
  useEffect(() => {
    if (!isImage || item.missing || thumbs.has(item.id)) return;
    let alive = true;
    call<string | null>("shelf_thumb", { id: item.id }).then((s) => {
      thumbs.set(item.id, s);
      if (alive) setSrc(s);
    });
    return () => {
      alive = false;
    };
  }, [item.id, item.missing, isImage]);
  return src;
}

function FileGlyph({ item }: { item: ShelfItem }) {
  const e = item.ext;
  const size = 30;
  if (item.kind === "text") return <FileText size={size} className="text-[#a5b4fc]" />;
  if (item.isDir) return <Folder size={size} className="text-[#fcd34d]" />;
  if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "heic", "ico"].includes(e)) return <FileImage size={size} className="text-[#f9a8d4]" />;
  if (["zip", "rar", "7z", "tar", "gz"].includes(e)) return <FileArchive size={size} className="text-[#fdba74]" />;
  if (["mp3", "wav", "flac", "m4a", "ogg", "aac"].includes(e)) return <FileAudio size={size} className="text-[#86efac]" />;
  if (["mp4", "mkv", "mov", "avi", "webm"].includes(e)) return <FileVideo size={size} className="text-[#fca5a5]" />;
  if (["xls", "xlsx", "csv", "ods"].includes(e)) return <FileSpreadsheet size={size} className="text-[#6ee7b7]" />;
  if (["js", "ts", "tsx", "rs", "py", "json", "html", "css", "c", "cpp", "java", "go"].includes(e)) return <FileCode size={size} className="text-[#7dd3fc]" />;
  if (["pdf", "doc", "docx", "txt", "md", "rtf", "ppt", "pptx"].includes(e)) return <FileText size={size} className="text-[#cbd5e1]" />;
  return <File size={size} className="text-[#cbd5e1]" />;
}

function Card({
  item,
  selected,
  onPointerDown,
  onClick,
  onDoubleClick,
  onContextMenu,
}: {
  item: ShelfItem;
  selected: boolean;
  onPointerDown: (e: React.PointerEvent) => void;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  const thumb = useThumb(item);
  return (
    <motion.div
      layout
      initial={{ opacity: 0, scale: 0.8 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.6, y: -12, transition: { duration: 0.18 } }}
      transition={{ type: "spring", stiffness: 500, damping: 34 }}
      onPointerDown={onPointerDown}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      title={item.kind === "text" ? item.text ?? "" : `${item.path ?? item.name}${item.size != null ? `\n${bytes(item.size)}` : ""}`}
      className="relative flex h-[104px] w-[86px] shrink-0 flex-col items-center gap-1.5 rounded-[14px] p-1.5 transition-colors"
      style={{
        background: selected ? "color-mix(in srgb, var(--accent) 22%, transparent)" : "var(--card)",
        outline: selected ? "1.5px solid var(--accent)" : "none",
      }}
    >
      <div className="flex h-[60px] w-[72px] items-center justify-center overflow-hidden rounded-[9px]">
        {item.kind === "text" ? (
          <div className="h-full w-full overflow-hidden rounded-[9px] bg-[var(--card-hover)] p-1.5 text-[8.5px] leading-tight text-[var(--muted)]">
            {item.text?.slice(0, 160)}
          </div>
        ) : thumb ? (
          <img src={thumb} alt="" className="h-full w-full object-cover" draggable={false} />
        ) : (
          <FileGlyph item={item} />
        )}
      </div>
      <div className="w-full truncate text-center text-[10.5px] leading-tight font-medium">{item.name}</div>
      {item.missing && (
        <div className="absolute top-1 right-1 rounded-full bg-[#3b1214] p-0.5" title="File was moved or deleted">
          <TriangleAlert size={12} className="text-[var(--warn)]" />
        </div>
      )}
    </motion.div>
  );
}

export default function ShelfPage() {
  const [items, setItems] = useState<ShelfItem[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const anchor = useRef<number | null>(null);
  const press = useRef<{ id: number; x: number; y: number; started: boolean } | null>(null);
  const strip = useRef<HTMLDivElement>(null);
  const dragEnded = useRef(0);
  const { showBanner, openMenu, hold } = useUi.getState();

  const refresh = useCallback(() => call<ShelfItem[]>("shelf_list").then(setItems), []);
  useEffect(() => {
    void refresh();
    return on<ShelfItem[]>("shelf", (list) => {
      setItems(list);
      setSelected((s) => new Set([...s].filter((id) => list.some((i) => i.id === id))));
    });
  }, [refresh]);

  const selectedIds = (fallback?: number) => {
    if (fallback != null && !selected.has(fallback)) return [fallback];
    return selected.size ? [...selected] : fallback != null ? [fallback] : [];
  };

  const locate = async (item: ShelfItem) => {
    const path = await open({ multiple: false, directory: item.isDir, title: `Locate ${item.name}` });
    if (typeof path === "string") await call("shelf_locate", { id: item.id, path }).catch(bannerError);
  };

  const copyFiles = async (ids: number[]) => {
    try {
      const what = await call<string>("shelf_copy", { ids });
      showBanner(what === "files" ? `Copied ${ids.length > 1 ? `${ids.length} files` : "file"} — paste with Ctrl+V` : "Copied text", { tone: "good" });
    } catch (e) {
      bannerError(e);
    }
  };

  const remove = (ids: number[]) => call("shelf_remove", { ids }).catch(bannerError);

  // Native drag-out starts once the pointer moves a few pixels with the button held.
  const startDrag = async (id: number) => {
    const ids = selectedIds(id);
    const list = items.filter((i) => ids.includes(i.id));
    const missing = list.find((i) => i.missing);
    if (missing) {
      showBanner(`"${missing.name}" was moved or deleted`, {
        tone: "error",
        action: { label: "Locate", run: () => locate(missing) },
      });
      return;
    }
    const release = hold();
    try {
      const res = await call<{ effect: string; removed: number[] }>("shelf_drag_out", { ids });
      if (res.removed.length) showBanner(`Moved ${res.removed.length > 1 ? `${res.removed.length} files` : "file"} out of the shelf`);
    } catch (e) {
      showBanner(errText(e), { tone: "error" });
    } finally {
      dragEnded.current = Date.now();
      release();
    }
  };

  useEffect(() => {
    const move = (e: PointerEvent) => {
      const p = press.current;
      if (!p || p.started || !(e.buttons & 1)) return;
      if (Math.hypot(e.clientX - p.x, e.clientY - p.y) > 6) {
        p.started = true;
        void startDrag(p.id);
      }
    };
    const up = () => {
      press.current = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("blur", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("blur", up);
    };
  });

  const onClick = (e: React.MouseEvent, item: ShelfItem) => {
    if (press.current?.started || Date.now() - dragEnded.current < 400) return;
    const idx = items.findIndex((i) => i.id === item.id);
    if (e.shiftKey && anchor.current != null) {
      const a = items.findIndex((i) => i.id === anchor.current);
      const [lo, hi] = a < idx ? [a, idx] : [idx, a];
      setSelected(new Set(items.slice(lo, hi + 1).map((i) => i.id)));
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      setSelected((s) => {
        const n = new Set(s);
        if (n.has(item.id)) n.delete(item.id);
        else n.add(item.id);
        return n;
      });
    } else {
      setSelected(new Set([item.id]));
      if (item.kind === "text" && item.text) {
        void call("shelf_copy", { ids: [item.id] }).then(() => showBanner("Copied text", { tone: "good" }));
      }
    }
    anchor.current = item.id;
  };

  const menuFor = (e: React.MouseEvent, item: ShelfItem) => {
    e.preventDefault();
    if (!selected.has(item.id)) setSelected(new Set([item.id]));
    const ids = selected.has(item.id) ? [...selected] : [item.id];
    const isFile = item.kind !== "text";
    openMenu(e.clientX, e.clientY, [
      { label: isFile ? "Open" : "Copy text", run: () => call("shelf_open", { id: item.id }).catch(bannerError), disabled: item.missing },
      ...(isFile
        ? [
            { label: "Show in Explorer", run: () => call("shelf_reveal", { id: item.id }).catch(bannerError) },
            { label: ids.length > 1 ? `Copy ${ids.length} files` : "Copy file", run: () => copyFiles(ids), disabled: item.missing },
            { label: "Copy path", run: () => call("shelf_copy_path", { ids }).catch(bannerError) },
          ]
        : []),
      ...(item.missing ? [{ label: "Locate…", run: () => locate(item) }] : []),
      { label: ids.length > 1 ? `Remove ${ids.length} items` : "Remove", run: () => remove(ids), danger: true },
    ]);
  };

  const addFiles = async () => {
    const release = hold();
    try {
      const picked = await open({ multiple: true, title: "Add files to shelf" });
      const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      if (paths.length) await call("shelf_add_paths", { paths });
    } finally {
      release();
    }
  };

  const sel = [...selected];
  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-1.5 text-[11.5px]">
        <span className="mr-1 font-semibold">Shelf</span>
        <span className="text-[var(--muted)]">
          {items.length} item{items.length === 1 ? "" : "s"}
          {sel.length ? ` · ${sel.length} selected` : ""}
        </span>
        <div className="ml-auto flex gap-1">
          <button className="btn h-[26px] px-2" onClick={addFiles} title="Add files">
            <Plus size={13} />
          </button>
          <button
            className="btn h-[26px] px-2"
            title="Add clipboard (text, image or files)"
            onClick={() => call("shelf_add_clipboard").catch(bannerError)}
          >
            <ClipboardPaste size={13} />
          </button>
          <button
            className="btn h-[26px] px-2"
            title="Select all"
            disabled={!items.length}
            onClick={() => setSelected(sel.length === items.length ? new Set() : new Set(items.map((i) => i.id)))}
          >
            <SquareCheck size={13} />
          </button>
          <button className="btn h-[26px] px-2" title="Copy (paste with Ctrl+V anywhere)" disabled={!sel.length} onClick={() => copyFiles(sel)}>
            <Copy size={13} />
          </button>
          <button className="btn h-[26px] px-2" title="Remove selected" disabled={!sel.length} onClick={() => remove(sel)}>
            <Trash2 size={13} />
          </button>
          <button
            className="btn h-[26px] px-2 text-[11px]"
            disabled={!items.length}
            onClick={() => {
              const before = items.length;
              void call("shelf_clear").then(() => showBanner(`Cleared ${before} item${before === 1 ? "" : "s"}`));
            }}
          >
            Clear
          </button>
        </div>
      </div>

      {items.length === 0 ? (
        <div className="flex-1 rounded-2xl border border-dashed border-[var(--line)]">
          <Empty
            icon={<FolderOpen size={26} />}
            title="Drop files here"
            hint="Drag files from Explorer, images from your browser, or add the clipboard. Drag them back out into any app."
          />
        </div>
      ) : (
        <div
          ref={strip}
          className="scroll-thin flex min-h-0 flex-1 items-start gap-2 overflow-x-auto overflow-y-hidden pb-1"
          onWheel={(e) => {
            const el = strip.current;
            if (el && el.scrollWidth > el.clientWidth) {
              el.scrollLeft += e.deltaY + e.deltaX;
              e.stopPropagation();
            }
          }}
          onClick={(e) => e.target === e.currentTarget && setSelected(new Set())}
        >
          <AnimatePresence initial={false}>
            {items.map((item) => (
              <Card
                key={item.id}
                item={item}
                selected={selected.has(item.id)}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  press.current = { id: item.id, x: e.clientX, y: e.clientY, started: false };
                }}
                onClick={(e) => onClick(e, item)}
                onDoubleClick={() => (item.missing ? locate(item) : call("shelf_open", { id: item.id }).catch(bannerError))}
                onContextMenu={(e) => menuFor(e, item)}
              />
            ))}
          </AnimatePresence>
        </div>
      )}
    </div>
  );
}
