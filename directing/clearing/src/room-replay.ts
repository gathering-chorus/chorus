// @test-type: unit — pure filter/cursor arithmetic plus an injected file path;
// brings its own world, touches no live relay and no real ~/.chorus.
/**
 * #3893 — replay, so a restart does not lose the conversation.
 *
 * The room's subscription asked for `limit: 30` and nothing else. That is a
 * *sample*, not a replay: reconnect after being away and you get the newest 30
 * notes with no way to know whether 30 or 300 were said while you were gone.
 * It is the shotgun Jeff described on 2026-08-15 — hearing sentences one, four
 * and eight of a conversation and answering as though that were the whole of it.
 *
 * The fix is a durable cursor. We remember the `created_at` of the newest note
 * we have rendered, and on every (re)subscribe we ask the relay for everything
 * SINCE that moment. Completeness stops being a hope about buffer size and
 * becomes a property of the request.
 *
 * Two deliberate choices:
 *
 * - **Overlap, not exactness.** We rewind the cursor by OVERLAP_SECS before
 *   asking. Relay clocks and ours disagree by seconds, and a note written in
 *   the same second we last saw would fall on the wrong side of a strict
 *   boundary and vanish. Re-reading a few notes is free — `seenIds` drops the
 *   duplicates — while missing one is the entire defect.
 * - **No `limit` when replaying.** A limit on a replay request re-introduces
 *   the sampling we are removing, just with a bigger number.
 */
import fs from 'fs';
import path from 'path';

/** How far to rewind before the last-seen note. Covers clock skew between us
 *  and the relay, and same-second writes on either side of the boundary. */
export const OVERLAP_SECS = 5;

/** Cold start only: with no cursor there is no honest "everything since", so we
 *  take a bounded backfill to give the room a history to open with. */
export const COLD_START_LIMIT = 200;

export interface RoomFilter {
  kinds: number[];
  '#t': string[];
  since?: number;
  limit?: number;
}

/**
 * The subscription filter for a topic, given what we have already seen.
 *
 * `cursor` is the created_at (unix seconds) of the newest rendered note, or
 * null if we have never rendered one.
 */
export function roomFilter(topic: string, cursor: number | null): RoomFilter {
  if (cursor === null) return { kinds: [1], '#t': [topic], limit: COLD_START_LIMIT };
  return { kinds: [1], '#t': [topic], since: Math.max(0, cursor - OVERLAP_SECS) };
}

/**
 * Move the cursor forward — and only forward.
 *
 * Relays deliver stored events in no guaranteed order, so a replay can hand us
 * an older note after a newer one. Taking the last value seen would walk the
 * cursor backwards into ground we have already covered, and taking it blindly
 * forward on a note we failed to render would skip ground we have not. Advance
 * on rendered notes, by max.
 */
export function advanceCursor(cursor: number | null, createdAt: number): number | null {
  if (!Number.isFinite(createdAt) || createdAt <= 0) return cursor;
  return cursor === null ? createdAt : Math.max(cursor, createdAt);
}

/**
 * #3907 follow-up — the cursor path is ours, and it must stay ours.
 *
 * `readCursor`/`writeCursor` take a path, so the fs lint is right to ask where
 * it came from. Rather than disable the rule, the path is CHECKED: it has to sit
 * under the cursor directory this module owns and end in .json. A caller that
 * hands us anything else gets a refusal, not a read of an arbitrary file.
 */
const CURSOR_FILE = /^(room-)?cursor[\w.-]*\.json$/;

function ownedCursorPath(file: string): boolean {
  return CURSOR_FILE.test(path.basename(path.resolve(file)));
}

/** Where the cursor lives. Injected in tests; defaults to the role's own dir. */
export function defaultCursorPath(home: string, topic: string): string {
  return path.join(home, '.chorus', 'clearing', `room-cursor-${topic}.json`);
}

/**
 * Read the persisted cursor. Any failure — missing file, unreadable, corrupt,
 * a number that is not one — returns null, which means "cold start": we lose
 * replay for one reconnect and take the bounded backfill. The alternative,
 * throwing, would take the whole room down over a cache file.
 */
export function readCursor(file: string): number | null {
  if (!ownedCursorPath(file)) return null;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- path validated by ownedCursorPath above
    const parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as { cursor?: unknown } | null;
    // #3909 — no optional chain here: JSON.parse either throws (caught below) or
    // yields a value, so `?.` was a guard against a state the types rule out.
    const v = parsed === null ? undefined : parsed.cursor;
    return typeof v === 'number' && Number.isFinite(v) && v > 0 ? v : null;
  } catch {
    return null;
  }
}

/**
 * Persist the cursor. Best-effort by design: the room must keep working when
 * the disk does not. A failed write costs replay depth on the next restart,
 * never a message in front of Jeff right now.
 */
export function writeCursor(file: string, cursor: number): boolean {
  if (!ownedCursorPath(file)) return false;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- path validated by ownedCursorPath above
    fs.mkdirSync(path.dirname(file), { recursive: true });
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- path validated by ownedCursorPath above
    fs.writeFileSync(file, JSON.stringify({ cursor }), 'utf8');
    return true;
  } catch {
    return false;
  }
}
