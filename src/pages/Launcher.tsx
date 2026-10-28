import { useEffect, useState } from "react";
import { AppWindow, Folder, FolderOpen, Globe, Plus, Rocket } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, on, type LaunchItem } from "../lib/api";
import { Empty } from "../components/ui";
import { bannerError, useUi } from "../notch/ui";

const ICON = { url: Globe, folder: Folder, app: AppWindow };

export default function LauncherPage() {
  const [items, setItems] = useState<LaunchItem[]>([]);
  const [target, setTarget] = useState("");
  const { openMenu, hold } = useUi.getState();

  useEffect(() => {
    const load = () => call<LaunchItem[]>("launcher_list").then(setItems);
    void load();
    return on("launcher", load);
  }, []);

  const add = async (t = target) => {
    if (!t.trim()) return;
    await call("launcher_add", { name: null, target: t }).catch(bannerError);
    setTarget("");
  };

  const browse = async (directory: boolean) => {
    const release = hold();
    try {
      const p = await open({ directory, multiple: false, title: directory ? "Pin a folder" : "Pin an app or file" });
      if (typeof p === "string") await add(p);
    } finally {
      release();
    }
  };

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-1.5">
        <input
          className="field min-w-0 flex-1 text-[12.5px]"
          placeholder="Paste a URL, folder or app path and press Enter"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <button className="btn h-[28px] text-[12px]" onClick={() => browse(false)} title="Pin an app or file">
          <Plus size={13} /> App
        </button>
        <button className="btn h-[28px] text-[12px]" onClick={() => browse(true)} title="Pin a folder">
          <FolderOpen size={13} /> Folder
        </button>
      </div>
      <div className="scroll-thin grid min-h-0 flex-1 auto-rows-min grid-cols-6 gap-1.5 overflow-y-auto">
        {items.map((it, i) => {
          const Icon = ICON[it.kind] ?? AppWindow;
          return (
            <button
              key={it.id}
              title={it.target}
              onClick={() => call("open_target", { target: it.target }).catch(bannerError)}
              onContextMenu={(e) =>
                openMenu(e.clientX, e.clientY, [
                  { label: "Open", run: () => call("open_target", { target: it.target }).catch(bannerError) },
                  { label: "Move left", disabled: i === 0, run: () => call("launcher_move", { id: it.id, delta: -1 }) },
                  { label: "Move right", disabled: i === items.length - 1, run: () => call("launcher_move", { id: it.id, delta: 1 }) },
                  { label: "Remove", danger: true, run: () => call("launcher_remove", { id: it.id }) },
                ])
              }
              className="flex h-[70px] flex-col items-center justify-center gap-1.5 rounded-xl bg-[var(--card)] px-1 transition-colors hover:bg-[var(--card-hover)]"
            >
              <span className="flex h-8 w-8 items-center justify-center rounded-[10px] bg-[var(--card-hover)] text-[14px] font-bold uppercase">
                {it.kind === "app" ? it.name.slice(0, 1) : <Icon size={16} />}
              </span>
              <span className="w-full truncate text-center text-[10.5px]">{it.name}</span>
            </button>
          );
        })}
        {!items.length && (
          <div className="col-span-6 h-[120px]">
            <Empty icon={<Rocket size={24} />} title="Pin apps, folders and links" hint="Click a tile to open it. Right-click to reorder or remove." />
          </div>
        )}
      </div>
    </div>
  );
}
