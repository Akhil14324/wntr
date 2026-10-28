import { useEffect, useRef, useState } from "react";
import { useSetting, useStore } from "../lib/store";

export type MascotState = "off" | "awake" | "dozing" | "wave";

/** Idle mascot: eyes after N quiet minutes, dozes 10 minutes later, waves when you return. */
export function useMascotState(): MascotState {
  const enabled = useSetting<boolean>("appearance.mascot", true);
  const idleMin = useSetting<number>("appearance.mascotIdleMin", 5);
  const idle = useStore((s) => s.idleSecs);
  const playing = useStore((s) => !!s.media.current?.playing);
  const focusing = useStore((s) => !!s.focus?.running);
  const [wave, setWave] = useState(false);
  const wasIdle = useRef(false);

  const threshold = Math.max(1, idleMin) * 60;
  const quiet = enabled && !playing && !focusing;
  const isIdle = quiet && idle >= threshold;

  useEffect(() => {
    if (wasIdle.current && !isIdle && quiet) {
      setWave(true);
      const t = window.setTimeout(() => setWave(false), 2600);
      wasIdle.current = false;
      return () => window.clearTimeout(t);
    }
    wasIdle.current = isIdle;
  }, [isIdle, quiet]);

  if (!enabled) return "off";
  if (wave) return "wave";
  if (!isIdle) return "off";
  return idle >= threshold + 600 ? "dozing" : "awake";
}

export default function Mascot({ height }: { height: number }) {
  const state = useMascotState();
  const eyeH = Math.round(height * 0.36);
  const eyeW = Math.round(eyeH * 0.62);
  if (state === "wave") {
    return (
      <span className="inline-flex items-center gap-1.5 text-[13px] font-semibold">
        <span className="inline-block origin-[70%_70%]" style={{ animation: "wave 1.2s ease-in-out 2" }}>
          👋
        </span>
        <span className="text-[var(--muted)]">Welcome back</span>
      </span>
    );
  }
  const closed = state === "dozing";
  return (
    <span className="relative inline-flex items-center justify-center gap-[7px]" style={{ height }}>
      {[0, 1].map((i) => (
        <span
          key={i}
          className="inline-block rounded-full bg-[var(--fg)]"
          style={{
            width: eyeW,
            height: closed ? 2 : eyeH,
            transition: "height .5s ease",
            animation: closed ? "none" : `blink 4.5s ease-in-out ${i * 0.04}s infinite`,
          }}
        />
      ))}
      {closed && (
        <span
          className="absolute -top-0.5 -right-3 text-[9px] font-bold text-[var(--muted)]"
          style={{ animation: "zz 2.4s ease-out infinite" }}
        >
          z
        </span>
      )}
    </span>
  );
}
