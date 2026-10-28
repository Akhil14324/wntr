import { useEffect, useState } from "react";
import {
  Cloud,
  CloudDrizzle,
  CloudFog,
  CloudLightning,
  CloudMoon,
  CloudRain,
  CloudSnow,
  CloudSun,
  Droplets,
  MapPin,
  Moon,
  RefreshCw,
  Sun,
  Wind,
  type LucideIcon,
} from "lucide-react";
import { call, errText, on } from "../lib/api";
import { dayLabel } from "../lib/format";
import { useSetting } from "../lib/store";
import { weatherInfo } from "../lib/weather";
import { Empty } from "../components/ui";

const ICONS: Record<string, LucideIcon> = {
  sun: Sun,
  moon: Moon,
  "cloud-sun": CloudSun,
  "cloud-moon": CloudMoon,
  cloud: Cloud,
  fog: CloudFog,
  drizzle: CloudDrizzle,
  rain: CloudRain,
  snow: CloudSnow,
  storm: CloudLightning,
};

export function WeatherIcon({ code, isDay = true, size = 18 }: { code?: number; isDay?: boolean; size?: number }) {
  const Icon = ICONS[weatherInfo(code, isDay).icon] ?? Cloud;
  return <Icon size={size} />;
}

export function useWeather() {
  const [data, setData] = useState<any>(null);
  const [error, setError] = useState("");
  const lat = useSetting<number | null>("weather.lat", null);
  const load = (force = false) =>
    call<any>("weather_get", { force })
      .then((d) => {
        setData(d);
        setError("");
      })
      .catch((e) => setError(errText(e)));
  useEffect(() => {
    void load();
    const t = window.setInterval(() => load(), 30 * 60 * 1000);
    const un = on("settings", () => load());
    return () => {
      window.clearInterval(t);
      un();
    };
     
  }, [lat]);
  return { data, error, reload: load };
}

export default function WeatherPage() {
  const { data, error, reload } = useWeather();
  const [city, setCity] = useState("");
  const [busy, setBusy] = useState(false);
  const [setupError, setSetupError] = useState("");

  const setup = async () => {
    setBusy(true);
    setSetupError("");
    try {
      await call("weather_set_city", { city });
    } catch (e) {
      setSetupError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!data) {
    return (
      <Empty icon={<CloudSun size={26} />} title="Set your city" hint={error || "Weather comes from Open-Meteo. Only the city coordinates are sent; no location tracking."}>
        <div className="mt-1 flex gap-1.5">
          <input
            className="field w-[200px] text-[12.5px]"
            placeholder="e.g. Jakarta"
            value={city}
            onChange={(e) => setCity(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && setup()}
          />
          <button className="btn btn-accent text-[12px]" disabled={busy || !city.trim()} onClick={setup}>
            Save
          </button>
        </div>
        {setupError && <div className="text-[11.5px] text-[var(--bad)]">{setupError}</div>}
      </Empty>
    );
  }

  const c = data.current ?? {};
  const d = data.daily ?? {};
  const unit = data.unit === "fahrenheit" ? "°F" : "°C";
  const info = weatherInfo(c.weather_code, c.is_day === 1);
  const days: number[] = (d.time ?? []).map((_: string, i: number) => i).slice(1, 4);

  return (
    <div className="flex h-full items-center gap-6">
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-1 text-[11.5px] text-[var(--muted)]">
          <MapPin size={11} /> <span className="max-w-[200px] truncate">{data.place}</span>
          <button className="icon-btn h-5 w-5" onClick={() => reload(true)} aria-label="Refresh">
            <RefreshCw size={11} />
          </button>
        </div>
        <div className="flex items-center gap-3">
          <WeatherIcon code={c.weather_code} isDay={c.is_day === 1} size={42} />
          <div className="font-display text-[44px] leading-none font-semibold tabular">
            {Math.round(c.temperature_2m)}
            <span className="text-[22px] text-[var(--muted)]">{unit}</span>
          </div>
        </div>
        <div className="text-[13px] font-medium">{info.label}</div>
        <div className="flex gap-3 text-[11.5px] text-[var(--muted)]">
          <span>Feels {Math.round(c.apparent_temperature)}°</span>
          <span className="flex items-center gap-0.5">
            <Droplets size={11} /> {c.relative_humidity_2m}%
          </span>
          <span className="flex items-center gap-0.5">
            <Wind size={11} /> {Math.round(c.wind_speed_10m)} km/h
          </span>
        </div>
      </div>
      <div className="grid flex-1 grid-cols-3 gap-2">
        {days.map((i) => (
          <div key={i} className="flex flex-col items-center gap-1 rounded-2xl bg-[var(--card)] py-2.5">
            <span className="text-[11.5px] text-[var(--muted)]">{dayLabel(d.time[i]).split(" ")[0]}</span>
            <WeatherIcon code={d.weather_code[i]} size={22} />
            <span className="text-[13px] font-semibold tabular">
              {Math.round(d.temperature_2m_max[i])}° <span className="text-[var(--muted)]">{Math.round(d.temperature_2m_min[i])}°</span>
            </span>
            {d.precipitation_probability_max?.[i] != null && (
              <span className="text-[10.5px] text-[#7dd3fc]">{d.precipitation_probability_max[i]}% rain</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
