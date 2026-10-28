import { useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import type { PageDef } from "./pages";

/** Dots on the bottom edge; hovering reveals an icon for every page. */
export default function PageDots({
  pages,
  current,
  onJump,
}: {
  pages: PageDef[];
  current?: string;
  onJump: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      className="relative flex h-[18px] shrink-0 items-center justify-center"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <AnimatePresence initial={false} mode="wait">
        {open ? (
          <motion.div
            key="icons"
            className="flex items-center gap-0.5 rounded-full bg-[var(--card)] px-1.5 py-0.5"
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            transition={{ duration: 0.15 }}
          >
            {pages.map((p) => {
              const Icon = p.icon;
              const active = p.id === current;
              return (
                <button
                  key={p.id}
                  title={`${p.label} (${p.key})`}
                  onClick={() => onJump(p.id)}
                  className="flex h-[18px] w-[22px] items-center justify-center rounded-full transition-colors hover:bg-[var(--card-hover)]"
                  style={{ color: active ? "var(--accent)" : "var(--muted)" }}
                >
                  <Icon size={12} />
                </button>
              );
            })}
          </motion.div>
        ) : (
          <motion.div
            key="dots"
            className="flex items-center gap-[5px] py-1"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.12 }}
          >
            {pages.map((p) => (
              <motion.span
                key={p.id}
                layout
                className="block h-[5px] rounded-full"
                animate={{
                  width: p.id === current ? 16 : 5,
                  backgroundColor: p.id === current ? "var(--fg)" : "var(--faint)",
                }}
                transition={{ type: "spring", stiffness: 500, damping: 35 }}
              />
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
