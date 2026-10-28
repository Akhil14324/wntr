import { BatteryCharging, BatteryFull, BatteryLow, BatteryMedium, Lock, Moon, Settings, Wifi, WifiOff } from "lucide-react";
import { call } from "../lib/api";
import { clock, longDate } from "../lib/format";
import { useNow, useSetting, useStore } from "../lib/store";

function BatteryBadge() {
  const p = useStore((s) => s.status.power);
  if (!p.hasBattery) return null;
  const color = p.charging ? "var(--good)" : p.percent <= 20 ? "var(--bad)" : "var(--fg)";
  const Icon = p.charging ? BatteryCharging : p.percent <= 20 ? BatteryLow : p.percent < 70 ? BatteryMedium : BatteryFull;
  return (
    <span className="flex items-center gap-1 text-[11px] font-semibold tabular" style={{ color }} title="Battery">
      <Icon size={15} />
      {p.percent >= 0 ? `${p.percent}%` : ""}
    </span>
  );
}

export default function Header() {
  const now = useNow(1000);
  const net = useStore((s) => s.status.network);
  const caps = useStore((s) => s.status.capsLock);
  const dnd = useSetting<number>("dnd.until", 0) > now;
  const d = new Date(now);
  return (
    <div className="relative flex h-[26px] shrink-0 items-center justify-between text-[12px]">
      <div className="flex w-[110px] items-center gap-2">
        <BatteryBadge />
      </div>
      <div className="flex items-center gap-2">
        <span className="text-[var(--muted)]">{longDate(d)}</span>
        <span className="tabular font-semibold">{clock(d)}</span>
      </div>
      <div className="flex w-[110px] items-center justify-end gap-2 text-[var(--muted)]">
        {caps && <Lock size={13} className="text-[var(--warn)]" aria-label="Caps Lock on" />}
        {dnd && <Moon size={13} aria-label="Do Not Disturb" />}
        <span title={net.connected ? net.name || "Connected" : "Offline"}>
          {net.connected ? <Wifi size={14} /> : <WifiOff size={14} className="text-[var(--bad)]" />}
        </span>
        <button className="icon-btn h-6 w-6" onClick={() => call("open_settings", {})} aria-label="Settings">
          <Settings size={14} />
        </button>
      </div>
    </div>
  );
}
