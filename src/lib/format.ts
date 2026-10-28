export function pad(n: number) {
  return n < 10 ? `0${n}` : `${n}`;
}

/** 3:07 or 1:02:03 */
export function duration(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

export function clock(d = new Date(), seconds = false) {
  const h = d.getHours();
  const m = d.getMinutes();
  const base = `${h}:${pad(m)}`;
  return seconds ? `${base}:${pad(d.getSeconds())}` : base;
}

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** Wed 28 Oct 2026 */
export function longDate(d = new Date()) {
  return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

export function shortDate(d: Date) {
  return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]}`;
}

export function dayLabel(iso: string) {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  return `${DAYS[date.getDay()]} ${d}`;
}

export function isSameDay(a: Date, b: Date) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** "Today 14:30", "Tomorrow 9:00", "Fri 1 Nov 9:00" */
export function relativeDateTime(ms: number) {
  const d = new Date(ms);
  const now = new Date();
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  const t = clock(d);
  if (isSameDay(d, now)) return `Today ${t}`;
  if (isSameDay(d, tomorrow)) return `Tomorrow ${t}`;
  if (isSameDay(d, yesterday)) return `Yesterday ${t}`;
  return `${shortDate(d)} ${t}`;
}

/** "in 12 min", "in 2 h 5 min", "now", "5 min ago" */
export function countdown(ms: number, now = Date.now()) {
  const diff = ms - now;
  const abs = Math.abs(diff);
  const mins = Math.round(abs / 60000);
  let text: string;
  if (mins < 1) return diff >= 0 ? "now" : "just now";
  if (mins < 60) text = `${mins} min`;
  else if (mins < 60 * 24) text = `${Math.floor(mins / 60)} h ${mins % 60 ? `${mins % 60} min` : ""}`.trim();
  else text = `${Math.round(mins / 1440)} d`;
  return diff >= 0 ? `in ${text}` : `${text} ago`;
}

export function bytes(n: number | null | undefined) {
  if (n == null) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function todayIso(d = new Date()) {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Value for <input type="datetime-local"> */
export function toLocalInput(ms: number | null) {
  if (!ms) return "";
  const d = new Date(ms);
  return `${todayIso(d)}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fromLocalInput(v: string): number | null {
  if (!v) return null;
  const t = new Date(v).getTime();
  return Number.isNaN(t) ? null : t;
}
