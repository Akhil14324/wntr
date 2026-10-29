import { useEffect, useRef, useState } from "react";
import { Minus, Plus, RefreshCw, Search, X } from "lucide-react";
import { call, errText, type Lyrics, type NowPlaying } from "../lib/api";
import { duration } from "../lib/format";
import { lineAt } from "../lib/lrc";
import { applyLyrics, loadLyrics, positionAt, useNow, useSetting, useStore } from "../lib/store";
import { Empty } from "../components/ui";
import { useUi } from "../notch/ui";
import { control } from "./Music";

interface Hit {
  id: number;
  track: string;
  artist: string;
  album: string;
  duration: number;
  synced: boolean;
}

function SearchPanel({ initial, onClose }: { initial: string; onClose: () => void }) {
  const [q, setQ] = useState(initial);
  const [hits, setHits] = useState<Hit[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const key = useStore((s) => s.lyrics.key);
  const hold = useUi((s) => s.hold);
  useEffect(() => hold(), [hold]);

  const run = async () => {
    if (!q.trim()) return;
    setBusy(true);
    setError("");
    try {
      setHits(await call<Hit[]>("lyrics_search", { query: q }));
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    void run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="absolute inset-0 z-10 flex flex-col gap-2 bg-[var(--notch-bg)]">
      <div className="flex items-center gap-2">
        <input
          autoFocus
          className="field flex-1 text-[12.5px]"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && run()}
          placeholder="Song title and artist"
        />
        <button className="btn text-[12px]" onClick={run} disabled={busy}>
          <Search size={13} /> Search
        </button>
        <button className="icon-btn" onClick={onClose} aria-label="Close search">
          <X size={16} />
        </button>
      </div>
      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto">
        {error && <div className="text-[12px] text-[var(--bad)]">{error}</div>}
        {busy && <div className="text-[12px] text-[var(--muted)]">Searching…</div>}
        {hits && !hits.length && !busy && <div className="text-[12px] text-[var(--muted)]">No results.</div>}
        {hits?.map((h) => (
          <button
            key={h.id}
            className="flex w-full items-center justify-between gap-2 rounded-lg px-2 py-1.5 text-left hover:bg-[var(--card)]"
            onClick={async () => {
              try {
                const l = await call<Lyrics>("lyrics_choose", { key, id: h.id });
                applyLyrics(key, l);
                onClose();
              } catch (e) {
                setError(errText(e));
              }
            }}
          >
            <div className="min-w-0">
              <div className="truncate text-[12.5px] font-medium">{h.track}</div>
              <div className="truncate text-[11px] text-[var(--muted)]">
                {h.artist} {h.album ? `· ${h.album}` : ""}
              </div>
            </div>
            <div className="shrink-0 text-[11px] tabular text-[var(--muted)]">
              {h.synced ? <span className="mr-2 text-[var(--accent)]">synced</span> : null}
              {duration(h.duration * 1000)}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

/** Synced lyrics for the current track, shown inline on the music page. */
export function LyricsView({ np }: { np: NowPlaying }) {
  const lyrics = useStore((s) => s.lyrics);
  const offsets = useSetting<Record<string, number>>("music.lyricOffsets", {});
  const setSettings = useStore((s) => s.setSettings);
  const now = useNow(200);
  const [searching, setSearching] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const userScroll = useRef(0);

  const offset = offsets[lyrics.key] ?? 0;
  const pos = positionAt(np, now) + offset * 1000;
  const idx = lineAt(lyrics.lines, pos);

  useEffect(() => {
    const el = box.current?.querySelector<HTMLElement>(`[data-line="${idx}"]`);
    const c = box.current;
    if (!el || !c || Date.now() - userScroll.current < 4000) return;
    c.scrollTo({ top: el.offsetTop - c.clientHeight / 2 + el.clientHeight / 2, behavior: "smooth" });
  }, [idx]);

  const setOffset = (v: number) => {
    const clamped = Math.max(-5, Math.min(5, Math.round(v * 10) / 10));
    void setSettings({ music: { lyricOffsets: { [lyrics.key]: clamped } } });
  };

  return (
    <div className="relative flex min-h-0 flex-1 flex-col gap-1">
      <div className="flex items-center gap-1 text-[10.5px] text-[var(--muted)]">
        <button className="icon-btn h-5 w-5" onClick={() => setOffset(offset - 0.5)} title="Lyrics earlier" aria-label="Lyrics earlier">
          <Minus size={11} />
        </button>
        <span className="w-12 text-center tabular" title="Lyric offset for this song">
          {offset > 0 ? "+" : ""}
          {offset.toFixed(1)} s
        </span>
        <button className="icon-btn h-5 w-5" onClick={() => setOffset(offset + 0.5)} title="Lyrics later" aria-label="Lyrics later">
          <Plus size={11} />
        </button>
        <button className="icon-btn ml-auto h-5 w-5" onClick={() => setSearching(true)} title="Search lyrics" aria-label="Search lyrics">
          <Search size={11} />
        </button>
      </div>

      <div className="relative min-h-0 flex-1">
        {lyrics.status === "loading" && <Empty title="Finding lyrics…" />}
        {(lyrics.status === "none" || lyrics.status === "error") && (
          <Empty title={lyrics.status === "error" ? "Couldn't reach LRCLIB" : "No synced lyrics found"} hint="Try a manual search with a different title or artist.">
            <div className="mt-1 flex gap-1.5">
              <button className="btn text-[12px]" onClick={() => setSearching(true)}>
                <Search size={13} /> Search
              </button>
              <button className="btn text-[12px]" onClick={() => np && loadLyrics(np, true)}>
                <RefreshCw size={13} /> Retry
              </button>
            </div>
          </Empty>
        )}
        {lyrics.status === "ready" && lyrics.lines.length > 0 && (
          <div
            ref={box}
            className="scroll-thin h-full overflow-y-auto py-[40%] [mask-image:linear-gradient(transparent,black_22%,black_78%,transparent)]"
            onWheel={() => (userScroll.current = Date.now())}
          >
            {lyrics.lines.map((l, i) => {
              const active = i === idx;
              const dist = Math.abs(i - idx);
              return (
                <button
                  key={i}
                  data-line={i}
                  onClick={() => np.canSeek && control("seek", Math.max(0, l.time - offset * 1000))}
                  className="block w-full origin-left rounded-md px-1 py-[3px] text-left transition-all duration-300 hover:bg-[var(--card)]"
                  style={{
                    fontSize: active ? 16 : 14,
                    fontWeight: active ? 700 : 600,
                    color: active ? "var(--fg)" : "var(--faint)",
                    opacity: active ? 1 : Math.max(0.35, 1 - dist * 0.15),
                    filter: active ? "none" : `blur(${Math.min(dist, 3) * 0.25}px)`,
                  }}
                >
                  {l.text || "♪"}
                </button>
              );
            })}
          </div>
        )}
        {lyrics.status === "ready" && !lyrics.lines.length && lyrics.plain && (
          <div className="scroll-thin h-full overflow-y-auto pr-2 text-[12.5px] leading-relaxed whitespace-pre-wrap text-[var(--muted)]">
            <div className="mb-1 text-[11px] text-[var(--warn)]">No synced lyrics — showing plain text</div>
            {lyrics.plain}
          </div>
        )}
        {searching && <SearchPanel initial={`${np.title} ${np.artist}`.trim()} onClose={() => setSearching(false)} />}
      </div>
    </div>
  );
}
