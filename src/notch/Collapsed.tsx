import { useEffect, useRef, useState } from "react";
import { BatteryCharging, BatteryLow, Check, Moon, Timer } from "lucide-react";
import { clock, duration } from "../lib/format";
import { lineAt } from "../lib/lrc";
import { positionAt, useNow, useSetting, useStore } from "../lib/store";
import Mascot, { useMascotState } from "./Mascot";
import { useUi } from "./ui";

type Center =
  | { kind: "focus"; text: string }
  | { kind: "timer"; text: string }
  | { kind: "lyric"; text: string }
  | { kind: "mascot" }
  | { kind: "clock"; text: string };

function useCenter(): Center {
  const focus = useStore((s) => s.focus);
  const timers = useStore((s) => s.timers);
  const np = useStore((s) => s.media.current);
  // Lyrics and countdowns need sub-second updates; the clock doesn't.
  const now = useNow(np?.playing ? 200 : focus?.running || timers.length ? 500 : 1000);
  const lyrics = useStore((s) => s.lyrics);
  const inPill = useSetting<boolean>("music.lyricsInPill", true);
  const offsets = useSetting<Record<string, number>>("music.lyricOffsets", {});
  const mascot = useMascotState();

  if (focus?.running) return { kind: "focus", text: duration(focus.endsAt - now) };
  const t = timers.filter((x) => x.running).sort((a, b) => a.endsAt - b.endsAt)[0];
  if (t) return { kind: "timer", text: duration(t.endsAt - now) };
  if (inPill && np?.playing && lyrics.status === "ready" && lyrics.lines.length) {
    const off = (offsets[lyrics.key] ?? 0) * 1000;
    const i = lineAt(lyrics.lines, positionAt(np, now) + off);
    const text = i >= 0 ? lyrics.lines[i].text : "";
    if (text) return { kind: "lyric", text };
  }
  if (mascot !== "off") return { kind: "mascot" };
  return { kind: "clock", text: clock(new Date(now)) };
}

let measureCtx: CanvasRenderingContext2D | null = null;
function textWidth(text: string, font: string) {
  if (!measureCtx) measureCtx = document.createElement("canvas").getContext("2d");
  if (!measureCtx) return text.length * 7;
  measureCtx.font = font;
  return measureCtx.measureText(text).width;
}

/** The pill grows with the current lyric line. */
function pillWidth(c: Center, pillW: number, pillH: number) {
  if (c.kind !== "lyric") return null;
  const fontSize = Math.round(pillH * 0.38);
  const w = textWidth(c.text, `600 ${fontSize}px "Segoe UI Variable Text", "Segoe UI", sans-serif`);
  return Math.round(Math.min(560, Math.max(pillW, w + pillH * 2 + 36)));
}

function Equalizer({ size }: { size: number }) {
  return (
    <div className="flex items-center gap-[2px]" style={{ height: size }}>
      {[0.9, 0.55, 1, 0.7].map((d, i) => (
        <span
          key={i}
          className="w-[3px] rounded-full bg-[var(--accent)]"
          style={{
            height: "100%",
            transformOrigin: "50% 50%",
            animation: `eq ${0.7 + d * 0.5}s ease-in-out ${i * 0.12}s infinite`,
          }}
        />
      ))}
    </div>
  );
}

export default function Collapsed({ height, pillW }: { height: number; pillW: number }) {
  const c = useCenter();
  const wantW = pillWidth(c, pillW, height);
  const setPillW = useUi((s) => s.setPillW);
  useEffect(() => setPillW(wantW), [wantW, setPillW]);
  useEffect(() => () => setPillW(null), [setPillW]);
  const np = useStore((s) => s.media.current);
  const power = useStore((s) => s.status.power);
  const done = useStore((s) => s.completedToday);
  const dnd = useSetting<number>("dnd.until", 0) > Date.now();
  const showArt = useSetting<boolean>("music.showArt", true);
  const icon = Math.round(height * 0.52);
  const fontSize = Math.round(height * 0.38);

  // Briefly show the charging bolt when the charger state flips.
  const [flash, setFlash] = useState(false);
  const prev = useRef(power.charging);
  useEffect(() => {
    if (prev.current !== power.charging) {
      prev.current = power.charging;
      setFlash(true);
      const t = window.setTimeout(() => setFlash(false), 4000);
      return () => window.clearTimeout(t);
    }
  }, [power.charging]);

  const low = power.hasBattery && !power.charging && power.percent >= 0 && power.percent <= 20;
  let left: React.ReactNode = null;
  if (c.kind === "focus") left = <Timer size={icon} className="text-[var(--accent)]" />;
  else if (np?.playing && np.art && showArt)
    left = <img src={np.art} alt="" className="rounded-[5px] object-cover" style={{ width: icon + 2, height: icon + 2 }} />;
  else if (power.hasBattery && (power.charging || flash))
    left = (
      <div className="flex h-[18px] w-[18px] items-center justify-center rounded-full bg-[var(--good)]">
        <BatteryCharging size={12} className="text-black" strokeWidth={2.5} />
      </div>
    );
  else if (low) left = <BatteryLow size={icon} className="text-[var(--bad)]" />;
  else if (dnd) left = <Moon size={icon - 2} className="text-[var(--muted)]" />;

  let right: React.ReactNode = null;
  if (np?.playing) right = <Equalizer size={icon - 2} />;
  else if (done > 0)
    right = (
      <span className="flex items-center gap-0.5 text-[11px] font-semibold text-[var(--good)]">
        <Check size={12} strokeWidth={3} />
        {done}
      </span>
    );

  const accentText = c.kind === "focus" || c.kind === "timer";
  return (
    <div className="flex h-full items-center justify-between" style={{ paddingInline: height * 0.42 }}>
      <div className="flex w-[24px] justify-start">{left}</div>
      <div className="min-w-0 flex-1 overflow-hidden text-center">
        {c.kind === "mascot" ? (
          <Mascot height={height} />
        ) : (
          <span
            key={c.kind === "lyric" ? c.text : c.kind}
            className="tabular block truncate font-semibold"
            style={{
              fontSize: c.kind === "lyric" ? fontSize : fontSize + 1,
              color: accentText ? "var(--accent)" : "var(--fg)",
              animation: c.kind === "lyric" ? "none" : undefined,
            }}
          >
            {c.text}
          </span>
        )}
      </div>
      <div className="flex w-[24px] justify-end">{right}</div>
    </div>
  );
}
