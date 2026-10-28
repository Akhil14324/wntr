import { useEffect, useRef } from "react";
import { motion } from "framer-motion";
import { useUi } from "./ui";

export default function ContextMenu() {
  const menu = useUi((s) => s.menu);
  const close = useUi((s) => s.closeMenu);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu, close]);

  if (!menu) return null;
  const width = 190;
  const x = Math.min(menu.x, window.innerWidth - width - 8);
  const y = Math.min(menu.y, window.innerHeight - menu.items.length * 30 - 16);
  return (
    <motion.div
      ref={ref}
      className="pointer-events-auto fixed z-50 rounded-xl border border-[var(--line)] bg-[#1b1b20] p-1 text-[12.5px] text-white shadow-2xl"
      style={{ left: x, top: y, width }}
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.1 }}
    >
      {menu.items.map((it) => (
        <button
          key={it.label}
          disabled={it.disabled}
          onClick={() => {
            close();
            it.run();
          }}
          className={`block w-full rounded-lg px-2.5 py-1.5 text-left transition-colors hover:bg-white/10 disabled:opacity-40 ${
            it.danger ? "text-[#fca5a5]" : ""
          }`}
        >
          {it.label}
        </button>
      ))}
    </motion.div>
  );
}
