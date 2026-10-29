import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { call, on, type Alert, type Popup } from "../lib/api";
import { chime } from "../lib/chime";
import { useSetting, useStore } from "../lib/store";
import { enabledPages } from "./pages";
import { useUi } from "./ui";
import Collapsed from "./Collapsed";
import Header from "./Header";
import PageDots from "./PageDots";
import { AlertCard, HudView, PopupView } from "./Overlays";
import ContextMenu from "./ContextMenu";
import BannerView from "./BannerView";

/** Logical size of the transparent notch window (matches notch.rs). */
const WIN_W = 820;

export const SPRING = { type: "spring" as const, stiffness: 430, damping: 36, mass: 0.9 };

const SIZES = {
  small: { pillW: 190, pillH: 30, panelW: 540, panelH: 214 },
  medium: { pillW: 216, pillH: 34, panelW: 604, panelH: 236 },
  large: { pillW: 244, pillH: 38, panelW: 680, panelH: 264 },
};
type SizeName = keyof typeof SIZES;

function useSize() {
  const name = useSetting<string>("general.notchSize", "medium");
  const autoLarger = useSetting<boolean>("general.autoLarger", true);
  const customW = useSetting<number>("general.customWidth", 216);
  const customH = useSetting<number>("general.customHeight", 34);
  const monitorWidth = useStore((s) => s.monitorWidth);
  return useMemo(() => {
    if (name === "custom") {
      const k = Math.max(0.7, Math.min(1.4, customW / SIZES.medium.pillW));
      return {
        pillW: customW,
        pillH: customH,
        panelW: Math.min(780, Math.round(SIZES.medium.panelW * k)),
        panelH: Math.min(300, Math.round(SIZES.medium.panelH * Math.max(1, k))),
      };
    }
    let n: SizeName = (name in SIZES ? name : "medium") as SizeName;
    if (autoLarger && monitorWidth > 1920) n = n === "small" ? "medium" : "large";
    return SIZES[n];
  }, [name, autoLarger, customW, customH, monitorWidth]);
}

function useTheme() {
  const theme = useSetting<string>("appearance.theme", "dark");
  const accent = useSetting<string>("appearance.accent", "#7dd3fc");
  const color = useSetting<string>("appearance.notchColor", "#000000");
  const opacity = useSetting<number>("appearance.notchOpacity", 1);
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme === "light" ? "light" : "dark";
    root.style.setProperty("--accent", accent);
    const hex = theme === "light" && color === "#000000" ? "#fafafa" : color;
    const r = parseInt(hex.slice(1, 3), 16) || 0;
    const g = parseInt(hex.slice(3, 5), 16) || 0;
    const b = parseInt(hex.slice(5, 7), 16) || 0;
    root.style.setProperty("--notch-bg", `rgba(${r}, ${g}, ${b}, ${Math.max(0.3, Math.min(1, opacity))})`);
  }, [theme, accent, color, opacity]);
}

type Mode = "collapsed" | "popup" | "hud" | "alert" | "expanded";

export default function Notch() {
  useTheme();
  const settings = useStore((s) => s.settings);
  const size = useSize();
  const minimal = useSetting<boolean>("general.minimalMode", false);
  const pages = enabledPages(settings?.general?.pages);
  const page = useUi((s) => s.page) || pages[0]?.id;
  const setPage = useUi((s) => s.setPage);
  const holds = useUi((s) => s.holds);
  const menu = useUi((s) => s.menu);

  const [expanded, setExpanded] = useState(false);
  const [popup, setPopup] = useState<Popup | null>(null);
  const [hud, setHud] = useState<{ kind: "volume" | "brightness"; level: number; muted?: boolean } | null>(null);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [dropActive, setDropActive] = useState(false);
  const [dir, setDir] = useState(1);

  const expandedRef = useRef(false);
  const insideRef = useRef(false);
  const stickyRef = useRef(false);
  const enteredRef = useRef(false);
  const timers = useRef({ expand: 0, collapse: 0 });
  const popupQueue = useRef<Popup[]>([]);
  const holdsRef = useRef(0);
  holdsRef.current = holds + (menu ? 1 : 0);

  expandedRef.current = expanded;

  const expandDelay = settings?.general?.expandDelayMs ?? 120;
  const collapseDelay = settings?.general?.collapseDelayMs ?? 450;

  const collapseNow = useCallback(() => {
    const el = document.activeElement as HTMLElement | null;
    if (el && el !== document.body) el.blur();
    stickyRef.current = false;
    useUi.getState().closeMenu();
    setExpanded(false);
  }, []);

  const tryCollapse = useCallback(() => {
    if (!expandedRef.current) return;
    const typing = document.activeElement?.matches("input, textarea, select");
    if (holdsRef.current > 0 || typing) return;
    if (stickyRef.current && !enteredRef.current) return;
    collapseNow();
  }, [collapseNow]);

  const scheduleCollapse = useCallback(() => {
    window.clearTimeout(timers.current.collapse);
    timers.current.collapse = window.setTimeout(() => {
      if (!insideRef.current) tryCollapse();
    }, collapseDelay);
  }, [collapseDelay, tryCollapse]);

  // Cursor hit-testing happens in Rust (the window is click-through outside the notch).
  useEffect(
    () =>
      on<{ inside: boolean }>("notch-hover", ({ inside }) => {
        insideRef.current = inside;
        if (inside) {
          enteredRef.current = true;
          window.clearTimeout(timers.current.collapse);
          if (!expandedRef.current) {
            window.clearTimeout(timers.current.expand);
            timers.current.expand = window.setTimeout(() => {
              if (insideRef.current) setExpanded(true);
            }, expandDelay);
          }
        } else {
          window.clearTimeout(timers.current.expand);
          scheduleCollapse();
        }
      }),
    [expandDelay, scheduleCollapse],
  );

  // Re-check after a hold (menu, drag, editor) is released.
  useEffect(() => {
    if (holds === 0 && !menu && !insideRef.current && expandedRef.current) scheduleCollapse();
  }, [holds, menu, scheduleCollapse]);

  // Global hotkey / tray click.
  useEffect(
    () =>
      on("notch-toggle", () => {
        if (expandedRef.current) {
          collapseNow();
        } else {
          stickyRef.current = true;
          enteredRef.current = insideRef.current;
          setExpanded(true);
          void call("notch_focus");
        }
      }),
    [collapseNow],
  );

  // Clicking another app collapses the notch.
  useEffect(() => {
    const onBlur = () => {
      window.setTimeout(() => {
        if (!insideRef.current && !document.hasFocus()) {
          stickyRef.current = false;
          tryCollapse();
        }
      }, 150);
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, [tryCollapse]);

  // Drag files over the notch: open the shelf.
  useEffect(
    () =>
      on<{ state: string; accept?: boolean; own?: boolean }>("shelf-drag", (e) => {
        if (e.state === "enter" && e.accept) {
          setDropActive(true);
          setPage("shelf");
          setExpanded(true);
        } else if (e.state === "leave") {
          setDropActive(false);
          if (!insideRef.current) scheduleCollapse();
        }
      }),
    [scheduleCollapse, setPage],
  );

  useEffect(
    () =>
      on("shelf-added", () => {
        useUi.getState().showBanner("Added to shelf", { tone: "good" });
      }),
    [],
  );

  // Status popups (one at a time).
  useEffect(() => {
    let timer = 0;
    const next = () => {
      const p = popupQueue.current.shift() ?? null;
      setPopup(p);
      if (p) timer = window.setTimeout(next, 2600);
    };
    const un = on<Popup>("popup", (p) => {
      popupQueue.current = popupQueue.current.filter((x) => x.kind !== p.kind).concat(p).slice(-4);
      if (!timer || !popupQueue.current.length) {
        window.clearTimeout(timer);
        next();
      }
    });
    return () => {
      un();
      window.clearTimeout(timer);
    };
  }, []);

  // Volume / brightness HUD.
  const showVolumeHud = useSetting<boolean>("status.volume", true);
  const showBrightnessHud = useSetting<boolean>("status.brightness", true);
  useEffect(() => {
    let timer = 0;
    const show = (h: NonNullable<typeof hud>) => {
      setHud(h);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setHud(null), 1600);
    };
    const a = on<{ level: number; muted: boolean; own: boolean }>("volume", (v) => {
      if (!v.own && showVolumeHud) show({ kind: "volume", level: v.level, muted: v.muted });
    });
    const b = on<{ level: number }>("brightness", (v) => {
      if (showBrightnessHud) show({ kind: "brightness", level: v.level / 100 });
    });
    return () => {
      a();
      b();
      window.clearTimeout(timer);
    };
  }, [showVolumeHud, showBrightnessHud]);

  // Alerts + chimes.
  const chimeOn = useSetting<boolean>("focus.chime", true);
  useEffect(() => {
    const a = on<Alert>("alert", (al) => {
      setAlerts((list) => [...list.filter((x) => !(x.kind === al.kind && x.title === al.title)), al]);
      if (al.kind !== "focus" && al.kind !== "timer") chime("alert");
    });
    const b = on<{ kind: "focus-end" | "break-end" | "timer" }>("chime", (c) => {
      if (chimeOn || c.kind === "timer") chime(c.kind);
    });
    return () => {
      a();
      b();
    };
  }, [chimeOn]);

  // Alerts auto-hide after a while (the toast stays in Action Center).
  useEffect(() => {
    if (!alerts.length) return;
    const t = window.setTimeout(() => setAlerts((l) => l.slice(1)), 60_000);
    return () => window.clearTimeout(t);
  }, [alerts]);

  const dismissAlert = useCallback((al: Alert) => setAlerts((l) => l.filter((x) => x !== al)), []);

  const go = useCallback(
    (delta: number) => {
      const i = Math.max(0, pages.findIndex((p) => p.id === page));
      const next = pages[(i + delta + pages.length) % pages.length];
      setDir(delta);
      setPage(next.id);
    },
    [pages, page, setPage],
  );

  const jump = useCallback(
    (id: string) => {
      const from = pages.findIndex((p) => p.id === page);
      const to = pages.findIndex((p) => p.id === id);
      setDir(to >= from ? 1 : -1);
      setPage(id);
    },
    [pages, page, setPage],
  );

  // Keyboard: arrows, Esc, page letters (ignored while typing).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement)?.matches?.("input, textarea, select");
      if (e.key === "Escape") {
        if (typing) (e.target as HTMLElement).blur();
        else collapseNow();
        return;
      }
      if (typing || e.ctrlKey || e.altKey || e.metaKey || !expandedRef.current) return;
      if (e.key === "ArrowLeft") go(-1);
      else if (e.key === "ArrowRight") go(1);
      else {
        const p = pages.find((x) => x.key.toLowerCase() === e.key.toLowerCase());
        if (p) jump(p.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go, jump, pages, collapseNow]);

  // Mouse wheel switches pages unless a list under the cursor can scroll.
  // One gesture switches one page. A trackpad swipe keeps sending wheel events
  // while its momentum runs out, so the page stays locked until the events stop.
  const wheel = useRef({ acc: 0, lastEvent: 0, locked: false });
  const onWheel = (e: React.WheelEvent) => {
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    let el = e.target as HTMLElement | null;
    while (el && el !== e.currentTarget) {
      const style = getComputedStyle(el);
      const vertical = Math.abs(e.deltaY) >= Math.abs(e.deltaX);
      if (vertical && /(auto|scroll)/.test(style.overflowY) && el.scrollHeight > el.clientHeight + 1) {
        const atTop = el.scrollTop <= 0;
        const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
        if ((delta < 0 && !atTop) || (delta > 0 && !atBottom)) return;
      }
      if (!vertical && /(auto|scroll)/.test(style.overflowX) && el.scrollWidth > el.clientWidth + 1) return;
      el = el.parentElement;
    }
    const now = Date.now();
    const gap = now - wheel.current.lastEvent;
    wheel.current.lastEvent = now;
    if (gap > 200) wheel.current = { acc: 0, lastEvent: now, locked: false };
    if (wheel.current.locked) return;
    wheel.current.acc += delta;
    if (Math.abs(wheel.current.acc) >= 50) {
      go(wheel.current.acc > 0 ? 1 : -1);
      wheel.current.locked = true;
      wheel.current.acc = 0;
    }
  };

  const alert = alerts[0] ?? null;
  const mode: Mode = alert ? "alert" : expanded ? "expanded" : hud ? "hud" : popup ? "popup" : "collapsed";
  const collapsedW = useUi((s) => s.pillW) ?? size.pillW;

  const geom = useMemo(() => {
    switch (mode) {
      case "expanded":
        return { w: size.panelW, h: size.panelH, r: 26 };
      case "alert":
        return { w: Math.min(size.panelW, 470), h: 124, r: 26 };
      case "hud":
        return { w: size.pillW + 120, h: size.pillH + 8, r: (size.pillH + 8) / 2 };
      case "popup":
        return { w: size.pillW + 170, h: size.pillH + 8, r: (size.pillH + 8) / 2 };
      default:
        return minimal
          ? { w: size.pillW, h: 6, r: 3 }
          : { w: collapsedW, h: size.pillH, r: size.pillH / 2 };
    }
  }, [mode, size, minimal, collapsedW]);

  // Tell Rust which part of the window takes the mouse.
  useEffect(() => {
    const pad = mode === "expanded" ? 2 : 6;
    const hitH = mode === "collapsed" && minimal ? 14 : geom.h + pad;
    const rects = [{ x: (WIN_W - geom.w) / 2 - pad, y: 0, w: geom.w + pad * 2, h: hitH }];
    if (menu) rects.push({ x: 0, y: 0, w: WIN_W, h: 520 });
    void call("notch_set_hit", { rects });
  }, [geom, mode, minimal, menu]);

  const current = pages.find((p) => p.id === page) ?? pages[0];
  const PageComp = current?.component;

  return (
    <div className="pointer-events-none fixed inset-0 flex justify-center">
      <motion.div
        className="pointer-events-auto relative overflow-hidden"
        style={{ background: "var(--notch-bg)", boxShadow: mode === "collapsed" ? "none" : "0 10px 40px rgba(0,0,0,.35)" }}
        initial={false}
        animate={{
          width: geom.w,
          height: geom.h,
          borderBottomLeftRadius: geom.r,
          borderBottomRightRadius: geom.r,
        }}
        transition={SPRING}
        onWheel={mode === "expanded" ? onWheel : undefined}
        onContextMenu={(e) => e.preventDefault()}
      >
        <AnimatePresence initial={false} mode="popLayout">
          {mode === "collapsed" && !minimal && (
            <Fade key="collapsed" w={geom.w} h={geom.h}>
              <Collapsed height={size.pillH} pillW={size.pillW} />
            </Fade>
          )}
          {mode === "popup" && popup && (
            <Fade key={`popup-${popup.at}`} w={geom.w} h={geom.h}>
              <PopupView popup={popup} />
            </Fade>
          )}
          {mode === "hud" && hud && (
            <Fade key="hud" w={geom.w} h={geom.h}>
              <HudView hud={hud} />
            </Fade>
          )}
          {mode === "alert" && alert && (
            <Fade key={`alert-${alert.at}`} w={geom.w} h={geom.h}>
              <AlertCard alert={alert} onDone={() => dismissAlert(alert)} />
            </Fade>
          )}
          {mode === "expanded" && (
            <Fade key="expanded" w={geom.w} h={geom.h}>
              <div className="relative flex h-full flex-col px-4 pt-1.5 pb-1">
                <Header />
                <div className="relative min-h-0 flex-1">
                  <button
                    className="icon-btn absolute top-1/2 -left-3 z-10 h-7 w-5 -translate-y-1/2 text-[var(--faint)] hover:text-[var(--fg)]"
                    onClick={() => go(-1)}
                    aria-label="Previous page"
                  >
                    <ChevronLeft size={16} />
                  </button>
                  <button
                    className="icon-btn absolute top-1/2 -right-3 z-10 h-7 w-5 -translate-y-1/2 text-[var(--faint)] hover:text-[var(--fg)]"
                    onClick={() => go(1)}
                    aria-label="Next page"
                  >
                    <ChevronRight size={16} />
                  </button>
                  <AnimatePresence initial={false} custom={dir} mode="popLayout">
                    <motion.div
                      key={current?.id}
                      custom={dir}
                      className="absolute inset-0 px-4"
                      initial={{ opacity: 0, x: dir * 40 }}
                      animate={{ opacity: 1, x: 0 }}
                      exit={{ opacity: 0, x: dir * -40 }}
                      transition={SPRING}
                    >
                      {PageComp && <PageComp />}
                    </motion.div>
                  </AnimatePresence>
                  {dropActive && (
                    <div className="pointer-events-none absolute inset-1 z-20 flex items-center justify-center rounded-2xl border-2 border-dashed border-[var(--accent)] bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-[13px] font-semibold text-[var(--accent)]">
                      Drop to add to shelf
                    </div>
                  )}
                </div>
                <PageDots pages={pages} current={current?.id} onJump={jump} />
                <BannerView />
              </div>
            </Fade>
          )}
        </AnimatePresence>
      </motion.div>
      <ContextMenu />
    </div>
  );
}

/** Content wrapper sized to the target geometry so it never reflows while the shell animates. */
function Fade({ w, h, children }: { w: number; h: number; children: React.ReactNode }) {
  return (
    <motion.div
      className="absolute top-0 left-1/2"
      style={{ width: w, height: h, marginLeft: -w / 2 }}
      initial={{ opacity: 0, scale: 0.96, filter: "blur(4px)" }}
      animate={{ opacity: 1, scale: 1, filter: "blur(0px)" }}
      exit={{ opacity: 0, scale: 0.98, filter: "blur(4px)", transition: { duration: 0.12 } }}
      transition={{ ...SPRING, opacity: { duration: 0.18, delay: 0.06 } }}
    >
      {children}
    </motion.div>
  );
}
