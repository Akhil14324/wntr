import { AnimatePresence, motion } from "framer-motion";
import { useUi } from "./ui";

/** Small in-notch toast at the bottom of the expanded panel (undo, errors). */
export default function BannerView() {
  const banner = useUi((s) => s.banner);
  return (
    <AnimatePresence>
      {banner && (
        <motion.div
          key={banner.id}
          className="absolute right-0 bottom-5 left-0 z-30 flex justify-center"
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 8 }}
          transition={{ duration: 0.18 }}
        >
          <div
            className="flex max-w-[86%] items-center gap-3 rounded-full px-3.5 py-1.5 text-[12px] font-medium shadow-xl"
            style={{
              background: banner.tone === "error" ? "#3b1214" : "#1f1f25",
              color: banner.tone === "error" ? "#fecaca" : banner.tone === "good" ? "#bbf7d0" : "#f5f5f7",
              border: "1px solid rgba(255,255,255,.08)",
            }}
          >
            <span className="truncate">{banner.text}</span>
            {banner.action && (
              <button
                className="shrink-0 font-semibold text-[var(--accent)] hover:underline"
                onClick={() => {
                  banner.action?.run();
                  useUi.setState({ banner: null });
                }}
              >
                {banner.action.label}
              </button>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
