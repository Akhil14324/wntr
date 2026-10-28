import {
  AlarmClock,
  BatteryCharging,
  BatteryLow,
  BatteryMedium,
  Bluetooth,
  BluetoothOff,
  CalendarDays,
  Check,
  GlassWater,
  Keyboard,
  ListTodo,
  Lock,
  LockOpen,
  Sun,
  Timer,
  Video,
  Volume1,
  Volume2,
  VolumeX,
  Wifi,
  WifiOff,
  X,
} from "lucide-react";
import { call, type Alert, type Popup } from "../lib/api";
import { useSetting } from "../lib/store";
import { bannerError } from "./ui";

function Row({ icon, title, detail, tone }: { icon: React.ReactNode; title: string; detail?: string; tone?: string }) {
  return (
    <div className="flex h-full items-center justify-between gap-3 px-4">
      <div className="flex min-w-0 items-center gap-2.5">
        <span style={{ color: tone ?? "var(--fg)" }}>{icon}</span>
        <span className="truncate text-[13px] font-semibold">{title}</span>
      </div>
      {detail && <span className="shrink-0 truncate text-[12.5px] font-semibold tabular" style={{ color: tone ?? "var(--muted)" }}>{detail}</span>}
    </div>
  );
}

export function PopupView({ popup }: { popup: Popup }) {
  const d = popup.data ?? {};
  switch (popup.kind) {
    case "charging":
      return <Row icon={<BatteryCharging size={18} />} title="Charging" detail={d.percent >= 0 ? `${d.percent}%` : ""} tone="var(--good)" />;
    case "unplugged":
      return <Row icon={<BatteryMedium size={18} />} title="On battery" detail={d.percent >= 0 ? `${d.percent}%` : ""} />;
    case "low":
      return <Row icon={<BatteryLow size={18} />} title="Battery low" detail={`${d.percent}%`} tone="var(--bad)" />;
    case "connected":
      return <Row icon={<Wifi size={17} />} title={d.wifi ? "Wi-Fi connected" : "Connected"} detail={d.name} tone="var(--accent)" />;
    case "disconnected":
      return <Row icon={<WifiOff size={17} />} title="Disconnected" detail={d.name} tone="var(--bad)" />;
    case "bt-connected":
      return (
        <Row
          icon={<Bluetooth size={17} />}
          title={d.name || "Bluetooth device"}
          detail={d.battery != null ? `${d.battery}%` : "Connected"}
          tone="#60a5fa"
        />
      );
    case "bt-disconnected":
      return <Row icon={<BluetoothOff size={17} />} title={d.name || "Bluetooth device"} detail="Disconnected" />;
    case "caps-on":
      return <Row icon={<Lock size={16} />} title="Caps Lock" detail="On" tone="var(--warn)" />;
    case "caps-off":
      return <Row icon={<LockOpen size={16} />} title="Caps Lock" detail="Off" />;
    case "language":
      return <Row icon={<Keyboard size={17} />} title={d.code || "Keyboard"} detail={d.name} tone="var(--accent)" />;
    default:
      return <Row icon={<Check size={16} />} title={popup.kind} />;
  }
}

export function HudView({ hud }: { hud: { kind: "volume" | "brightness"; level: number; muted?: boolean } }) {
  const pct = Math.round(Math.max(0, Math.min(1, hud.level)) * 100);
  const muted = hud.kind === "volume" && (hud.muted || pct === 0);
  const Icon = hud.kind === "brightness" ? Sun : muted ? VolumeX : pct < 50 ? Volume1 : Volume2;
  return (
    <div className="flex h-full items-center gap-3 px-4">
      <Icon size={17} className={muted ? "text-[var(--muted)]" : ""} />
      <div className="h-[5px] flex-1 overflow-hidden rounded-full bg-[var(--card-hover)]">
        <div
          className="h-full rounded-full bg-[var(--fg)] transition-[width] duration-150"
          style={{ width: `${muted ? 0 : pct}%` }}
        />
      </div>
      <span className="w-8 text-right text-[12px] font-semibold tabular">{muted ? "Mute" : pct}</span>
    </div>
  );
}

const KIND_ICON: Record<Alert["kind"], React.ReactNode> = {
  task: <ListTodo size={18} />,
  water: <GlassWater size={18} />,
  event: <CalendarDays size={18} />,
  focus: <Timer size={18} />,
  timer: <AlarmClock size={18} />,
};

export function AlertCard({ alert, onDone }: { alert: Alert; onDone: () => void }) {
  const snooze = useSetting<number[]>("tasks.snooze", [5, 10, 30]);
  const bottles = useSetting<number[]>("water.bottles", [250, 500]);
  const run = (p: Promise<unknown>) => p.then(onDone).catch(bannerError);
  const d = alert.data ?? {};

  let actions: React.ReactNode = null;
  if (alert.kind === "task") {
    actions = (
      <>
        <button className="btn btn-accent" onClick={() => run(call("task_complete", { id: d.id }))}>
          <Check size={14} /> Done
        </button>
        {snooze.map((m) => (
          <button key={m} className="btn" onClick={() => run(call("task_snooze", { id: d.id, minutes: m }))}>
            {m} min
          </button>
        ))}
      </>
    );
  } else if (alert.kind === "water") {
    actions = bottles.map((ml) => (
      <button key={ml} className="btn btn-accent" onClick={() => run(call("water_add", { ml }))}>
        + {ml} ml
      </button>
    ));
  } else if (alert.kind === "event" && d.joinUrl) {
    actions = (
      <button className="btn btn-accent" onClick={() => run(call("open_target", { target: d.joinUrl }))}>
        <Video size={14} /> Join
      </button>
    );
  } else if (alert.kind === "focus" && !d.autoStarted) {
    const label = d.next === "focus" ? "Start focus" : d.next === "long" ? "Start long break" : "Start break";
    actions = (
      <button className="btn btn-accent" onClick={() => run(call("focus_command", { action: "start" }))}>
        <Timer size={14} /> {label}
      </button>
    );
  }

  return (
    <div className="flex h-full flex-col justify-between px-5 pt-3 pb-3">
      <div className="flex items-start gap-3">
        <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--card-hover)] text-[var(--accent)]">
          {KIND_ICON[alert.kind]}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14px] font-semibold">{alert.title}</div>
          <div className="truncate text-[12px] text-[var(--muted)]">{alert.body}</div>
        </div>
        <button className="icon-btn h-7 w-7 shrink-0" onClick={onDone} aria-label="Dismiss">
          <X size={15} />
        </button>
      </div>
      <div className="flex items-center gap-1.5 text-[12px]">
        {actions}
        <button className="btn ml-auto" onClick={onDone}>
          Dismiss
        </button>
      </div>
    </div>
  );
}
