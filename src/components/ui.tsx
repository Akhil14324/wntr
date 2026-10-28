import type { ReactNode } from "react";

export function Ring({
  value,
  size = 64,
  stroke = 6,
  color = "var(--accent)",
  children,
}: {
  value: number;
  size?: number;
  stroke?: number;
  color?: string;
  children?: ReactNode;
}) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const v = Math.max(0, Math.min(1, value));
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90">
        <circle cx={size / 2} cy={size / 2} r={r} stroke="var(--line)" strokeWidth={stroke} fill="none" />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          stroke={color}
          strokeWidth={stroke}
          fill="none"
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - v)}
          style={{ transition: "stroke-dashoffset 0.6s cubic-bezier(.2,.8,.2,1)" }}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">{children}</div>
    </div>
  );
}

export function Empty({ icon, title, hint, children }: { icon?: ReactNode; title: string; hint?: string; children?: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1.5 text-center">
      {icon && <div className="text-[var(--faint)]">{icon}</div>}
      <div className="text-[13px] font-medium">{title}</div>
      {hint && <div className="max-w-[340px] text-[11.5px] text-[var(--muted)]">{hint}</div>}
      {children}
    </div>
  );
}

export function Tabs<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex gap-0.5 rounded-[9px] bg-[var(--card)] p-0.5 text-[11.5px]">
      {options.map((o) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          className={`rounded-[7px] px-2 py-0.5 transition-colors ${
            value === o.value ? "bg-[var(--card-hover)] font-semibold" : "text-[var(--muted)] hover:text-[var(--fg)]"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export const TASK_COLORS: Record<string, string> = {
  blue: "#60a5fa",
  green: "#4ade80",
  yellow: "#facc15",
  orange: "#fb923c",
  red: "#f87171",
  purple: "#c084fc",
};

export function Toggle({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label?: string }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => onChange(!on)}
      className="relative h-[20px] w-[34px] shrink-0 rounded-full transition-colors"
      style={{ background: on ? "var(--accent)" : "var(--card-hover)" }}
    >
      <span
        className="absolute top-[2px] h-[16px] w-[16px] rounded-full bg-white shadow transition-all"
        style={{ left: on ? 16 : 2 }}
      />
    </button>
  );
}
