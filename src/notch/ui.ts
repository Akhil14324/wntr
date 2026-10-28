import { create } from "zustand";

export interface Banner {
  id: number;
  text: string;
  tone?: "info" | "error" | "good";
  action?: { label: string; run: () => void };
}

export interface MenuItem {
  label: string;
  run: () => void;
  danger?: boolean;
  disabled?: boolean;
}

interface Ui {
  banner: Banner | null;
  /** Context menu anchored at window coordinates. */
  menu: { x: number; y: number; items: MenuItem[] } | null;
  /** Something inside the notch needs it to stay open (drag, menu, editor). */
  holds: number;
  page: string;
  /** Width the collapsed pill wants (grows with lyric lines); null = default. */
  pillW: number | null;
  setPillW: (w: number | null) => void;
  showBanner: (text: string, opts?: { tone?: Banner["tone"]; action?: Banner["action"]; ms?: number }) => void;
  openMenu: (x: number, y: number, items: MenuItem[]) => void;
  closeMenu: () => void;
  hold: () => () => void;
  setPage: (p: string) => void;
}

let bannerTimer = 0;

export const useUi = create<Ui>((set) => ({
  banner: null,
  menu: null,
  holds: 0,
  page: "",
  pillW: null,
  setPillW: (w) => set((s) => (s.pillW === w ? s : { pillW: w })),
  showBanner: (text, opts) => {
    const id = Date.now();
    set({ banner: { id, text, tone: opts?.tone, action: opts?.action } });
    window.clearTimeout(bannerTimer);
    bannerTimer = window.setTimeout(() => {
      set((s) => (s.banner?.id === id ? { banner: null } : s));
    }, opts?.ms ?? (opts?.action ? 5000 : 2600));
  },
  openMenu: (x, y, items) => set({ menu: { x, y, items } }),
  closeMenu: () => set({ menu: null }),
  hold: () => {
    set((s) => ({ holds: s.holds + 1 }));
    let released = false;
    return () => {
      if (released) return;
      released = true;
      set((s) => ({ holds: Math.max(0, s.holds - 1) }));
    };
  },
  setPage: (p) => set({ page: p }),
}));

export function bannerError(e: unknown) {
  const text = typeof e === "string" ? e : e && typeof e === "object" && "message" in e ? String((e as any).message) : String(e);
  useUi.getState().showBanner(text, { tone: "error", ms: 4000 });
}
