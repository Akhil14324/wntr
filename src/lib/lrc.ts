export interface LrcLine {
  time: number; // ms
  text: string;
}

/** Parse LRC text ("[mm:ss.xx] line", multiple stamps per line allowed). */
export function parseLrc(text: string): LrcLine[] {
  const out: LrcLine[] = [];
  let offset = 0;
  for (const raw of text.split(/\r?\n/)) {
    const off = raw.match(/^\[offset:\s*([+-]?\d+)\s*\]/i);
    if (off) {
      offset = Number(off[1]);
      continue;
    }
    const stamps = [...raw.matchAll(/\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/g)];
    if (!stamps.length) continue;
    const content = raw.replace(/\[[^\]]*\]/g, "").trim();
    for (const s of stamps) {
      const min = Number(s[1]);
      const sec = Number(s[2]);
      const frac = s[3] ? Number(s[3].padEnd(3, "0")) : 0;
      out.push({ time: min * 60000 + sec * 1000 + frac - offset, text: content });
    }
  }
  return out.sort((a, b) => a.time - b.time);
}

/** Index of the line playing at `ms`, or -1 before the first line. */
export function lineAt(lines: LrcLine[], ms: number): number {
  let lo = 0;
  let hi = lines.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].time <= ms) {
      ans = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return ans;
}
