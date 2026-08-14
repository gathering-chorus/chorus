/**
 * reply-gap.ts — #3864: the join that makes reply delivery correlatable.
 *
 * chorus-hooks stamps `reply.emitted` when a role's final reply stands at the
 * Stop hook (the transcript is Claude Code delivery); Clearing's tailer
 * stamps `reply.rendered` with the same content hash when it routes that
 * reply into the room. An emitted whose rendered never arrives inside the
 * window is a `reply.delivery.gap` — the event Jeff currently produces by
 * asking "did you see that?" (2026-08-13: a bridge abort during a Clearing
 * restart took 20 minutes of hand-correlation; this makes it one event).
 *
 * Pure core (detectGaps) + a thin watch loop the service wires at boot with
 * injectable readTail/emit — the delivery-worker test pattern.
 */

export interface SpineEv {
  event: string;
  role?: string;
  hash?: string;
  timestamp?: string;
}

export interface Gap { role: string; hash: string; emittedAt: string }

export const REPLY_GAP_WINDOW_MS = 60_000;

export function gapKey(g: { role: string; hash: string }): string {
  return `${g.role}:${g.hash}`;
}

/**
 * Pure: given spine events, now, and the window, return gaps not yet fired.
 * A gap = reply.emitted older than the window with no reply.rendered carrying
 * the same role+hash. Malformed events are skipped, never thrown on — a bad
 * line in the spine must not kill the watcher.
 */
export function detectGaps(
  events: SpineEv[],
  nowMs: number,
  windowMs: number,
  alreadyFired: Set<string>,
): Gap[] {
  const rendered = new Set<string>();
  for (const e of events) {
    if (e.event === 'reply.rendered' && e.role && e.hash) rendered.add(gapKey({ role: e.role, hash: e.hash }));
  }
  const gaps: Gap[] = [];
  for (const e of events) {
    if (e.event !== 'reply.emitted' || !e.role || !e.hash || !e.timestamp) continue;
    const at = Date.parse(e.timestamp);
    if (Number.isNaN(at) || nowMs - at <= windowMs) continue;
    const key = gapKey({ role: e.role, hash: e.hash });
    if (rendered.has(key) || alreadyFired.has(key)) continue;
    gaps.push({ role: e.role, hash: e.hash, emittedAt: e.timestamp });
  }
  return gaps;
}

export type ReadTail = () => Promise<string>;
export type EmitGap = (gap: Gap) => Promise<void>;

/** Parse the spine tail into events; non-JSON lines are skipped. */
export function parseSpineTail(raw: string): SpineEv[] {
  const out: SpineEv[] = [];
  for (const line of raw.split('\n')) {
    if (!line.trim()) continue;
    try {
      const v = JSON.parse(line);
      if (v && typeof v.event === 'string') out.push(v);
    } catch { /* partial or foreign line — skip */ }
  }
  return out;
}

/**
 * Watch loop: every interval, read the spine tail, detect, emit each gap
 * once. Returns a stop function. `fired` is bounded by clearing entries older
 * than 10 windows — a long-running pulse must not grow a set forever.
 */
export function startReplyGapWatch(
  readTail: ReadTail,
  emitGap: EmitGap,
  intervalMs = 15_000,
  windowMs = REPLY_GAP_WINDOW_MS,
  now: () => number = Date.now,
): () => void {
  const fired = new Map<string, number>(); // key → firedAt
  const timer = setInterval(() => {
    void (async () => {
      try {
        const events = parseSpineTail(await readTail());
        const nowMs = now();
        for (const [k, at] of fired) {
          if (nowMs - at > windowMs * 10) fired.delete(k);
        }
        for (const gap of detectGaps(events, nowMs, windowMs, new Set(fired.keys()))) {
          fired.set(gapKey(gap), nowMs);
          await emitGap(gap);
        }
      } catch (e) {
        // Loud, not fatal: the watcher's own failure must not take pulse down,
        // but silence here would be a monitor that cannot report itself broken.
        console.error('[reply-gap] scan failed:', e instanceof Error ? e.message : e);
      }
    })();
  }, intervalMs);
  return () => clearInterval(timer);
}
