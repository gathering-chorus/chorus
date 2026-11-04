/**
 * reply-gap.ts — #3864, #4887: is a role's reply in the room?
 *
 * chorus-hooks stamps `reply.emitted` (with a content hash) when a role's final
 * reply stands at the Stop hook. The watch asks the room's own record — the
 * Clearing v2 buffer, GET :3472/api/messages — whether a row with that hash is
 * there. An emitted reply older than the window with no room row is a
 * `reply.delivery.gap`.
 *
 * #4887: it used to pair reply.emitted with a `reply.rendered` spine echo the v1
 * tailer wrote. v1 no longer renders the room, so the echo was ~0 a day and every
 * reply read as a gap (Abby's 13:02:57 and 13:03:10, on screen, both "lost"). The
 * echo and its emitter retired in the same land; there is one source, the room.
 *
 * Dependency: the room buffer is on the messages retire list — when the messages
 * flow moves the room's record, readRoom moves with it.
 *
 * Pure core (detectGaps) + a thin watch loop the service wires at boot with
 * injectable readTail/readRoom/emit — the delivery-worker test pattern.
 */
import { contentHash } from '../../chorus-sdk/reply/hash';

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

/** One row of the room's record, as GET /api/messages returns it. */
export interface RoomMsg { from?: string; text?: string; ts?: string }

/** One scan: the gaps to fire, and how many replies the room could not answer for. */
export interface Scan { gaps: Gap[]; unmeasured: number }

/**
 * Pure: given spine events, the room's rows (null = the room could not be read),
 * now and the window, return the gaps not yet fired.
 *
 * A reply.emitted older than the window is delivered when a room row hashes to
 * its hash, a gap when none does — but only if the room's record reaches back to
 * it. A reply older than the room's oldest row (the buffer scrolled) or a room
 * that cannot be read is UNMEASURED, never a gap: the watch says what it cannot
 * see instead of alarming on it. Malformed events are skipped, never thrown on.
 */
export function detectGaps(
  events: SpineEv[],
  room: RoomMsg[] | null,
  nowMs: number,
  windowMs: number,
  alreadyFired: Set<string>,
): Scan {
  const inRoom = new Set<string>();
  let oldest = Infinity;
  for (const m of room ?? []) {
    if (typeof m.text === 'string') inRoom.add(contentHash(m.text));
    const at = m.ts ? Date.parse(m.ts) : NaN;
    if (!Number.isNaN(at) && at < oldest) oldest = at;
  }
  const gaps: Gap[] = [];
  let unmeasured = 0;
  for (const e of events) {
    if (e.event !== 'reply.emitted' || !e.role || !e.hash || !e.timestamp) continue;
    const at = Date.parse(e.timestamp);
    if (Number.isNaN(at) || nowMs - at <= windowMs) continue;
    if (inRoom.has(e.hash)) continue;
    if (room === null || at < oldest) { unmeasured += 1; continue; }
    const key = gapKey({ role: e.role, hash: e.hash });
    if (alreadyFired.has(key)) continue;
    gaps.push({ role: e.role, hash: e.hash, emittedAt: e.timestamp });
  }
  return { gaps, unmeasured };
}

export type ReadTail = () => Promise<string>;
/** The room's rows, or null when the room cannot be read. */
export type ReadRoom = () => Promise<RoomMsg[] | null>;
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
 * Watch loop: every interval, read the spine tail and the room, detect, emit
 * each gap once. An unreadable room is logged once when it starts, never gapped. Returns a stop function. `fired` is bounded by clearing entries older
 * than 10 windows — a long-running pulse must not grow a set forever.
 */
export function startReplyGapWatch(
  readTail: ReadTail,
  readRoom: ReadRoom,
  emitGap: EmitGap,
  intervalMs = 15_000,
  windowMs = REPLY_GAP_WINDOW_MS,
  now: () => number = Date.now,
): () => void {
  const fired = new Map<string, number>(); // key → firedAt
  let roomUnread = false;
  const timer = setInterval(() => {
    void (async () => {
      try {
        const events = parseSpineTail(await readTail());
        const room = await readRoom();
        if ((room === null) !== roomUnread) {
          roomUnread = room === null;
          console.error(JSON.stringify({ event: 'reply.gap.watch', room: roomUnread ? 'unreadable' : 'readable' }));
        }
        const nowMs = now();
        for (const [k, at] of fired) {
          if (nowMs - at > windowMs * 10) fired.delete(k);
        }
        for (const gap of detectGaps(events, room, nowMs, windowMs, new Set(fired.keys())).gaps) {
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
