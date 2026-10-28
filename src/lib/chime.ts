/** Generated chimes (Web Audio). No audio files needed. */

let ctx: AudioContext | null = null;

function audio() {
  if (!ctx) ctx = new AudioContext();
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

function tone(freq: number, start: number, length: number, type: OscillatorType = "sine", gain = 0.18) {
  const a = audio();
  const osc = a.createOscillator();
  const g = a.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  const t0 = a.currentTime + start;
  g.gain.setValueAtTime(0.0001, t0);
  g.gain.exponentialRampToValueAtTime(gain, t0 + 0.02);
  g.gain.exponentialRampToValueAtTime(0.0001, t0 + length);
  osc.connect(g).connect(a.destination);
  osc.start(t0);
  osc.stop(t0 + length + 0.05);
}

export type ChimeKind = "focus-end" | "break-end" | "timer" | "alert";

export function chime(kind: ChimeKind) {
  try {
    switch (kind) {
      case "focus-end":
        // Rising major triad: done, relax.
        [523.25, 659.25, 783.99, 1046.5].forEach((f, i) => tone(f, i * 0.16, 0.9));
        break;
      case "break-end":
        // Two bright bells: back to work.
        tone(880, 0, 0.7, "triangle");
        tone(1174.66, 0.28, 1.0, "triangle");
        break;
      case "timer":
        [0, 0.25, 0.5].forEach((t) => tone(988, t, 0.18, "square", 0.08));
        tone(1318.5, 0.8, 0.8, "sine");
        break;
      default:
        tone(784, 0, 0.4, "sine", 0.12);
        tone(1046.5, 0.14, 0.5, "sine", 0.12);
    }
  } catch {
    // Audio can fail without an output device; chimes are optional.
  }
}
